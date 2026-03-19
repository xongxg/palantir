# ADR-10: Event Bus 选型

> 状态：✅ 已决策 | 日期：2026-03-19

## 问题

服务间异步通信用哪个 Event Bus？

## 决策

**InProcessBus → Fluvio / NATS JetStream**，Kafka 备选。

## 分层实现

```
EventPublisher / EventSubscriber trait
  ├── InProcessBus（tokio broadcast）← 开发 / 单进程
  ├── FluvioBus（Rust 原生）         ← 微服务生产首选
  ├── NatsBus（NATS JetStream）      ← 保守备选，生产案例多
  └── KafkaBus                      ← 大数据量场景，未来
```

## Topic 规范

```
ontology.events.{entity_type}.{upsert|delete|link}
ingest.jobs.created
workflow.triggers
agent.feedback
```

## 选型理由

- **Fluvio**：Rust 原生实现，生态契合度最高
- **NATS**：单二进制，延迟极低，运维简单，生产案例多（保守备选）
- **Kafka**：留作未来大数据量场景替换选项（trait 已抽象，换实现不改业务）

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策 |
