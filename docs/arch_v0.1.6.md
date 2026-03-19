# Palantir Architecture — v0.1.6

> 版本：v0.1.6 | 日期：2026-03-19 | 状态：设计阶段
>
> 更新规则：每日 refine → patch 版本递增；服务新增/删除 → minor；底层存储/协议更换 → major
>
> **v0.1.6 变更**：ADR-18 Apache Arrow + DataFusion 加入 L2；计算分层升级为四层（L1-L4）；Arrow/Redis 配合机制；embedding-svc 独立服务；BGE-small-zh 替换 all-MiniLM

---

## 1. 核心原则

| 原则 | 说明 |
|------|------|
| 领域边界清晰 | 每个服务对应一个有界上下文，不跨域直接访问数据库 |
| 事件驱动骨干 | 写操作产生 OntologyEvent → Event Bus → 订阅方异步消费 |
| 同步调用极简 | 只有"需要立即返回结果"的场景才同步（Agent → Function） |
| Library / Service 分离 | 核心逻辑在 `crates/`（可测试），服务壳在 `services/`（薄封装） |
| Trait 优先 | 每个基础设施依赖都有 trait 抽象，换实现不改调用方 |
| 成本感知 | 优先本地计算（ONNX embedding、SurrealDB 内置向量），按需引入外部服务 |

---

## 2. Workspace 结构

```
palantir/
├── crates/
│   ├── palantir-ontology-manager/   # 现有：adapter / mapping / model
│   ├── palantir-domain/             # 现有：领域模型
│   ├── palantir-persistence/        # 现有：SQLite 持久化
│   ├── palantir-pipeline/           # 现有：Saga / transform 原语
│   ├── palantir-agent/              # 重构：AI Agent 核心库
│   ├── palantir-event-bus/          # NEW：EventPublisher/Subscriber trait
│   ├── palantir-function-core/      # NEW：Function/Logic trait + 注册表
│   └── palantir-auth-core/          # NEW：Policy 类型 + 评估器 trait
├── services/
│   ├── api-gateway/                 # JWT 解析 + 路由 + SSE 转发（唯一前端入口）
│   ├── embedding-svc/               # NEW：集中式向量化（fastembed-rs + BGE-small-zh）
│   ├── ontology-svc/                # Ontology CRUD + 事件流（Single Source of Truth）
│   ├── ingest-svc/                  # 数据摄入（Source / Mapping / Cursor）
│   ├── function-svc/                # Function / Logic 注册与执行
│   ├── agent-svc/                   # AI Agent 推理入口
│   ├── workflow-svc/                # Workflow 编排 + Saga 补偿
│   └── auth-svc/                    # RBAC + ABAC + ReBAC 权限服务
└── frontend/                        # 独立前端项目（React + TypeScript + Vite）
    ├── src/
    │   ├── pages/                   # 路由页面
    │   ├── components/              # 通用组件
    │   ├── api/                     # openapi-typescript 生成的类型 + client
    │   └── features/
    │       ├── ontology/            # Ontology 图可视化（React Flow）
    │       ├── ingest/              # Source / Mapping 管理
    │       ├── function/            # CEL Web IDE（Monaco Editor）
    │       ├── agent/               # Agent 对话（SSE 流式）
    │       └── workflow/            # Workflow 设计器
    ├── package.json
    └── vite.config.ts
```

---

## 3. 服务职责速览

| 服务 | 职责 | 主要复用 crate |
|------|------|--------------|
| `ontology-svc` | TBox/ABox CRUD、事件发布、/v1/sync 离线合并 | palantir-ontology-manager, palantir-persistence |
| `ingest-svc` | Source/Mapping 管理、摄入调度、游标续传 | palantir-ontology-manager（adapter/mapping）|
| `function-svc` | Function/Logic 注册与执行，CEL 表达式引擎 | palantir-function-core |
| `agent-svc` | LLM 推理、Multi-Agent、语义缓存、AgentTrace | palantir-agent |
| `workflow-svc` | Workflow 编排、Cron/事件触发、Saga 补偿 | palantir-pipeline |
| `auth-svc` | RBAC+ABAC+ReBAC 策略管理与评估（< 5ms） | palantir-auth-core |

