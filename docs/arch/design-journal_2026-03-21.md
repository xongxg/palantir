# Design Journal — 2026-03-21

## Ontology 核心（P0 阶段）

### 问题诊断

今天回滚了一个未完成的 demo 分支，重新从干净状态推进 Ontology 核心。

诊断出三个 P0 问题：

| 问题 | 根因 |
|------|------|
| 重复对象 | `ontology_objects` 无 UNIQUE 约束，重复 promote = 重复行 |
| 映射不持久 | 无 `object_type_mappings` 表，每次 promote 设置丢失，无法 re-sync |
| entity_type 不校验 | sync 路径默认写 `"default"` 字符串，FK 约束实际靠 seed 行撑住 |

---

### 决策：`external_id` 作为去重键

**背景：** 用 label 去重不可靠（label 是展示用字符串，可能重复）。
**决策：** 引入 `external_id` 列，由调用方传入业务主键值（如 `id` 字段的值）。

```
UNIQUE INDEX ON ontology_objects(entity_type_id, external_id)
```

- `external_id = NULL`：SQLite NULL != NULL，多行 NULL 不冲突 → sync 路径永远 INSERT
- `external_id = Some(pk)`：ON CONFLICT DO UPDATE → promote 路径幂等 upsert

---

### 新增：`object_type_mappings` 表

```sql
CREATE TABLE object_type_mappings (
    id              TEXT PRIMARY KEY,
    dataset_id      TEXT NOT NULL UNIQUE,   -- 每个 dataset 一份映射
    entity_type_id  TEXT NOT NULL,
    primary_key_col TEXT NOT NULL DEFAULT '',
    field_mapping   TEXT NOT NULL DEFAULT '{}',
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
)
```

- promote 时自动 upsert 一条记录
- `GET /api/datasets/:id/mapping` 可回填 UI 上次配置
- 未来 re-sync 可读取此表自动重做 promote

---

### 简化：统一写入路径

原来三个方法：
- `create_ontology_object` — 无 lineage，connections_sync 用
- `create_ontology_object_with_lineage` — sync 路径用
- `upsert_ontology_object` — promote 路径用（新增）

简化后：前两个变成 `upsert_ontology_object` 的薄包装（`external_id = None`），一条 SQL 语句统一所有写入。外部 API 不变，call site 零改动。

---

### promote API

```
POST /api/datasets/:id/promote
{
  "entity_type_id":  "uuid",   // 已有实体类型
  "new_type_name":   "客户",    // 或新建
  "primary_key_col": "id",     // 去重键，空则每次全量 insert
  "field_mapping":   { "src_field": "target_attr" }
}
```

返回：
```json
{ "ok": true, "promoted": 123, "total": 123, "dedup": true }
```

---

---

## Palantir 对标：数据模型职责混淆问题

### 背景对比

**Palantir 官方模型**：
```
Dataset（不可变，列式存储）
  ↓ ObjectType 绑定（mapping 元数据）
Object（实时视图，不复制数据）
```

**我们当前模型**：
```
Source → sync → ontology_objects（entity_type="default"，raw cache）
                       ↓ promote
              ontology_objects（entity_type=真实类型，typed copy）
```

### 核心矛盾

`ontology_objects` 同时承担两个语义完全不同的角色：

| 角色 | entity_type_id | 写入时机 | 用途 |
|------|----------------|----------|------|
| Raw 数据缓存 | `"default"` | sync | promote 的数据源、预览 |
| Ontology 语义对象 | 真实 UUID | promote | 图谱查询、Logic 计算 |

**后果**：`count_dataset_records` 查 `WHERE dataset_id = ?`，promote 后 raw + typed 混在一起，计数翻倍；promote 可能把自己之前 promote 出的 typed 对象再次当源数据读取（自我循环）。

### 收敛方向（不过度设计）

不需要立即对标 Palantir 的"实时视图"，当前阶段最合理的职责分离：

**方案 A（最小改动）**：加查询过滤条件

```sql
-- promote 的数据源（只读 raw）
WHERE dataset_id = ? AND sync_run_id != 'promote'

-- 图谱 / Logic 查询（只读 typed）
WHERE entity_type_id != 'default'
```

**方案 B（更清晰）**：拆表

```
dataset_raw_records  ← sync 写入，raw preview 和 promote 数据源
ontology_objects     ← 只放 promote 后的 typed 对象
```

