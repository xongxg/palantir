---
name: project_frontend_design
description: 前端导航流程（方案A已确认）、Admin/Business 分域、Shell 结构（2026-03-21 更新）
type: project
---

## 导航与业务流程（方案 A，2026-03-21 确认）

```
/ → Projects 列表
  → /project/:id → 统一工作台（project_workspace.html）
       ① 数据接入（Sources + Mapping + Sync）
       ② 数据模型（Schema / Browse / Graph）
       ③ 数据探索（占位，待建）
       ⚙ 设置（改名 / 删除）
```

**Why:** Sources 是手段不是目的，用户心智是"解决业务问题"而非"连数据源"。Project 作为唯一入口，给用户明确的步骤感。

**How to apply:** 新功能优先放在 project_workspace.html 的对应 Tab 内，而不是新增顶栏入口。

- `/sources`、`/ontology` 顶栏保留作为全局管理员视角（跨项目）
- 日常业务流程全部在 `/project/:id` 内完成
- 详见 `docs/arch/frontend/navigation-flow_v0.1.0.md`

## 路由映射

| 路径 | 用途 |
|------|------|
| `/` | Projects 列表（主入口）|
| `/project/:id` | 统一工作台（新，主业务入口）|
| `/sources` | 全局数据源管理（管理员）|
| `/ontology` | 全局 Ontology 管理（管理员）|
| `/ingest/project/:id` | 旧项目页（保留兼容）|
