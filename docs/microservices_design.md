# Palantir 微服务架构设计

> 版本：v1.0 | 日期：2026-03-19

---

## 一、整体原则

| 原则 | 说明 |
|------|------|
| 领域驱动边界 | 每个服务对应一个有界上下文，不跨域直接访问数据库 |
| 事件驱动骨干 | 写操作产生 OntologyEvent → Event Bus → 订阅方异步消费 |
| 同步调用极简 | 跨服务同步调用仅用于"需要立即返回结果"的场景（Agent 调 Function） |
| Library + Service 分离 | 核心逻辑放 `crates/`（可测试），服务壳放 `services/`（薄封装） |
| Auth Sidecar | 权限校验通过中间件注入，业务代码不感知 |

---

## 二、Workspace 结构

```
palantir/
├── Cargo.toml                    # workspace root
│
├── crates/                       # 库 crate（无 main.rs，可单元测试）
│   ├── palantir-ontology-manager/  # 现有：adapter/mapping/manager/model
│   ├── palantir-domain/            # 现有：领域模型（finance/flight/order...）
│   ├── palantir-persistence/       # 现有：SQLite 持久化
│   ├── palantir-pipeline/          # 现有：saga/transform 原语
│   ├── palantir-agent/             # 现有 → 重构为 AI Agent 核心库
│   ├── palantir-event-bus/         # NEW：Event Bus trait + NATS/内存实现
│   ├── palantir-function-core/     # NEW：Function/Logic trait + 注册表
│   └── palantir-auth-core/         # NEW：Policy 类型 + 评估器 trait
│
└── services/                     # 可部署服务（有 main.rs，薄封装）
    ├── ontology-svc/               # Ontology CRUD + 事件流
    ├── ingest-svc/                 # 数据摄入（重构自 palantir-ingest-api）
    ├── function-svc/               # Function/Logic 注册与执行
    ├── agent-svc/                  # AI Agent 推理入口
    ├── workflow-svc/               # Workflow 编排 + Saga
    └── auth-svc/                   # RBAC + ABAC + ReBAC 权限服务
```

---

## 三、Event Bus 设计（`palantir-event-bus`）

所有服务间异步通信的基础设施抽象。

### 核心 trait

```rust
// crates/palantir-event-bus/src/lib.rs

#[async_trait]
pub trait EventPublisher: Send + Sync {
    async fn publish(&self, topic: &str, event: &OntologyEvent) -> Result<()>;
}

#[async_trait]
pub trait EventSubscriber: Send + Sync {
    async fn subscribe(
        &self,
        topic: &str,
        handler: Arc<dyn Fn(OntologyEvent) -> BoxFuture<'static, ()> + Send + Sync>,
    ) -> Result<SubscriptionHandle>;
}
```

### 实现

| 实现 | 用途 |
|------|------|
| `InProcessBus` | 单进程测试/开发 |
| `NatsBus` | 生产环境（NATS JetStream） |
| `KafkaBus` | 大数据量场景 |

### Topic 规范

```
ontology.events.upsert     # OntologyEvent::Upsert
ontology.events.delete     # OntologyEvent::Delete
ontology.events.link       # OntologyEvent::Link
ingest.jobs.created        # 新摄入任务
workflow.triggers          # Workflow 触发信号
agent.feedback             # 用户反馈写回
```

---

## 四、服务详细设计

---

### 4.1 `ontology-svc` — Ontology 核心服务

**定位：** 整个系统的 Single Source of Truth，不依赖任何业务服务。

#### API 契约

```
# Schema（TBox）管理
GET    /v1/schema                          → 获取完整 Schema
PUT    /v1/schema/entities/{type}          → 注册/更新 EntitySchema
DELETE /v1/schema/entities/{type}          → 删除 EntitySchema

# 对象（ABox）操作
POST   /v1/objects                         → Upsert OntologyObject
DELETE /v1/objects/{id}                    → Delete
GET    /v1/objects/{id}                    → 按 ID 查询
POST   /v1/objects/query                   → 条件查询（filter + pagination）
GET    /v1/objects/{id}/history            → bi-temporal 历史

# 关系操作
POST   /v1/links                           → 创建 Link
GET    /v1/objects/{id}/links              → 查询对象的所有关联
DELETE /v1/links/{from}/{to}/{rel}         → 删除 Link

# 事件流
GET    /v1/events/stream                   → SSE 实时事件流
GET    /v1/events?since={cursor}           → 拉取历史事件
```

#### 内部模块结构

```
services/ontology-svc/src/
├── main.rs           # axum 路由装配
├── routes/
│   ├── objects.rs    # CRUD handlers
│   ├── schema.rs     # Schema handlers
│   ├── links.rs      # Link handlers
│   └── events.rs     # SSE stream handler
├── store/
│   ├── sqlite.rs     # SQLite 实现（基于 palantir-persistence）
│   └── pg.rs         # PostgreSQL 实现（生产）
└── publisher.rs      # 写入成功后发布 OntologyEvent
```