---

## 4. 基础设施选型

| 层 | 选型 | 阶段 | 理由 |
|----|------|------|------|
| Ontology 存储 | SurrealDB（RocksDB → TiKV）| 全程 | 原生图遍历 + 多模型 + Rust SDK |
| 文件存储 | RustFS（S3-compatible，单二进制）| 全程 | 用户上传场景，本地 FS 无法多实例共享；`object_store` crate 抽象 |
| 向量搜索 | SurrealDB 内置 → LanceDB → Qdrant | 按需演进 | MemoryStore trait，逐级引入 |
| 本地 Embedding | fastembed-rs + BGE-small-zh（512维）| 全程 | 中文优化，零 API 成本，独立 embedding-svc |
| 列式计算 | Apache Arrow + DataFusion | 全程 | L2 分析查询引擎，CEL 编译执行，Arrow IPC 序列化 |
| 缓存 | Redis | 全程 | 短期热数据、Semantic Cache |
| 事件总线 | InProcessBus → Fluvio / NATS JetStream | 单体→微服务 | EventPublisher trait 抽象 |
| CEL 引擎 | `cel-interpreter` crate | P1 | 安全无副作用的业务表达式 |

---

## 5. Event Bus 分层实现

```
EventPublisher / EventSubscriber trait
  ├── InProcessBus（tokio broadcast）← 开发/单进程
  ├── FluvioBus（Rust 原生）         ← 微服务生产首选
  ├── NatsBus（NATS JetStream）      ← 保守备选，生产案例多
  └── KafkaBus                      ← 大数据量场景，未来
```

Topic 规范：`ontology.events.{upsert|delete|link}`、`ingest.jobs.created`、`workflow.triggers`、`agent.feedback`

---

## 6. 向量搜索成本控制策略

**本地 Embedding（零边际成本）**
- ONNX Runtime + all-MiniLM-L6-v2（384维），替代 OpenAI Embedding API

**分层检索（Tiered Retrieval）**
```
1. Semantic Cache 命中？→ 直接返回（零成本）
2. BM25 全文搜索（SurrealDB 内置）→ 命中率 ~60%
3. 本地向量搜索（SurrealDB 内置）→ 命中率 ~90%
4. Qdrant（可选，>100万条向量时引入）
```

**选择性 Embedding 策略**
```rust
// 仅满足条件的 Memory 才写入向量索引
if memory.confidence >= 0.85 && memory.access_count > 2 && !is_expired(&memory) {
    embed_and_index(memory);
}
```

**时间衰减 + 淘汰**
- 写入 → Redis TTL 72h → 若被访问提升到 SurrealDB 向量索引
- 向量索引中 30 天无访问 → 自动剔除

**向量存储演进路径**

| 阶段 | 方案 | 触发条件 |
|------|------|---------|
| MVP | SurrealDB 内置向量 | 默认 |
| 中期 | LanceDB（嵌入式，无独立进程）| 向量 > 50万 或 P99 > 200ms |
| 生产 | Qdrant 自托管 或 LanceDB + S3 | 多节点部署需求 |

---

## 7. Agent Long-term Memory 架构

```
写入条件：confidence >= 0.85
  ↓
Layer 1：SurrealDB（结构化元数据）
  字段：user_id, intent, summary, confidence + links to OntologyObject
  用途：精确查询、权限控制、关联关系、审计

Layer 2：向量索引（SurrealDB 内置 → LanceDB → Qdrant）
  字段：memory_id + embedding
  用途：语义相似检索，few-shot 动态注入

检索流程：向量 ANN → memory_id → SurrealDB 批量取完整内容
```

---

## 8. Function 执行模型（三层）

| 层 | 技术 | 用户 | 优先级 |
|----|------|------|--------|
| Layer 1 | Rust 编译时注册（`#[ontology_function]` 宏）| 平台开发者 | P0 |
| Layer 2 | CEL 表达式 + Monaco Web IDE（Schema 感知补全）| 业务分析师 | P1 |
| Layer 3 | WASM 沙箱 | 第三方扩展 | 接口占坑，暂不实现 |

