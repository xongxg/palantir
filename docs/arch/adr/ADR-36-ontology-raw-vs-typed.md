# ADR-36: ontology_objects 表的职责边界——Raw 缓存 vs Typed 对象

**日期：** 2026-03-21
**状态：** 已采纳
**关联：** ADR-35（ontology upsert identity）、ADR-34（dataset 存储版本化）

---

## 问题

当前 sync 路径将原始数据以 `entity_type_id = "default"` 写入 `ontology_objects`，promote 路径再从同一张表读取并写回 typed 对象。导致：

1. **计数翻倍**：promote 后 `count_dataset_records(dataset_id)` 返回 raw + typed 之和
2. **自循环风险**：重复 promote 可能把上次 promote 产生的 typed 对象当源数据再次处理
3. **职责混淆**：同一张表既是"原始数据暂存区"，又是"语义化 Ontology ABox"

---

## 对标：Palantir Foundry 的职责划分

Palantir 将两者完全分离：

- **Dataset**：不可变的原始数据（列式存储，Parquet）
- **Ontology Object**：Dataset 的实时视图，由 ObjectType binding 动态计算，不存储副本

我们当前阶段不具备实时视图能力（需要 DataFusion 等计算引擎），但可以在存储层做职责划分。

---

## 决策

### 当前阶段：方案 A（查询过滤，最小改动）

不拆表，通过 `sync_run_id` 列区分记录类别：

- `sync_run_id != 'promote'` → raw 同步数据（供 promote 读取、预览）
- `sync_run_id = 'promote'` → typed ontology 对象（供图谱、Logic 查询）

**受影响的查询**：

```sql
-- list_dataset_records（promote 数据源 + 预览）
SELECT ... FROM ontology_objects
WHERE dataset_id = ? AND sync_run_id != 'promote'
ORDER BY created_at ASC LIMIT ? OFFSET ?

-- count_dataset_records（同上）
SELECT COUNT(*) FROM ontology_objects
WHERE dataset_id = ? AND sync_run_id != 'promote'

-- list_ontology_objects（图谱 / API 查询）
SELECT ... FROM ontology_objects
WHERE entity_type_id != 'default'
-- （entity_type_id = 'default' 的行是 raw 缓存，不对外展示）
```

**优点**：改动范围极小，不破坏现有数据，两个查询均可加索引。

---

## 演进路径

| 阶段 | 方案 | 触发条件 |
|------|------|---------|
| **当前（Phase 1）** | 方案 A：`sync_run_id` 过滤 | 已决策 |
| **Phase 2** | 方案 B：拆表（`dataset_raw_records` + `ontology_objects`）| 原始数据量 > 100万行，SQLite 性能瓶颈 |
| **Phase 3** | 方案 C：不复制，Object = manifest 文件实时视图 | 引入 DataFusion 后 |

方案 A → B 迁移路径：新建 `dataset_raw_records` 表，将 `entity_type_id = 'default'` 的行迁移过去，修改 sync 写入目标，promote 读取来源改为新表。上层 API 接口不变。

---

## 否决的方案

**立即拆表（方案 B）**：改动范围较大，当前数据量不构成 SQLite 性能问题，过早优化，否决。

**立即实现实时视图（方案 C）**：需要 DataFusion / Arrow，与当前 SQLite 架构差距大，否决。
