# Bounded Context（BC）独立建模设计

> **文档版本**：v0.5
> **状态**：草稿·待确认
> **日期**：2026-03-25
> **关联文档**：
> - `ontology_manager_design.md` — Ontology Manager 功能设计
> - `ontology_ai_architecture.md` — 整体架构思考

---

## 一、问题：Fold ≠ BC

### 1.1 当前模型的局限

现在系统中 **fold = BC（1:1）**：每个 fold 在图中渲染为一个 hull，fold 内所有 ET 属于同一个 BC。

这个映射在小规模时成立，但随着 Ontology 增长，问题暴露：

```
Sales Fold（当前：一个 hull，一个 BC）
  ├── Order
  ├── OrderItem
  ├── Invoice
  ├── Customer        ← 语义上应该是独立 BC
  ├── CustomerAddress ← 语义上应该属于 Customer BC
  ├── Product         ← 语义上应该是独立 BC
  └── Category        ← 语义上应该属于 Product BC
```

- **Order / OrderItem / Invoice** 围绕"交易"凝聚 → Order Management BC
- **Customer / CustomerAddress** 围绕"客户"凝聚 → Customer BC
- **Product / Category** 围绕"商品"凝聚 → Product Catalog BC

这三个语义凝聚域内聚力强、彼此松耦合，是 DDD 意义上的独立 Bounded Context。
把它们压进同一个 BC hull，语义信息丢失。

### 1.2 正确的 DDD 层次

```
Workspace（公司）
  └── Project（部门）
        └── Fold（小组）
              └── Bounded Context（语义凝聚域）  ← 当前缺失的一层
                    └── Entity Type
                          └── Entity Field
```

| 层级 | 组织类比 | 语义含义 |
|------|---------|---------|
| **Workspace** | 公司 | 数据隔离边界；跨 Workspace = 外部系统 |
| **Project** | 部门 | 业务职能单元；跨 Project = 跨部门协作 |
| **Fold** | 小组 | 共享语义契约的最小团队单元 |
| **BC** | 小组内的语义域 | 内聚的领域概念集合；BC 不跨 Fold |
| **ET** | 具体业务实体 | 领域对象的类型定义 |

**关系性质随层级自然分级：**

```
同一 Fold 内 BC 间   → 小组内部分工，强内聚，无需正式协议
跨 Fold 同 Project  → 跨小组协作，需要 customer_supplier / conformist / acl
跨 Project          → 跨部门协作，通常走 Shared Kernel 或 open_host
```

**Shared Kernel fold** 在组织类比里 = **"共享服务团队"**：
不属于任何一个部门，所有部门都可以引用，变更需全公司协商。

---

## 二、设计方案 B：独立 `bounded_contexts` 表

### 2.1 数据模型

```sql
-- 新表：BC 一等公民
CREATE TABLE bounded_contexts (
    id            TEXT PRIMARY KEY,
    fold_id       TEXT NOT NULL REFERENCES folds(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    color         TEXT NOT NULL DEFAULT '#6366f1',
    description   TEXT,
    auto_detected INTEGER NOT NULL DEFAULT 1,  -- 1=系统推断, 0=手动创建
    created_at    TEXT NOT NULL,
    UNIQUE(fold_id, name)
);

-- entity_types 新增 bc_id（可空，向后兼容）
ALTER TABLE entity_types ADD COLUMN bc_id TEXT REFERENCES bounded_contexts(id);
ALTER TABLE entity_types ADD COLUMN fold_id TEXT REFERENCES folds(id);
-- 注：bc_id 和 fold_id 都存，bc_id 优先；bc_id 为空时回退到 fold 级 BC
```

**向后兼容策略**：
- 现有 ET 的 `bc_id = NULL`，图中回退为 fold 级 BC（现有行为不变）
- 迁移脚本可批量跑自动检测，为存量 ET 补充 `bc_id`

### 2.2 概念关系

```
folds
  └── bounded_contexts (fold_id → folds.id)
        └── entity_types (bc_id → bounded_contexts.id)
              └── entity_fields
```

Fold 和 BC 各司其职：
- **fold**：`data_sources.fold_id` 仍然决定数据归属，不变
- **bc**：纯语义边界，不参与数据摄入链路

---