**自然语言路径：** 业务描述 → LLM（注入 Schema）→ 生成 CEL → 用户确认 → 保存为 Logic
**CEL 前端：** Monaco Editor + CEL language def（~200行）+ Schema 感知 CompletionItemProvider（~1-2天）

---

## 9. Agent 流式协议演进（ADR-17）

| 阶段 | 协议 | 触发条件 |
|------|------|---------|
| 现阶段 | **SSE**（单向，Server → Client）| 默认，文本流式输出 |
| 中期 | **WebSocket**（双向全双工）| Proactive Agent 主动推送 / 用户打断生成 |
| 未来 | **WebRTC**（P2P 音视频）| 语音输入输出功能上线时 |

**"停止生成"不依赖 WebSocket**，独立 HTTP 接口 + 后端 `CancellationToken` 即可：
```
POST /v1/query          → 发起查询，返回 SSE stream
POST /v1/query/{id}/cancel → 取消，后端 token.cancel()
```

**SSE 和 WebSocket 可并存**：agent-svc 同时暴露两个端点，客户端按能力选择，互不干扰。

---

## 10. 系统闭环

```
外部数据 → ingest-svc → ontology-svc（写 OntologyEvent）
                              ↓ Event Bus
                   ┌──────────┴───────────┐
              workflow-svc            agent-svc
              （触发 Action）        （Proactive 预计算）
                    ↓
              function-svc（Logic 只读推导）
                    ↓
              ontology-svc（写回，闭环）
```

---

## 11. Workflow 触发器

```
TriggerManager
  ├── CronScheduler   → TriggerEvent（定时，全量扫描）
  └── EventListener   → TriggerEvent（实时，单对象上下文）
          ↓
  WorkflowEngine（统一执行，DAG 并发）
          ↓
  Saga 补偿（on_failure → 补偿 Function）
```

**幂等保障：** 同一 `object_id` 设 Redis TTL 冷却窗口，防短时间重复触发。
**有状态聚合：** 不引入 Flink，通过触发 Logic 查 SurrealDB 实现（Ontology 即状态存储）。

---

## 12. 权限评估流程

```
POST /v1/authorize
  1. RBAC：subject Role 是否包含 (action, resource_type)？
  2. ABAC：resource 属性是否满足 Policy 条件？
  3. ReBAC：subject 与 resource 图关系是否满足规则？
  → Allow / Deny / AllowWithMask
```

**热路径目标：** P99 < 5ms（结果缓存 Redis，短 TTL）

---

## 13. 合规架构（分级实现）

| 优先级 | 内容 |
|--------|------|
| P0 | 数据分类（TBox 字段打标签：Public / Internal / Confidential / PII）|
| P0 | 全链路访问审计（who + what + when + IP，写 WORM）|
| P1 | 不可篡改审计链（哈希链）+ WORM 存储 |
| P1 | Crypto-Shredding（删 DEK 而非数据，实现被遗忘权）|
| P2 | 字段级加密（PII 写入前加密，Vault/KMS 管密钥）|
| P2 | 保留策略引擎（EntityType 绑定保留期，workflow-svc 定时执行）|

---

## 14. ADR 决策速览