**方案 C（Palantir 终态，P2）**：不复制数据，Object = 从 manifest 文件实时读取的视图

### 当前阶段决策

> **采用方案 A**，不拆表，在查询层加 `sync_run_id != 'promote'` 过滤。改动范围最小，足以消除自循环和计数翻倍问题，等真实使用场景验证后再决定是否升级到方案 B。

见 ADR-36。

---

### 下一步

- [ ] 实施方案 A：`list_dataset_records` / `count_dataset_records` 加 `sync_run_id != 'promote'` 过滤
- [ ] ingest_fold.html promote UI 加 `primary_key_col` 输入 + `/mapping` 回填
- [ ] sync 路径不再用 `"default"`，改为先确保 entity type 存在再写入
- [ ] Ontology → Logic 层（只读计算，派生属性）

---

## Sources ↔ Ontology 管道打通（2026-03-21）

### 背景

原来两个流程完全独立：
- Sources：连接器 → sync → S3（raw files）
- Ontology：手动 Import tab 上传 → 手动 Promote

用户每次 sync 后都要手动去 Ontology 页重新 promote，不顺畅。

### 设计决策：`object_type_mappings` 作为桥接表

```
DataSource → sync → S3(manifest) → auto_promote_if_mapped → OntologyObject
                         ↑
               object_type_mappings（一次配置，永久生效）
               dataset_id → entity_type_id + primary_key_col + field_mapping
```

一次配置映射后，后续每次 sync 完成自动触发 promote（幂等 upsert，主键去重）。

### 已实现（P0）

**后端（main.rs）**：
- `auto_promote_if_mapped(dataset_id)` — sync 完成后自动调用，读 mapping 表 → 读 S3 manifest → upsert ontology objects
- `POST /api/datasets/:id/mapping` — 保存映射配置（entity_type_id + primary_key_col + field_mapping JSON）
- `GET /api/datasets/:id/mapping` — 读取已有映射，供 UI 回填
- 两个 sync 路径（spawn task + 直接 sync）均在 `finish_sync_run` 后调用 `auto_promote_if_mapped`

**前端（sources.html）**：
- 每个 Dataset 卡片并行加载映射状态，显示 `🔗 EntityTypeName`（绿色）或 `⚠ 未映射`（黄色）
- "配置映射" 按钮打开 Modal：加载样本 → 推断列类型 → 选主键 → 选/新建 ET → 保存
- Modal 保存后 Toast 提示"后续 Sync 将自动 Promote 到 Ontology"

**前端（ontology.html）**：
- Import tab 改为 Schema-first 流程：先发现 Schema → "保存为 Entity Type"（仅存 schema）→ 单独 "Promote → Ontology"（写数据实例）
- 避免 "保存 ET" 时误写数据实例

### 关联关系自动推断

列名以 `_id` / `_fk` 结尾（如 `airport_id`、`airline_fk`），去掉后缀后与已有 Entity Type 名称做大小写不敏感匹配，自动推断为关联关系并展示在"关联关系"区域，用户可确认或修改。

**决策依据**：FK 命名约定是行业惯例，误判率低；用户仍可手动修正或忽略。复杂场景（值域相似度匹配、ML 推断）留待 P2。

### is_required 字段 Bug（已修复）

`saveSchemaAsET` JS 发送 `required: f.pk`，但后端 `AddFieldReq` 字段名为 `is_required`。导致所有 ET 字段的 `is_required` 被 serde 忽略，默认为 false 但实际上字段根本未被接受。

修复：`is_required: f.pk` → 与后端字段名对齐。

### 命名冲突 Bug（已修复）

`SaveMappingReq` struct 在 main.rs 中定义了两次（旧的 connector mapping 用，新的 dataset mapping 用），导致编译失败。

修复：新 struct 重命名为 `SaveDatasetMappingReq`。

---

## Project 统一工作台（方案 A，2026-03-21 确认）

### 背景

原来 Sources / Ontology 作为顶栏独立入口，用户进入时缺乏业务上下文，不知道"为什么在这里"、"连完数据要做什么"。Sources 是手段不是目的，需要在 Project 语境下才有意义。

### 决策：Project 作为唯一业务入口

```
/ (Projects 列表)
  → 点击项目卡片 → /project/:id（统一工作台）
       ① 数据接入   ← 连接数据源 + 配置映射 + Sync
       ② 数据模型   ← Schema / Browse / Graph
       ③ 数据探索   ← 占位，后续扩展
       ⚙ 设置       ← 改名 / 删除
```