## 三、BC 自动检测算法

### 3.1 触发时机

1. **首次 promote** 一个 fold 内的 ET 时（fold 内 ET 数量 ≥ 2）
2. **用户主动触发**："重新分析 BC 边界"按钮
3. **新 ET 加入已有 fold** 时，增量计算其归属建议

### 3.2 多信号融合

```
信号 1 — FK 结构拓扑（权重最高）
  在同一 fold 内，对 entity_types 之间的 FK 关系（link_type_mappings / ontology_links）
  构建无向图，运行 Union-Find 连通分量算法
  → 每个连通分量 = 一个候选 child-BC
  → 孤立 ET（无 FK 连接）= 独立候选 BC

信号 2 — DDD 角色（辅助命名）
  在每个连通分量内，找 Aggregate Root（入度最高的 ET）
  → 以 Aggregate Root 的名字作为该 child-BC 的默认名称
  e.g. Order 连通分量 → "Order Management"

信号 3 — 字段名语义（P2，需 embedding）
  将 ET 的表名+字段名向量化，在 fold 内做 K-means 聚类
  与信号1结果取交集，作为置信度加分项

综合：信号1确定分组，信号2确定名称，信号3验证置信度
```

### 3.3 算法伪代码

```rust
fn detect_child_bcs(fold_id: &str, entity_types: &[ET], links: &[FKLink]) -> Vec<SuggestedBC> {
    // 1. 只保留同一 fold 内的 ET 和它们之间的 FK 链接
    let fold_ets: Set<ET> = entity_types.filter(et => et.fold_id == fold_id);
    let intra_links: Vec<FKLink> = links.filter(l =>
        fold_ets.contains(l.from_et) && fold_ets.contains(l.to_et)
    );

    // 2. Union-Find 求连通分量
    let mut uf = UnionFind::new(&fold_ets);
    for link in intra_links {
        uf.union(link.from_et, link.to_et);
    }
    let components: Vec<Set<ET>> = uf.components();

    // 3. 每个连通分量 → 一个候选 BC
    components.map(|component| {
        // 找 Aggregate Root（入度最高）
        let agg_root = component.max_by(|et| inbound_degree(et, intra_links));
        SuggestedBC {
            name: suggest_bc_name(agg_root),
            et_ids: component.map(|et| et.id),
            confidence: calc_confidence(component, intra_links),
            ddd_roles: classify_roles(component, intra_links),
        }
    })
}
```

### 3.4 置信度计算

```
置信度 = 连通分量内部 FK 密度 / (内部密度 + 跨组件 FK 数)

内聚高、耦合低 → 置信度高（≥ 0.8）
内聚低、耦合高 → 置信度低（< 0.5），提示用户谨慎确认
```

---

## 四、用户交互流程

### 4.1 BC 推断确认界面

```
Promote 完成后（或用户点击"分析 BC 边界"）

┌──────────────────────────────────────────────────────────────┐
│  系统检测到 Sales Fold 内可能存在 3 个 Bounded Context        │
│                                                              │
│  ┌── 建议 BC 1 ──────────────── 置信度：92% ──┐             │
│  │  Order Management                          │             │
│  │  ● Order（Aggregate Root）                 │             │
│  │  ● OrderItem  ● Invoice                   │  [接受] [拆] │
│  └────────────────────────────────────────────┘             │
│                                                              │
│  ┌── 建议 BC 2 ──────────────── 置信度：88% ──┐             │
│  │  Customer                                  │             │
│  │  ● Customer（Aggregate Root）              │  [接受] [拆] │
│  │  ● CustomerAddress                        │             │
│  └────────────────────────────────────────────┘             │
│                                                              │
│  ┌── 建议 BC 3 ──────────────── 置信度：76% ──┐             │
│  │  Product Catalog                           │             │
│  │  ● Product（Aggregate Root）               │  [接受] [拆] │
│  │  ● Category                               │             │
│  └────────────────────────────────────────────┘             │
│                                                              │
│   [全部接受]   [手动调整]   [忽略，保持 Fold 级 BC]           │
└──────────────────────────────────────────────────────────────┘
```

### 4.2 手动调整操作

