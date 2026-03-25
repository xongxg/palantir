//! Dataset backend trait hierarchy.
//!
//! Every backend represents a different physical storage/source type that
//! can materialise a versioned Dataset.  Business code depends only on the
//! trait, never on a concrete implementation.
//!
//! ┌─────────────────────────────────────────────────────────┐
//! │                   DatasetBackend (core)                  │
//! │  write_batch / read_records / commit / delete_version    │
//! └──────────┬──────────────┬──────────────┬────────────────┘
//!            │              │              │              │
//!   ObjectStore         Stream         Database         Api
//! (S3/RustFS/local)  (Kafka/Kinesis) (PG/MySQL/Oracle) (REST/GraphQL)

pub mod object_store;
pub mod stream;
pub mod database;
pub mod api;

pub use object_store::ObjectStoreBackend;
pub use stream::StreamBackend;
pub use database::DatabaseBackend;
pub use api::ApiBackend;
