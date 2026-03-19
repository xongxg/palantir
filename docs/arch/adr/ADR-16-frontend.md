# ADR-16: 前端选型

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

前端技术栈如何选型？前后端如何集成？

## 决策

**React + TypeScript + Vite**，前后端分离，utoipa 生成 OpenAPI → openapi-typescript 生成类型。

## 理由

- React 生态最成熟，组件库丰富（React Flow、Monaco Editor）
- Vite 构建速度快，开发体验好
- TypeScript 类型安全，与后端 OpenAPI 类型对齐
- 前后端分离，独立部署，前端零感知服务演进

## 类型集成链

```
Rust（utoipa 注解）→ OpenAPI JSON → openapi-typescript → TypeScript 类型
```

## Feature 模块

| Feature | 技术 |
|---------|------|
| ontology | React Flow（图可视化）|
| ingest | 标准表单组件 |
| function | Monaco Editor + CEL language def |
| agent | SSE 流式对话 |
| workflow | Workflow 设计器 |

## 完整结构

见 [../../frontend/arch.md](../../frontend/arch.md)