| 操作 | 说明 |
|------|------|
| 接受建议 | 按推断结果创建 child-BC，ET 自动绑定 bc_id |
| 重命名 | 修改 BC 名称（不影响 ET 归属） |
| 拆分 | 把一个 BC 内的某个 ET 拆出，单独成为新 BC |
| 合并 | 把两个建议 BC 合并成一个 |
| 移动 ET | 把某个 ET 从 BC-A 拖到 BC-B |
| 忽略 | 不分 child-BC，整个 fold 仍为一个大 BC |

---

## 五、图可视化变化

### 5.1 两级 Hull

```
渲染顺序（从底到顶）：
  1. Fold hull（最外层）：浅色填充 + 实线边框，fill-opacity: 0.06
  2. Child-BC hull（内层）：各 BC 独立颜色，fill-opacity: 0.13，实线边框
  3. 节点层
  4. 边层
  5. BC cross-link 弧线层
```

**视觉规则**：
- 同一 fold 内的 child-BC 使用 fold 颜色的不同深度/色调变体（避免颜色噪音）
- 跨 fold 的 child-BC 使用 BC_PALETTE 不同颜色
- Fold hull 的标签放在左上角，child-BC 标签放在 hull 内部左上角（小字）
- 跨 child-BC 的 FK 边 = 橙色（BC 耦合信号）
- 同一 child-BC 内的 FK 边 = 蓝色（内聚信号）

### 5.2 buildBCsFromFolds 改造方向

```javascript
// 当前：一个 fold → 一个 BC
folds.map(f => ({ id: f.id, name: f.name, ids: [...], isFold: true }))

// 目标：两层
folds.map(f => ({
  id: f.id, name: f.name, ids: [...], isFold: true,   // fold 外层 hull
  childBCs: childBCsForFold(f, nodes, apiChildBCs),    // child-BC 内层 hulls
}))

// childBCsForFold:
//   优先用后端返回的 bounded_contexts（用户已确认的）
//   若无，则前端实时跑连通分量算法（草图模式）
```

---

## 六、跨 Fold BC 关系（Context Map）

### 6.1 完整覆盖方案

跨 Fold 的 BC 关系统一用一张 `bc_relationships` 表表达，覆盖 DDD Context Map 全部 5 种模式：

```sql
CREATE TABLE bc_relationships (
    id                TEXT PRIMARY KEY,
    from_bc_id        TEXT NOT NULL REFERENCES bounded_contexts(id) ON DELETE CASCADE,
    to_bc_id          TEXT NOT NULL REFERENCES bounded_contexts(id) ON DELETE CASCADE,
    relationship_type TEXT NOT NULL,
    -- 'shared_kernel'     : 双方共同维护同一 ET，变更需协商（最强治理约束）
    -- 'customer_supplier' : from 依赖 to，to 应考虑 from 的需求（有协商机制）
    -- 'conformist'        : from 直接遵从 to 的模型，不协商（无约束，接受变化）
    -- 'acl'               : from 有翻译层隔离 to 的变化（Ontology 层打标签，翻译在 Pipeline）
    -- 'open_host'         : to 发布稳定协议供 from 消费（未来对接 Action Type 版本管理）
    notes             TEXT,
    created_at        TEXT NOT NULL,
    UNIQUE(from_bc_id, to_bc_id, relationship_type)
);
```

### 6.2 三机制完整覆盖所有场景

```
场景                          机制
─────────────────────────────────────────────────────────
fold 内语义分组               → bounded_contexts 表（bc_id）
跨 fold 共享核心概念          → fold_type='shared_kernel' + relationship_type='shared_kernel'
跨 fold 单向依赖（有协商）    → relationship_type='customer_supplier'
跨 fold 单向依赖（无协商）    → relationship_type='conformist'
跨 fold 隔离翻译              → relationship_type='acl'（Ontology 层标签，翻译在 Pipeline）
跨 fold 发布稳定协议          → relationship_type='open_host'（未来接 Action Type）
```

**没有漏洞。** 跨 Fold 的 BC 关系被完整覆盖。

### 6.3 每种类型的 UI 行为差异

