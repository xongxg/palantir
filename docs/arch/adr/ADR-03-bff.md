# ADR-03: BFF 边界

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

是否需要独立 BFF（Backend for Frontend）？聚合逻辑放在哪里？

## 决策

**BFF 薄到极致**：`api-gateway` 只做 JWT 解析 + 路由 + SSE 转发，不承载任何业务聚合逻辑。

## 理由

- 聚合逻辑注册为 Function，前端和 Agent 复用同一个 Function
- 避免 BFF 成为新的单体

## 演进路径

```
阶段一（模块化单体）：Gateway 调 in-process 模块
阶段二（微服务）：    Gateway 转发 HTTP，前端零感知
```

## 参考

Palantir 官方无独立 BFF，Ontology Function 层即聚合层，本项目同样选择。

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
