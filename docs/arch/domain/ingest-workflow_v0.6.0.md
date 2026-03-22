# 数据接入工作流设计 v0.6.0

> 版本：v0.6.0
> 日期：2026-03-21
> 状态：P0 已实现
> 前置文档：[ingest-workflow v0.5.0](ingest-workflow_v0.5.0.md)

---

## 变更记录

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.5.0 | 2026-03-21 | Sources ↔ Ontology 管道；Schema-first 导入；sync_mode 绑定 data_sources |
| **v0.6.0** | **2026-03-21** | **sync_mode 下沉至 dataset 级别；两层版本控制分离；UI 单一入口（消除双重配置）** |

---

## 一、核心架构决策：sync_mode 绑定 Dataset，而非 Source

### 问题（v0.5.0 的设计缺陷）

v0.5.0 将 `sync_mode` 放在 `data_sources` 表，即**一个 Source 的所有 Dataset 共用同一同步策略**。

这不合理：同一个 S3 Source 下可能存放多种性质不同的数据文件——

```
hr_ds (S3 Source)
  ├── employees.csv   → 状态快照，每次推全量  → SNAPSHOT
  ├── events.csv      → 事件流，历史行不能删   → APPEND
  └── products.csv    → 按商品 ID 合并更新    → UPSERT
```

强制共用一个 sync_mode 会导致其中某些 dataset 的处理语义错误。

### 解决方案（v0.6.0）

**sync_mode 下沉到 `object_type_mappings`（dataset 级别）。**

每个 Dataset 独立配置同步策略，和它绑定的 Entity Type 一起保存：

```sql
-- object_type_mappings（v0.6.0 新增 sync_mode 列）
ALTER TABLE object_type_mappings
  ADD COLUMN sync_mode TEXT NOT NULL DEFAULT 'snapshot';
```

对应 API：

| 方法 | 路径 | 用途 |
|------|------|------|
| `POST` | `/api/datasets/:id/mapping` | 保存映射 + sync_mode（一体） |
| `GET`  | `/api/datasets/:id/mapping` | 读取映射含 sync_mode（供 UI 回填） |
| `PUT`  | `/api/datasets/:id/sync-mode` | 单独更新 sync_mode（已配置映射后可随时调整） |

---

## 二、两层存储的正交性

这是本版本最重要的架构结论，需明确写入技术方案。

### 两层分别是什么

| 层级 | 表 / 存储 | 职责 |
|------|-----------|------|
| **原始文件层** | `dataset_versions` + RustFS | 每次 Sync 产生新版本快照（v1, v2, v3...） |
| **语义对象层** | `ontology_objects` + `object_type_mappings` | 业务实体的语义实例，受 sync_mode 控制 |

### 两层正交

这两层是**完全正交**的两件事：

```
每次 Sync
  │
  ├─▶ [原始文件层]  新建 dataset_version（v3）→ 写 RustFS manifest + parquet
  │                 ← 与 sync_mode 无关，每次都创建新版本，支持回溯
  │
  └─▶ [语义对象层]  auto_promote_if_mapped()
                    ← sync_mode 在此生效，决定如何合并到 ontology_objects
```

**结论：**

- RustFS 的 v1/v2/v3 是**原始数据的时间轴快照**，三种 sync_mode 都会生成新版本，这层与同步策略无关
- sync_mode 只影响**语义对象层**：怎么把这次同步的数据合并到 `ontology_objects` 表中

### sync_mode 对语义层的具体影响

| sync_mode | `ontology_objects` 层行为 | 适用场景 |
|-----------|--------------------------|---------|
| `snapshot`（默认） | 先 `DELETE WHERE dataset_id = ?`，再全量 upsert | 每次推送完整状态快照（最常见） |
| `append` | 只 INSERT 新记录，已有 external_id 不更新 | 事件流、日志、增量导出（历史行必须保留） |
| `upsert` | 按 external_id 合并：有则更新，无则插入 | 状态同步、缓慢变化维度（主数据更新） |

**注意：sync_mode 的选择直接影响同步策略实施，配置后每次 Sync 自动按此策略执行，无需人工干预。**

---

## 三、`auto_promote_if_mapped` 实现（含 sync_mode 分支）

```
auto_promote_if_mapped(dataset_id):
  1. get_object_type_mapping(dataset_id) → 无则 return
  2. 读取 sync_mode（默认 snapshot）
  3. 读 RustFS manifest → records

  4. if sync_mode == "snapshot":
       delete_ontology_objects_by_dataset(dataset_id)  ← 清空本 dataset 的旧对象
       upsert_all(records)

     if sync_mode == "append":
       upsert_all(records)
       // ON CONFLICT(entity_type_id, external_id) DO NOTHING
       // → 已有 external_id 不更新，自然实现"只追加"

     if sync_mode == "upsert":
       upsert_all(records)
       // ON CONFLICT(entity_type_id, external_id) DO UPDATE SET ...
       // → 标准 upsert，新记录覆盖旧记录同主键行
```