| relationship_type | 修改被依赖方 ET 时 | Context Map 显示 | 实现优先级 |
|-------------------|--------------------|-----------------|-----------|
| `shared_kernel` | 弹协商警告，列出所有共同拥有方 | 双向箭头，特殊图标 | P1 |
| `customer_supplier` | 提示"N 个 customer BC 受影响" | 单向箭头（from→to） | P1 |
| `conformist` | 无警告（downstream 接受变化） | 单向箭头，虚线 | P1 |
| `acl` | 提示"有翻译层，请检查映射" | 单向箭头 + 🛡 图标 | P2 |
| `open_host` | 提示"发布协议变更，通知消费方" | 广播图标 | P2（依赖 Action Type）|

### 6.4 与 Shared Kernel fold 的关系

`fold_type = 'shared_kernel'` 和 `relationship_type = 'shared_kernel'` 互补：
- **fold_type**：标记这个 fold 的 ET 全局可见（查询层行为）
- **relationship_type**：记录哪些 BC 显式参与共同维护（治理层行为）

一个 Shared Kernel fold 内的 ET 可以被多个 BC 通过 `shared_kernel` 关系引用，
也可以有些 BC 只是 `conformist`（遵从但不参与维护）。

---

## 七、API 设计

```
// Child-BC CRUD
GET    /api/folds/:fold_id/bcs                   -- 列出某 fold 的所有 child-BC
POST   /api/folds/:fold_id/bcs                   -- 手动创建 child-BC
PUT    /api/bcs/:bc_id                           -- 重命名 / 修改描述
DELETE /api/bcs/:bc_id                           -- 删除（ET 的 bc_id 置空）

// ET 的 BC 归属
PUT    /api/ontology/schema/:et_id/bc            -- 将 ET 归属到某个 child-BC
       Request: { "bc_id": "bc_xxx" | null }

// BC 自动推断
POST   /api/folds/:fold_id/bcs/infer             -- 触发自动推断，返回建议（不写入）
       Response: { "suggestions": [SuggestedBC] }
POST   /api/folds/:fold_id/bcs/apply-suggestions -- 接受推断结果，批量写入
       Request: { "suggestions": [SuggestedBC], "mode": "all" | "selected" }

// 跨 BC 关系（Context Map）
GET    /api/bcs/:bc_id/relationships             -- 查询某 BC 的所有跨 BC 关系
POST   /api/bc-relationships                     -- 创建跨 BC 关系
       Request: { from_bc_id, to_bc_id, relationship_type, notes }
DELETE /api/bc-relationships/:id                 -- 删除关系

// Context Map — 两个粒度
GET    /api/projects/:project_id/context-map
       -- 部门级视图：本 project 内所有 BC + 跨 fold 关系
       -- 包含跨 project 的 Shared Kernel 引用（只读，显示外部依赖）

GET    /api/context-map
       -- 公司级视图：所有 project 的 BC 作为节点
       -- 跨 project 的 bc_relationships 作为边（跨部门协作全景）
       -- Shared Kernel fold 高亮显示（共享服务团队）
```

---

## 七（续）、Link Type 与 BC 的闭环

### 核心洞察

**Link Type（关系）是 BC 边界的物理体现。** 三层视角描述的是同一件事：

```
TBox（Schema 层）   link_type_mappings    Order.customer_id → Customer ET
ABox（实例层）      ontology_links        order_001 --[HAS_CUSTOMER]--> customer_001
治理层（BC）        bc_relationships      Order BC --[customer_supplier]--> Customer BC
```

这三层不是独立的——**一条跨 BC 的 FK 定义，就是一条 bc_relationship 的物理来源。**

### 两种根本不同性质的 Link

| | Intra-BC Link | Cross-BC Link |
|--|--------------|---------------|
| 语义 | 聚合内部结构（Order → OrderItem） | BC 边界接缝（Order → Customer） |
| 耦合强度 | 强，正常 | 应最小化，只引用对方 Aggregate Root |
| 变更影响 | 仅本 BC 内部 | 影响跨 BC 协议，需治理 |
| 图中颜色 | 蓝色 | 橙色 |
| 治理约束 | 无 | 受 bc_relationships.relationship_type 约束 |

### 闭环设计：link_type_mappings 关联 bc_relationships

```sql
-- link_type_mappings 新增字段，指向对应的跨 BC 治理规则
ALTER TABLE link_type_mappings
    ADD COLUMN bc_relationship_id TEXT REFERENCES bc_relationships(id);
-- NULL  = intra-BC link（同 BC 内部，无跨域治理）
-- 非空  = cross-BC link，受对应 bc_relationship 约束
```

