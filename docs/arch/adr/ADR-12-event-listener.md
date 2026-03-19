# ADR-12: EventListener 复杂度控制

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

EventListener 是否需要内置复杂的流处理逻辑（窗口、聚合）？

## 决策

**tokio async 无状态过滤**，有状态聚合通过 Logic + SurrealDB 实现。

## 理由

- 不引入 Flink / 流处理框架，避免运维复杂度
- Ontology 即状态存储：历史数据在 SurrealDB，Logic 查询即可聚合
- EventListener 保持无状态，可水平扩展

## 实现模式

```rust
// EventListener：只做过滤，无状态
subscriber.subscribe("ontology.events.Contract.>", |event| async {
    if matches_filter(&event) {
        trigger_workflow(event).await;
    }
}).await?;

// 有状态聚合：在 Logic 中查 SurrealDB
fn aggregate_logic(object: &OntologyObject) -> Vec<OntologyEvent> {
    let history = surreal.query("SELECT * FROM Contract WHERE ...").await?;
    // 聚合计算
}
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