---

## 四、UI 单一入口（消除双重配置）

### v0.5.0 的问题

存在两个配置入口，功能重叠：

1. **Project Workspace** → "配置映射 →" 按钮 → 弹窗（Schema 发现 + ET 映射 + sync_mode）
2. **Ontology Import tab** → 同一套配置，且才是真正持久化的入口

两处配置结果不完全同步，用户困惑"哪边算数"。

### v0.6.0 方案：Project Workspace 不再内嵌配置

```
Project Workspace（数据接入层）
  └── 每个 Dataset 行
        ├── [状态徽章]：已映射 / Schema 可推导 / 暂无数据
        ├── [sync_mode 下拉]：仅在已配置映射时显示（PUT /api/datasets/:id/sync-mode）
        └── [配置映射 →] 按钮 → 跳转 /ontology?from=PROJECT_ID&dataset=DATASET_ID

Ontology Import tab（语义配置层，单一真相）
  └── 自动选中目标 Dataset
        ├── Schema 发现（列类型、主键、忽略）
        ├── 关联关系推断
        ├── ET 名称输入 / 已有 ET 下拉
        ├── sync_mode 选择器（持久化到 object_type_mappings）
        └── "保存为 Entity Type" → 立即持久化
```

**设计原则：**
- Project Workspace = 数据接入层（Source、Sync、文件管理）
- Ontology = 语义配置层（ET 映射、Schema、sync_mode）
- 两层职责不交叉，每层只做自己的事

### Navigation Context

Project Workspace → Ontology 时携带参数：

```
/ontology?from=PROJECT_ID&dataset=DATASET_ID
```

Ontology 读取参数：
- `from` → Workspace 链接变为"← 返回项目"，href = `/project/PROJECT_ID`
- `dataset` → 自动切换到 Import tab 并选中该 Dataset，加载已保存的映射（ET 名称、主键、sync_mode）

---

## 五、选型指南：如何选择 sync_mode

```
问：这个数据集，每次推送的是…？
│
├── 完整当前状态（所有记录）
│     → SNAPSHOT（最常见，最安全，默认）
│
├── 只有新增的行（增量）
│     → APPEND
│     注：历史行不会被删，适合日志/事件
│
└── 新增 + 修改的行，按主键区分
      → UPSERT
      注：需配置主键列（primary_key_col）
```

**误选的代价：**

| 误选 | 后果 |
|------|------|
| 应该用 SNAPSHOT，却用了 APPEND | 历史已删除的记录仍残留在 Ontology（幽灵数据） |
| 应该用 APPEND，却用了 SNAPSHOT | 每次 Sync 历史事件被清空，只保留最新一批 |
| 应该用 UPSERT，却用了 APPEND | 同一主键出现重复对象（若无 external_id 约束）|

---

## 六、已修复的关键 Bug（v0.6.0）

| Bug | 现象 | 修复 |
|-----|------|------|
| `sd-sync-mode` 元素找不到 | Source header 移除后 JS 仍引用旧元素 | 移除旧引用，改为 `updateDatasetSyncMode(dsId, mode)` |
| Import tab 打开时 ET 下拉为空 | `entityTypes` 在 Import tab 未加载（只有 Schema tab 加载） | `selectDataset()` 开头加 `if (!entityTypes.length) await loadEntityTypes()` |
| 已保存 ET 名称不回填 | 加载 mapping 后只设置了下拉值，未填 text input | 找到 savedET 后同步写入 `promote-new-type` input |
| sync_mode 不回填 | 加载 mapping 后未还原 `promote-sync-mode` 选择器 | 读取 `m.sync_mode` → 设置 `promote-sync-mode` value |

---

## 七、数据流总结

```
用户操作：Project Workspace → 点击"配置映射 →"
                │
                ▼
          跳转 /ontology?from=X&dataset=Y
                │
                ▼
          Ontology Import tab
          ├── 加载 Dataset 列表（loadDatasets）
          ├── 自动选中 Dataset Y（selectDataset）
          ├── 加载已有 ET（loadEntityTypes）
          ├── 还原已保存映射：ET 名称、主键、sync_mode
          ├── 用户调整 Schema / 选择 sync_mode
          └── "保存为 Entity Type"
                │
                ▼
          object_type_mappings
          { dataset_id, entity_type_id, primary_key_col,
            field_mapping, sync_mode }
                │
                ▼（下次 Sync 触发）
          auto_promote_if_mapped(dataset_id)
          按 sync_mode 执行：
          snapshot → 清空 + 全量写
          append   → 只追加新记录
          upsert   → 按主键合并
                │
                ▼
          ontology_objects（语义对象，供 Browse / Graph / Query）
```