用户进入项目后有明确的步骤感：**先接数据 → 再定义模型 → 最后探索**。

顶栏的独立 Sources / Ontology 链接保留作为**全局管理入口**（跨项目管理），日常业务流程全部在项目内完成。

### 实现

- 新建 `ui/project_workspace.html`：4 Tab 统一工作台
  - Tab 1 数据接入：Source 列表 + 详情（Datasets mapping 状态 + Sync 历史）+ Mapping Modal
  - Tab 2 数据模型：Sub-tabs Schema / Browse / Graph（内嵌 ontology 功能）
  - Tab 3 数据探索：占位页，引导用户先完成前两步
  - Tab 4 设置：项目改名 + 删除
- 新增路由 `GET /project/:id` → `project_workspace_page`
- 新增 `PATCH /api/projects/:id` → `patch_project`（项目改名）
- 新增 `db().rename_project(id, name)` in palantir-persistence
- `projects.html` 的"进入 →"链接从 `/ingest/project/:id` 改为 `/project/:id`
- `gotoProject(id)` 同步更新

### 设计原则

- **数据手段化**：Sources 不是终点，是为 Ontology 模型供数据的手段
- **步骤可感知**：① → ② → ③ 的数字标号给用户清晰的进度感
- **全局入口保留**：/sources、/ontology 作为平台管理员视角（跨项目），不删除
- **渐进式**：③ 数据探索先占位，等图查询能力成熟后填入，不强行做半成品

---

## Ontology 建模原则：领域优先，数据其次（2026-03-21 确认）

### 触发场景

`aircraft` ET 已有 schema + 实例，新业务 `airline` 属性高度重合。  
问题：是否需要 Schema Fork / 继承 / 复用？

### 结论：不需要，这是错误的提问方式

**错误出发点**（数据驱动）：
```
看到列相似 → 怎么复用 schema？
```

**正确出发点**（领域驱动）：
```
这个业务实体是什么？ → 再决定 ET → 最后才是列怎么映射
```

### 判断依据

`Aircraft`（具体飞机，尾号标识）和 `Airline`（航空公司，IATA 标识）是完全不同的业务实体。
schema 相似是行业属性的巧合，不是"它们相同"的证明。

- `aircraft.manufacturer = "Boeing"` → 这架飞机的制造商
- `airline.manufacturer = "Boeing"`  → 这家公司主要运营的机型制造商

同一个词，不同语义。**应该各自独立定义，不应复用。**

### 正确建模方式

```
Airline ──[OPERATES]──▶ Aircraft
两个独立 ET，一个 Link Type，字段重叠是正常的
```

Dataset 的列只决定"能有什么属性"，不决定"应该有什么属性"。  
`airline` 数据集里有 `model` 列，不代表 Airline ET 一定需要它——业务上不需要就在 mapping 里忽略。

### 对映射向导的影响

向导不应以"列发现"为第一步（引导用户从数据出发），  
应以"业务实体确认"为第一步（引导用户从语义出发）：

```
① 业务确认："这个 Dataset 代表什么业务实体？" → 用户命名
② Schema 发现：从数据推导列，供参考，用户决定取舍
③ 映射配置：列名 → 属性名（别名/忽略均可）
④ 关联关系：发现 _id/_fk 列 → 建议创建 Link Type
```

### ADR 编号

ADR-37：Ontology 建模以领域语义为主，dataset 列结构为辅助参考。

---

## 业务层级语义澄清（下午）

### Project / Fold / DataSource 的业务含义

经讨论确认三层语义：

```
Project   = 大业务线（如：航空集团数字化、供应链管理）
Fold      = 子业务线 / 子域（如：运营部门、财务部门、维修部门）
DataSource= 具体数据源（如：一个 CSV 文件夹、一个 S3 路径、一个 DB 表）
```

Fold 的核心价值：**业务部门上传数据的自然单位**。同一 Fold 下的多个 CSV/JSON 文件是同一业务团队一批推送的，它们之间的关联关系是业务上预先设计好的（不需要猜），推断置信度最高。

这与 Palantir Foundry 的 Folder 概念对应：Folder 管数据的存储位置，不管语义。

---

## ADR-38：Fold 是数据组织层，Ontology 是 Project 级语义层

