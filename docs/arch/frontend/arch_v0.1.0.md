# Frontend 子架构

> 状态：设计阶段 | 日期：2026-03-19 | ADR-16

## 技术栈

| 项目 | 选型 |
|------|------|
| 框架 | React 18 + TypeScript |
| 构建 | Vite |
| 路由 | React Router v6 |
| 状态管理 | Zustand（轻量）|
| 样式 | Tailwind CSS |
| 图可视化 | React Flow |
| 代码编辑器 | Monaco Editor |
| API 类型生成 | openapi-typescript |
| HTTP 客户端 | ky / axios |
| SSE | 原生 EventSource API |

---

## 目录结构

```
frontend/
├── src/
│   ├── pages/                   # 路由页面（薄层，只做组合）
│   │   ├── OntologyPage.tsx
│   │   ├── IngestPage.tsx
│   │   ├── FunctionPage.tsx
│   │   ├── AgentPage.tsx
│   │   └── WorkflowPage.tsx
│   ├── components/              # 通用组件（无业务状态）
│   │   ├── Layout/
│   │   ├── Table/
│   │   └── Form/
│   ├── api/                     # openapi-typescript 生成 + client 封装
│   │   ├── generated/           # 自动生成，不手写
│   │   └── client.ts            # 统一 base URL + 认证 header
│   └── features/                # 业务功能模块（各自独立）
│       ├── ontology/            # Ontology 图可视化（React Flow）
│       ├── ingest/              # Source / Mapping 管理
│       ├── function/            # CEL Web IDE（Monaco Editor）
│       ├── agent/               # Agent 对话（SSE 流式）
│       └── workflow/            # Workflow 设计器
├── package.json
└── vite.config.ts
```

---

## 类型集成链

```
Rust 后端（utoipa 注解）
  ↓
POST /openapi.json（各服务暴露）
  ↓
api-gateway 聚合 OpenAPI schema
  ↓
openapi-typescript 生成 TypeScript 类型
  ↓
frontend/src/api/generated/
```

CI 中自动运行 `openapi-typescript`，类型始终与后端同步。

---

## Feature 模块设计

### ontology — Ontology 图可视化

```
React Flow 画布
  ├── EntityTypeNode（TBox 节点）
  ├── OntologyObjectNode（ABox 节点）
  └── RelationshipEdge（RELATE 边）

操作：
  - 拖拽创建关系
  - 点击节点查看属性
  - 多跳图遍历展开
```

### function — CEL Web IDE

```
Monaco Editor
  ├── CEL language definition（语法高亮）
  ├── Schema 感知 CompletionItemProvider（自动补全）
  └── 实时 CEL 语法校验
```

### agent — SSE 流式对话

```
对话界面
  ├── EventSource 连接 /v1/query
  ├── 流式渲染（Markdown）
  ├── "停止生成"按钮 → POST /v1/query/{id}/cancel
  └── AgentTrace 展开（可选）
```

---

## 待细化

- [ ] Workflow 设计器 UI 方案（React Flow 复用 or 独立）
- [ ] 认证 / 登录页面（JWT 存储策略）
- [ ] 国际化（i18n）需求确认
- [ ] 移动端适配优先级

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
