# agent-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

LLM 推理入口、Multi-Agent 编排、语义缓存、AgentTrace 追踪。

---

## API 端点

```
POST /v1/query                   → 发起查询（返回 SSE stream）
POST /v1/query/{id}/cancel       → 取消生成（CancellationToken）
GET  /v1/query/{id}/trace        → 查看 AgentTrace
WS   /v1/ws                      → WebSocket（Proactive Agent / 打断，中期）
```

---

## 查询流程

```
用户请求
  ↓
1. embedding-svc.embed(query)         → query vector
2. Semantic Cache 命中？→ 直接返回
3. ontology-svc 注入 Schema context
4. function-svc 注入可用 Function 列表
5. LLM（planner）→ 执行计划
6. executor 并发调用 Function
7. LLM（synthesizer）→ 流式输出
8. AgentTrace 记录全链路
9. Memory 写回（confidence >= 0.85）
```

---

## Long-term Memory（ADR-06）

```
写入条件：confidence >= 0.85 && access_count > 2
  ↓
Layer 1：SurrealDB（结构化元数据）
Layer 2：embedding-svc 向量化 → SurrealDB 内置向量索引

检索：向量 ANN → memory_id → SurrealDB 批量取完整内容
热数据：Redis TTL 72h
```

---

## 流式协议（ADR-17）

```
现阶段：SSE（Server-Sent Events）
中期：  WebSocket（Proactive 推送 / 用户打断）
未来：  WebRTC（语音功能）
```

---

## AgentTrace

每次查询记录完整执行链路：

```rust
pub struct AgentTrace {
    pub query_id: Uuid,
    pub steps: Vec<TraceStep>,     // planner / function call / synthesizer
    pub total_tokens: u32,
    pub latency_ms: u64,
    pub cache_hit: bool,
}
```

---

## 复用 crate

- `palantir-agent`（重构后）：planner + executor + semantic cache

---

## 待细化

- [ ] Multi-Agent 任务分配策略
- [ ] Semantic Cache key 设计（query hash + schema version）
- [ ] AgentTrace 存储与查询 API
- [ ] 流式输出格式（SSE event 规范）

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v0.1.0 | 2026-03-19 | 初始版本，架构设计阶段 |
