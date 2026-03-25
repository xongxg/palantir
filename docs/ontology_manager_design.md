# Ontology Manager — 详细设计方案

> **文档版本**：v0.1
> **状态**：草稿
> **日期**：2026-03-25
> **受众**：后端团队、前端团队、产品

---

## 一、背景与目标

### 1.1 现状

系统当前已具备 Ontology 的核心能力：

- 通过 Import tab 将 Dataset 的 Schema 提升（Promote）为 Entity Type
- 在 Schema tab 管理 Entity Type 及其字段
- 通过 FK 列配置 Relationship（Link Type）
- Entity Type 按 fold（BC）组织

但随着 Ontology 数据量增长，**管理层能力缺失**已成为主要风险：

| 风险 | 当前状态 | 后果 |
|------|---------|------|
| 字段类型被静默修改 | 无任何保护 | 历史 Ontology 对象与新 Schema 不一致 |
| 无法判断 ET 是否可用 | 没有状态概念 | 未完成的 Schema 会被数据写入 |
| 无法知道一个 ET 的数据从哪来 | 无血缘展示 | 排障困难，影响信任度 |
| 跨 BC 共享概念无处放 | 无 Shared Kernel | 各 BC 各自定义 Customer，语义分裂 |
| 技术公共字段反复定义 | 无 Interface 机制 | `created_at` 在每个 ET 里单独加 |

### 1.2 目标

建设一套 **DDD 融合的 Ontology Manager**，具备：

1. **Schema 安全**：变更有保护，不会静默破坏历史数据
2. **生命周期管理**：ET 有状态，draft → active → deprecated
3. **数据血缘**：每个 ET 清楚地知道自己的数据从哪来
4. **Shared Kernel**：跨 BC 的核心业务概念有统一的归处和治理
5. **System Interface**：技术公共字段有约定，不重复定义

### 1.3 与 Palantir 的差异定位

Palantir Foundry 有 Ontology Manager，但其 Interface Type 借用了 Java OOP 的结构契约思维，将"业务共识"和"技术复用"两种性质不同的需求混在一个机制里处理。

我们的差异化：**两层分离**

```
Palantir Interface Type
    └── 混合了：业务共识 + 技术复用

我们的方案
    ├── Shared Kernel fold  →  业务语义层（DDD）
    │     跨 BC 的核心业务概念，变更需多 BC 协商
    └── System Interface    →  技术约定层
          Auditable / Identifiable / Versioned 等字段契约
```

这是本系统在 Ontology 管理领域的核心差异化设计。

---

## 二、整体架构

### 2.1 四层概念模型

