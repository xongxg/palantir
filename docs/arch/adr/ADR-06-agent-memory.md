# ADR-06: Agent Long-term Memory 存储

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Agent 长期记忆存在哪里？如何检索？

## 决策

**分层存储**，`MemoryStore` trait 抽象，阶段间只换实现。

## 架构

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

## 写入条件

```rust
if memory.confidence >= 0.85 && memory.access_count > 2 && !is_expired(&memory) {
    embed_and_index(memory);
}
```

## 两步写入

无需分布式事务：Qdrant 是纯索引，丢失可重建。

## 扩容路径

SQLite + in-process usearch → Postgres + Qdrant → Citus + Qdrant 多节点