#### 关键依赖

```toml
[dependencies]
palantir-ontology-manager = { path = "../../crates/palantir-ontology-manager" }
palantir-persistence      = { path = "../../crates/palantir-persistence" }
palantir-event-bus        = { path = "../../crates/palantir-event-bus" }
axum                      = "0.8"
tokio                     = { version = "1", features = ["full"] }
sqlx                      = { version = "0.8", features = ["sqlite", "postgres", "runtime-tokio"] }
serde_json                = "1"
tokio-stream              = "0.1"   # SSE
```

#### 核心数据流

```
HTTP POST /objects
    → deserialize OntologyObject
    → store.upsert()          # 写 SQLite/PG
    → publisher.publish()     # 发 ontology.events.upsert
    → 200 OK
```

---

### 4.2 `ingest-svc` — 数据摄入服务

**定位：** 负责从外部数据源拉取数据，经 Mapping 转换后写入 `ontology-svc`。

#### API 契约

```
# Source 管理
POST   /v1/sources                         → 注册 SourceAdapter 配置
GET    /v1/sources                         → 列出所有 Source
GET    /v1/sources/{id}                    → 查询 Source 状态（cursor、上次运行时间）
DELETE /v1/sources/{id}                    → 删除

# Mapping 管理
POST   /v1/mappings                        → 注册 TOML Mapping
GET    /v1/mappings/{source_id}            → 查询 Source 对应的 Mapping

# 任务控制
POST   /v1/sources/{id}/run               → 立即触发一次摄入
GET    /v1/runs                            → 列出运行历史
GET    /v1/runs/{run_id}                   → 查询运行详情（进度、错误）
```

#### 内部模块结构

```
services/ingest-svc/src/
├── main.rs
├── routes/
│   ├── sources.rs
│   ├── mappings.rs
│   └── runs.rs
├── runner/
│   ├── scheduler.rs    # 定时/事件触发
│   ├── worker.rs       # OntologyManager.run() 封装
│   └── cursor_store.rs # 游标持久化（断点续传）
└── client/
    └── ontology.rs     # HTTP client → ontology-svc
```

#### 关键依赖

```toml
[dependencies]
palantir-ontology-manager = { path = "../../crates/palantir-ontology-manager" }
palantir-event-bus        = { path = "../../crates/palantir-event-bus" }
reqwest                   = { version = "0.12", features = ["json"] }
tokio-cron-scheduler      = "0.13"   # 定时触发
```

#### Source 配置存储（持久化）

```rust
struct SourceConfig {
    id:           String,
    kind:         SourceKind,    // Csv | Postgres | Rest | Kafka
    connection:   serde_json::Value,
    mapping_toml: String,
    schedule:     Option<String>, // cron 表达式
    cursor:       Option<serde_json::Value>,
    last_run_at:  Option<OffsetDateTime>,
}
```

---

### 4.3 `function-svc` — 计算/查询服务

**定位：** Logic（单对象推导）和 Function（参数化图计算）的注册与执行引擎。Agent 的主要工具调用对象。

#### API 契约

```
# Function 注册
POST   /v1/functions                       → 注册 Function 定义
GET    /v1/functions                       → 列出所有 Function
GET    /v1/functions/{name}               → 查询 Function 签名

# Function 执行
POST   /v1/functions/{name}/execute       → 执行（传入参数，返回 Value）

# Logic 注册
POST   /v1/logics                          → 注册 Logic（绑定到 EntityType）
GET    /v1/logics/{entity_type}           → 列出某类型的所有 Logic

# Logic 求值
GET    /v1/objects/{id}/logics/{name}     → 对指定对象求值 Logic
```

#### `palantir-function-core` 库核心 trait

```rust
// crates/palantir-function-core/src/lib.rs

/// Logic：绑定到单个对象，无参数，纯推导
#[async_trait]
pub trait Logic: Send + Sync {
    fn name(&self) -> &str;
    fn entity_type(&self) -> &str;
    async fn evaluate(
        &self,
        object: &OntologyObject,
        reader: &dyn OntologyReader,
    ) -> Result<Value, FunctionError>;
}

/// Function：参数化计算，可跨对象图遍历
#[async_trait]
pub trait Function: Send + Sync {
    fn name(&self) -> &str;
    fn signature(&self) -> &FunctionSignature;  // 参数类型声明
    async fn execute(
        &self,
        args: &BTreeMap<String, Value>,
        reader: &dyn OntologyReader,
    ) -> Result<Value, FunctionError>;
}

/// Registry：注册/查找
pub struct FunctionRegistry {
    functions: BTreeMap<String, Arc<dyn Function>>,
    logics:    BTreeMap<String, Arc<dyn Logic>>,
}
```

#### 内部模块结构

