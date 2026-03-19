# ontology-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

TBox/ABox CRUD、OntologyEvent 发布、离线同步（/v1/sync）。

**Single Source of Truth**：所有 Ontology 数据的权威来源。

---

## API 端点

```
# TBox（Schema 管理）
POST   /v1/schema/entity-types        → 定义 EntityType
GET    /v1/schema/entity-types        → 列出所有 EntityType
PUT    /v1/schema/entity-types/{id}   → 更新定义
DELETE /v1/schema/entity-types/{id}   → 删除

# ABox（对象管理）
POST   /v1/objects                    → 创建 OntologyObject
GET    /v1/objects/{id}               → 按 ID 查询
PUT    /v1/objects/{id}               → 更新
DELETE /v1/objects/{id}               → 删除
GET    /v1/objects?entity_type=X      → 按类型列表

# 关系
POST   /v1/links                      → 建立关系（RELATE）
DELETE /v1/links/{id}                 → 删除关系
GET    /v1/objects/{id}/neighbors     → 图遍历（N 跳）

# 离线同步（ADR-05）
GET    /v1/sync/snapshot              → 全量快照
POST   /v1/sync/delta                 → 增量合并

# 事件流（SSE）
GET    /v1/events/stream              → 实时事件推送
```

---

## 存储模型（SurrealDB）

```
TBox：
  entity_schema { id, name, fields: [{name, type, classification}], version }

ABox 点：
  {entity_type}:{uuid} { ...attrs, valid_from, valid_to, tx_time, version, provenance }

ABox 边：
  RELATE {from} -> {rel_type} -> {to} { ...attrs }

事件日志：
  ontology_event { entity_type, seq, op, payload, ts }
```

---

## 数据分类标签（ADR-09）

TBox 字段级别打标签，驱动加密、审计、保留策略：

```
Public / Internal / Confidential / PII
```

---

## 复用 crate

- `palantir-ontology-manager`：SourceAdapter、OntologyEvent、TomlMapping
- `palantir-persistence`：SQLite 备用实现

---

## 事件发布

每次写操作后发布到 Event Bus：

```
ontology.events.{entity_type}.upsert
ontology.events.{entity_type}.delete
ontology.events.{entity_type}.link
```

---

## 待细化

- [ ] Three-Way Merge 冲突标记格式
- [ ] 双时态（bi-temporal）查询 API 设计
- [ ] 图遍历深度限制策略

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