**一旦建立这个连接，整个模型完全闭环：**

```
bounded_contexts ←── entity_types.bc_id
      ↕
bc_relationships ←── link_type_mappings.bc_relationship_id
      ↕
ontology_links   （ABox 实例，来自 link_type_mappings 推导）
```

### 三条推论

**推论 1：从 Link Types 反向自动推断 bc_relationships**

```
系统检测到：
  link_type_mappings: Order ET（Sales Fold）→ Customer ET（Finance Fold）

→ 自动建议：Sales BC 与 Finance BC 之间存在跨 BC 依赖
→ 建议 relationship_type：customer_supplier（Order 依赖 Customer）
→ 用户确认后写入 bc_relationships，并回填 link_type_mappings.bc_relationship_id
```

**推论 2：Cross-BC Link 的治理约束检测**

```
DDD 原则：跨 BC 只应引用对方的 Aggregate Root，不应引用对方内部 Entity/Value Object

→ 系统检测：FK 指向的是对方 BC 的 Value Object？→ 警告「建议只引用 Aggregate Root」
→ relationship_type = 'acl' 时：提示「存在翻译层，不应直接建立 link type，应通过 ACL ET 中转」
→ relationship_type = 'conformist' 时：允许，但标注「遵从关系，对方变更时此 link 可能受影响」
```

**推论 3：Context Map = Link Type 的 BC 级聚合视图**

```
实例图谱（Graph tab）：ontology_object 作为节点，ontology_link 作为边
Context Map（独立视图）：BC 作为节点，link_type_mappings 的跨 BC 汇总作为边

两个视图互相导航：
  Context Map 点击某条跨 BC 边 → 展开该边对应的所有具体 link instances
  Graph tab 点击橙色边 → 跳转到 Context Map 高亮对应的 BC 关系
```

### 完整数据流

```
1. Promote departments.csv → Department ET（Sales BC）
2. 系统检测 division_id 列 → 指向 Division ET（Operations BC）
3. 自动创建 link_type_mapping: Department → Division
4. 系统提示：「检测到跨 BC 依赖，建议建立 bc_relationship」
5. 用户确认：Sales BC --[customer_supplier]--> Operations BC
6. bc_relationships 写入，link_type_mappings.bc_relationship_id 回填
7. Graph tab：Department-Division 边显示橙色
8. Context Map：Sales BC → Operations BC 显示带箭头的依赖弧线
```

---

## 八、数据库变更总览

| 变更 | 类型 | 说明 |
|------|------|------|
| `CREATE TABLE bounded_contexts` | 新建 | fold_id, name, color, auto_detected |
| `CREATE TABLE bc_relationships` | 新建 | 跨 BC 关系，5 种 relationship_type |
| `entity_types ADD COLUMN bc_id` | 加字段 | 可空，向后兼容；NULL = fold 级 BC |
| `entity_types ADD COLUMN fold_id` | 加字段 | 从 data_source 推导，promote 时写入 |
| `folds ADD COLUMN fold_type` | 加字段 | 'bc'(默认) \| 'shared_kernel' |
| `link_type_mappings ADD COLUMN bc_relationship_id` | 加字段 | 指向对应的跨 BC 治理规则；NULL = intra-BC |
| ~~`bc_shared_kernel_refs`~~ | 废弃 | 由 `bc_relationships` 统一替代 |

`bc_relationships` 天然支持跨 project（只引用 bc_id，不限 project），
无需额外表变更即可表达跨部门协作关系。

所有变更向后兼容，存量数据无需迁移。

---

## 九、BC 自动推断多阶段演进

### 核心原则（贯穿所有阶段）

```
系统计算建议 → 用户确认/修改 → 反馈改善下次建议
永远是工具，业务决策由人来做（ADR-39）
```

### 阶段划分

#### 阶段 1 — 结构拓扑（已实现）

