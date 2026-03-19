# ADR-19: 独立 Embedding 服务

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

向量化（embedding）逻辑放在各服务内部还是独立出来？

## 决策

**独立 `embedding-svc`**，多服务共享，HTTP 调用。

## 理由

- 避免各服务各自加载 ONNX 模型，节省内存（BGE-small-zh 模型约 100MB+）
- 统一管理模型版本，一处升级全局生效
- 无状态，可水平扩展

## 技术选型

- `fastembed-rs`：Rust 原生 embedding 库
- `BGE-small-zh`（512维）：替换 all-MiniLM-L6-v2，中文优化，零 API 成本

## 单点风险与对冲

**风险**：embedding-svc 不在写路径上，挂掉不丢数据。

**Circuit Breaker**：
```rust
match embedding_client.embed(query).await {
    Ok(vec) => semantic_cache.lookup(vec).await,
    Err(_)  => None,  // 降级：跳过 semantic cache，直接走 LLM
}
```

## 优先级队列

```
HIGH：agent-svc 实时请求（用户查询）
LOW：ingest-svc 批量请求（数据摄入）
```

防止批量任务阻塞实时查询。

## 扩容

无状态服务，多实例 + 轮询负载均衡，不需要共享任何状态。
