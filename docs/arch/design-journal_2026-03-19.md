# 架构设计思考日志 — 2026-03-19

> 记录当日架构讨论的推演过程、被否决的方向、以及关键转折点。
> ADR 文件记录"决策是什么"，本日志记录"我们是怎么想到这个决策的"。

---

## 1. 从 SurrealDB 到 NebulaGraph + TiDB 的推演

### 起点：SurrealDB 的诱惑

SurrealDB 在设计之初是一个非常合理的选择：
- 原生图遍历（RELATE 语法，无需应用层实现）
- 文档模型天然对齐 OntologyObject 的灵活 schema
- 内置向量搜索（MVP 阶段省掉一个服务）
- 官方 Rust SDK

把 Ontology TBox + ABox + 权限 + 身份 + 向量全放进一个库，运维简单，开发体验好。

### 第一个疑问：单点太重

随着权限模型讨论深入，发现 SurrealDB 要承载的东西越来越多：
- Ontology 图核心（TBox + ABox + 关系）
- 用户、角色、组织（身份）
- 权限策略配置（EntityTypePermission / ABAC Policy）
- 审计日志（append-only）
- Agent Memory（向量 + 元数据）

**关键问题**：SurrealDB 2023 年才发布 1.x，生产稳定性未知。把所有核心数据压在一个新数据库上，爆炸半径过大。一旦出问题，整个系统瘫痪。

### 第二个疑问：招聘和运维成本

- 国内懂 SurrealDB 的工程师极少
- SurrealQL 非标准 SQL，查询无法移植
- 监控/备份/故障排查工具不成熟

### 转折：是否换 MongoDB？

考虑了用 MongoDB 存储身份和权限，原因是：
- JSON 灵活 schema
- 国内生产案例丰富
- 运维成熟

**否决理由**：ReBAC（基于关系的访问控制）要求能做 User → OntologyObject 的跨图遍历。User 本身就是一个 ABox 对象，权限关系（MANAGES / MEMBER_OF）也是图边。如果身份存 MongoDB、Ontology 存 SurrealDB，就需要在应用层拼接两个系统的数据——这正是我们极力避免的"应用层图遍历"。

### 最终方向：职责分离

**核心洞察**：SurrealDB 的核心价值是**图遍历**，不是"万能数据库"。

- **NebulaGraph**：只做图（TBox + ABox + 关系）。中国产，开源，美团/京东/快手生产案例，Rust SDK 可用。
- **TiDB**：承接所有结构化数据（身份/权限/审计/Agent Memory 元数据）。MySQL 兼容，水平扩容，国内生态最好。sqlx 是最成熟的 Rust DB crate。

爆炸半径缩小：SurrealDB（现阶段过渡期）只影响图，其他全用 TiDB，问题互不影响。

---

## 2. 权限模型的设计推演

### 起点：RBAC 够用吗？

简单 RBAC（用户 → 角色 → 权限）对 Ontology 平台不够用：
- 数据权限不只是"能不能操作这个 EntityType"，还需要"能不能看某个具体对象"
- Palantir 的核心价值就是细粒度数据权限

### 四粒度演进

```
RBAC  → "Alice 是 Editor 角色，可以写 Contract 类型"  （粗粒度，EntityType 级）
ReBAC → "Alice MANAGES 这个合同，所以可以看"          （对象级，图关系驱动）
ABAC  → "status=active 且 amount < 100万 的合同才可见" （行级，CEL 表达式）
Field → "合同金额字段是 CONFIDENTIAL，只有 Finance 角色" （字段级，分类标签）
```

**关键设计决策**：User 本身是 ABox 对象。这让 ReBAC 的图遍历可以统一在 NebulaGraph 中完成，无需跨系统拼接。

### 缓存的必要性

权限检查在每次数据读写时都会发生，高频场景（Agent 查询、列表页加载）会产生大量权限计算请求。

三层缓存设计：
```
L1：AccessDecision（2min）  → 最终结果缓存，命中率最高
L2：EnrichedIdentity（5min）→ 身份上下文，TiDB + NebulaGraph 两步查询代价高
L3：RBAC 角色权限（30min） → 角色配置变化频率最低
```

**刻意不缓存的项目**：
- ReBAC 边（图关系随时变化，如人员调岗）
- ABAC CEL 结果（对象属性随时变化）
- 字段可见矩阵（按需计算，结构复杂）

缓存失效机制：NATS 事件驱动，权限配置变更 → 发布事件 → 订阅方主动清缓存，避免轮询。

---

## 3. Embedding-svc 单点风险讨论

### 问题

独立出 embedding-svc 后，所有向量化请求集中到一个服务。如果这个服务挂了，影响：
- 语义缓存（agent-svc 的语义相似度判断失效）
- 实时 embedding（用户上传文件时的向量化）
- 批量 embedding（后台索引任务）

### 缓解方案

1. **Circuit Breaker（断路器）**：embedding-svc 不可用时，agent-svc 跳过语义缓存直接走 LLM，降级而非崩溃
2. **优先级队列**：实时请求（用户交互）优先于批量任务（后台索引）
3. **水平扩展**：embedding-svc 无状态，可以多实例部署，通过负载均衡分流
4. **fastembed-rs 本地化**：模型文件随服务部署，无外部依赖

单点风险可接受，因为降级路径清晰（LLM without semantic cache），不影响核心业务。

---

## 4. 内部服务通信：gRPC vs HTTP

### 讨论点

内部服务之间的调用（如 agent-svc → function-svc）是否应该继续用 HTTP JSON？