```
┌──────────────────────────────────────────────────────────────┐
│  Layer 4: Action Type                                        │
│  对 ET 定义结构化写操作，附带参数/校验/审计/Webhook           │
│  → 所有写操作必须经过 Action，不允许直接修改对象              │
├──────────────────────────────────────────────────────────────┤
│  Layer 3: Shared Kernel（业务语义层）                         │
│  fold_type = 'shared_kernel'，全局可见                       │
│  核心业务概念：Customer / Product / Contract / Employee       │
│  + Context Map：BC 间关系可视化                              │
├──────────────────────────────────────────────────────────────┤
│  Layer 2: System Interface（技术约定层）                      │
│  Auditable / Identifiable / Versioned / Locatable            │
│  ET 实现 Interface → 字段自动校验                            │
├──────────────────────────────────────────────────────────────┤
│  Layer 1: Entity Type + Relationship（本体核心）              │
│  Schema 定义、字段类型、关联关系、fold 归属、状态生命周期      │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 实现路线图

```
Phase   内容                              优先级   交付物
─────────────────────────────────────────────────────────────
P0      Schema 变更保护                   立即     breaking change 检测 + 迁移策略 UI
P1a     ET 状态生命周期                   近期     draft/active/deprecated + UI 标记
P1b     数据血缘视图                      近期     Schema tab ET 详情血缘面板
P2a     Shared Kernel fold               中期     fold_type + 全局可见 + 协商确认
P2b     Context Map 可视化               中期     BC 关系图谱
P2c     System Interface                 中期     interfaces 表 + ET 实现 + 字段校验
P2d     Action Type                      长期     写操作治理框架
```

---

## 三、P0 — Schema 变更保护

### 3.1 问题描述

当前在 Schema tab 修改 Entity Type 的字段（改类型、删字段）时，操作直接生效，没有任何保护。此时：

- 已有的 Ontology 对象字段值与新 Schema 不兼容
- 下次 Promote 时用新 Schema 覆盖，历史数据静默丢失或类型错误

### 3.2 Breaking Change 判定规则

以下操作判定为 Breaking Change：

| 操作 | 触发条件 | 原因 |
|------|---------|------|
| 字段类型变更 | 任何情况 | 历史值与新类型不兼容 |
| 字段删除 | 已有 Ontology 对象时 | 历史值丢失 |
| 主键列变更 | 任何情况 | 对象唯一标识混乱 |
| 字段 ID/名称修改 | 已有 Ontology 对象时 | 已有对象引用失效 |

非 Breaking Change（可直接保存）：
- 新增字段
- 修改字段描述/展示名
- 修改字段可见性

### 3.3 迁移策略

用户在确认框里选择一种迁移策略后才能保存：

| 策略 | 说明 | 适用场景 |
|------|------|---------|
| **Drop** | 丢弃所有已有对象的该字段值 | 字段废弃不要了 |
| **Cast** | 类型兼容转换 | `string → integer`（若值能转换） |
| **Rename** | 将旧字段值迁移到新字段名 | 字段改名但数据保留 |

### 3.4 数据库变更

```sql
-- 记录迁移历史（可选，供审计）
CREATE TABLE schema_migrations (
    id           TEXT PRIMARY KEY,
    et_id        TEXT NOT NULL REFERENCES entity_types(id),
    field_name   TEXT NOT NULL,
    change_type  TEXT NOT NULL,  -- 'type_change' | 'delete' | 'rename' | 'pk_change'
    old_value    TEXT,           -- 旧类型 / 旧字段名
    new_value    TEXT,           -- 新类型 / 新字段名
    strategy     TEXT NOT NULL,  -- 'drop' | 'cast' | 'rename'
    affected_count INTEGER,      -- 受影响的 Ontology 对象数量
    applied_by   TEXT,
    applied_at   TEXT NOT NULL
);
```

### 3.5 API 变更

```
// 现有接口，新增前置检查
PUT /api/ontology/fields/:id
  Request: { field_type, ... }
  新增响应字段：
  {
    "breaking": true,
    "affected_count": 1234,
    "change_type": "type_change",
    "old_type": "string",
    "new_type": "integer",
    "strategies": ["drop", "cast"]
  }
  当 breaking=true 时，前端必须再次携带 strategy 参数才会真正执行。

DELETE /api/ontology/fields/:id
  新增响应字段：
  {
    "breaking": true,
    "affected_count": 1234,
    "strategies": ["drop"]
  }
```

### 3.6 UI 交互

```
用户点击"保存字段类型修改"
  │
  ▼
后端检测到 breaking change
  │
  ▼
弹出确认框：
  ┌─────────────────────────────────────────────────────┐
  │  ⚠️  Breaking Change 警告                           │
  │                                                     │
  │  将字段 "age" 从 string 改为 integer                │
  │  当前有 1,234 条 Ontology 对象包含此字段             │
  │                                                     │
  │  请选择迁移策略：                                   │
  │  ○ Drop  — 丢弃所有已有对象的 age 字段值            │
  │  ○ Cast  — 尝试将字段值转换为 integer（失败则为空） │
  │                                                     │
  │  [取消]                      [确认执行迁移]         │
  └─────────────────────────────────────────────────────┘
```

### 3.7 验收标准

- [ ] 修改字段类型，后端返回 `breaking: true` 且包含 `affected_count`
- [ ] 前端显示确认弹框，未选策略时无法提交
- [ ] 选 Drop 后，已有 Ontology 对象中该字段被清空
- [ ] 选 Cast 后，能转换的值转换，不能转换的置为 null
- [ ] 新增字段直接保存，不触发确认框
- [ ] 迁移记录写入 `schema_migrations` 表

---

## 四、P1a — ET 状态生命周期

### 4.1 状态定义

```
draft ──► active ──► deprecated
```

| 状态 | 含义 | 允许的操作 |
|------|------|-----------|
| `draft` | 设计阶段，Schema 未完成 | 可编辑字段；不允许 Promote 数据写入 |
| `active` | 正式可用 | 正常使用；字段变更需 breaking change 检查 |
| `deprecated` | 下线中 | 只读；已有数据保留；Promote 时警告；不允许新建同名 ET |

状态转换规则：
- `draft → active`：手动确认（"发布此 Entity Type"）
- `active → deprecated`：手动操作，需确认影响范围（N 个 Dataset 在喂入）
- `deprecated → active`：允许（恢复）
- 不允许删除有历史数据的 active/deprecated ET（只能 deprecated）

### 4.2 数据库变更

```sql
-- entity_types 表新增字段
ALTER TABLE entity_types ADD COLUMN status TEXT NOT NULL DEFAULT 'active';
-- status: 'draft' | 'active' | 'deprecated'
```

### 4.3 API 变更

```
// 状态变更接口
PUT /api/ontology/schema/:id/status
  Request: { "status": "active" | "deprecated" | "draft" }
  Response: { "ok": true, "affected_datasets": 3 }
  // deprecated 时返回受影响的 dataset 数量供确认

