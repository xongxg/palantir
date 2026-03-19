# ADR-02: Function 执行模型

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

Function / Logic 如何注册与执行？支持哪些用户群体？

## 决策

**三层执行模型**：

| 层 | 技术 | 用户 | 优先级 |
|----|------|------|--------|
| Layer 1 | Rust 编译时注册（`#[ontology_function]` 宏）| 平台开发者 | P0 |
| Layer 2 | CEL 表达式 + Monaco Web IDE（Schema 感知补全）| 业务分析师 | P1 |
| Layer 3 | WASM 沙箱 | 第三方扩展 | 接口占坑，暂不实现 |

## 自然语言路径

```
业务描述 → LLM（注入 Schema）→ 生成 CEL → 用户确认 → 保存为 Logic
```

自然语言是输入，结构化 CEL 是输出，LLM 不直接执行。

## 关键前提

`build.rs` 从 ontology-svc 自动生成强类型 Rust 代码（类似 prost codegen）。
`#[ontology_function]` 宏自动注册 + 生成 OpenAI tool schema。

## CEL 前端

Monaco Editor + CEL language def（~200行）+ Schema 感知 CompletionItemProvider（约 1-2 天工作量）。
