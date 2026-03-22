# ADR-35: Ontology 对象的去重与身份标识

**日期：** 2026-03-21
**状态：** 已采纳
**关联：** ADR-34（dataset 存储与版本）

---

## 背景

Dataset → Ontology 的 promote 流程最初使用简单 INSERT，导致：
- 重复执行 promote 产生重复对象
- 无法判断哪两条记录是同一业务实体的不同版本

---

## 决策

### 1. `external_id` 作为业务身份键

`ontology_objects` 表新增 `external_id TEXT` 列，存放业务主键值（由 promote 时的 `primary_key_col` 参数指定）。

建立 UNIQUE INDEX：
```sql
CREATE UNIQUE INDEX idx_oo_upsert ON ontology_objects(entity_type_id, external_id)
```

**NULL 语义**：SQLite 中 NULL != NULL，因此 `external_id = NULL` 的行不互相冲突。
- sync 路径：`external_id = NULL` → 总是 INSERT（保留全量历史快照）
- promote 路径：`external_id = pk_value` → INSERT ON CONFLICT DO UPDATE（幂等 upsert）

### 2. `object_type_mappings` 表持久化 promote 配置

每次 promote 后保存 `{ dataset_id, entity_type_id, primary_key_col, field_mapping }` 到独立表。

作用：
- UI 可回填上次配置，避免重复设置
- 未来自动 re-sync：dataset 新版本到来时，可按此配置重做 promote

### 3. 统一写入方法

所有写入路径统一走 `upsert_ontology_object()`，旧方法改为薄包装，零 call site 改动。

---

## 后果

- 重复 promote 幂等，数据不翻倍
- promote 配置持久化，支持未来 re-sync
- sync 路径不受影响，仍然全量 INSERT（血缘完整）

---

## 否决的方案

**用 `label` 去重**：label 是展示用字符串，不保证业务唯一性，否决。
**强制所有写入都有 external_id**：sync 路径无法提前知道业务主键，否决。
