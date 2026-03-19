# ADR-07: Ontology 主存储 — SurrealDB

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Ontology（TBox + ABox）存储在哪里？

## 决策

**SurrealDB** 作为 Ontology 主存储。

## 理由

- 原生图遍历（RELATE + `->` 语法），无需应用层实现
- 文档模型天然对齐 OntologyObject 灵活 Schema
- 内置向量搜索（MVP 阶段无需额外服务）
- 官方 Rust SDK
- TiKV 水平扩容路径清晰

## 存储模型

```
TBox：    entity_schema 表
ABox 点： 每个 EntityType 一个 Table，id = {type}:{uuid}
ABox 边： RELATE 语句，边可带属性
Events：  append-only event 表，seq 作为 cursor
```

## 扩容路径

内存模式（开发）→ RocksDB（生产）→ TiKV（大规模）

## 否决方案

| 方案 | 否决原因 |
|------|---------|
| MongoDB | 图遍历需应用层实现 |
| Cassandra | 查询模式固定，灵活性差 |
| Neo4j | 扩容贵（Enterprise 才支持集群）|
| 纯 Postgres | 分片运维复杂，图遍历靠递归 CTE 性能差 |
| TiDB | 无原生图，需自己实现遍历 |

## 风险对冲

`OntologyRepository` trait 抽象，`SqliteRepository` 作为备用实现。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
