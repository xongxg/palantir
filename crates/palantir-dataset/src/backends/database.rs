//! DatabaseBackend — read datasets directly from a relational/columnar database.
//!
//! Designed for: PostgreSQL, MySQL, Oracle, SQL Server, ClickHouse, Snowflake.
//!
//! Usage pattern:
//!   1. connect() → DatabaseBackend instance
//!   2. list_tables() / describe_table() → discover schema
//!   3. query_full_table() or query_incremental() → Vec<DbRecord>
//!   4. The caller materialises results into a Dataset version via DatasetWriter
//!
//! Key difference from ObjectStoreBackend: data is NOT stored here;
//! this backend reads from a live database and streams rows out.
//! DatasetWriter is still responsible for persisting the snapshot.
//!
//! All implementations are STUBS — return "not yet implemented".

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Column descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    /// SQL type string, e.g. "VARCHAR(255)", "BIGINT", "TIMESTAMP".
    pub sql_type: String,
    pub nullable: bool,
    pub is_primary_key: bool,
}

/// Table / view descriptor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub estimated_row_count: Option<i64>,
}

/// A single row returned from a database query.
/// Field order matches the SELECT column list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DbRecord {
    pub fields: std::collections::HashMap<String, Value>,
}

/// Options for incremental / change-data-capture reads.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncrementalOptions {
    /// Column used as high-watermark (e.g. "updated_at", "id").
    pub watermark_column: String,
    /// Last seen watermark value; rows with value > this will be returned.
    pub last_watermark: Option<Value>,
}

/// Pluggable relational/columnar database backend.
///
/// Implement this trait to connect a new database engine.
/// Current status: ALL implementations are stubs.
#[async_trait]
pub trait DatabaseBackend: Send + Sync {
    /// List all tables / views accessible to the configured user.
    async fn list_tables(&self) -> Result<Vec<TableInfo>>;

    /// Describe columns of a specific table.
    async fn describe_table(&self, schema: Option<&str>, table: &str) -> Result<TableInfo>;

    /// Read all rows from a table (full snapshot).
    /// `batch_size` controls how many rows are fetched per round-trip.
    async fn query_full_table(
        &self,
        schema: Option<&str>,
        table: &str,
        batch_size: usize,
    ) -> Result<Vec<DbRecord>>;

    /// Read only rows newer than `opts.last_watermark` (incremental sync).
    async fn query_incremental(
        &self,
        schema: Option<&str>,
        table: &str,
        opts: &IncrementalOptions,
        batch_size: usize,
    ) -> Result<Vec<DbRecord>>;

    /// Execute an arbitrary SQL query (read-only).
    async fn execute_query(&self, sql: &str) -> Result<Vec<DbRecord>>;

    /// Test connectivity. Returns Ok(()) if reachable, Err otherwise.
    async fn ping(&self) -> Result<()>;
}

// ── Stubs ─────────────────────────────────────────────────────────────────────

macro_rules! ni {
    ($name:expr) => {
        anyhow::bail!("{}: not yet implemented", $name)
    };
}

/// Stub for PostgreSQL.
pub struct PostgresDbBackend;
impl PostgresDbBackend {
    pub async fn connect(_url: &str) -> Result<Self> { ni!("PostgresDbBackend::connect") }
}
#[async_trait]
impl DatabaseBackend for PostgresDbBackend {
    async fn list_tables(&self) -> Result<Vec<TableInfo>> { ni!("PostgresDbBackend::list_tables") }
    async fn describe_table(&self, _s: Option<&str>, _t: &str) -> Result<TableInfo> { ni!("PostgresDbBackend::describe_table") }
    async fn query_full_table(&self, _s: Option<&str>, _t: &str, _b: usize) -> Result<Vec<DbRecord>> { ni!("PostgresDbBackend::query_full_table") }
    async fn query_incremental(&self, _s: Option<&str>, _t: &str, _o: &IncrementalOptions, _b: usize) -> Result<Vec<DbRecord>> { ni!("PostgresDbBackend::query_incremental") }
    async fn execute_query(&self, _sql: &str) -> Result<Vec<DbRecord>> { ni!("PostgresDbBackend::execute_query") }
    async fn ping(&self) -> Result<()> { ni!("PostgresDbBackend::ping") }
}