**倾向 gRPC 的理由**：
- Protobuf 二进制序列化，比 JSON 快 3-10x
- 强类型 schema，接口变更编译期发现
- 双向流式（Agent 的长任务很有用）
- 类似 Nacos + Dubbo 那套二进制 RPC 的体验

**保留 HTTP 的场景**：
- 对外 API（前端、第三方集成）
- 简单的管理接口（健康检查、元数据查询）

**决策**：内部服务 → gRPC (tonic + prost)，外部接口 → HTTP + JSON (axum + utoipa)

---

## 5. Agent 工具调用：MCP vs Tool Calling

### 背景

agent-svc 需要调用 ontology-svc / function-svc 的能力。有两种协议选择：

**Tool Calling（内部 API 包装）**
- 在代码中用 `#[ontology_function]` 宏标注函数
- 自动生成工具描述（JSON Schema）
- LLM 选择工具 → 直接 gRPC 调用
- 零外部依赖，性能最好

**MCP（Model Context Protocol）**
- Anthropic 推出的标准化工具协议
- 适合**外部**集成（第三方服务、外部数据源）
- 开销更大，适合松耦合场景

**决策**：
- 内部 API → Tool Calling（#[ontology_function] 宏 + gRPC）
- 外部集成 → MCP Client
- 暂不实现，记录决策即可

---

## 6. 可插拔基础设施：从单一架构到多客户适配

### 触发点

架构讨论到后期，提出了一个现实问题：
> 不同客户对存储基础设施有完全不同的要求。金融/政企要私有化，国内企业用阿里云，国际客户用 AWS，军工医疗甚至完全离线。

### 核心矛盾

如果为每个客户维护一套代码，维护成本爆炸。但如果写死一套基础设施，又无法适配不同客户。

### 解法：Trait 抽象 + DeploymentProfile 配置驱动

所有基础设施依赖通过 Trait 定义接口：
```
OntologyGraphStore → NebulaGraph / Neo4j / ArangoGraph
StructuredStore    → TiDB / MySQL / PolarDB / GaussDB
ObjectStore        → RustFS / S3 / OSS / OBS
VectorStore        → TiDB Vector / Qdrant / LanceDB
EventBus           → NATS / RocketMQ / Kafka / SQS
CacheStore         → Redis / 本地内存
KeyManager         → Vault / AliKMS / AWS KMS
```

每个客户只需要一份 `deployment.toml`，启动时 `InfrastructureContainer::from_profile()` 动态装配对应实现。业务代码完全不感知底层存储是什么。

**数据主权天然实现**：
- AirGapped Profile → 所有数据在本地，无外网请求
- Aliyun Profile → 数据在阿里云中国区，满足 PIPL
- AWS Profile（EU Region）→ 满足 GDPR

---

## 7. 架构文档组织方式讨论

### 问题

`docs/ontology_ai_architecture.md` 一个大文件越来越难维护，搜索不便，版本追踪困难。

### 决策：分层目录 + 版本化文件名

```
docs/arch/
├── overview_v0.1.6.md        # 总览，ADR 索引
├── adr/                       # 每个决策独立文件
│   ├── ADR-01-*.md
│   └── ...
├── services/                  # 每个服务子架构
├── crates/                    # 共享库架构
├── infrastructure/            # 基础设施细节
├── domain/                    # 领域模型
└── frontend/                  # 前端架构
```

**文件名包含版本号**（如 `_v0.1.0.md`）的原因：
- git 可以追踪历史，但文件名版本让"打开文件就知道版本"
- 不同版本并存时不会覆盖，便于对比

**ADR 的价值**：记录"为什么"，而不只是"是什么"。6 个月后看到 `NebulaGraph + TiDB`，如果没有 ADR-27，根本不知道当初为什么不用 SurrealDB，也不知道评估过哪些备选方案。

---

## 今日决策汇总

| 编号 | 决策 | 关键推理 |
|------|------|---------|
| ADR-20 | 内部服务用 gRPC | 性能 + 强类型，外部保持 HTTP |
| ADR-21 | Consul 服务发现 | 自注册，K8s DNS 接管，ServiceDiscovery trait 抽象 |
| ADR-22 | function-svc 为出站集中出口 | 审计、限速、API Key 管理统一收口 |
| ADR-23 | Gateway 五层防御 | TLS → Rate Limit → JWT → auth-svc → Audit |
| ADR-24 | Dual Token 安全方案 | Access Token 在内存（防 XSS），Refresh Token HttpOnly Cookie（防 CSRF） |
| ADR-25 | MCP / Tool Calling 分场景 | 内部用宏包装，外部用 MCP；暂不实现 |
| ADR-26 | 四粒度权限模型 | RBAC → ReBAC → ABAC → Field，三层缓存，事件驱动失效 |
| ADR-27 | NebulaGraph + TiDB 替代 SurrealDB | 爆炸半径缩小，国内生态，运维成熟度 |
| ADR-28 | 可插拔基础设施 | DeploymentProfile 驱动，支持多云 + 离线，数据主权天然隔离 |

---

## 未解决的问题（留待下次讨论）

- [ ] 多租户方案（ADR-04 暂缓）：SaaS 场景下，租户隔离是用 namespace 还是独立 Schema？
- [ ] NebulaGraph 的 Rust SDK 成熟度：`nebula-rust` crate 活跃度如何，是否需要封装 HTTP API 作为备选？
- [ ] TiDB Vector vs Qdrant 切换时机：50 万向量 / P99 > 200ms 的监控告警如何实现？
- [ ] cargo xtask dev：进程管理如何处理 NebulaGraph 的多进程（storaged + graphd + metad）？
- [ ] Helm Chart：K8s 部署时，DeploymentProfile 如何通过 ConfigMap 注入？
