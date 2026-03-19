# ADR-11: Workflow 触发器

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Workflow 如何触发？定时和事件驱动如何统一？

## 决策

**Cron + EventListener 统一进 TriggerManager**，共用 WorkflowEngine 执行。

## 架构

```
TriggerManager
  ├── CronScheduler   → TriggerEvent（定时，全量扫描）
  └── EventListener   → TriggerEvent（实时，单对象上下文）
          ↓
  WorkflowEngine（统一执行，DAG 并发）
          ↓
  Saga 补偿（on_failure → 补偿 Function）
```

## 幂等保障

同一 `object_id` 设 Redis TTL 冷却窗口，防短时间重复触发。

## 有状态聚合

不引入 Flink，通过触发 Logic 查 SurrealDB 实现（Ontology 即状态存储）。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