// Promote 接口新增前置校验
POST /api/datasets/:id/promote
  // 当 entity_type_id 对应的 ET 状态为 draft 时：
  Response: { "ok": false, "error": "entity type is in draft status" }
  // 当状态为 deprecated 时：
  Response: { "ok": false, "error": "entity type is deprecated" }
```

### 4.4 UI 变更

- Schema tab ET 列表：每个 ET 名称旁显示状态徽标（`draft` 灰色、`active` 绿色、`deprecated` 橙色）
- ET 详情右上角：状态操作按钮（"发布" / "废弃" / "恢复"）
- Import tab 的 ET 下拉选择器：过滤掉 `deprecated` 状态的 ET，`draft` 状态的 ET 显示但禁用

### 4.5 验收标准

- [ ] 新建 ET 默认 `active`（兼容现有数据）；可在创建时选择 `draft`
- [ ] `draft` 状态的 ET，Promote 时被拒绝，返回明确错误信息
- [ ] `deprecated` 状态的 ET，Promote 时被拒绝
- [ ] 废弃操作前显示"N 个 Dataset 正在映射此类型"的影响确认
- [ ] ET 状态变更记录在 `schema_migrations` 表（`change_type: 'status_change'`）

---

## 五、P1b — 数据血缘视图

### 5.1 目标

在 Schema tab 的 ET 详情页里，展示"哪些数据集在为这个 Entity Type 提供数据"，建立 Dataset → ET 的可见血缘。

### 5.2 数据来源

现有数据库已有完整链路，无需新增表：

```
dataset_mappings.entity_type_id
  → datasets.source_id
    → data_sources.fold_id / name / source_type
      → folds.name（BC 名称）
```

### 5.3 API

```
// 新增接口
GET /api/ontology/schema/:et_id/lineage
Response:
{
  "entity_type_id": "et_abc",
  "sources": [
    {
      "dataset_id": "ds_001",
      "dataset_name": "customers_2024.csv",
      "source_id": "src_001",
      "source_name": "CRM系统",
      "source_type": "s3",
      "fold_id": "fold_sales",
      "fold_name": "Sales BC",
      "record_count": 12450,
      "last_synced_at": "2026-03-24T10:30:00Z",
      "sync_mode": "upsert",
      "primary_key_col": "customer_id"
    }
  ],
  "total_records": 12450
}
```

### 5.4 UI

Schema tab → ET 详情 → 新增"数据来源"折叠面板：

```
数据来源 (3 个 Dataset)                              [展开 ▼]
─────────────────────────────────────────────────────────────
  📄 customers_2024.csv          Sales BC · CRM系统
     12,450 条 · upsert · 最后同步 2026-03-24

  📄 customer_export.xlsx        Finance BC · ERP系统
     8,200 条 · snapshot · 最后同步 2026-03-23

  📄 api_customers               Shared Kernel · REST API
     5,100 条 · append · 最后同步 2026-03-24
─────────────────────────────────────────────────────────────
  合计：25,750 条 Ontology 对象
```

### 5.5 验收标准

- [ ] GET lineage 接口返回正确的数据源列表
- [ ] 无映射的 ET 返回空 sources 数组
- [ ] 同一 ET 有多个 Dataset 喂入时全部展示
- [ ] record_count 来自 dataset_versions 的最新版本

---

## 六、P2a — Shared Kernel（业务语义层）

### 6.1 设计原则

Shared Kernel 是 DDD 中的一个战术模式：

> 两个或多个 BC 明确约定，共同拥有、共同维护某一部分领域模型。
> 任何一方的变更都必须与所有共同拥有方协商。

在我们的系统中，Shared Kernel 表现为一个特殊的 fold：全局可见，变更有多 BC 协商机制。

### 6.2 数据库变更

```sql
-- folds 表新增字段
ALTER TABLE folds ADD COLUMN fold_type TEXT NOT NULL DEFAULT 'bc';
-- fold_type: 'bc' | 'shared_kernel'