**核心决策：** Fold 仅约束 DataSource，Entity Type 和 Link Type 无 fold_id，天然是 Project 级全局语义层。

**关键推论：**
- 跨 Fold 的业务关联 → 直接定义 Link Type，零额外配置
- 语义边界是 Project，不是 Fold
- 重组 Fold（业务架构调整）不破坏 Ontology 定义
- 跨 Project 关联 = 未来的 Federation 概念（P2）

数据库已天然满足此设计：`entity_types`、`ontology_links` 均无 `fold_id` 字段。

**ADR 文件：** `docs/arch/adr/ADR-38-fold-scope-and-cross-fold-links.md`

---

## 映射向导设计原则：自动优先，确认为辅

**背景：** 对于标准格式数据（CSV/JSON/SQL），80% 以上的字段映射可以自动完成，用户无需逐列配置。

**原则：** 系统默认全部自动映射，用户以"审查 + 排除"姿态操作：

| 自动完成 | 用户决策 |
|---------|---------|
| 列名 → 属性名（1:1 直接映射） | 业务实体名称确认 |
| 类型推断（string/number/boolean/date）| 忽略哪些列 |
| 主键识别（`id`/`_id`/`{entity}_id`）| 跨 Fold 关联确认 |
| FK 关联推断（`_id`/`_fk` 后缀匹配）| — |
| 同 Fold 内关联（高置信，默认勾选）| — |

**UX 模式改变：**
- 旧：空白起点，用户逐列添加配置
- 新：打开即全选，用户排除不需要的

---

## Bug Fix：S3 多文件同步只创建一个 Dataset

**现象：** hr_ds 同步了 employees.csv / orders.json / products.csv 三个文件，但 Ontology Schema 里只有 1 条可配置的 Dataset。

**根因：** `sync_source_handler` 对 `csv` 类型的本地文件夹有 per-file 模式（每个文件一个 Dataset），但 `s3` 类型直接走 single-file 模式，将所有选中文件的数据全部写入同一个 Dataset。

**修复：** 在 `sync_source_handler` 中为 S3/FTP 类型添加 per-file 模式。当 `selected_files.len() > 1` 时，每个文件：
- 创建独立的 Dataset（以文件名命名，如 `employees.csv`）
- 创建独立的 SyncRun 和 DatasetVersion
- 并发 spawn 独立同步任务

修复后 hr_ds 同步 3 个文件 → 生成 3 个独立 Dataset → Ontology Schema 可分别配置映射。

**对称性：** S3 per-file 模式与 CSV folder 模式逻辑完全对称，用同一个 `mode: "folder"` + `job_ids` 响应结构。

---

## 整体进展评估（截止 2026-03-21 下午）

| 层 | 完成度 | 说明 |
|----|--------|------|
| 数据模型 | 85% | 架构已正确，待加 raw/typed 过滤 |
| API | 60% | 核心完整，Link Type 独立 CRUD 缺失 |
| UI | 40% | project_workspace.html 已写，待 rebuild；Fold 分组/跨 Fold 推断未做 |

**下一步 P0：**
1. `cargo build` + 重启（映射向导新 HTML 未生效）
2. `list_dataset_records` 加 `sync_run_id != 'promote'` 过滤

**下一步 P1：**
- Fold 分组展示 DataSources
- 映射向导 Step 3 关联推断按 Fold 边界分层

---

## Dataset Sync Mode 设计（晚）

### 问题

同一数据源第二批数据推送时，和第一批是什么关系？系统需要明确的数据管理语义。

### 现有基础

`dataset_versions` 表已提供版本控制（v1/v2/v3 各自独立 manifest，不物理覆盖），`schema_change` 检测兼容/破坏性变更，rollback/gc 已有。

**缺失**：Sync Mode 声明——新批次数据是替换、追加还是合并？

### Palantir Transaction Type 参考

SNAPSHOT（替换）/ APPEND（追加）/ UPDATE（upsert）三种事务类型，写入时显式声明。

### 我们的设计

`data_sources.config` 增加 `sync_mode` 字段：
- `snapshot`（默认，已实现）：全量替换 current version
- `append`（P1）：新批次追加，历史行保留
- `upsert`（P1）：按主键合并，适合状态同步

Schema 演变处理：compatible 变更静默接受，breaking 变更阻止自动 promote 要求用户确认。

**ADR 待写**：Dataset Sync Mode（P1 阶段再正式落地）
