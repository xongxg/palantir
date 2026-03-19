# ADR-13: 向量搜索成本控制

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

向量搜索如何控制成本？避免过度依赖外部 API？

## 决策

**本地 ONNX embedding + 分层检索 + 选择性 embedding**，Qdrant 按需引入。

## 分层检索

```
1. Semantic Cache 命中？→ 直接返回（零成本）
2. BM25 全文搜索（SurrealDB 内置）→ 命中率 ~60%
3. 本地向量搜索（SurrealDB 内置）→ 命中率 ~90%
4. Qdrant（可选，>100 万条向量时引入）
```

## 选择性 Embedding

```rust
if memory.confidence >= 0.85 && memory.access_count > 2 && !is_expired(&memory) {
    embed_and_index(memory);
}
```

## 时间衰减 + 淘汰

- 写入 → Redis TTL 72h → 若被访问提升到 SurrealDB 向量索引
- 向量索引中 30 天无访问 → 自动剔除

## 向量存储演进

| 阶段 | 方案 | 触发条件 |
|------|------|---------|
| MVP | SurrealDB 内置向量 | 默认 |
| 中期 | LanceDB（嵌入式）| 向量 > 50万 或 P99 > 200ms |
| 生产 | Qdrant 自托管 | 多节点部署需求 |