-- BC 与 Shared Kernel 的显式引用关系（用于 Context Map）
CREATE TABLE bc_shared_kernel_refs (
    bc_fold_id        TEXT NOT NULL REFERENCES folds(id) ON DELETE CASCADE,
    sk_entity_type_id TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL DEFAULT 'conformist',
    -- conformist: BC 完全遵从 SK 定义
    -- customer_supplier: BC 影响 SK 的演进方向
    created_at        TEXT NOT NULL,
    PRIMARY KEY (bc_fold_id, sk_entity_type_id)
);
```

### 6.3 Shared Kernel fold 的行为规则

| 场景 | 行为 |
|------|------|
| 创建 Shared Kernel fold | 仅平台管理员可操作（初期：无角色系统则任意人可创建，但 UI 有显著提示） |
| Shared Kernel ET 在 Import | 全局 ET 选择器中可见，不限于当前 project |
| 修改 Shared Kernel ET 字段 | 显示协商警告："此 ET 被 N 个 BC 引用，变更需与相关团队确认" |
| 删除 Shared Kernel ET | 强制校验：存在引用的 BC 数量 > 0 时，需显式确认或先解除引用 |

### 6.4 API 变更

```
// 创建 fold 时支持 fold_type
POST /api/projects/:project_id/folds
  Request: { "name": "Core Domain", "fold_type": "shared_kernel" }

// 列出全局可见的 Shared Kernel folds
GET /api/shared-kernels
  Response: { "folds": [...], "entity_types": [...] }

// 查询某 ET 被哪些 BC 引用（用于协商警告）
GET /api/ontology/schema/:et_id/bc-refs
  Response: { "refs": [{ "bc_fold_id", "bc_name", "relationship_type" }] }

// BC 声明引用某 Shared Kernel ET
POST /api/bc-shared-kernel-refs
  Request: { "bc_fold_id": "...", "sk_entity_type_id": "...", "relationship_type": "conformist" }
```

### 6.5 Context Map 可视化

在 Graph tab 或独立 "Context Map" 面板中展示：

```
┌─────────────┐   conformist    ┌─────────────────────┐
│  Sales BC   │ ──────────────► │  Shared Kernel      │
└─────────────┘                 │  ● Customer         │
                                │  ● Product          │
┌─────────────┐   conformist    │  ● Contract         │
│  Finance BC │ ──────────────► └─────────────────────┘
└─────────────┘

┌─────────────┐ customer/supplier ┌─────────────┐
│  Sales BC   │ ─────────────────► │ Inventory BC│
└─────────────┘                    └─────────────┘
```

### 6.6 验收标准

- [ ] 创建 fold 时可选 `fold_type: shared_kernel`，UI 有特殊图标标识
- [ ] Shared Kernel fold 中的 ET 在所有 project 的 Import/Schema 中可见
- [ ] 修改 Shared Kernel ET 字段时，弹出包含受影响 BC 列表的协商确认框
- [ ] GET /api/shared-kernels 返回所有 shared_kernel fold 及其 ET
- [ ] Context Map 面板正确展示 BC 间引用关系

---

## 七、P2c — System Interface（技术约定层）

### 7.1 设计原则

System Interface 解决纯技术层面的问题：多个 ET 都需要 `created_at`、`updated_at` 这类字段，不应每次手动添加，也不应各自为政。

与 Shared Kernel 的区别：
- Interface 是技术契约，没有业务语义，不需要跨团队协商
- Interface 的"实现"是声明式的，系统自动校验字段是否存在

### 7.2 数据库变更

```sql
CREATE TABLE interfaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,
    description TEXT,
    is_builtin   INTEGER NOT NULL DEFAULT 0,  -- 内置 Interface 不可删除
    created_at  TEXT NOT NULL
);

CREATE TABLE interface_fields (
    interface_id TEXT NOT NULL REFERENCES interfaces(id) ON DELETE CASCADE,
    field_name   TEXT NOT NULL,
    field_type   TEXT NOT NULL,
    required     INTEGER NOT NULL DEFAULT 1,
    description  TEXT,
    PRIMARY KEY (interface_id, field_name)
);