| ADR | 问题 | 决策 | 状态 |
|-----|------|------|------|
| 1 | CQRS | 不做；OntologyReader trait 留逃生门 | ✅ |
| 2 | Function 执行模型 | Rust / CEL / 自然语言三层 | ✅ |
| 3 | BFF 边界 | Gateway 只路由+JWT，聚合在 Function | ✅ |
| 4 | 多租户 | 暂缓，待商量 | ⏸ |
| 5 | 离线同步 | CRDT 内嵌 ontology-svc /v1/sync，palantir-sync-client 独立库 | ✅ |
| 6 | Agent Memory 存储 | SurrealDB 结构化 + 向量按阶段演进，MemoryStore trait 抽象 | ✅ |
| 7 | Ontology 存储 | SurrealDB（文档+图+向量），TiKV 扩容路径 | ✅ |
| 8 | 文件存储 | RustFS 从 P0 起用（用户上传场景本地 FS 无法多实例共享），object_store crate 抽象 | ✅ |
| 9 | 合规架构 | 数据分类 → WORM → Crypto-Shredding → 字段加密，分 P0/P1/P2 | ✅ |
| 10 | Event Bus 选型 | InProcessBus → Fluvio（Rust）/ NATS，Kafka 备选 | ✅ |
| 11 | Workflow 触发器 | Cron + EventListener 统一 TriggerManager，共用 WorkflowEngine | ✅ |
| 12 | EventListener 复杂度 | tokio async 无状态过滤，有状态聚合 via Logic + SurrealDB | ✅ |
| 13 | 向量成本控制 | 本地 ONNX embedding + 分层检索 + 选择性 embedding，Qdrant 按需引入 | ✅ |
| 16 | 前端选型 | React + TypeScript + Vite；前后端分离；utoipa 生成 OpenAPI → openapi-typescript 生成类型 | ✅ |
| 17 | Agent 流式协议 | 现阶段 SSE；中期升级 WebSocket（Proactive Agent / 打断）；WebRTC 仅语音功能时引入 | ✅ |
| 14 | 计算分层模型 | 四层：L1 Redis / L2 本地内存（DashMap+Arrow+petgraph）/ L3 SurrealDB / L4 专项服务 | ✅ |
| 18 | L2 计算引擎 | Apache Arrow RecordBatch + DataFusion（列式分析/CEL）；Arrow IPC 序列化到 Redis | ✅ |
| 19 | Embedding 服务 | 独立 embedding-svc（fastembed-rs + BGE-small-zh）；多服务共享，HTTP 调用 | ✅ |
| 15 | 事件序列粒度 | 按 EntityType 独立序列；NATS Subject 层级实现，一个 Stream；SurrealDB 复合主键 | ✅ |

---

## 15. 四层计算模型（ADR-14 更新）

```
L1：集中内存（Redis）          延迟 < 1ms    跨服务共享，有 TTL
L2：本地内存（进程内）          延迟 μs 级    服务私有，三种结构
L3：磁盘持久化（SurrealDB）     延迟 ms 级    全量数据，L2 重建来源
L4：专项计算服务（独立进程）     延迟 ms~s 级  embedding / LLM
```

**各层存放内容**

| 层 | 技术 | 存放内容 |
|----|------|---------|
| L1 | Redis | Semantic Cache、ReBAC 授权结果、Workflow 冷却窗口、Arrow IPC 快照、Agent Memory 热数据（TTL 72h）、分布式锁 |
| L2 | DashMap + Arrow + petgraph | DashMap：点查询 by ID；Arrow RecordBatch + DataFusion：列式分析/CEL；petgraph：图遍历 |
| L3 | SurrealDB + RustFS | 全量 Ontology（历史版本）、事件日志、冷查询、文件存储 |
| L4 | embedding-svc + agent-svc | 向量化（fastembed-rs + BGE-small-zh）、LLM 推理 |

**L2 三种结构各司其职**

```
DashMap<OntologyId, OntologyObject>   → 点查询，O(1)
Arrow RecordBatch per EntityType      → 分析查询，DataFusion SQL / CEL 编译执行
petgraph                              → 图遍历，BFS / DFS / 最短路径
```

OntologyEvent 到来时三者同步更新：
```rust
fn handle_upsert(object: OntologyObject) {
    self.map.insert(object.id.clone(), object.clone());          // DashMap
    self.batches.entry(&object.entity_type).upsert_row(&object); // Arrow
    self.graph.update_node(&object);                             // petgraph
}
```

**Arrow ↔ Redis 配合**

```
服务启动（L2 空）：
  Redis 取 Arrow IPC 快照 → 反序列化 → 暖 L2（毫秒级）
  Redis miss → SurrealDB 重建 → 写回 Redis

DataFusion 查询结果：
  RecordBatch → Arrow IPC → Redis（key = hash(entity_type+query+version), TTL 短）
  下次相同查询 → Redis 命中，跳过 DataFusion

OntologyEvent 到来：
  L2 RecordBatch patch + Redis 失效相关 key（同步）
```

**查询路由**