```
services/function-svc/src/
├── main.rs
├── routes/
│   ├── functions.rs
│   └── logics.rs
├── registry.rs         # FunctionRegistry（内存 + 持久化）
├── executor.rs         # 执行引擎（超时、沙箱、并发限制）
└── client/
    └── ontology.rs     # 查询 ontology-svc 获取对象
```

---

### 4.4 `agent-svc` — AI Agent 服务

**定位：** LLM 推理入口，协调 Function 调用、语义缓存、Multi-Agent、AgentTrace。

#### API 契约

```
# 查询（主入口）
POST   /v1/query                           → Agent 推理（支持流式 SSE）
  Body: { "query": "...", "user_id": "...", "stream": true }
  Response: SSE stream of AgentChunk | AgentResult

# Trace（审计）
GET    /v1/traces                          → 列出 traces（按 user_id/时间过滤）
GET    /v1/traces/{id}                     → 完整 trace 详情

# 反馈
POST   /v1/feedback                        → 提交 UserFeedback
  Body: { "trace_id": "...", "rating": 1, "corrected_output": "..." }

# 缓存状态
GET    /v1/cache/stats                     → 命中率、缓存条目数
DELETE /v1/cache                           → 手动清空（调试用）
```

#### `palantir-agent` 库重构目标

```
crates/palantir-agent/src/
├── lib.rs
├── planner.rs          # Plan-then-Execute：LLM → 步骤列表
├── executor.rs         # 并发执行 Function 调用
├── multi_agent/
│   ├── supervisor.rs   # 路由 + 汇总
│   └── specialist.rs   # 专门领域 Agent
├── reflection.rs       # Critic Agent / Self-Reflection
├── memory/
│   ├── short_term.rs   # 会话上下文
│   ├── long_term.rs    # Ontology AgentMemory 对象
│   └── episodic.rs     # AgentTrace 归档
├── cache/
│   ├── semantic.rs     # embedding ANN 语义缓存
│   └── invalidator.rs  # OntologyEvent 驱动失效
├── safety/
│   ├── hallucination.rs # 对照 Ontology 校验
│   ├── injection.rs     # Prompt Injection 检测
│   └── circuit.rs       # Circuit Breaker 四级降级
├── trace.rs            # AgentTrace 结构 + 写回
└── feedback.rs         # UserFeedback 处理
```

#### 关键数据结构

```rust
// Agent 查询请求
pub struct AgentQuery {
    pub query:   String,
    pub user_id: String,
    pub context: Option<Vec<Message>>,  // 会话历史
    pub stream:  bool,
}

// 推理计划
pub struct AgentPlan {
    pub steps:     Vec<PlanStep>,
    pub reasoning: String,
}

pub enum PlanStep {
    CallFunction { name: String, args: BTreeMap<String, Value> },
    LookupObject { id: OntologyId },
    Synthesize   { prompt: String },
}

// Trace
pub struct AgentTrace {
    pub trace_id:            Uuid,
    pub user_id:             String,
    pub query:               String,
    pub plan:                AgentPlan,
    pub function_calls:      Vec<FunctionCallRecord>,
    pub raw_llm_output:      String,
    pub final_output:        String,
    pub hallucination_flags: Vec<HallucinationFlag>,
    pub confidence:          f32,
    pub latency_ms:          u64,
    pub timestamp:           OffsetDateTime,
}
```

#### 关键依赖

```toml
[dependencies]
palantir-agent        = { path = "../../crates/palantir-agent" }
palantir-event-bus    = { path = "../../crates/palantir-event-bus" }
async-openai          = "0.28"   # LLM API（兼容 Claude/OpenAI 接口）
ort                   = "2.0"    # ONNX Runtime（本地 embedding）
usearch               = "2.0"    # ANN 向量搜索（语义缓存）
redis                 = { version = "0.27", features = ["tokio-comp"] }
tokio-stream          = "0.1"
```

---

### 4.5 `workflow-svc` — 工作流编排服务

**定位：** 基于事件触发或手动触发，按预定义步骤编排 Action，支持 Saga 补偿。

#### API 契约

```
# Workflow 定义
POST   /v1/workflows                       → 注册 Workflow 定义
GET    /v1/workflows                       → 列出所有 Workflow
GET    /v1/workflows/{id}                 → 查询定义详情

# Workflow 触发
POST   /v1/workflows/{id}/trigger         → 手动触发，返回 run_id
GET    /v1/runs                            → 列出运行实例
GET    /v1/runs/{run_id}                  → 查询运行状态 + 步骤详情
POST   /v1/runs/{run_id}/cancel           → 取消运行

# Trigger 规则（OntologyEvent → 自动触发）
POST   /v1/triggers                        → 注册事件触发规则
GET    /v1/triggers                        → 列出触发规则
```

#### Workflow 定义格式（TOML）

