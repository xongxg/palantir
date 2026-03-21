# ADR-10: Event Bus 选型与可插拔后端

> 状态：✅ 已决策（v2.0）| 最近更新：2026-03-19 | 依赖：[ADR-28](ADR-28-pluggable-storage.md)

---

## 问题

1. 服务间异步通信用哪个 Event Bus？
2. 国内客户已有 RocketMQ / Kafka 投资，不应强制引入 NATS，如何复用？
3. RocketMQ 的 Rust SDK 不成熟，如何低成本支持？

---

## 决策（v2.0）

**NATS JetStream 作为唯一默认实现；Kafka / RocketMQ 适配等到有具体客户需求时再加；trait 保持薄但预留 capability 扩展点。**

核心理由：
- 系统消息模式是**服务间协调**（中低频），不是大数据流，Kafka 的核心优势用不上
- NATS JetStream 功能完全覆盖需求，且单二进制运维极简，契合 AirGapped 场景
- 国内云适配是 P2，提前设计是过度工程；等有真实客户需求再加具体实现
- trait 抽象不能变成最小公分母——各后端语义差异大（push/pull、分区、延迟消息），用 capability 接口按需扩展

> **Fluvio 降级为观察项**：Rust 原生但生产案例少、运维工具不成熟，暂不实现。

---

## 1. 实现矩阵

```
EventBus trait
  ├── InProcessBus  （tokio broadcast）  ← P0：单元测试 / 开发模式
  ├── NatsBus       （async-nats）       ← P0：所有生产环境默认
  ├── KafkaBus      （rdkafka）          ← P2：客户已有 Kafka/ONS/DMS/CKafka 集群时
  └── RocketMqHttpBus（reqwest）         ← P2：客户有纯 RocketMQ 且无 Kafka 兼容时
```

**P2 触发条件**：有真实客户提出「我们已有 X，不想再运维 NATS」，再实现对应适配器。

---

## 2. Trait 定义

### 核心 trait（薄，所有实现必须满足）

```rust
pub trait EventBus: Send + Sync {
    async fn publish(&self, topic: &str, payload: Bytes) -> Result<()>;

    /// 持久化订阅：重启后从上次 cursor 续读，At-Least-Once
    async fn subscribe_durable(
        &self,
        topic:         &str,
        consumer_name: &str,
    ) -> Result<BoxStream<'static, (Bytes, AckHandle)>>;
}

pub struct AckHandle(/* backend-specific */);
impl AckHandle {
    pub async fn ack(self)  -> Result<()> { ... }
    pub async fn nack(self) -> Result<()> { ... }
}
```

### Capability 扩展（可选，各后端按需实现）

```rust
/// 延迟消息（RocketMQ 原生支持，NATS 不支持）
pub trait DelayedMessage: EventBus {
    async fn publish_delayed(
        &self,
        topic:   &str,
        payload: Bytes,
        delay:   Duration,
    ) -> Result<()>;
}

/// 批量消费（Kafka 场景下的吞吐优化）
pub trait BatchConsumer: EventBus {
    async fn subscribe_batch(
        &self,
        topic:         &str,
        consumer_name: &str,
        max_batch:     usize,
    ) -> Result<BoxStream<'static, (Vec<Bytes>, AckHandle)>>;
}
```

业务代码只依赖核心 `EventBus` trait。需要延迟消息时，显式 downcast 到 `DelayedMessage`，而不是强迫所有实现 mock 不支持的功能。

---

## 3. NatsBus 配置（当前唯一生产实现）

```toml
# deployment.toml
[event_bus]
backend = "nats"
url     = "nats://localhost:4222"
```

- `async-nats` crate，原生 async，Rust 生态最成熟
- JetStream 提供持久化、At-Least-Once、Durable Consumer cursor

---

## 4. P2 适配器选择决策树（有需求时参考）

```
客户已有 MQ 投资
  │
  ├── Kafka 集群 / 阿里云 ONS / 华为云 DMS / 腾讯云 CKafka
  │     → KafkaBus（rdkafka，Kafka 兼容协议覆盖三家云）
  │
  └── 纯 RocketMQ，未开 Kafka 兼容
        → RocketMqHttpBus（reqwest + HTTP API，无需原生 SDK）
```

Kafka topic 命名注意：`.` 替换为 `_`（Kafka topic 名不支持 `.`）。

---

## 5. 跨服务 Trace 传播

所有消息在 payload 中携带 trace context（ADR-30 规范），与具体 MQ 后端无关：

```rust
#[derive(Serialize, Deserialize)]
struct EventEnvelope<T> {
    trace_id: String,
    span_id:  String,
    payload:  T,
}
```

消费端从 envelope 恢复 OpenTelemetry context，span 挂到原始 trace 树，实现跨 MQ 全链路追踪。

---

## 6. Topic 规范

```
ontology.events.{entity_type}.{upsert|delete|link}
ingest.jobs.created
workflow.triggers
agent.feedback
```

---

## 版本历史

| 版本 | 日期 | 变更内容 |
|------|------|---------|
| v1.0 | 2026-03-19 | 初始决策：Fluvio 首选，NATS 备选，Kafka 未来 |
| v2.0 | 2026-03-19 | NATS 升为唯一默认实现；Kafka/RocketMQ 适配降为 P2（有真实客户需求时再加）；trait 增加 capability 扩展点（DelayedMessage/BatchConsumer）；Fluvio 降为观察项 |
