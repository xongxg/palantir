//! ApiBackend — pull records from a remote HTTP API.
//!
//! Designed for: REST (JSON/XML), GraphQL, OData, SOAP (legacy).
//!
//! Usage pattern:
//!   1. build backend with auth config
//!   2. discover_schema() → ApiSchema (field names + types inferred from sample)
//!   3. fetch_all() or fetch_page() in a loop → Vec<ApiRecord>
//!   4. Caller writes results to a Dataset version via DatasetWriter
//!
//! Pagination styles supported (as stubs):
//!   - Page-number:  ?page=1&size=100
//!   - Cursor-based: ?cursor=<token>
//!   - Offset-based: ?offset=0&limit=100
//!   - Link-header:  RFC 5988 `Link: <url>; rel="next"`
//!
//! All implementations are STUBS — return "not yet implemented".

use anyhow::Result;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single record fetched from the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiRecord {
    pub fields: std::collections::HashMap<String, Value>,
}

/// Inferred schema from an API response sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiSchema {
    pub fields: Vec<ApiFieldInfo>,
    /// JSON path to the records array in the response, e.g. "data.items".
    pub records_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiFieldInfo {
    pub name: String,
    /// Inferred type: "string" | "number" | "boolean" | "object" | "array".
    pub inferred_type: String,
    pub nullable: bool,
}

/// Authentication method for the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ApiAuth {
    None,
    BearerToken { token: String },
    BasicAuth { username: String, password: String },
    ApiKey { header_name: String, key: String },
    OAuth2ClientCredentials { token_url: String, client_id: String, client_secret: String },
}

/// Pagination strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PaginationStrategy {
    None,
    PageNumber { page_param: String, size_param: String, page_size: usize },
    Cursor { cursor_param: String, next_cursor_path: String },
    Offset { offset_param: String, limit_param: String, page_size: usize },
    LinkHeader,
}

/// Pluggable HTTP API backend.
///
/// Implement this trait to connect a new API source.
/// Current status: ALL implementations are stubs.
#[async_trait]
pub trait ApiBackend: Send + Sync {
    /// Fetch a small sample from the API and infer the schema.
    async fn discover_schema(&self) -> Result<ApiSchema>;

    /// Fetch all records (handles pagination internally).
    async fn fetch_all(&self) -> Result<Vec<ApiRecord>>;

    /// Fetch a single page/batch given an optional continuation token.
    /// Returns `(records, next_cursor)`. `next_cursor = None` means last page.
    async fn fetch_page(&self, cursor: Option<&str>) -> Result<(Vec<ApiRecord>, Option<String>)>;

    /// Test connectivity — performs a lightweight request (e.g. HEAD or minimal GET).
    async fn ping(&self) -> Result<()>;
}

// ── Stubs ─────────────────────────────────────────────────────────────────────

macro_rules! ni {
    ($name:expr) => {
        anyhow::bail!("{}: not yet implemented", $name)
    };
}

/// Stub for a generic REST/JSON API.
pub struct RestApiBackend;
impl RestApiBackend {
    pub fn new(
        _base_url: &str,
        _auth: ApiAuth,
        _pagination: PaginationStrategy,
        _records_path: &str,
    ) -> Result<Self> {
        ni!("RestApiBackend::new")
    }
}
#[async_trait]
impl ApiBackend for RestApiBackend {
    async fn discover_schema(&self) -> Result<ApiSchema> { ni!("RestApiBackend::discover_schema") }
    async fn fetch_all(&self) -> Result<Vec<ApiRecord>> { ni!("RestApiBackend::fetch_all") }
    async fn fetch_page(&self, _cursor: Option<&str>) -> Result<(Vec<ApiRecord>, Option<String>)> { ni!("RestApiBackend::fetch_page") }
    async fn ping(&self) -> Result<()> { ni!("RestApiBackend::ping") }
}

/// Stub for GraphQL API.
pub struct GraphQlApiBackend;
impl GraphQlApiBackend {
    pub fn new(_endpoint: &str, _auth: ApiAuth, _query: &str) -> Result<Self> {
        ni!("GraphQlApiBackend::new")
    }
}
#[async_trait]
impl ApiBackend for GraphQlApiBackend {
    async fn discover_schema(&self) -> Result<ApiSchema> { ni!("GraphQlApiBackend::discover_schema") }
    async fn fetch_all(&self) -> Result<Vec<ApiRecord>> { ni!("GraphQlApiBackend::fetch_all") }
    async fn fetch_page(&self, _cursor: Option<&str>) -> Result<(Vec<ApiRecord>, Option<String>)> { ni!("GraphQlApiBackend::fetch_page") }
    async fn ping(&self) -> Result<()> { ni!("GraphQlApiBackend::ping") }
}

/// Stub for OData v4 APIs (SAP, Dynamics 365, SharePoint).
pub struct ODataApiBackend;
impl ODataApiBackend {
    pub fn new(_service_root: &str, _auth: ApiAuth) -> Result<Self> {
        ni!("ODataApiBackend::new")
    }
}
#[async_trait]
impl ApiBackend for ODataApiBackend {
    async fn discover_schema(&self) -> Result<ApiSchema> { ni!("ODataApiBackend::discover_schema") }
    async fn fetch_all(&self) -> Result<Vec<ApiRecord>> { ni!("ODataApiBackend::fetch_all") }
    async fn fetch_page(&self, _cursor: Option<&str>) -> Result<(Vec<ApiRecord>, Option<String>)> { ni!("ODataApiBackend::fetch_page") }
    async fn ping(&self) -> Result<()> { ni!("ODataApiBackend::ping") }
}
