# workflow-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

Workflow 编排、Cron/事件触发、Saga 补偿、保留策略执行。

---

## API 端点

```
# Workflow 定义
POST   /v1/workflows                  → 创建 Workflow
GET    /v1/workflows                  → 列出
PUT    /v1/workflows/{id}             → 更新
DELETE /v1/workflows/{id}             → 删除

# 执行管理
GET    /v1/runs                       → 执行历史
GET    /v1/runs/{id}                  → 执行详情（含步骤状态）
POST   /v1/runs/{id}/cancel           → 取消

# 触发器管理
POST   /v1/triggers                   → 注册触发器（Cron / EventListener）
DELETE /v1/triggers/{id}              → 删除
```

---

## 触发器架构（ADR-11）

```
TriggerManager
  ├── CronScheduler   → TriggerEvent（定时，全量扫描）
  └── EventListener   → TriggerEvent（实时，单对象上下文）
          ↓
  WorkflowEngine（统一执行，DAG 并发）
          ↓
  Saga 补偿（on_failure → 补偿 Function）
```

---

## Workflow 定义结构

```rust
pub struct WorkflowDefinition {
    pub id: WorkflowId,
    pub name: String,
    pub trigger: TriggerConfig,      // Cron | EventPattern
    pub steps: Vec<WorkflowStep>,    // DAG
    pub on_failure: Option<CompensationConfig>,
}

pub struct WorkflowStep {
    pub id: StepId,
    pub function_id: FunctionId,
    pub depends_on: Vec<StepId>,     // DAG 依赖
    pub timeout: Duration,
}
```

---

## Saga 补偿

```
Step 1 执行成功 → Step 2 执行失败
  ↓
触发补偿链（逆序）：
  compensate(Step 1) ← 注册的补偿 Function
```

---

## 幂等保障

```
同一 object_id 触发 → Redis SET NX {wf:cooldown:{object_id}} TTL
冷却窗口内重复触发 → 忽略
```

---

## 保留策略引擎（ADR-09 P2）

Cron 定时触发，扫描 EntityType 绑定的保留期，执行归档/删除。

---

## 复用 crate

- `palantir-pipeline`：Saga / transform 原语

---

## 待细化

- [ ] DAG 执行并发度控制
- [ ] 失败重试策略（指数退避）
- [ ] Workflow 版本管理与热更新
- [ ] 执行历史存储与查询优化

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