```toml
id      = "risk-review-on-contract-upsert"
name    = "合同风险审查"
version = "v1"

[trigger]
type      = "ontology_event"
entity    = "Contract"
event     = "Upsert"
condition = "attrs.amount > 1000000"   # 金额超过百万才触发

[[steps]]
id     = "risk_check"
action = "function:risk.evaluate_contract"
args   = { contract_id = "{{trigger.object.id}}" }

[[steps]]
id      = "notify"
action  = "function:notification.send"
args    = { to = "risk_team", content = "{{steps.risk_check.output}}" }
depends = ["risk_check"]

[compensation]
on_failure = "function:audit.log_failure"
```

#### 内部模块结构

```
services/workflow-svc/src/
├── main.rs
├── routes/
│   ├── workflows.rs
│   ├── runs.rs
│   └── triggers.rs
├── engine/
│   ├── scheduler.rs    # 监听 Event Bus，匹配触发规则
│   ├── runner.rs       # 步骤执行引擎（DAG 并发）
│   ├── saga.rs         # 补偿事务（基于 palantir-pipeline）
│   └── state.rs        # 运行状态持久化
└── client/
    ├── function.rs     # 调用 function-svc
    └── ontology.rs     # 查询 ontology-svc
```

---

### 4.6 `auth-svc` — 权限服务

**定位：** 集中管理 RBAC + ABAC + ReBAC 策略，提供统一的权限评估 API。

#### API 契约

```
# Role 管理（RBAC）
POST   /v1/roles                           → 创建 Role
PUT    /v1/roles/{id}                     → 更新（绑定 permissions）
GET    /v1/roles                           → 列出所有 Role
POST   /v1/users/{id}/roles               → 为用户分配 Role

# Policy 管理（ABAC/ReBAC）
POST   /v1/policies                        → 创建 Policy 规则
GET    /v1/policies                        → 列出规则

# 权限评估（热路径，需 <5ms）
POST   /v1/authorize
  Body:    { "subject": "user:alice", "action": "read", "resource": "object:contract:42" }
  Returns: { "allowed": true, "reason": "role:analyst -> read:Contract" }

# 批量评估（用于前端权限渲染）
POST   /v1/authorize/batch
  Body:    [{ "action": "...", "resource": "..." }, ...]
  Returns: [{ "allowed": true/false }, ...]
```

#### `palantir-auth-core` 库核心类型

```rust
// crates/palantir-auth-core/src/lib.rs

pub struct Permission {
    pub action:      Action,        // Read | Write | Delete | Execute
    pub resource:    ResourceSpec,  // ObjectType | InstanceId | Field
    pub field_policy: FieldPolicy,  // Allow | Deny | Mask(fn)
}

pub enum ResourceSpec {
    AnyOfType(String),                    // 某类型的所有对象
    SpecificObject(OntologyId),           // 某个具体对象
    FieldOf(OntologyId, String),          // 某对象的某字段
}

pub enum FieldPolicy {
    Allow,
    Deny,
    Mask(Arc<dyn Fn(&Value) -> Value + Send + Sync>),
}

/// 统一评估器 trait
#[async_trait]
pub trait PolicyEvaluator: Send + Sync {
    async fn authorize(
        &self,
        subject: &Subject,
        action:  Action,
        resource: &ResourceSpec,
    ) -> AuthDecision;
}

pub enum AuthDecision {
    Allow,
    Deny { reason: String },
    AllowWithMask(FieldPolicy),
}
```

#### 三维评估流程

```
请求到达 authorize API
    │
    ├── 1. RBAC 检查：subject 的 Role 是否包含 (action, resource_type) 权限？
    │       是 → 基础放行，继续检查
    │       否 → Deny（fast path）
    │
    ├── 2. ABAC 检查：resource 的属性是否满足 Policy 条件？
    │       如：Contract.status == "draft" 时禁止 Read
    │
    └── 3. ReBAC 检查：subject 与 resource 的图关系是否满足规则？
            如：User -> OWNS -> Contract → 允许 Write
            查询 ontology-svc 图关系，缓存结果
```

#### 内部模块结构

```
services/auth-svc/src/
├── main.rs
├── routes/
│   ├── authorize.rs    # 评估 API（热路径优化）
│   ├── roles.rs
│   └── policies.rs
├── evaluator/
│   ├── rbac.rs
│   ├── abac.rs
│   └── rebac.rs        # 调用 ontology-svc 查图关系
├── store/
│   └── policy_store.rs # Policy 持久化
└── cache.rs            # 权限结果缓存（TTL 短，ReBAC 图结果）
```

---

## 五、服务间通信总览