CREATE TABLE entity_type_interfaces (
    et_id        TEXT NOT NULL REFERENCES entity_types(id) ON DELETE CASCADE,
    interface_id TEXT NOT NULL REFERENCES interfaces(id) ON DELETE CASCADE,
    PRIMARY KEY (et_id, interface_id)
);
```

### 7.3 内置 Interface（系统预置，不可删除）

| Interface | 字段 | 类型 | 含义 |
|-----------|------|------|------|
| `Auditable` | `created_at` | datetime | 创建时间 |
| | `updated_at` | datetime | 最后更新时间 |
| | `updated_by` | string | 最后更新者 |
| `Identifiable` | `id` | string | 业务标识 |
| | `name` | string | 展示名 |
| | `external_id` | string | 外部系统 ID |
| `Versioned` | `version` | integer | 版本号 |
| | `valid_from` | datetime | 有效开始时间 |
| | `valid_to` | datetime | 有效结束时间 |
| `Locatable` | `latitude` | float | 纬度 |
| | `longitude` | float | 经度 |
| | `address` | string | 地址 |

### 7.4 ET 实现 Interface 的效果

1. **Schema tab 展示**：ET 详情显示"实现的 Interface"列表，每个 Interface 展开后可见其字段要求
2. **字段预填**：选择实现某 Interface 后，其字段自动添加到 ET 的 Schema（如果不存在）
3. **Promote 校验**：Promote 时检查实现了 `Auditable` 的 ET，其数据中是否包含 `created_at` 字段；不包含时给出警告（不阻断）
4. **全局一致性**：通过 Interface，保证跨 BC 的技术字段命名统一

### 7.5 API

```
// Interface CRUD
GET    /api/interfaces                         -- 列出所有 Interface
POST   /api/interfaces                         -- 创建自定义 Interface
DELETE /api/interfaces/:id                     -- 删除（内置不可删）

// ET 与 Interface 的关联
GET    /api/ontology/schema/:et_id/interfaces  -- 查询某 ET 实现的 Interface
POST   /api/ontology/schema/:et_id/interfaces  -- ET 实现某 Interface
       Request: { "interface_id": "..." }
DELETE /api/ontology/schema/:et_id/interfaces/:interface_id

// 初始化内置 Interface（启动时自动执行）
POST   /api/interfaces/seed-builtins
```

### 7.6 验收标准

- [ ] 系统启动时自动写入 4 个内置 Interface（Auditable / Identifiable / Versioned / Locatable）
- [ ] ET 可以关联多个 Interface
- [ ] 关联 Interface 后，其字段自动出现在 Schema 编辑器（如已存在则跳过）
- [ ] Promote 时对实现了 Interface 的 ET 做字段存在性校验，缺少必填字段时给出警告
- [ ] 自定义 Interface 可以创建和删除；内置 Interface 不可删除
- [ ] Schema tab ET 详情展示已实现的 Interface 列表

---

## 八、P2d — Action Type（写操作治理）

> 本节为长期规划，仅做方向性设计，不做实现细节约束。

### 8.1 核心思想

参考 Palantir Action Type 设计：**所有写操作必须经过定义好的 Action，不允许直接操作 Ontology 对象**。

这使得：
- 写操作有明确的参数约束，不允许随意改任意字段
- 每次写操作都有完整审计记录
- 写操作可以触发副作用（通知、Webhook、Pipeline）

### 8.2 数据模型方向

```
action_types               -- 操作类型定义（如"更新客户状态"）
  id, name, et_id, description

action_parameters          -- 操作的输入参数定义
  action_id, name, type, required, default_value

action_rules               -- 参数到字段变更的映射规则
  action_id, parameter_name, target_field, rule_expr

action_submissions         -- 操作执行历史（审计链）
  id, action_type_id, submitted_by, submitted_at, parameters_json, result
