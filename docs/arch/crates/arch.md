# 共享库 Crates 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 原则

- 核心逻辑在 `crates/`（可测试，无 HTTP 依赖）
- 服务壳在 `services/`（薄封装，只做 HTTP 绑定）
- crate 之间单向依赖，不允许循环

---

## 现有 Crates

### palantir-ontology-manager

```
职责：SourceAdapter trait、CsvAdapter、TomlMapping、OntologyEvent、OntologyManager
核心类型：
  - SourceAdapter：stream(cursor) → BoxStream<CanonicalRecord>
  - OntologyEvent：Upsert / Link / Delete
  - OntologyObject：{ id, entity_type, attrs, time: TimeBounds, provenance }
  - TomlMapping：apply(record, schema) → Vec<OntologyEvent>
被使用：ontology-svc、ingest-svc
```

### palantir-persistence

```
职责：SQLite 持久化层（sqlx）
被使用：ontology-svc（备用实现）、其他服务状态持久化
```

### palantir-pipeline

```
职责：Saga / transform 原语（Filter / Join / Aggregate / Sort / Select）
被使用：workflow-svc
```

### palantir-agent

```
职责：AI Agent 核心逻辑（重构中）
  - planner：将用户意图转为执行计划
  - executor：并发调用 Function
  - semantic cache：向量相似度缓存
被使用：agent-svc
```

### palantir-domain

```
职责：通用业务实体（Employee / Order / Flight 等示例）
被使用：examples/、tests/
```

---

## 新增 Crates（P0/P1）

### palantir-event-bus（NEW，P0）

```
职责：EventPublisher / EventSubscriber trait + 实现
核心 trait：
  pub trait EventPublisher: Send + Sync {
      async fn publish(&self, topic: &str, event: &OntologyEvent) -> Result<()>;
  }
  pub trait EventSubscriber: Send + Sync {
      async fn subscribe(&self, pattern: &str, handler: BoxedHandler) -> Result<()>;
  }
实现：
  - InProcessBus（tokio broadcast）← 开发 / 单进程
  - FluvioBus（Rust 原生）         ← 微服务首选
  - NatsBus（NATS JetStream）      ← 保守备选
被使用：ontology-svc、ingest-svc、workflow-svc、agent-svc
```

### palantir-function-core（NEW，P1）

```
职责：Function / Logic trait + FunctionRegistry
核心 trait：
  pub trait FunctionRegistry {
      fn register(&mut self, meta: FunctionMeta, handler: BoxedHandler);
      async fn invoke(&self, id: &str, ctx: InvokeContext) -> InvokeResult;
  }
宏：#[ontology_function] → 自动注册 + 生成 OpenAI tool schema
被使用：function-svc、agent-svc（tool schema 注入）
```

### palantir-auth-core（NEW，P2）

```
职责：Permission / Policy 类型 + PolicyEvaluator trait
核心 trait：
  pub trait PolicyEvaluator: Send + Sync {
      async fn evaluate(&self, req: &AuthzRequest) -> AuthzResult;
  }
逃生门：未来可换 OPA / Cedar
被使用：auth-svc、api-gateway（JWT 验证）
```

---

## 依赖关系图

```
palantir-domain
  ↑
palantir-ontology-manager
  ↑
palantir-event-bus    palantir-persistence
  ↑                        ↑
ontology-svc          ingest-svc

palantir-function-core
  ↑
function-svc ← agent-svc

palantir-auth-core
  ↑
auth-svc ← api-gateway

palantir-pipeline
  ↑
workflow-svc

palantir-agent
  ↑
agent-svc
```

---

## 待细化

- [ ] palantir-event-bus：InProcessBus 实现（tokio broadcast channel）
- [ ] palantir-function-core：`#[ontology_function]` 宏实现
- [ ] palantir-auth-core：RBAC 简单实现（PolicyEvaluator）
- [ ] crate 版本管理策略（workspace 统一版本 vs 独立版本）