```
┌─────────────────────────────────────────────────────────────┐
│                      API Gateway / BFF                       │
└──────┬──────────┬──────────┬──────────┬──────────┬──────────┘
       │          │          │          │          │
       ▼          ▼          ▼          ▼          ▼
  agent-svc  ontology-svc ingest-svc workflow-svc auth-svc
       │          │          │          │
       │    (写后发布)         │    (监听触发)
       │          └──────────┴──→ [Event Bus: NATS/Kafka]
       │                              │
       │                    ┌─────────┼──────────┐
       │                    ▼         ▼          ▼
       │              agent-svc  workflow-svc  ingest-svc
       │              (Proactive) (Auto-trigger) (增量同步)
       │
       └──→ function-svc (同步 HTTP/gRPC，热路径)
                │
                └──→ ontology-svc (只读查询)

所有服务 → auth-svc (每个请求经 middleware 鉴权)
```

### 调用类型分类

| 调用方向 | 类型 | 协议 | 理由 |
|---------|------|------|------|
| agent-svc → function-svc | 同步 | HTTP/gRPC | 需要立即返回结果参与 LLM 推理 |
| ontology-svc → Event Bus | 异步 | NATS Publish | 写操作不阻塞 |
| workflow-svc ← Event Bus | 异步 | NATS Subscribe | 事件触发工作流 |
| agent-svc ← Event Bus | 异步 | NATS Subscribe | ProactiveAgent 预计算 |
| ingest-svc → ontology-svc | 同步 | HTTP | 批量写，需确认成功 |
| 所有服务 → auth-svc | 同步 | HTTP | 需要立即知道是否允许 |

---

## 六、实现顺序与里程碑

### P0 — 基础骨架（可运行端到端摄入）

```
[Week 1] palantir-event-bus crate（InProcess 实现）
[Week 1] ontology-svc：HTTP API + SQLite store + 事件发布
[Week 2] ingest-svc：Source/Mapping 管理 + 触发摄入
[Week 2] 端到端验证：CSV → ingest-svc → ontology-svc → SSE 事件流
```

### P1 — 计算与 Agent（可查询推理）

```
[Week 3] palantir-function-core crate
[Week 3] function-svc：注册 + 执行引擎
[Week 4] palantir-agent 重构（planner + executor + semantic cache）
[Week 4] agent-svc：/v1/query 流式 API
[Week 4] 端到端验证：用户查询 → agent-svc → function-svc → ontology-svc → 流式回答
```

### P2 — 流程与权限（生产就绪）

```
[Week 5] workflow-svc：Workflow 定义 + 事件触发 + Saga
[Week 6] palantir-auth-core crate
[Week 6] auth-svc：RBAC + ABAC + ReBAC 评估
[Week 6] 所有服务接入 auth 中间件
```

---

## 七、本地开发启动（docker-compose 草图）

```yaml
services:
  nats:
    image: nats:latest
    ports: ["4222:4222"]

  ontology-svc:
    build: ./services/ontology-svc
    environment:
      DATABASE_URL: sqlite:///data/ontology.db
      NATS_URL: nats://nats:4222
    ports: ["8001:8000"]

  ingest-svc:
    build: ./services/ingest-svc
    environment:
      ONTOLOGY_URL: http://ontology-svc:8000
      NATS_URL: nats://nats:4222
    ports: ["8002:8000"]

  function-svc:
    build: ./services/function-svc
    environment:
      ONTOLOGY_URL: http://ontology-svc:8000
    ports: ["8003:8000"]

  agent-svc:
    build: ./services/agent-svc
    environment:
      FUNCTION_URL: http://function-svc:8000
      ONTOLOGY_URL: http://ontology-svc:8000
      NATS_URL: nats://nats:4222
      LLM_API_KEY: ${LLM_API_KEY}
    ports: ["8004:8000"]

  workflow-svc:
    build: ./services/workflow-svc
    environment:
      FUNCTION_URL: http://function-svc:8000
      ONTOLOGY_URL: http://ontology-svc:8000
      NATS_URL: nats://nats:4222
    ports: ["8005:8000"]

  auth-svc:
    build: ./services/auth-svc
    environment:
      ONTOLOGY_URL: http://ontology-svc:8000
      DATABASE_URL: sqlite:///data/auth.db
    ports: ["8006:8000"]
```

---

## 八、架构关键决策记录（ADR）

> 记录设计阶段已确认的所有关键决策，供后续实现参考。

---

### ADR-1：不引入 CQRS

**决策：** 不做读写分离，ontology-svc 同时承担写入和查询。

**理由：**
- Ontology 写入（OntologyEvent）和读出（OntologyObject）数据形态一致，无需投影
- 当前数据量不需要独立扩容读路径
- Postgres JSONB + GIN 索引 + links 表可满足 1-3 跳图遍历

**逃生门：** `OntologyReader` 设计为 trait，未来出现查询瓶颈时，可在不改调用方的情况下替换为图数据库实现。

**触发重新评估的信号：** 查询 P99 > 500ms 且加索引无法改善；需要全文搜索；图遍历超过 4 跳且频繁。

---

### ADR-2：Function 三层执行模型

**决策：** 按用户技术能力分三层，逐层降低门槛。

