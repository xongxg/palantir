# 前端导航与业务流程设计 v0.1.0

> 版本：v0.1.0 | 日期：2026-03-21
> 决策：方案 A — Project 作为唯一业务入口
> 关联：design-journal_2026-03-21.md § Project 统一工作台

---

## 一、问题背景

原来的导航结构是扁平的：

```
顶栏：Sources | Ontology | Projects
```

三个入口互相平行，没有业务语境。用户直接进 Sources 时：
- 不知道"我连这个数据是为了哪个项目"
- 不知道"连完之后要干什么"
- Sources 和 Ontology 的关系不清晰

根本原因：**Sources 是手段，不是目的**。用户的心智是"我要解决一个业务问题"，而不是"我要连一个数据源"。

---

## 二、方案选型

| 方案 | 描述 | 选择 |
|------|------|------|
| 方案 A | Project 作为唯一入口，Sources/Ontology 内嵌为 Tab | ✅ 采用 |
| 方案 B | 在 Sources 页加项目上下文引导条 | 小改动，治标不治本 |

---

## 三、确定方案：Project 统一工作台

### 路由结构

```
/                    → Projects 列表（首页）
/project/:id         → Project 统一工作台（主业务入口）✅ 新增
/sources             → 全局数据源管理（管理员视角，保留）
/ontology            → 全局 Ontology 管理（管理员视角，保留）
/ingest/project/:id  → 旧项目页（保留，向后兼容）
```

### 工作台 Tab 结构

```
/project/:id
├── ① 数据接入    /project/:id#ingest
│   ├── 左侧：数据源列表 + 新建按钮
│   └── 右侧：数据源详情
│       ├── Datasets（映射状态 + 配置映射 Modal）
│       └── Sync 历史
│
├── ② 数据模型    /project/:id#model
│   ├── Sub-tab: Schema（Entity Type 列表 + 字段管理）
│   ├── Sub-tab: Browse（按 ET 筛选查看 Object 实例）
│   └── Sub-tab: Graph（Ontology 关系图）
│
├── ③ 数据探索    /project/:id#explore   [占位，待建]
│   └── 引导用户先完成 ① ②
│
└── ⚙ 设置        /project/:id#settings
    ├── 项目改名
    └── 删除项目
```

### 用户旅程

```
新用户
  │
  ▼
创建项目（Projects 页 → 新建）
  │
  ▼
① 数据接入
  ├── 新建数据源（CSV / S3 / DB / REST）
  ├── Sync Now
  └── 配置映射（Dataset → Entity Type）
  │
  ▼
② 数据模型
  ├── 查看自动推断的 Schema
  ├── 新建 / 编辑 Entity Type
  ├── Browse 查看 Promote 后的数据实例
  └── Graph 查看实体关系图
  │
  ▼
③ 数据探索（待建）
  └── 跨实体图查询 / NL 查询
```

---

## 四、全局管理入口（管理员）

顶栏保留 `/sources` 和 `/ontology` 作为**跨项目全局视图**：
- `/sources`：查看所有项目的数据源，批量管理同步任务
- `/ontology`：查看全局 Entity Type 注册表，Schema 版本管理

日常业务用户不需要进这两个页面，数据工程师/平台管理员才需要。

---

## 五、设计原则

1. **手段隐藏，目的前置**：Sources 不露出为一级导航，只在项目内作为步骤
2. **步骤可感知**：① ② ③ 数字标号给用户明确的进度感和方向感
3. **渐进式暴露**：③ 数据探索先占位，能力成熟前不做半成品
4. **全局保留**：管理员视角的全局入口不删除，隐藏在顶栏次要位置
5. **Project 即 Context**：所有业务操作都在 Project 上下文内发生，数据来源、Schema、Ontology 都归属到项目

---

## 六、对标 Palantir

Palantir Foundry 的用户从 **Workspace** 进入，Workspace 内有 Projects，每个 Project 内：
- Datasets（接数据）
- Pipeline Builder（加工数据）
- Ontology（语义建模）
- Apps（消费数据）

本平台当前实现 Datasets + Ontology 两层，与 Palantir 的 Workspace → Project → Dataset/Ontology 结构对齐。

---

## 七、后续演进

| 阶段 | 内容 |
|------|------|
| 当前 P0 | ① 数据接入 + ② 数据模型 完整可用 |
| P1 | ③ 数据探索：图查询 + 条件过滤 |
| P2 | ③ 数据探索：NL 查询（AI Agent 接入） |
| P3 | ④ 数据应用：Action / Report / Workflow |
