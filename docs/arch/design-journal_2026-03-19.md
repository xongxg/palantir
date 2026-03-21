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
| ADR-10 v2.0 | EventBus 可插拔后端（国内云扩展） | NATS 升为默认；KafkaBus 覆盖阿里云 ONS / 华为云 DMS / 腾讯云 CKafka；RocketMqHttpBus 作为 HTTP 兜底；Fluvio 降为观察项 |

---

## 8. Ontology → Agent 语义理解与业务熟悉路径

### 问题背景

拥有 Ontology 之后，如何让 Agent 真正理解业务语义、熟悉业务逻辑？

### 核心结论：不是"训练模型"，而是"运行时注入 + 记忆积累"

这是一个重要的架构认知：**我们不需要 fine-tune LLM**。业务理解通过三个机制在运行时实现。

### 机制一：Schema 骨架注入（静态理解）

Ontology TBox（EntityType + RelationshipType + FieldDef）序列化后注入 Agent System Prompt：

```
"你可以操作以下实体：
 Contract { amount: Number, status: String, ... }
 Employee { name: String, department: Reference<Dept>, ... }
 关系：Employee -[WORKS_FOR]→ Department ..."
```

LLM 通过 schema context 理解业务实体结构，无需额外训练。字段分级（Public / Internal / Confidential / PII）也随 schema 一起注入，Agent 天然知道哪些数据需要保护。

### 机制二：函数注册表 — 业务能力的语义桥

```rust
#[ontology_function]
fn calculate_contract_risk(contract: &Contract) -> f64 { ... }
```

宏自动生成 OpenAI Tool schema，LLM 看到的是结构化工具描述，知道"能做什么"。
三层业务逻辑（Rust 函数 → CEL 表达式 → 自然语言生成 CEL）覆盖工程师、分析师、业务用户三类受众。

### 机制三：运行时语义理解（RAG + 图遍历）

```
用户提问："这个合同相关的所有员工风险"
  ↓
1. embed-svc → 向量化 → 命中语义缓存？
  ↓ 未命中
2. 注入 schema context + 可用 Function 列表
  ↓
3. LLM 生成执行计划（graph traversal + function calls）
  ↓
4. NebulaGraph 图遍历（2-5 hop）拿到关联对象
  ↓
5. function-svc 执行 CEL / Rust 计算
  ↓
6. LLM 合成自然语言结果
```

**关键点**：Ontology 提供 schema 骨架，图遍历提供语义连接，embedding 提供相似度检索，三者缺一不可。

### 机制四：记忆积累（Agent 熟悉业务的长效机制）

```
每次执行结果满足：confidence ≥ 0.85 && access_count > 2
  ↓
存入 agent-memory（向量化 + 链接到 OntologyObject）
  ↓
下次类似问题 → few-shot 注入 → Agent 直接复用历史成功路径
```

**本质**：高置信度的成功交互 = 业务知识的自然沉淀。不需要人工标注，生产流量驱动学习。

### 事件驱动保鲜

业务数据变更 → NATS OntologyEvent → agent-svc 刷新 schema 缓存 + embedding-svc 重建向量索引。Agent 不轮询，始终感知最新业务状态。

### 当前实现状态

| 模块 | 状态 |
|------|------|
| SourceAdapter（REST/SQL/CSV/JSON） | ✅ 已实现，Bug 修复中 |
| Ontology ingest pipeline | 🚧 SourceAdapter → OntologyObject 链路待打通 |
| embedding-svc | 📄 ADR-19 设计完成，待实现 |
| agent-svc | 📄 ADR-25 设计完成，待实现 |
| Agent Memory 积累 | 📄 ADR-06 设计完成，待实现 |

**重点**：先把 SourceAdapter → ingest pipeline → OntologyObject 这条链路打通，有了高质量结构化数据，上层 Agent 才有东西可理解。

---

## 9. LlmProvider 可插拔设计

### 背景