```
请求进来
  ↓
L1 Redis 命中？→ 返回（< 1ms）
  ↓ miss
L2 本地内存
  ├── 点查询   → DashMap（μs）
  ├── 分析查询 → Arrow + DataFusion（μs~ms）
  └── 图遍历   → petgraph（μs）
  ↓ miss / 冷数据
L3 SurrealDB（ms），结果暖 L1 + L2
  ↓ 需要向量化 / LLM
L4 embedding-svc / agent-svc（ms~s）
```

**一致性机制**：写路径只走 L3，OntologyEvent 驱动 L2 patch + L1 失效，L4 无状态。

---

## 16. 事件序列粒度（ADR-15）

**决策**：事件序列按 EntityType 独立，通过 NATS Subject 层级实现，不创建多个 Stream。

**NATS Subject 规范**

```
Stream 名：ontology-events（唯一）
Subject：  ontology.events.{entity_type}.{op}

示例：
  ontology.events.Employee.upsert
  ontology.events.Contract.upsert
  ontology.events.Contract.delete
  ontology.events.Transaction.link
```

消费者按需订阅，cursor 天然 per-EntityType：

```rust
// workflow-svc 只关心 Contract
subscriber.subscribe("ontology.events.Contract.>", handler).await?;
// agent-svc 全量
subscriber.subscribe("ontology.events.>", handler).await?;
```

**SurrealDB 事件表：复合主键**

```sql
-- (entity_type, seq) 复合唯一键
-- seq 按 entity_type 独立递增
INSERT INTO ontology_event {
    entity_type: "Contract",
    seq: sequence::next("seq_Contract"),
    payload: ...,
};
```

**消费者 Cursor 结构**

```rust
// 从单值改为 Map
pub struct ConsumerCursor(pub BTreeMap<String, u64>);
// { "Employee": 42, "Contract": 17, "Transaction": 156 }
```

**OntologyObject.version 不变**：已经是 per-object 级别，本身正确。

---

## 17. 本地开发启动

**不使用 docker-compose**。Rust 跨平台，栈内所有依赖均为单二进制，直接跑进程即可。

```bash
cargo xtask dev   # 并发启动全部依赖 + services
cargo xtask stop  # 全部停止
```

`xtask` 内部启动顺序：
1. `surreal start` — SurrealDB
2. `nats-server -js` — NATS JetStream
3. `redis-server` — Redis
4. `rustfs server` — RustFS
5. 等依赖健康检查通过
6. `cargo run -p ontology-svc / ingest-svc / ...`

**Docker 使用场景（降级为可选）：**
- CI/CD 环境隔离（GitHub Actions）
- 生产 Kubernetes 需要镜像
- Windows 上运行 Redis（无官方原生版）

---

## 18. 实现顺序

**P0 — 基础骨架**
- `palantir-event-bus`（InProcessBus）
- `ontology-svc`：HTTP API + SurrealDB + 事件发布
- `ingest-svc`：Source/Mapping + 摄入调度

**P1 — 计算与 Agent**
- `palantir-function-core` + `function-svc`（Rust Layer + CEL Layer）
- `palantir-agent` 重构（planner + executor + semantic cache）
- `agent-svc`：/v1/query 流式 API

**P2 — 流程与权限**
- `workflow-svc`：Workflow + 事件触发 + Saga
- `palantir-auth-core` + `auth-svc`

---

## 19. 逃生门汇总（关键 trait 抽象）

| trait | 今天实现 | 未来替换 |
|-------|---------|---------|
| `OntologyObjectStore` | SurrealDB | Postgres / TiDB |
| `OntologyGraphStore` | SurrealDB | Neo4j / TigerGraph |
| `OntologyReader` | SurrealDB | 只读副本 / 图数据库 |
| `EventPublisher/Subscriber` | InProcessBus | Fluvio / NATS / Kafka |
| `MemoryStore` | SurrealDB 内置向量 | LanceDB / Qdrant |
| `PolicyEvaluator` | RBAC 简单实现 | OPA / Cedar |
| `ObjectStore`（文件）| RustFS | MinIO / S3 / 云 OSS |
