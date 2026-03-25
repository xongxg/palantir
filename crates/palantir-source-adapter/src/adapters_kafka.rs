/// Kafka / RocketMQ / Pulsar 消息队列适配器
///
/// 与批量适配器不同，Kafka 是流式数据源：
///   - fetch_preview: 从 topic 开头消费最新 N 条（earliest offset）
///   - stream: 持续消费，直到 since cursor 之后的消息
///   - discover_schema: 从前 10 条消息推断 schema
///
/// 消息格式支持：JSON（默认）/ Avro（需 schema registry）/ 原始字节
///
/// 依赖：`rdkafka` crate（librdkafka Rust binding）
///
/// 配置示例（deployment.toml）：
/// ```toml
/// [sources.order_events]
/// type             = "kafka"
/// brokers          = "kafka-1:9092,kafka-2:9092"
/// topic            = "order-events"
/// group_id         = "palantir-ingest"
/// from_beginning   = true      # false = 只消费新消息
/// message_format   = "json"    # json | avro | raw
/// schema_registry  = ""        # Avro 时需要
/// sasl_username    = ""
/// sasl_password    = ""
/// max_poll_records = 500
/// ```
use crate::adapters::{DiscoveredSchema, SourceAdapter, SourceDescriptor};
use crate::errors::AdapterError;
use crate::model::{CanonicalRecord, Cursor};
use async_trait::async_trait;
use futures_core::Stream;
use futures_util::stream;

#[derive(Debug, Clone, Copy)]
pub enum KafkaMessageFormat {
    Json,
    Avro,
    Raw,
}

pub struct KafkaAdapter {
    pub id:              String,
    pub ns:              String,
    pub schema:          String,

    /// Kafka broker 地址列表（逗号分隔）
    pub brokers:         String,
    pub topic:           String,
    pub group_id:        String,
    /// true = 从头消费；false = 只消费新消息
    pub from_beginning:  bool,
    pub message_format:  KafkaMessageFormat,
    /// Avro Schema Registry URL（可选）
    pub schema_registry: Option<String>,
    /// SASL 认证（可选）
    pub sasl_username:   Option<String>,
    pub sasl_password:   Option<String>,
    /// 单次 poll 最大消息数
    pub max_poll_records: usize,
}

impl KafkaAdapter {
    pub fn new(
        id: impl Into<String>,
        brokers: impl Into<String>,
        topic: impl Into<String>,
        ns: impl Into<String>,
        schema: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(), ns: ns.into(), schema: schema.into(),
            brokers: brokers.into(), topic: topic.into(),
            group_id: "palantir-ingest".into(),
            from_beginning: true,
            message_format: KafkaMessageFormat::Json,
            schema_registry: None,
            sasl_username: None, sasl_password: None,
            max_poll_records: 500,
        }
    }

    pub fn with_group_id(mut self, g: impl Into<String>) -> Self { self.group_id = g.into(); self }
    pub fn with_format(mut self, f: KafkaMessageFormat) -> Self { self.message_format = f; self }
    pub fn with_sasl(mut self, u: impl Into<String>, p: impl Into<String>) -> Self {
        self.sasl_username = Some(u.into()); self.sasl_password = Some(p.into()); self
    }
    pub fn from_latest(mut self) -> Self { self.from_beginning = false; self }
}

#[async_trait]
impl SourceAdapter for KafkaAdapter {
    fn id(&self) -> &str { &self.id }
    fn adapter_type(&self) -> &'static str { "kafka" }

    async fn describe(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.id.clone(),
            adapter_type: "kafka".to_string(),
            has_cursor: true,  // Kafka offset 天然是 cursor
            partitions: None,  // TODO: 从 metadata 获取 partition 数
        }
    }

    async fn test_connection(&self) -> Result<String, AdapterError> {
        // TODO:
        // let consumer = rdkafka::ClientConfig::new()
        //     .set("bootstrap.servers", &self.brokers)
        //     .create::<StreamConsumer>()?;
        // consumer.fetch_metadata(Some(&self.topic), Duration::from_secs(5))?;
        Err(AdapterError::Message("Kafka adapter: not yet implemented".to_string()))
    }

    async fn fetch_preview(&self, _limit: usize) -> Result<Vec<serde_json::Value>, AdapterError> {
        // TODO:
        // 1. 创建临时 consumer（随机 group_id，avoid committing offsets）
        // 2. seek 到 earliest offset
        // 3. 消费 _limit 条消息后断开
        // 4. 按 message_format 解析为 serde_json::Value
        Err(AdapterError::Message("Kafka adapter: not yet implemented".to_string()))
    }

    async fn discover_schema(&self) -> Result<DiscoveredSchema, AdapterError> {
        // TODO: 消费前 10 条，用 discover_from_records 推断
        // 如果 message_format = Avro，可直接从 schema registry 获取精确 schema
        Err(AdapterError::Message("Kafka adapter: not yet implemented".to_string()))
    }

    fn stream(
        &self,
        _since: Option<Cursor>,
    ) -> Box<dyn Stream<Item = Result<CanonicalRecord, AdapterError>> + Unpin + Send> {
        // TODO:
        // 1. 如果 since = Some(offset)，seek 到该 offset
        // 2. 否则按 from_beginning 配置决定起始位置
        // 3. 持续 poll，每条消息包装为 CanonicalRecord，cursor = kafka offset
        Box::new(stream::iter(vec![Err(AdapterError::Message(
            "Kafka adapter: not yet implemented".to_string(),
        ))]))
    }
}