Agent 接入 LLM 时，不应绑定单一 provider。不同部署场景对 LLM 有完全不同的要求：
- 生产环境追求效果 → Claude / GPT-4
- 国内私有化部署 → 通义千问（数据不出国）
- 离线 / AirGapped → Ollama 本地模型
- 开发调试阶段 → Ollama 本地（省钱）

### 设计决策

**LlmProvider trait** 纳入 `InfrastructureContainer`，与 ADR-28 可插拔基础设施保持一致：

```rust
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn chat(&self, messages: Vec<Message>) -> Result<String, LlmError>;
    async fn chat_with_tools(&self, messages: Vec<Message>, tools: Vec<ToolDef>)
        -> Result<LlmResponse, LlmError>;
}
```

**关键洞察**：OpenAI API 格式已成事实标准。Ollama / LM Studio / vLLM / 国内大部分云厂商（通义、豆包、智谱）均兼容。因此只需一个 `OpenAiCompatibleProvider` 实现，通过 `base_url` 切换目标：

```rust
pub struct OpenAiCompatibleProvider {
    base_url: String,  // "https://api.openai.com" 或 "http://localhost:11434/v1"
    api_key:  String,  // 本地 Ollama 填任意字符串即可
    model:    String,  // "claude-opus-4-6" / "qwen-max" / "llama3.2" / "deepseek-r1"
}
```

**配置驱动，业务代码零改动**：

```toml
# deployment.toml

# 生产环境
[llm]
provider = "anthropic"
model    = "claude-opus-4-6"
api_key  = "${ANTHROPIC_API_KEY}"

# 国内私有化
[llm]
provider = "openai_compatible"
base_url = "https://dashscope.aliyuncs.com/compatible-mode/v1"
model    = "qwen-max"
api_key  = "${DASHSCOPE_API_KEY}"

# AirGapped 离线
[llm]
provider = "openai_compatible"
base_url = "http://localhost:11434/v1"
api_key  = "ollama"
model    = "qwen2.5:14b"
```

### DeploymentProfile 对照

| Profile | LLM | 原因 |
|---------|-----|------|
| Standard | Claude API | 效果最好 |
| Aliyun | 通义千问 | 数据不出国，满足 PIPL |
| AWS | OpenAI / Claude | 国际环境 |
| AirGapped | Ollama 本地 | 完全离线，无外网请求 |
| Dev | Ollama 本地 | 省钱，快速迭代 |

### 实现计划

待 ingest pipeline 打通后，在详细设计阶段写入代码：
1. `palantir-infrastructure` crate 中定义 `LlmProvider` trait
2. 实现 `OpenAiCompatibleProvider`（覆盖 Ollama / 通义 / OpenAI）
3. 实现 `AnthropicProvider`（Claude 原生 API）
4. `InfrastructureContainer` 增加 `llm_provider()` 方法
5. `deployment.toml` 增加 `[llm]` 配置节

---

## 10. 数据源覆盖范围扩展

### 背景

Palantir 官方支持的数据源远不止文件和 REST API。真实企业环境中，数据分布在对象存储、关系型数据库、文档数据库、搜索引擎和消息队列中。

### 数据源分类与优先级

**P0（已实现）**
| 类型 | 文件 |
|------|------|
| CSV 文件 | `adapters_csv.rs` |
| JSON / JSONL | `adapters_json.rs` |
| SQL（SQLite） | `adapters_sql.rs` |
| REST API | `adapters_rest.rs` |

**P1（存根已就位，待实现）**
| 类型 | 文件 | 依赖 crate | 覆盖范围 |
|------|------|-----------|---------|
| S3 / 对象存储 | `adapters_s3.rs` | `object_store` | AWS S3 / 阿里云 OSS / 腾讯 COS / 华为 OBS / MinIO |
| PostgreSQL / MySQL | `adapters_postgres.rs` | `sqlx`（已有，加 feature） | 主流关系型数据库 |
| MongoDB | `adapters_mongodb.rs` | `mongodb` | 文档型数据库 |
| Elasticsearch | `adapters_elasticsearch.rs` | `reqwest`（已有） | 日志 / 全文搜索 |
| Kafka | `adapters_kafka.rs` | `rdkafka` | 实时流、消息队列 |
| Excel / ODS | `adapters_excel.rs` | `calamine` | 企业常见电子表格 |

