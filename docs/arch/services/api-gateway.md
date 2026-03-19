# api-gateway 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

唯一前端入口：JWT 解析 + 路由转发 + SSE 代理。BFF 薄到极致（ADR-03）。

---

## 原则

- **不承载任何业务逻辑**
- **不聚合跨服务数据**（聚合逻辑注册为 Function，走 function-svc）
- 只做：认证 → 路由 → 转发

---

## 路由规则

```
/v1/schema/*        → ontology-svc
/v1/objects/*       → ontology-svc
/v1/links/*         → ontology-svc
/v1/sync/*          → ontology-svc
/v1/sources/*       → ingest-svc
/v1/mappings/*      → ingest-svc
/v1/ingest/*        → ingest-svc
/v1/uploads/*       → ingest-svc
/v1/functions/*     → function-svc
/v1/logics/*        → function-svc
/v1/query/*         → agent-svc（SSE 转发）
/v1/ws              → agent-svc（WebSocket 代理）
/v1/workflows/*     → workflow-svc
/v1/runs/*          → workflow-svc
/v1/authorize       → auth-svc
/v1/roles/*         → auth-svc
/v1/policies/*      → auth-svc
```

---

## 中间件链

```
请求进来
  ↓
1. JWT 解析 + 验证（提取 subject / roles）
2. 注入 X-User-Id / X-Roles Header
3. 路由匹配 → 转发到对应服务
4. SSE / WebSocket 透传（不缓冲）
5. 访问日志（who + what + when + IP）→ auth-svc 审计
```

---

## 演进路径（ADR-03）

```
阶段一（模块化单体）：Gateway 调 in-process 模块（函数调用）
阶段二（微服务）：    Gateway 转发 HTTP（前端零感知）
```

---

## 复用 crate

- `palantir-ingest-api`（演化而来）：现有 Axum 骨架

---

## 待细化

- [ ] JWT 验证库选型（jsonwebtoken crate）
- [ ] 速率限制（Rate Limiting）策略
- [ ] 健康检查聚合端点（/health 聚合所有服务状态）
- [ ] OpenAPI 聚合（utoipa 各服务 schema 合并）
