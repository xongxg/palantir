# embedding-svc 子架构

> 状态：设计阶段 | 日期：2026-03-19

## 职责

集中式向量化服务，多服务共享，本地 ONNX 推理，零 API 成本。

---

## API 端点

```
POST /v1/embed                        → 单条文本向量化
POST /v1/embed/batch                  → 批量向量化（推荐）
GET  /v1/health                       → 健康检查
GET  /v1/model/info                   → 当前模型信息
```

---

## 技术选型（ADR-19）

| 项目 | 选型 |
|------|------|
| 推理库 | `fastembed-rs` |
| 模型 | BGE-small-zh（512 维）|
| 运行时 | ONNX Runtime（ort crate）|
| 语言优化 | 中文优化，替换 all-MiniLM-L6-v2（384维）|

---

## 请求优先级队列

```
HIGH：agent-svc 实时请求（用户查询，低延迟优先）
LOW： ingest-svc 批量请求（数据摄入，吞吐量优先）
```

防止批量任务阻塞实时查询延迟。

---

## 调用方降级策略（Circuit Breaker）

```rust
// agent-svc 内部
match embedding_client.embed(query).await {
    Ok(vec) => semantic_cache.lookup(vec).await,
    Err(_)  => None,  // 跳过 semantic cache，直接走 LLM
}
```

embedding-svc 不在写路径，挂掉不丢数据，功能降级用户无感知。

---

## 扩容策略

无状态服务，多实例 + 轮询负载均衡：

```bash
embedding-svc --port 8081
embedding-svc --port 8082
```

触发扩容信号：P99 延迟 > 50ms 或 CPU 持续 > 80%。

---

## 待细化

- [ ] 批量请求的最优 batch size（fastembed-rs 测试）
- [ ] 优先级队列具体实现（tokio priority channel）
- [ ] 模型热加载（不重启服务更换模型）
