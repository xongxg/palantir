---
name: ontology_refactor_plan
description: Dataset→Ontology 数据模型职责划分、当前实现状态、演进路径（2026-03-21 更新）
type: project
---

## 当前实现状态（2026-03-21）

### P0 已完成
- `UNIQUE INDEX ON ontology_objects(entity_type_id, external_id)` — promote 幂等去重
- `object_type_mappings` 表 — 每个 dataset 保存一份 promote 配置（entity_type、primary_key_col、field_mapping）
- `upsert_ontology_object()` — 统一写入，`external_id = None` 时无脑 INSERT，有值时 ON CONFLICT DO UPDATE
- `POST /api/datasets/:id/promote` — 重新实现，支持 primary_key_col + 保存 mapping
- `GET /api/datasets/:id/mapping` — 返回上次 promote 配置，供 UI 回填

### Sources ↔ Ontology 管道（已完成，2026-03-21）
- `auto_promote_if_mapped(dataset_id)` — sync 完成后自动触发，读 mapping 表 → 读 S3 manifest → upsert ontology objects
- `POST /api/datasets/:id/mapping` + `GET /api/datasets/:id/mapping` — 配置/查询映射
- Sources UI：每个 dataset 显示映射状态 badge，"配置映射" modal（列类型推断 + ET 选择 + 主键选择）
- Ontology Import tab：Schema-first 流程（"保存为 Entity Type" = 仅 schema，"Promote" = 独立写实例）
- 关联推断：列名 `_id`/`_fk` 后缀匹配已有 ET 名称
- **Fold 范围关联推断原则（2026-03-21 确认）**：同 Fold 内关联置信度最高，跨 Fold 需用户确认，跨 Project 不自动推断
- **CSV 批量语义**：业务部门一批推送的 CSVs 同属一个 Fold（子业务线），关联关系是业务预设的；Project = 大业务线，Fold = 子业务线
- **ADR-38（关键）**：Fold = 数据组织层；Ontology = Project 级语义层，二者完全解耦。跨 Fold 关联直接用 Link Type，无需额外配置。Project 是语义边界，Fold 不是。对应 Palantir Foundry：Folder 管数据位置，Link Type 连任意 Object Type。

### 已识别的职责混淆问题（ADR-36）

`ontology_objects` 同时存了两类数据：

| 类型 | entity_type_id | sync_run_id | 用途 |
|------|----------------|-------------|------|
| Raw 缓存 | `"default"` | 实际 run_id | promote 的数据源、预览 |
| Typed 对象 | 真实 UUID | `"promote"` | 图谱查询、Logic 计算 |

**后果**：不加过滤时 `count_dataset_records` 翻倍，重复 promote 可能自循环。

**当前决策（方案 A）**：查询层加 `sync_run_id != 'promote'` 过滤，不拆表。待实施。

---

## 整体进展评估（2026-03-21）

### 数据模型层 — 85% ✅

结构已正确对齐架构目标：
- `entity_types` / `ontology_links` 无 `fold_id`，天然是 Project 级全局语义层
- `folds` 只约束 `data_sources`，不约束语义
- `object_type_mappings` + `link_type_mappings` 完整
- 待做：`list_dataset_records` 加 `sync_run_id != 'promote'` 过滤（raw vs typed 混表）

### API 层 — 60% ⚠️

核心功能完整，缺口：
- ✗ Link Type 无独立 CRUD 接口（目前只在 promote 时隐式创建）
- ✗ `list_dataset_records` raw/typed 过滤未加

完整 API 清单：12 个页面路由 + ~45 个 API 端点（见 ingest-workflow_v0.5.0.md）

### UI 层 — 40% ⚠️

HTML 已写但需 rebuild 才生效（`include_str!` 编译时嵌入）：
- ✅ 已写待 rebuild：`project_workspace.html`（4 Tab 统一工作台）、映射向导 3 步含业务实体确认
- ✗ Fold 分组展示 DataSources（当前平铺）
- ✗ 映射向导关联推断区分同 Fold / 跨 Fold
- ✗ Link Type 管理界面

### 下一步优先级

| 优先级 | 任务 |
|--------|------|
| P0 | `cargo build` + 重启，验证已写 HTML 效果 |
| P0 | `list_dataset_records` 加 `sync_run_id != 'promote'` 过滤 |
| P1 | Fold 分组展示 DataSources（Project Workspace 数据接入 tab） |
| P1 | 映射向导 Step 3 关联推断按 Fold 边界分层（同 Fold 高置信 / 跨 Fold 需确认） |
| P2 | Link Type 独立管理界面 |

---

## Palantir Foundry 对标方案（终态参考）

**核心原则：Ontology 对象 = Dataset 的实时视图，不复制数据**

```
原始数据源
  ↓ Connector 同步
Dataset（列式，不变）
  ↓ ObjectType 绑定 Dataset（mapping 元数据持久化）
Object（实时从 Dataset 按 mapping 读取）
  ↓
LinkType（由边 Dataset 驱动：from_id, to_id 两列）
```

---

## 演进路径

| 阶段 | 内容 | 时机 |
|------|------|------|
| **当前 P0（已完成）** | 主键去重 + mapping 持久化 | ✅ |
| **待做 P0** | `list_dataset_records` 加 `sync_run_id != 'promote'` 过滤 | 下次改 |
| **P1** | 增量更新（diff by external_id）+ LinkType 数据驱动 | 功能验证后 |
| **P2（拆表）** | `dataset_raw_records` + `ontology_objects` 完全分离 | SQLite 性能瓶颈时 |
| **P3（实时视图）** | Object = manifest 文件实时读取，不复制 | 引入 DataFusion 后 |

---

**Why:** promote-copy 模式在当前阶段够用，但 raw/typed 混表会导致计数翻倍和自循环风险，需要在查询层加过滤（方案 A）作为过渡。

**How to apply:** 涉及 `count_dataset_records` / `list_dataset_records` 的地方，确认是读 raw 还是读 typed，加对应的 `sync_run_id` 过滤条件。