**P2（规划中，未建文件）**
| 类型 | 说明 |
|------|------|
| RocketMQ / Pulsar | 国内消息队列 |
| SaaS（飞书 / 钉钉 / 纷享销客） | 企业 SaaS 集成 |
| SAP / 用友 / 金蝶 | ERP 系统 |

### 关键设计决策

**object_store crate 覆盖所有 S3 兼容存储**：一套代码，通过 endpoint 配置切换 AWS / 阿里云 / 腾讯云 / MinIO，符合 ADR-28 可插拔基础设施的思路。

**Kafka 与批量适配器的本质差异**：
- 批量适配器：拉取快照（静态文件或 SQL 结果集）
- Kafka：持续消费流，cursor = Kafka offset，天然支持断点续传
- `stream()` 方法在 Kafka 场景下是真正的无限流，而非一次性迭代

**实现原则**：当前阶段 P1/P2 均返回 `Err("not yet implemented")`，trait 接口已稳定，实现时只需填充逻辑，调用方无需改动。

---

## 未解决的问题（留待下次讨论）

- [ ] 多租户方案（ADR-04 暂缓）：SaaS 场景下，租户隔离是用 namespace 还是独立 Schema？
- [ ] NebulaGraph 的 Rust SDK 成熟度：`nebula-rust` crate 活跃度如何，是否需要封装 HTTP API 作为备选？
- [ ] TiDB Vector vs Qdrant 切换时机：50 万向量 / P99 > 200ms 的监控告警如何实现？
- [ ] cargo xtask dev：进程管理如何处理 NebulaGraph 的多进程（storaged + graphd + metad）？
- [ ] Helm Chart：K8s 部署时，DeploymentProfile 如何通过 ConfigMap 注入？

---

## 后续自然延伸点（ADR 待写）

### ADR-29（已完成）
国内企业大量使用 Nacos / Zookeeper / Apollo 等配置中心和服务发现组件，强制引入 Consul 浪费已有投资。
决策：ServiceDiscovery + ConfigCenter 双 Trait，Nacos 二合一，Dubbo 生态兼容。

### ADR-30（已完成）：可插拔可观测性（监控 + 追踪 + 日志 + 故障定位）

核心决策：
- **OpenTelemetry OTLP** 作为统一中间层，覆盖 Traces + Metrics + Logs
- `TelemetryProvider` trait：`tracer()` + `meter()` + `report_error()`（Sentry-like 聚合）
- `LogSink` trait：仅 AirGapped 离线场景用，其他走 OTLP log exporter
- **审计日志不在此 ADR**，复用 ADR-09 的 `AuditLog` trait
- `trace_id` 通过 W3C TraceContext 贯穿 HTTP / gRPC / NATS 消息
- Workflow 心跳 metric + Alertmanager 规则防卡死无感知
- Agent 工具调用强制 `#[tracing::instrument]` 埋点
- NebulaGraph：`PROFILE <nGQL>` 定位慢查询；TiDB：`slow_query` 系统表

→ 详见 [ADR-30](adr/ADR-30-observability.md)

### ADR-31（待讨论）：可插拔日志收集细化（AirGapped 归档策略）
- AirGapped 本地文件 rotate + gzip 归档的具体参数
- 离线环境日志定期导出（U 盘 / 内网传输）流程

### ADR-32（待讨论）：代码实现 — palantir-infrastructure crate
把 ADR-28 / ADR-29 定义的所有 Trait 在 `palantir-infrastructure` crate 中落地：
- Standard Profile 的默认实现优先（NebulaGraph + TiDB + NATS + Redis + Consul）
- InfrastructureContainer::from_profile() 实现
- 单元测试用 InMemory 实现