| 层 | 技术 | 用户 | 状态 |
|----|------|------|------|
| Layer 1 | Rust 编译时注册 | 平台开发者 | P0 实现 |
| Layer 2 | CEL 表达式 + Web IDE | 业务分析师 | P1 实现 |
| Layer 3 | WASM 沙箱 | 第三方扩展 | 接口占坑，暂不实现 |

**自然语言路径：** 业务用户描述意图 → LLM 注入 Ontology Schema 生成 CEL → 用户确认 → 保存为 Logic。自然语言是输入，结构化定义是输出，用户始终确认结构化结果。

**核心前提：** build.rs 从 ontology-svc 拉取 Schema → 自动生成强类型 Rust 代码（类似 prost codegen）。

---

### ADR-3：BFF 薄到极致，聚合下沉到 Function

**决策：** `palantir-ingest-api` 演化为 API Gateway，只做路由 + JWT 解析 + SSE 转发，不含业务聚合逻辑。

**聚合方式：** 视图聚合逻辑注册为 Function（如 `contract_detail`），前端和 Agent 调用同一个 Function，Gateway 透传请求。

**两阶段演进：**
- 阶段一（模块化单体）：Gateway 调用 in-process 模块
- 阶段二（微服务）：Gateway 将请求转发到对应服务，前端零感知

---

### ADR-4：多租户策略

**状态：** 暂缓，待与同事商量后决定。

---

### ADR-5：离线同步 / CRDT 内嵌 ontology-svc

**决策：** CRDT 冲突解决作为 ontology-svc 内部的 `sync` 模块，不拆独立服务。

**理由：**
- Three-Way Merge（读 LCA → 合并 → 写入）必须是原子操作，跨服务无法保证
- LCA 查询依赖 events 表，天然在 ontology-svc 内
- 类比 CouchDB：同步协议内嵌在数据库服务，不独立

**边界划分：**
- 服务端：ontology-svc 新增 `/v1/sync` 端点 + sync 模块（LCA 查找、Three-Way Merge、冲突标记）
- 客户端：`palantir-sync-client` 独立库（本地 WAL、离线队列、delta 发送）

**与 Raft 的关系：** 不竞争。Raft 解决服务端集群高可用（未来需要时引入），CRDT 解决客户端离线合并。可以叠加使用。

---

### ADR-6：Agent Long-term Memory 分层存储

**决策：** 结构化元数据和向量索引分两层存储，按访问模式匹配最优存储。

```
Layer 1：结构化元数据 → ontology-svc（Postgres）
  字段：user_id, intent, summary, confidence, created_at
        + links to OntologyObject（关联到具体业务对象）
  用途：精确查询、权限控制、关联关系、审计

Layer 2：向量索引 → Qdrant
  字段：memory_id（指向 Layer 1）+ embedding
  用途：语义相似检索，few-shot 动态注入

检索流程：Qdrant ANN → memory_id 列表 → ontology-svc 批量取完整内容
```

**写回路径：**
1. confidence >= 0.85 → 触发持久化（低置信度只写 Redis 短期缓存）
2. agent-svc → ontology-svc 写 AgentMemory 对象 → 返回 memory_id
3. agent-svc → Qdrant 写 embedding（key = memory_id）
4. 两步写入无需分布式事务（Qdrant 是纯索引，丢失可重建）

**扩容路径：**
- 阶段一（开发）：SQLite + in-process usearch
- 阶段二（生产）：Postgres + Qdrant（docker-compose 加一行）
- 阶段三（大规模）：Postgres Citus + Qdrant 多节点分片

**接口抽象：**
```
trait MemoryStore {
    save(memory: AgentMemory)
    find_similar(embedding, limit) → Vec<AgentMemory>
    find_by_user(user_id) → Vec<AgentMemory>
    find_by_object(object_id) → Vec<AgentMemory>
}
```
每个阶段只换实现，调用方不感知。

---

### 决策总览

| ADR | 问题 | 决策 | 状态 |
|-----|------|------|------|
| ADR-1 | CQRS | 不做，trait 留逃生门 | ✅ 确认 |
| ADR-2 | Function 执行模型 | Rust / CEL / 自然语言三层 | ✅ 确认 |
| ADR-3 | BFF 边界 | Gateway 薄到极致，聚合在 Function | ✅ 确认 |
| ADR-4 | 多租户 | 暂缓 | ⏸ 待定 |
| ADR-5 | 离线同步 | CRDT 内嵌 ontology-svc | ✅ 确认 |
| ADR-6 | Agent Long-term Memory | Postgres + Qdrant 分层 | ✅ 确认 |

---

### ADR-7：Ontology 存储选型 — SurrealDB

**决策：** 使用 SurrealDB 作为 Ontology（TBox + ABox）的主存储。

