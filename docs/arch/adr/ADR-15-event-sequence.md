# ADR-15: 事件序列粒度

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Event Bus 的事件序列是全局一个还是按 EntityType 独立？

## 决策

**按 EntityType 独立序列**，通过 NATS Subject 层级实现，不创建多个 Stream。

## NATS Subject 规范

```
Stream 名：ontology-events（唯一）
Subject：  ontology.events.{entity_type}.{op}

示例：
  ontology.events.Employee.upsert
  ontology.events.Contract.upsert
  ontology.events.Contract.delete
  ontology.events.Transaction.link
```

## 消费者订阅示例

```rust
// workflow-svc 只关心 Contract
subscriber.subscribe("ontology.events.Contract.>", handler).await?;
// agent-svc 全量
subscriber.subscribe("ontology.events.>", handler).await?;
```

## SurrealDB 事件表

```sql
-- (entity_type, seq) 复合唯一键，seq 按 entity_type 独立递增
INSERT INTO ontology_event {
    entity_type: "Contract",
    seq: sequence::next("seq_Contract"),
    payload: ...,
};
```

## ConsumerCursor 结构

```rust
// 从单值改为 Map
pub struct ConsumerCursor(pub BTreeMap<String, u64>);
// { "Employee": 42, "Contract": 17, "Transaction": 156 }
```

## OntologyObject.version

不变：已经是 per-object 级别，本身正确。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