/// Stub for MySQL / MariaDB.
pub struct MySqlDbBackend;
impl MySqlDbBackend {
    pub async fn connect(_url: &str) -> Result<Self> { ni!("MySqlDbBackend::connect") }
}
#[async_trait]
impl DatabaseBackend for MySqlDbBackend {
    async fn list_tables(&self) -> Result<Vec<TableInfo>> { ni!("MySqlDbBackend::list_tables") }
    async fn describe_table(&self, _s: Option<&str>, _t: &str) -> Result<TableInfo> { ni!("MySqlDbBackend::describe_table") }
    async fn query_full_table(&self, _s: Option<&str>, _t: &str, _b: usize) -> Result<Vec<DbRecord>> { ni!("MySqlDbBackend::query_full_table") }
    async fn query_incremental(&self, _s: Option<&str>, _t: &str, _o: &IncrementalOptions, _b: usize) -> Result<Vec<DbRecord>> { ni!("MySqlDbBackend::query_incremental") }
    async fn execute_query(&self, _sql: &str) -> Result<Vec<DbRecord>> { ni!("MySqlDbBackend::execute_query") }
    async fn ping(&self) -> Result<()> { ni!("MySqlDbBackend::ping") }
}

/// Stub for Oracle Database.
pub struct OracleDbBackend;
impl OracleDbBackend {
    pub async fn connect(_url: &str) -> Result<Self> { ni!("OracleDbBackend::connect") }
}
#[async_trait]
impl DatabaseBackend for OracleDbBackend {
    async fn list_tables(&self) -> Result<Vec<TableInfo>> { ni!("OracleDbBackend::list_tables") }
    async fn describe_table(&self, _s: Option<&str>, _t: &str) -> Result<TableInfo> { ni!("OracleDbBackend::describe_table") }
    async fn query_full_table(&self, _s: Option<&str>, _t: &str, _b: usize) -> Result<Vec<DbRecord>> { ni!("OracleDbBackend::query_full_table") }
    async fn query_incremental(&self, _s: Option<&str>, _t: &str, _o: &IncrementalOptions, _b: usize) -> Result<Vec<DbRecord>> { ni!("OracleDbBackend::query_incremental") }
    async fn execute_query(&self, _sql: &str) -> Result<Vec<DbRecord>> { ni!("OracleDbBackend::execute_query") }
    async fn ping(&self) -> Result<()> { ni!("OracleDbBackend::ping") }
}

/// Stub for ClickHouse (columnar OLAP).
pub struct ClickHouseDbBackend;
impl ClickHouseDbBackend {
    pub async fn connect(_url: &str) -> Result<Self> { ni!("ClickHouseDbBackend::connect") }
}
#[async_trait]
impl DatabaseBackend for ClickHouseDbBackend {
    async fn list_tables(&self) -> Result<Vec<TableInfo>> { ni!("ClickHouseDbBackend::list_tables") }
    async fn describe_table(&self, _s: Option<&str>, _t: &str) -> Result<TableInfo> { ni!("ClickHouseDbBackend::describe_table") }
    async fn query_full_table(&self, _s: Option<&str>, _t: &str, _b: usize) -> Result<Vec<DbRecord>> { ni!("ClickHouseDbBackend::query_full_table") }
    async fn query_incremental(&self, _s: Option<&str>, _t: &str, _o: &IncrementalOptions, _b: usize) -> Result<Vec<DbRecord>> { ni!("ClickHouseDbBackend::query_incremental") }
    async fn execute_query(&self, _sql: &str) -> Result<Vec<DbRecord>> { ni!("ClickHouseDbBackend::execute_query") }
    async fn ping(&self) -> Result<()> { ni!("ClickHouseDbBackend::ping") }
}