**理由：**
- 原生图遍历语法（RELATE + 箭头语法），不需要应用层拼 JOIN
- 灵活 Schema，直接对应 `attrs: BTreeMap<AttrId, Value>`，无需提前定义列
- Rust SDK 原生支持（`surrealdb` crate）
- 水平扩容路径清晰：本地 RocksDB → 生产 TiKV（分布式，经过大规模验证）
- 单二进制，docker-compose 一行启动

**存储模型：**
- TBox：`entity_schema` 表，存 EntityType 定义
- ABox Objects：每个 EntityType 一个 Table，id 格式 `{type}:{uuid}`
- ABox Links：`RELATE from -> rel -> to SET attrs`，边可带属性
- Events：`event` 表，append-only，seq 严格递增作为 cursor

**扩容路径：**
- 开发：SurrealDB + 内存模式（单二进制）
- 生产：SurrealDB + RocksDB
- 大规模：SurrealDB + TiKV（水平分片）

**风险对冲：**
- `OntologyRepository` 保持 trait 抽象
- 同时维护 `SqliteRepository`（开发备用）
- 若 SurrealDB 出问题，切 Postgres + pgvector，调用方不感知

**否决的方案：**
- MongoDB：图遍历要应用层实现，复杂度转移
- Cassandra：适合 events 表，不适合 objects + links 查询
- Neo4j：技术匹配但水平扩容贵且复杂
- 纯 Postgres：扩容需要 Citus 等方案，用户不想承担运维复杂度

---

### ADR-6 补充：向量存储演进路径（更新）

**原决策：** Postgres + Qdrant 分层存储。

**更新后：** 结合 ADR-7（SurrealDB），向量存储按阶段演进：

| 阶段 | 向量存储 | 理由 |
|------|---------|------|
| MVP | SurrealDB 内置向量搜索 | 已有 SurrealDB，零额外部署，百万级以内可用 |
| 生产（>50万向量） | LanceDB（嵌入式）或 Qdrant | LanceDB 无需独立服务；Qdrant 成熟度更高 |
| 大规模 | Qdrant 多节点分片 | 专业向量数据库，水平扩容完善 |

**`MemoryStore` trait 保持不变，各阶段只换实现。**

---

### ADR-8：文件存储选型 — RustFS

**决策：** 用户上传的异构文件使用 RustFS 存储，`object_store` crate 统一抽象。

**文件存储层：**
- 原始文件（Blob）→ RustFS（S3-compatible，Rust 实现）
- 文件元数据 → SurrealDB（file_upload 对象，关联 ingest job）

**接入方式：** `object_store` crate 的 AmazonS3 接口，S3-compatible 后端透明切换。

```
开发：LocalFileSystem（零依赖）
生产：RustFS（单二进制，Rust，无 GC）
备选：MinIO（更保守）/ 云 S3（直接上云）
```

**文件类型解析（SourceAdapter 扩展）：**

| 格式 | crate | 状态 |
|------|-------|------|
| CSV/TSV | `csv` | 已有 |
| Excel | `calamine` | 新增 |
| JSON/JSONL | `serde_json` | 已有 |
| Parquet | `parquet`（arrow） | 新增 |
| PDF | `pdf-extract` | 新增，复杂表格走 LLM |
| 图片 | `kamadak-exif` | 新增，元数据 |

**设计原则：** 文件类型决定如何读，Mapping TOML 决定如何映射，两者解耦，同一 Mapping 可复用于不同格式。

---

## 九、完整基础设施选型汇总

| 层 | 选型 | 语言 | 理由 |
|----|------|------|------|
| Ontology 存储 | SurrealDB | Rust | 多模型：文档+图+向量，TiKV 扩容 |
| 文件存储 | RustFS | Rust | S3-compatible，无 GC，object_store 抽象 |
| 向量搜索（MVP） | SurrealDB 内置 | Rust | 零额外部署，百万级以内够用 |
| 向量搜索（生产） | LanceDB / Qdrant | Rust | MemoryStore trait 切换 |
| 缓存 | Redis | C | 短期缓存、Semantic Cache |
| 事件总线 | NATS JetStream | Go | 低延迟，单二进制，唯一非 Rust |
| 嵌入模型 | ONNX Runtime（ort） | — | 本地推理，无外部依赖 |

---

### ADR-9：合规架构设计

**核心诉求：** GDPR/PIPL（被遗忘权）、SOX/金融监管（不可篡改审计）、SOC 2（访问控制+加密）

#### 六个架构级决策

**1. 不可篡改审计日志**
- 双写：SurrealDB（业务）+ WORM 存储（合规）
- WORM 选项：RustFS Object Lock / NATS JetStream 不可删 retention / S3 Object Lock
- 哈希链：`event[n].hash = sha256(payload + event[n-1].hash)`，篡改可检测

**2. 数据分类（TBox 层打标签）**
- 四级：Public / Internal / Confidential / PII
- 标签定义在 EntitySchema 的每个字段上
- 驱动加密、访问控制、审计、保留策略

