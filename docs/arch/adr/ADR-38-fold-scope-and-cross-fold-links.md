# ADR-38: Fold 是数据组织单元，Ontology 是 Project 级语义层

> 日期：2026-03-21
> 状态：已采纳
> 关联：ADR-37（Domain-first Ontology）、ingest-workflow_v0.5.0

---

## 背景

Project 下可以有多个 Fold（子业务线），每个 Fold 管理一组数据源（CSV、DB connector 等）。
当不同 Fold 之间存在业务关联时，需要明确：关联关系定义在哪一层？

典型场景：
```
Project: 航空集团数字化
  Fold: 运营部门  → Aircraft ET, Route ET
  Fold: 财务部门  → Invoice ET, Contract ET
  Fold: 维修部门  → MaintenanceRecord ET, Technician ET
```

`MaintenanceRecord.aircraft_id` → 引用运营部门的 `Aircraft`：这个关联该如何处理？

---

## 决策

**Fold 仅是数据源的组织单元，Ontology（Entity Type + Link Type）是 Project 级别的全局语义层。**

跨 Fold 的业务关联，通过标准 **Link Type** 在 Ontology 层定义，不需要任何额外配置。

---

## 数据模型层次

```
projects (大业务线)
  ├── folds → data_sources → datasets     ← 数据组织层（fold_id 约束）
  └── entity_types + ontology_links        ← 语义层（Project 级别，不分 Fold）
```

`entity_types` 和 `ontology_links` 表上**无 fold_id 字段**，天然支持跨 Fold 引用。

---

## Palantir Foundry 对应实践

Foundry 中 Folder 只管理 Dataset 的存储位置，Object Type 和 Link Type 属于全局 Ontology。
Link Type 可连接任意两个 Object Type，无论其底层 Dataset 位于哪个 Folder。
这正是 Ontology 作为"语义统一视图"的核心价值：**数据分散，语义统一**。

---

## 理由

### 1. 业务关联不受数据存储位置约束

`MaintenanceRecord` 维护的是 `Aircraft`，这是业务事实。
无论这两个数据集在哪个 Fold 里，这个语义关系是固定的，不应因组织架构的调整而改变。

### 2. Fold 的调整不应破坏 Ontology

如果跨 Fold 关联需要特殊处理，那么重新组织 Fold 结构（业务重组）就会破坏已有的 Link Type 定义。
将语义层与组织层解耦，保证 Ontology 的稳定性。

### 3. 与 ADR-37 一致

ADR-37 强调领域语义优先于数据结构。同理，语义关联应优先于数据组织结构。

---

## 映射向导中的关联发现分层

Step 3（关联推断）按置信度分层展示，不屏蔽跨 Fold 候选：

| 关联范围 | 置信度 | 默认状态 |
|---------|--------|---------|
| 同 Fold，`_id`/`_fk` 列名匹配 | 极高 | 默认勾选 |
| 同 Fold，值域重叠匹配 | 高 | 推荐勾选 |
| 跨 Fold，同 Project，列名匹配 | 中 | 展示但不勾选 |
| 跨 Fold，同 Project，值域重叠 | 中低 | 折叠展示 |
| 跨 Project | 低 | 不自动推断 |

---

## 边界：Project 是语义边界，Fold 不是

| 场景 | 处理方式 |
|------|---------|
| 同 Fold 内关联 | Link Type，高置信自动推断 |
| 跨 Fold，同 Project | Link Type，需用户确认意图 |
| 跨 Project | 明确的跨域 Link Type 配置（P2，类似 Foundry Federation） |

---

## 后续演进

当跨 Project 关联成为真实需求时（多个大业务线共享实体，如全公司统一的 Employee ET），
引入**跨 Project Reference Type**，类似 Palantir 的 Ontology Federation 概念。
触发条件：≥2 个 Project 需要共享同一 ET 定义。
