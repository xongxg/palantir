pub mod backends;
pub mod backend;   // kept for internal use by local.rs / s3.rs / store.rs
pub mod local;
pub mod manifest;
pub mod s3;
pub mod store;
pub mod writer;

// ── Backend traits ─────────────────────────────────────────────────────────────
pub use backends::object_store::ObjectStoreBackend;
pub use backends::stream::{StreamBackend, StreamConsumer, StreamRecord,
                            KafkaBackend, KinesisBackend, PulsarBackend};
pub use backends::database::{DatabaseBackend, DbRecord, TableInfo, ColumnInfo, IncrementalOptions,
                              PostgresDbBackend, MySqlDbBackend, OracleDbBackend, ClickHouseDbBackend};
pub use backends::api::{ApiBackend, ApiRecord, ApiSchema, ApiFieldInfo, ApiAuth, PaginationStrategy,
                        RestApiBackend, GraphQlApiBackend, ODataApiBackend};

// ── Object store impls (real implementations) ─────────────────────────────────
/// Backwards-compat alias: `StorageBackend` == `ObjectStoreBackend`.
pub use backend::StorageBackend;
pub use local::LocalFsBackend;
pub use s3::S3Backend;

// ── Dataset versioning layer ───────────────────────────────────────────────────
pub use manifest::{DatasetManifest, DatasetSchema, FileEntry, SchemaField};
pub use store::{DatasetStore, read_records_from_manifest_path};
pub use writer::DatasetWriter;