**3. 字段级加密**
- PII/Confidential 字段写入前加密，存储密文
- 密钥管理：HashiCorp Vault 或自建 KMS
- 读取时按权限解密或返回 [MASKED]

**4. Crypto-Shredding（被遗忘权实现）**
- 每个 PII 对象使用独立 DEK（Data Encryption Key）加密
- 用户请求被遗忘 → 只删除 DEK → 数据永久无法解密
- 审计链路不断裂，合规层面视为已删除

**5. 数据保留策略引擎**
- EntityType 绑定保留期限（Transaction: 7年 / Employee: 3年 / AgentTrace: 2年）
- workflow-svc 定时任务执行：PII → Crypto-Shred，非PII → 物理删除/匿名化
- 保留执行本身写入审计日志

**6. 全链路访问审计**
- 所有读写操作记录：who + what + when + IP + result
- 与 AgentTrace 区分：AgentTrace 记录 AI 推理，访问审计记录所有 API 访问
- 写入 WORM 存储

#### 实现优先级
| 优先级 | 内容 |
|--------|------|
| P0 | 数据分类标签（其他所有合规的基础） |
| P0 | 访问审计日志 |
| P1 | 不可篡改审计链 + WORM |
| P1 | Crypto-Shredding |
| P2 | 字段级加密 |
| P2 | 保留策略引擎 |

**新增基础设施：** KMS / HashiCorp Vault（密钥管理）

---

### ADR-7 补充：SurrealDB vs TiDB 对比（更新）

**TiDB 的核心优势：**
- 8年+ 大规模生产验证，成熟度远超 SurrealDB
- TiFlash 列存引擎：HTAP，实时分析不影响写入性能
- MySQL 兼容，已有团队基础时迁移成本低

**SurrealDB 的核心优势：**
- 原生图遍历语法（Ontology 的核心需求）
- 多模型合一（文档 + 图 + 向量），运维简单
- Rust 原生 SDK

**决策框架：**

| 场景 | 推荐 |
|------|------|
| 全新项目，核心是图遍历 + Agent | SurrealDB（保持 ADR-7） |
| 团队已有 TiDB 基础 | TiDB + Qdrant，图查询应用层封装 |
| 需要大规模实时分析（TiFlash） | TiDB |

**`OntologyRepository` trait 拆分（新增）：**
- `OntologyObjectStore` — 对象 CRUD、属性过滤
- `OntologyGraphStore` — 图遍历、关系查询

今天 SurrealDB 同时实现两个 trait；未来图成为瓶颈时，可独立替换 `OntologyGraphStore` 为专用图集群，调用方零感知。

---

### ADR-10：Event Bus 分阶段选型

**分阶段策略：**

| 阶段 | 实现 | 理由 |
|------|------|------|
| 模块化单体（现在） | `InProcessBus`（tokio broadcast） | 零依赖，单进程内收发，开发体验最好 |
| 微服务拆分 | Fluvio（Rust 全栈）| 与技术栈一致，单二进制，生产可用 |
| 备选 | NATS JetStream | 更成熟，生产案例更多，Go 实现 |
| 未来极致性能 | Iggy（QUIC 传输）| 延迟更低，等生产案例成熟再用 |

**保障：** `EventPublisher` / `EventSubscriber` trait 已抽象，换实现不改业务代码。

**否决：** Kafka（运维重，量级不匹配）、RabbitMQ（Erlang，非 Rust 生态）

---

### ADR-11：Workflow 触发器设计

**决策：** 定时触发和事件触发统一由 `workflow-svc` 内的 `TriggerManager` 处理，共用同一个 `WorkflowEngine`。

**统一抽象：**
```
TriggerManager
  ├── CronScheduler     → TriggerEvent（定时，无具体对象上下文）
  └── EventListener     → TriggerEvent（事件，携带 object_id）
          ↓
  WorkflowEngine（统一执行）
```

**同一 Workflow 可绑定两种触发：**
- 定时：每月全量扫描
- 事件：实时单对象触发
- 两者互补，执行逻辑共用

**幂等性：** 事件触发对同一 `object_id` 设冷却窗口（Redis TTL），防止短时间重复触发。

---

### ADR-12：EventListener 不引入流处理引擎

**决策：** EventListener 使用 tokio async 实现无状态过滤，不引入 Flink 等流处理引擎。

**理由：**
- 绝大多数触发条件是单事件无状态过滤，tokio async 完全够用
- 有状态聚合（"员工本月累计消费"）通过触发 Logic 查 SurrealDB 实现，Ontology 本身就是状态存储
- 企业级场景事件频率不到每秒万级，不需要分布式流处理

**引入流处理引擎的信号（任一出现再评估）：**
- 事件频率超过每秒万级
- 需要跨流 CEP（复杂事件模式检测）
- Logic 查询 SurrealDB 成为性能瓶颈

**届时的 Rust 原生选项：** Arroyo（Rust，Flink-like）；保守选择：Apache Flink（JVM，最成熟）