```
算法：Union-Find + 边密度阈值
  · 统计 ET 对之间的实例边数量
  · coupling(A,B) = edges(A↔B) / min(degree_A, degree_B)
  · 阈值 0.40：强耦合才合并，弱引用不合并
  · 输出：child-BC 分组建议（前端实时计算，草图模式）

效果：
  · Default Fold 内自动识别出 3 个 child-BC
  · Employee/Assignment/Expense/Project/Office → HR 运营 BC
  · Department/Division/Contract/Vendor       → 组织+采购 BC
  · Region                                   → 参考数据 BC（单节点）
```

#### 阶段 2 — 多信号融合（P1）

```
在阶段1基础上叠加：
  Signal 1: FK 结构拓扑（阶段1结果，权重最高）
  Signal 2: DDD 角色权重
    · Aggregate Root（高入度）→ 倾向于定义 BC 边界
    · Value Object（无出向）  → 倾向于跟随引用它的 Aggregate Root
  Signal 3: 字段名约定
    · _id 后缀指向已知 ET → FK 暗示归属
    · 相似字段名模式（customer_id, client_id）→ 语义聚类提示

输出：建议 + 置信度百分比，呈现给用户确认
  e.g. "Employee BC（置信度 87%）— 理由：3 个强耦合 ET，Employee 是 Aggregate Root"
```

#### 阶段 3 — AI 语义理解（P2）

```
需要 embedding-svc：
  · 字段名 + 表名向量化（text embedding）
  · 在 embedding space 做 K-means / 层次聚类
  · 与阶段1+2结果取交集，不一致时提升警告级别

适用场景：
  · 纯 FK 结构无法区分时（两个 ET 无直接 FK 但语义相近）
  · 跨 fold 的 Shared Kernel 候选识别
```

#### 阶段 4 — 历史学习（P3）

```
存储用户每次 BC 确认/修改决策：
  bc_inference_history(fold_id, et_ids, suggested_bc, user_decision, feedback_type)
  feedback_type: 'accepted' | 'renamed' | 'split' | 'merged' | 'moved_et'

相同结构再次出现 → 自动建议（置信度：历史确认）
本质是 user_relation_hints 的 BC 版本
跨项目学习：相似行业数据模式 → 共享推断知识库（长期）
```

### 实现路线图

| Phase | 内容 | 前提 |
|-------|------|------|
| P0 | DB：bounded_contexts + bc_relationships + et.bc_id + folds.fold_type | 无 |
| P0 | Child-BC CRUD API + ET 归属 API | DB P0 |
| P1 | 阶段1后端化：/infer 接口（边密度算法，结果持久化） | BC API |
| P1 | BC 推断确认 UI（接受/拆分/合并/移动 ET） | /infer 接口 |
| P1 | 阶段2：多信号融合（DDD 角色 + 字段名约定）+ 置信度显示 | /infer 接口 |
| P1 | 跨 BC link 检测 + 自动建议 bc_relationship | BC API |
| P1 | 部门级 Context Map 可视化 | bc_relationships |
| P2 | 阶段3：Embedding 语义聚类辅助推断 | embedding-svc |
| P2 | 公司级 Context Map（跨 project 全景） | 部门级完成后 |
| P2 | Promote 时自动触发 BC 推断（增量更新） | P1 完成 |
| P2 | acl / open_host 治理行为 | Action Type / Pipeline |
| P3 | 阶段4：用户决策历史学习 | P1 确认 UI 完成 |
| P3 | 跨项目推断知识库（行业模式共享） | P3 历史学习 |

---

## 十、开放问题

| 问题 | 当前倾向 |
|------|---------|
| fold 外层 hull 是否保留，还是只显示 child-BC hull？ | 保留，作为视觉组织边界 |
| 同一 fold 内 child-BC 颜色：同色系变体 vs 独立颜色？ | 同色系（fold 色调 + 亮度变体） |
| BC 推断前端计算还是后端计算？ | 后端（结果需持久化，算法复杂） |
| 跨 fold 的 ET 是否可归属同一 child-BC？ | 不允许，BC 不跨 fold（小组是最小语义边界） |
| Shared Kernel fold 内是否也有 child-BC？ | 允许（共享服务团队内部也可分组） |
| 跨 project 的 bc_relationship 是否需要审批流？ | P2 考虑（跨部门协作需正式治理） |
| 公司级 Context Map 节点过多时如何分组？ | 按 project 折叠，点击展开 fold/BC |
