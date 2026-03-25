use std::sync::Arc;
use anyhow::Result;
use crate::store::MetadataStore;

/// Backend configuration. Parsed from the PALANTIR_DB environment variable.
///
/// Format examples:
///   sqlite:///data/palantir.db
///   postgresql://user:pass@host:5432/palantir
///   mysql://user:pass@host:3306/palantir
///   oracle://user:pass@host:1521/palantir
///   mongodb://user:pass@host:27017/palantir
///   dynamodb://ap-east-1?table_prefix=palantir_
///   cassandra://host1,host2:9042?keyspace=palantir&dc=datacenter1
#[derive(Debug, Clone)]
pub enum StoreConfig {
    Sqlite    { path: String },
    Postgres  { url: String },
    Mysql     { url: String },
    Oracle    { url: String },
    MongoDB   { url: String, database: String },
    DynamoDB  { region: String, table_prefix: String, endpoint: Option<String> },
    Cassandra { hosts: Vec<String>, keyspace: String, datacenter: String },
}

impl StoreConfig {
    /// Read PALANTIR_DB env var; default to SQLite at ./data/palantir.db
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("PALANTIR_DB")
            .unwrap_or_else(|_| "sqlite:///data/palantir.db".into());
        Self::parse(&url)
    }

    pub fn parse(url: &str) -> Result<Self> {
        if let Some(path) = url.strip_prefix("sqlite://") {
            return Ok(Self::Sqlite { path: path.to_string() });
        }
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            return Ok(Self::Postgres { url: url.to_string() });
        }
        if url.starts_with("mysql://") {
            return Ok(Self::Mysql { url: url.to_string() });
        }
        if url.starts_with("oracle://") {
            return Ok(Self::Oracle { url: url.to_string() });
        }
        if url.starts_with("mongodb://") || url.starts_with("mongodb+srv://") {
            let database = url.rsplit('/').next().unwrap_or("palantir").to_string();
            return Ok(Self::MongoDB { url: url.to_string(), database });
        }
        if url.starts_with("dynamodb://") {
            let rest = url.trim_start_matches("dynamodb://");
            let (region_part, query) = rest.split_once('?').unwrap_or((rest, ""));
            let region = region_part.to_string();
            let table_prefix = query.split('&')
                .find_map(|p| p.strip_prefix("table_prefix="))
                .unwrap_or("palantir_").to_string();
            let endpoint = query.split('&')
                .find_map(|p| p.strip_prefix("endpoint="))
                .map(|s| s.to_string());
            return Ok(Self::DynamoDB { region, table_prefix, endpoint });
        }
        if url.starts_with("cassandra://") {
            let rest = url.trim_start_matches("cassandra://");
            let (hosts_part, query) = rest.split_once('?').unwrap_or((rest, ""));
            let hosts = hosts_part.split(',').map(|s| s.to_string()).collect();
            let keyspace = query.split('&')
                .find_map(|p| p.strip_prefix("keyspace="))
                .unwrap_or("palantir").to_string();
            let datacenter = query.split('&')
                .find_map(|p| p.strip_prefix("dc="))
                .unwrap_or("datacenter1").to_string();
            return Ok(Self::Cassandra { hosts, keyspace, datacenter });
        }
        anyhow::bail!("unsupported database URL scheme: {}", url)
    }
}

/// Factory: build the correct adapter from config and return it behind the trait.
/// Business code only ever holds `Arc<dyn MetadataStore>`.
pub async fn build_store(config: StoreConfig) -> Result<Arc<dyn MetadataStore>> {
    match config {
        StoreConfig::Sqlite { path } => {
            #[cfg(feature = "sqlite")]
            {
                use crate::adapters::sqlite::SqliteStore;
                return Ok(Arc::new(SqliteStore::open(&path).await?));
            }
            #[cfg(not(feature = "sqlite"))]
            anyhow::bail!("sqlite feature not compiled — add features=[\"sqlite\"] in Cargo.toml")
        }
        StoreConfig::Postgres { url } => {
            #[cfg(feature = "postgres")]
            {
                use crate::adapters::postgres::PostgresStore;
                return Ok(Arc::new(PostgresStore::connect(&url).await?));
            }
            #[cfg(not(feature = "postgres"))]
            anyhow::bail!("postgres feature not compiled — add features=[\"postgres\"] in Cargo.toml")
        }
        StoreConfig::Mysql { url } => {
            #[cfg(feature = "mysql")]
            {
                use crate::adapters::mysql::MySqlStore;
                return Ok(Arc::new(MySqlStore::connect(&url).await?));
            }
            #[cfg(not(feature = "mysql"))]
            anyhow::bail!("mysql feature not compiled — add features=[\"mysql\"] in Cargo.toml")
        }
        StoreConfig::Oracle { url } => {
            #[cfg(feature = "oracle")]
            {
                use crate::adapters::oracle::OracleStore;
                return Ok(Arc::new(OracleStore::connect(&url).await?));
            }
            #[cfg(not(feature = "oracle"))]
            anyhow::bail!("oracle feature not compiled — add features=[\"oracle\"] in Cargo.toml")
        }
        StoreConfig::MongoDB { url, database } => {
            #[cfg(feature = "mongodb")]
            {
                use crate::adapters::mongodb::MongoStore;
                return Ok(Arc::new(MongoStore::connect(&url, &database).await?));
            }
            #[cfg(not(feature = "mongodb"))]
            anyhow::bail!("mongodb feature not compiled — add features=[\"mongodb\"] in Cargo.toml")
        }
        StoreConfig::DynamoDB { region, table_prefix, endpoint } => {
            #[cfg(feature = "dynamodb")]
            {
                use crate::adapters::dynamodb::DynamoDbStore;
                return Ok(Arc::new(DynamoDbStore::new(&region, &table_prefix, endpoint.as_deref()).await?));
            }
            #[cfg(not(feature = "dynamodb"))]
            anyhow::bail!("dynamodb feature not compiled — add features=[\"dynamodb\"] in Cargo.toml")
        }
        StoreConfig::Cassandra { hosts, keyspace, datacenter } => {
            #[cfg(feature = "cassandra")]
            {
                use crate::adapters::cassandra::CassandraStore;
                return Ok(Arc::new(CassandraStore::connect(hosts, &keyspace, &datacenter).await?));
            }
            #[cfg(not(feature = "cassandra"))]
            anyhow::bail!("cassandra feature not compiled — add features=[\"cassandra\"] in Cargo.toml")
        }
    }
}