```

### 8.3 与 Shared Kernel 的结合

Shared Kernel 的 ET 通常需要 Action Type 的保护：
- 跨 BC 共享的核心业务对象（如 Customer），不应由任意 BC 直接修改字段
- 应通过定义好的 Action（如"更新客户联系方式"）来约束写路径

---

## 九、数据库变更总览

| Phase | 表 | 变更类型 | 变更内容 |
|-------|----|---------|---------|
| P0 | `schema_migrations` | 新建 | 记录字段变更历史和迁移策略 |
| P1a | `entity_types` | 加字段 | `status TEXT DEFAULT 'active'` |
| P2a | `folds` | 加字段 | `fold_type TEXT DEFAULT 'bc'` |
| P2a | `bc_shared_kernel_refs` | 新建 | BC 与 Shared Kernel ET 的引用关系 |
| P2c | `interfaces` | 新建 | Interface 定义 |
| P2c | `interface_fields` | 新建 | Interface 的字段要求 |
| P2c | `entity_type_interfaces` | 新建 | ET 与 Interface 的多对多 |
| P2d | `action_types` 等 | 新建 | Action Type 框架（长期） |

所有新增字段均向后兼容，现有数据无需迁移。

---

## 十、多数据源接入治理

> **版本**：v0.1（2026-03-25）
> **状态**：已实现（前端字段集比对 + 自动切换同步模式）

### 10.1 问题背景

同一个 Entity Type 可能有多个物理数据源同时 promote（主备切换、新旧系统迁移、主表 + 扩展表等场景）。
若全部使用默认的 **Snapshot（全量替换）** 模式，第二个数据源 promote 后会清空第一个数据源写入的所有 Ontology 对象，导致数据丢失。

### 10.2 多源关系分类

系统在 Promote 页面选择 ET 时，自动比对当前数据集的字段集与 ET 已有字段集，将关系分为三类：

| 关系类型 | 判断条件 | 语义 | 建议同步模式 |
|---------|---------|------|------------|
| **重复源** | 当前字段集 ⊆ ET 已有字段，无新字段 | 主备、迁移、数据复制 | Upsert |
| **增强源** | 当前字段集与 ET 部分重叠，有新字段 | 主表 + 扩展表，补充维度信息 | Upsert |
| **互补源** | 当前字段集与 ET 已有字段完全不重叠 | 完全独立的补充数据 | Upsert |

三种情况均推荐 **Upsert**（按主键合并），原因：
- 相同主键行：更新字段值，不产生重复
- 新主键行：直接插入
- 多源数据在同一 ET 下通过主键自然聚合

### 10.3 UI 行为

1. **检测时机**：选择 ET（下拉框）时、切换同步模式时、打开数据集时，自动触发检测
2. **自动切换**：若当前同步模式为 Snapshot 且检测到多源，自动切换为 Upsert
3. **用户通知**：在 Schema 发现区域显示带颜色 badge 的提示横幅：
   - ⚠️ **重复源**：橙色提示，说明字段完全重合，建议确认是否真的需要两个源
   - ℹ️ **增强源**：信息提示，列出新增字段名称
   - ℹ️ **互补源**：信息提示，说明字段不重叠，合并安全
4. **用户可覆盖**：自动切换后用户仍可手动改回其他模式

### 10.4 数据模型支持

`ontology_objects` 表上的 `source_ids TEXT[]` 字段（JSON 数组）记录所有贡献过该对象的 `dataset_id`，支持：
- 字段级来源追溯（哪个字段来自哪个数据源）
- 多源下的血缘展示（P1）
- 数据源下线时的影响分析（P2）

### 10.5 后续演进

| 阶段 | 功能 | 依赖 |
|------|------|------|
| P0（已实现） | 字段集比对 + 自动切换 Upsert + 三类 badge 提示 | — |
| P1 | ET 上标记 Primary Source（权威源），其他源自动降级为 Enrichment | ET 状态系统 |
| P1 | 字段级来源展示：每个字段旁显示 `来自: departments.csv` | ontology_objects.source_ids 扩展 |
| P2 | 数据源下线时分析哪些 ET 字段受影响 | P1 字段级血缘 |

---

## 十一、开放问题

| 问题 | 当前决策 | 待确认 |
|------|---------|-------|
| 谁有权创建 Shared Kernel fold？ | 初期任意用户，UI 有提示 | 后续加角色系统后收紧 |
| Schema 迁移（Cast）失败时如何处理？ | 转换失败的值置为 null | 是否需要失败报告？ |
| ET 状态默认值？ | `active`（兼容现有数据） | 新建 ET 是否强制从 draft 开始？ |
| Context Map 是否需要手动维护？ | 手动声明 BC 间引用关系 | 未来是否自动推断？ |
| Interface 字段校验是警告还是阻断？ | Promote 时警告，不阻断 | 是否需要配置为阻断？ |
| Primary Source 由谁标记？ | 暂无，P1 设计 | 系统自动推断还是用户手动指定？ |
