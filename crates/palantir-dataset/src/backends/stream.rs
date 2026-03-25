//! StreamBackend — consume records from a message stream.
//!
//! Designed for: Kafka, AWS Kinesis, Apache Pulsar, RabbitMQ.
//!
//! Usage pattern:
//!   1. open_consumer() → StreamConsumer
//!   2. poll_batch() in a loop → Vec<StreamRecord>
//!   3. commit_offset() after records are safely persisted
//!   4. close_consumer()
//!
//! All implementations are STUBS — return "not yet implemented".

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A single record polled from the stream.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    /// Partition / shard identifier.
    pub partition: String,
    /// Monotonically increasing offset within the partition.
    pub offset: i64,
    /// Timestamp the record was produced (Unix ms).
    pub timestamp_ms: i64,
    /// Raw payload bytes (JSON, Avro, Protobuf, etc.).
    pub payload: Vec<u8>,
    /// Optional record key (used for partitioning / deduplication).
    pub key: Option<Vec<u8>>,
}

/// Consumer handle returned by `StreamBackend::open_consumer`.
/// Holds per-consumer state (offset tracking, group membership, etc.).
#[async_trait]
pub trait StreamConsumer: Send + Sync {
    /// Poll up to `max_records` records. Returns empty vec if no new records.
    async fn poll_batch(&mut self, max_records: usize) -> Result<Vec<StreamRecord>>;
    /// Acknowledge all records up to and including `offset` on `partition`.
    async fn commit_offset(&mut self, partition: &str, offset: i64) -> Result<()>;
    /// Seek to a specific offset (useful for replay / backfill).
    async fn seek(&mut self, partition: &str, offset: i64) -> Result<()>;
    /// Gracefully close the consumer, releasing broker-side resources.
    async fn close(self: Box<Self>) -> Result<()>;
}

/// Metadata about a stream topic / subject.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamTopicInfo {
    pub topic: String,
    pub partitions: Vec<String>,
    /// Earliest available offset per partition.
    pub earliest_offsets: std::collections::HashMap<String, i64>,
    /// Latest (head) offset per partition.
    pub latest_offsets: std::collections::HashMap<String, i64>,
}

/// Pluggable streaming backend.
///
/// Implement this trait to connect a new streaming system.
/// Current status: ALL implementations are stubs.
#[async_trait]
pub trait StreamBackend: Send + Sync {
    /// Open a consumer for `topic`, joining `consumer_group`.
    /// `start_from_latest = true` skips historical records on first connect.
    async fn open_consumer(
        &self,
        topic: &str,
        consumer_group: &str,
        start_from_latest: bool,
    ) -> Result<Box<dyn StreamConsumer>>;

    /// Inspect topic metadata (partitions, offsets).
    async fn topic_info(&self, topic: &str) -> Result<StreamTopicInfo>;

    /// List all accessible topics.
    async fn list_topics(&self) -> Result<Vec<String>>;
}

// ── Stubs ─────────────────────────────────────────────────────────────────────

macro_rules! ni {
    ($name:expr) => {
        anyhow::bail!("{}: not yet implemented", $name)
    };
}

/// Stub for Apache Kafka / Confluent Cloud.
pub struct KafkaBackend;
impl KafkaBackend {
    pub fn new(_brokers: &str, _security: Option<&str>) -> Result<Self> {
        ni!("KafkaBackend::new")
    }
}
#[async_trait]
impl StreamBackend for KafkaBackend {
    async fn open_consumer(&self, _topic: &str, _group: &str, _latest: bool) -> Result<Box<dyn StreamConsumer>> { ni!("KafkaBackend::open_consumer") }
    async fn topic_info(&self, _topic: &str) -> Result<StreamTopicInfo> { ni!("KafkaBackend::topic_info") }
    async fn list_topics(&self) -> Result<Vec<String>> { ni!("KafkaBackend::list_topics") }
}

/// Stub for AWS Kinesis.
pub struct KinesisBackend;
impl KinesisBackend {
    pub fn new(_region: &str) -> Result<Self> {
        ni!("KinesisBackend::new")
    }
}
#[async_trait]
impl StreamBackend for KinesisBackend {
    async fn open_consumer(&self, _topic: &str, _group: &str, _latest: bool) -> Result<Box<dyn StreamConsumer>> { ni!("KinesisBackend::open_consumer") }
    async fn topic_info(&self, _topic: &str) -> Result<StreamTopicInfo> { ni!("KinesisBackend::topic_info") }
    async fn list_topics(&self) -> Result<Vec<String>> { ni!("KinesisBackend::list_topics") }
}

/// Stub for Apache Pulsar.
pub struct PulsarBackend;
impl PulsarBackend {
    pub fn new(_service_url: &str) -> Result<Self> {
        ni!("PulsarBackend::new")
    }
}
#[async_trait]
impl StreamBackend for PulsarBackend {
    async fn open_consumer(&self, _topic: &str, _group: &str, _latest: bool) -> Result<Box<dyn StreamConsumer>> { ni!("PulsarBackend::open_consumer") }
    async fn topic_info(&self, _topic: &str) -> Result<StreamTopicInfo> { ni!("PulsarBackend::topic_info") }
    async fn list_topics(&self) -> Result<Vec<String>> { ni!("PulsarBackend::list_topics") }
}
