# palantir-meta-store — 多后端元数据存储抽象层设计

> **文档版本**：v0.2
> **状态**：已实现（阶段一完成）
> **创建**：2026-03-25 | **最后更新**：2026-03-25
> **受众**：后端团队

> ### 变更记录
> | 版本 | 日期 | 变更内容 |
> |------|------|----------|
> | v0.1 | 2026-03-25 | 初稿：palantir-meta-store 设计，adapter 作为 feature-flag 模块 |
> | v0.2 | 2026-03-25 | **架构演进**：各 adapter 独立为单独 crate（palantir-sqlite 等），`store/` 子目录集中管理；palantir-persistence 正式重命名为 palantir-sqlite；palantir-storage → palantir-dataset；palantir-ontology-manager → palantir-source-adapter |

---

## 一、背景与动机

### 1.1 现状

平台的所有元数据（Project、Fold、Entity Type、Dataset、Ontology Object 等）当前全部存储在 SQLite，由 `palantir-persistence` crate 的 `Db` 结构体直接封装 `SqlitePool`：

```rust
// 当前 palantir-persistence/src/db.rs
use sqlx::{Row, SqlitePool};

pub struct Db {
    pool: SqlitePool,   // 硬绑定 SQLite
}
```

`Db` 实现了约 80 个 `pub async fn` 方法，覆盖 Project、Fold、Entity Type、Dataset、Ontology 全域。

### 1.2 问题

- **无法切换后端**：SQLite 适合单机开发，生产环境需要 PostgreSQL；企业客户可能已有 MySQL 或 DynamoDB 基础设施；未来高写入场景可能需要 Cassandra
- **方言耦合**：代码中直接使用了 SQLite 专有函数（`strftime`、`json_patch`、`json_group_array`），迁移成本高
- **无抽象层**：业务层（`palantir-ingest-api`）直接依赖 `Db` 结构体，切换后端必须改业务代码

### 1.3 目标

新建 `palantir-meta-store` crate，作为**元数据存储的统一抽象层**：

- 业务层只依赖 trait，不感知后端
- 一个环境变量切换后端，零改代码
- 各后端按需编译（Cargo feature flag），不引入不必要的依赖
- 后续添加新后端只需新增一个 adapter，不改业务层

---

## 二、整体架构

### 2.1 设计模式：Port-Adapter

> **v0.2 演进说明**：各后端 adapter 从 palantir-meta-store 内部的 feature-flag 模块，演进为**独立 crate**，集中在 `crates/store/` 目录下。这样每个后端可独立演进、独立版本，且不强制其他后端跟随编译。

```
┌──────────────────────────────────────────────────────┐
│  业务层 palantir-ingest-api                           │
│  只看到：Arc<dyn MetadataStore>                       │
│  完全不感知后端                                        │
└────────────────────┬─────────────────────────────────┘
                     │ 依赖 trait
┌────────────────────▼─────────────────────────────────┐
│  crates/store/palantir-meta-store                     │
│  ┌─────────────────────────────────────────────────┐  │
│  │  MetadataStore trait（Port）· ~80 async fn      │  │
│  │  Row types（ProjectRow / FoldRow / ...）         │  │
│  │  StoreConfig + build_store 工厂                  │  │
│  └─────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────┘
           ↑ impl MetadataStore
┌──────────┬──────────┬────────────┬──────────┬─────────┐
│palantir  │palantir  │palantir    │palantir  │palantir │
│-sqlite   │-postgres │-mysql      │-dynamodb │-cassandra│
│✅ 实现   │🔲 stub   │🔲 stub     │🔲 stub   │🔲 stub  │
└──────────┴──────────┴────────────┴──────────┴─────────┘
 crates/store/ 下各自独立 crate，按需加入 workspace
```

### 2.2 crate 文件结构（v0.2 当前状态）

```
crates/
├── store/                              ← 元数据存储层（集中管理）
│   ├── palantir-meta-store/            ← Port（trait + types，零 DB 依赖）
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── types.rs               # 所有 Row 类型（唯一权威来源）
│   │       ├── store.rs               # MetadataStore trait
│   │       ├── config.rs              # StoreConfig + build_store 工厂
│   │       └── adapters/              # stub impls（编译通过，运行报错）
│   │           ├── sqlite.rs          # → 真实实现在 palantir-sqlite
│   │           ├── postgres.rs
│   │           ├── mysql.rs
│   │           ├── oracle.rs
│   │           ├── mongodb.rs
│   │           ├── dynamodb.rs
│   │           └── cassandra.rs
│   │
│   └── palantir-sqlite/               ← SQLite Adapter（✅ 完整实现）
│       └── src/
│           ├── lib.rs
│           └── db.rs                  # Db struct + 全部 SQL（~2300 行）
│
├── palantir-source-adapter/           ← 数据源接入适配器
├── palantir-dataset/                  ← Dataset 存储层
├── palantir-ingest-api/               ← Web API
└── palantir-agent/                    ← Agent 骨架
```

### 2.3 Workspace 变更（v0.2 当前状态）

```toml
# Cargo.toml（workspace root）
[workspace]
members = [
    # ── Metadata store layer ──────────────────────────────
    "crates/store/palantir-meta-store",
    "crates/store/palantir-sqlite",
    # 未来添加：
    # "crates/store/palantir-postgres",
    # "crates/store/palantir-mysql",

    # ── Data ingestion & dataset ──────────────────────────
    "crates/palantir-source-adapter",
    "crates/palantir-dataset",

    # ── Application ───────────────────────────────────────
    "crates/palantir-ingest-api",
    "crates/palantir-agent",
]
```

---

## 三、核心设计

### 3.1 Cargo.toml — Feature Flag 控制编译

```toml
# crates/palantir-meta-store/Cargo.toml
[package]
name    = "palantir-meta-store"
version = "0.1.0"
edition = "2024"

[features]
default   = ["sqlite"]
sqlite    = ["dep:sqlx", "sqlx?/sqlite"]
postgres  = ["dep:sqlx", "sqlx?/postgres"]
mysql     = ["dep:sqlx", "sqlx?/mysql"]
dynamodb  = ["dep:aws-sdk-dynamodb", "dep:aws-config"]
cassandra = ["dep:scylla"]

[dependencies]
anyhow      = "1"
serde       = { version = "1", features = ["derive"] }
serde_json  = "1"
uuid        = { version = "1", features = ["v4"] }
async-trait = "0.1"

# 可选后端依赖
sqlx             = { version = "0.8", features = ["runtime-tokio"], optional = true }
aws-sdk-dynamodb = { version = "1",   optional = true }
aws-config       = { version = "1",   optional = true }
scylla           = { version = "0.9", optional = true }
```

用户只安装自己需要的后端，其余**不编译进二进制**。

### 3.2 types.rs — 与后端无关的纯数据类型

```rust
// src/types.rs
// 所有 Row 类型从 palantir-persistence/db.rs 迁移过来
// 只有 Rust 原生类型，无任何数据库依赖

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityTypeRow {
    pub id: String,
    pub name: String,
    pub display_name: String,
    pub color: String,
    pub icon: String,
    pub fold_id: Option<String>,
    pub namespace: Option<String>,
    pub ddd_role: String,
    pub created_at: String,
}

// ... FoldRow, DataSourceRow, DatasetRow, OntologyObjectRow 等所有类型
```

**原则**：这些类型是 DTO（Data Transfer Object），任何后端的 adapter 都映射到这些统一类型后返回给业务层。

### 3.3 store.rs — MetadataStore trait（核心接口）

```rust
// src/store.rs
use async_trait::async_trait;
use anyhow::Result;
use crate::types::*;

#[async_trait]
pub trait MetadataStore: Send + Sync {

    // ── Projects ─────────────────────────────────────────────────────────────
    async fn create_project(&self, name: &str) -> Result<ProjectRow>;
    async fn list_projects(&self) -> Result<Vec<ProjectRow>>;
    async fn get_project(&self, id: &str) -> Result<Option<ProjectRow>>;
    async fn rename_project(&self, id: &str, name: &str) -> Result<()>;
    async fn delete_project(&self, id: &str) -> Result<()>;
    async fn project_stats(&self, project_id: &str) -> Result<(i64, Option<String>, String)>;
    async fn touch_project(&self, id: &str) -> Result<()>;

    // ── Folds（BC 边界）──────────────────────────────────────────────────────
    async fn create_fold(&self, project_id: &str, name: &str, description: Option<&str>) -> Result<FoldRow>;
    async fn list_folds(&self, project_id: &str) -> Result<Vec<FoldRow>>;
    async fn get_fold(&self, id: &str) -> Result<Option<FoldRow>>;
    async fn delete_fold(&self, id: &str) -> Result<()>;
    async fn fold_stats(&self, fold_id: &str) -> Result<(i64, i64, String)>;

    // ── Entity Types（Ontology Schema）───────────────────────────────────────
    async fn create_entity_type(
        &self, name: &str, display_name: &str, color: &str, icon: &str,
        fold_id: Option<&str>, ddd_role: &str, namespace: Option<&str>,
    ) -> Result<EntityTypeRow>;
    async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>>;
    async fn list_entity_types_for_fold(&self, fold_id: &str) -> Result<Vec<EntityTypeRow>>;
    async fn update_entity_type_ddd_role(&self, et_id: &str, ddd_role: &str) -> Result<()>;
    async fn update_entity_type_fold(&self, et_id: &str, fold_id: Option<&str>) -> Result<()>;
    async fn delete_entity_type(&self, id: &str) -> Result<()>;

    // ── Entity Fields ────────────────────────────────────────────────────────
    async fn add_entity_field(
        &self, entity_type_id: &str, name: &str, data_type: &str,
        is_required: bool, classification: &str,
    ) -> Result<EntityFieldRow>;
    async fn list_entity_fields(&self, entity_type_id: &str) -> Result<Vec<EntityFieldRow>>;
    async fn delete_entity_field(&self, id: &str) -> Result<()>;

    // ── Ontology Objects ─────────────────────────────────────────────────────
    async fn upsert_ontology_object(
        &self, entity_type_id: &str, entity_type_name: &str, label: &str,
        fields_json: &str, dataset_id: &str, external_id: &str, sync_mode: &str,
    ) -> Result<String>;
    async fn create_ontology_object_with_lineage(
        &self, entity_type_id: &str, entity_type_name: &str, label: &str,
        fields_json: &str, dataset_id: &str, sync_run_id: &str,
    ) -> Result<OntologyObjectRow>;
    async fn create_ontology_object(
        &self, entity_type_id: &str, entity_type_name: &str, label: &str, fields: &str,
    ) -> Result<OntologyObjectRow>;
    async fn list_ontology_objects(
        &self, entity_type_id: Option<&str>, limit: i64, offset: i64,
    ) -> Result<Vec<OntologyObjectRow>>;
    async fn get_ontology_object(&self, id: &str) -> Result<Option<OntologyObjectRow>>;
    async fn update_ontology_object(&self, id: &str, label: &str, fields: &str) -> Result<()>;
    async fn delete_ontology_object(&self, id: &str) -> Result<()>;
    async fn delete_ontology_objects_by_dataset(&self, dataset_id: &str) -> Result<()>;
    async fn get_ontology_graph(&self, project_id: Option<&str>) -> Result<serde_json::Value>;

    // ── Ontology Links ───────────────────────────────────────────────────────
    async fn create_link(
        &self, from_id: &str, to_id: &str, rel_type: &str, dataset_id: Option<&str>,
    ) -> Result<OntologyLinkRow>;
    async fn list_links_for_object(&self, object_id: &str) -> Result<Vec<OntologyLinkRow>>;
    async fn list_links_for_object_enriched(&self, object_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn delete_link(&self, id: &str) -> Result<()>;

    // ── Data Sources ─────────────────────────────────────────────────────────
    async fn create_data_source(
        &self, fold_id: &str, name: &str, source_type: &str,
        config: &str, group_id: Option<&str>, sync_mode: &str,
    ) -> Result<DataSourceRow>;
    async fn list_all_sources(&self) -> Result<Vec<DataSourceRow>>;
    async fn list_data_sources(&self, fold_id: &str) -> Result<Vec<DataSourceRow>>;
    async fn get_data_source(&self, id: &str) -> Result<Option<DataSourceRow>>;
    async fn update_data_source(
        &self, id: &str, name: &str, source_type: &str, config: &str, sync_mode: &str,
    ) -> Result<()>;
    async fn set_source_status(&self, id: &str, status: &str) -> Result<()>;
    async fn acquire_write_lock(&self, source_id: &str, run_id: &str) -> Result<bool>;
    async fn release_write_lock(&self, source_id: &str, status: &str, record_count: Option<i64>) -> Result<()>;
    async fn delete_data_source(&self, id: &str) -> Result<()>;
    async fn deprecate_data_source(&self, id: &str) -> Result<()>;
    async fn activate_data_source(&self, id: &str) -> Result<()>;

    // ── Sync Runs ────────────────────────────────────────────────────────────
    async fn create_sync_run(&self, source_id: &str) -> Result<SyncRunRow>;
    async fn get_sync_run(&self, id: &str) -> Result<Option<SyncRunRow>>;
    async fn list_sync_runs(&self, source_id: &str) -> Result<Vec<SyncRunRow>>;
    async fn update_sync_run_progress(&self, id: &str, processed: i64, current_item: Option<&str>) -> Result<()>;
    async fn set_sync_run_status(&self, id: &str, status: &str) -> Result<()>;
    async fn finish_sync_run(
        &self, id: &str, status: &str, total_records: i64,
        error_message: Option<&str>, error_type: Option<&str>,
    ) -> Result<()>;

    // ── Datasets ─────────────────────────────────────────────────────────────
    async fn create_dataset(&self, source_id: &str, name: &str) -> Result<DatasetRow>;
    async fn list_all_datasets(&self) -> Result<Vec<serde_json::Value>>;
    async fn list_datasets(&self, source_id: &str) -> Result<Vec<DatasetRow>>;
    async fn list_datasets_with_count(&self, source_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn get_dataset(&self, id: &str) -> Result<Option<DatasetRow>>;

    // ── Dataset Versions ─────────────────────────────────────────────────────
    async fn create_dataset_version(
        &self, dataset_id: &str, sync_run_id: &str, schema_json: &str,
    ) -> Result<DatasetVersionRow>;
    async fn commit_dataset_version(
        &self, version_id: &str, total_rows: i64, manifest_path: Option<&str>,
    ) -> Result<()>;
    async fn abort_dataset_version(&self, version_id: &str) -> Result<()>;
    async fn update_version_manifest_path(&self, version_id: &str, path: &str) -> Result<()>;
    async fn list_dataset_versions(&self, dataset_id: &str) -> Result<Vec<DatasetVersionRow>>;
    async fn rollback_dataset_version(&self, dataset_id: &str, version: i64) -> Result<()>;
    async fn get_prev_committed_schema(&self, dataset_id: &str, before_version: i64) -> Result<Option<String>>;
    async fn set_version_schema_change(&self, version_id: &str, change: &str) -> Result<()>;
    async fn old_dataset_versions(&self, dataset_id: &str, keep: i64) -> Result<Vec<DatasetVersionRow>>;
    async fn gc_version(&self, version_id: &str) -> Result<()>;
    async fn get_current_dataset_version(&self, dataset_id: &str) -> Result<Option<DatasetVersionRow>>;
    async fn list_dataset_records(&self, dataset_id: &str, limit: i64, offset: i64) -> Result<Vec<OntologyObjectRow>>;
    async fn count_dataset_records(&self, dataset_id: &str) -> Result<i64>;

    // ── Dataset Mappings ─────────────────────────────────────────────────────
    async fn save_object_type_mapping(
        &self, dataset_id: &str, et_id: &str, pk_col: &str,
        field_mapping: &str, sync_mode: &str,
    ) -> Result<()>;
    async fn update_dataset_sync_mode(&self, dataset_id: &str, sync_mode: &str) -> Result<()>;
    async fn list_mapped_dataset_ids(&self) -> Result<Vec<String>>;
    async fn get_object_type_mapping(&self, dataset_id: &str) -> Result<Option<serde_json::Value>>;

    // ── Link Type Mappings ───────────────────────────────────────────────────
    async fn save_link_type_mappings(&self, dataset_id: &str, links: &[LinkTypeMappingInput]) -> Result<()>;
    async fn get_link_type_mappings(&self, dataset_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn list_schema_links(&self) -> Result<Vec<serde_json::Value>>;
    async fn resolve_links_for_dataset(&self, dataset_id: &str) -> Result<usize>;

    // ── Platform Config ──────────────────────────────────────────────────────
    async fn get_platform_config(&self, key: &str) -> Result<Option<String>>;
    async fn set_platform_config(&self, key: &str, value: &str) -> Result<()>;
    async fn get_storage_config(&self) -> Result<serde_json::Value>;
    async fn set_storage_config(&self, cfg: &serde_json::Value) -> Result<()>;

    // ── Connectors（legacy graph）────────────────────────────────────────────
    async fn save_connector(&self, c: &ConnectorRow) -> Result<()>;
    async fn update_connector_metadata(&self, id: &str, headers: &str, samples: &str) -> Result<()>;
    async fn save_connector_mapping(&self, id: &str, config_json: &str) -> Result<()>;
    async fn load_connectors(&self, project_id: &str) -> Result<Vec<ConnectorRow>>;
    async fn delete_connector(&self, id: &str) -> Result<()>;
    async fn upsert_entity(&self, e: &EntityRow) -> Result<()>;
    async fn upsert_relationship(&self, r: &RelRow) -> Result<()>;
    async fn load_entities(&self, project_id: &str) -> Result<Vec<EntityRow>>;
    async fn load_relationships(&self, project_id: &str) -> Result<Vec<RelRow>>;
    async fn clear_project_graph(&self, project_id: &str) -> Result<()>;
    async fn save_build(&self, b: &BuildRow) -> Result<()>;
    async fn list_builds(&self, project_id: &str) -> Result<Vec<BuildRow>>;
}
```

### 3.4 config.rs — 后端选择与工厂函数

```rust
// src/config.rs
use std::sync::Arc;
use anyhow::Result;
use crate::store::MetadataStore;

/// 从环境变量 PALANTIR_DB 解析，格式：
///   sqlite:///path/to/data.db
///   postgresql://user:pass@host:5432/palantir
///   mysql://user:pass@host:3306/palantir
///   dynamodb://ap-east-1?table_prefix=palantir_
///   cassandra://host1,host2:9042?keyspace=palantir
pub enum StoreConfig {
    #[cfg(feature = "sqlite")]
    Sqlite   { path: String },

    #[cfg(feature = "postgres")]
    Postgres { url: String },

    #[cfg(feature = "mysql")]
    Mysql    { url: String },

    #[cfg(feature = "dynamodb")]
    DynamoDB { region: String, table_prefix: String, endpoint: Option<String> },

    #[cfg(feature = "cassandra")]
    Cassandra { hosts: Vec<String>, keyspace: String, datacenter: String },
}

impl StoreConfig {
    pub fn from_env() -> Result<Self> {
        let url = std::env::var("PALANTIR_DB")
            .unwrap_or_else(|_| "sqlite:///data/palantir.db".into());
        Self::parse(&url)
    }

    pub fn parse(url: &str) -> Result<Self> {
        if url.starts_with("sqlite://") {
            #[cfg(feature = "sqlite")]
            return Ok(Self::Sqlite { path: url.trim_start_matches("sqlite://").to_string() });
        }
        if url.starts_with("postgresql://") || url.starts_with("postgres://") {
            #[cfg(feature = "postgres")]
            return Ok(Self::Postgres { url: url.to_string() });
        }
        // ... 其他解析
        anyhow::bail!("unsupported or uncompiled backend: {}", url)
    }
}

/// 工厂函数：业务层调用一次，拿到 Arc<dyn MetadataStore>，此后不再关心后端
pub async fn build_store(config: StoreConfig) -> Result<Arc<dyn MetadataStore>> {
    match config {
        #[cfg(feature = "sqlite")]
        StoreConfig::Sqlite { path } => {
            use crate::adapters::sqlite::SqliteStore;
            Ok(Arc::new(SqliteStore::open(&path).await?))
        }

        #[cfg(feature = "postgres")]
        StoreConfig::Postgres { url } => {
            use crate::adapters::postgres::PostgresStore;
            Ok(Arc::new(PostgresStore::connect(&url).await?))
        }

        #[cfg(feature = "dynamodb")]
        StoreConfig::DynamoDB { region, table_prefix, endpoint } => {
            use crate::adapters::dynamodb::DynamoDbStore;
            Ok(Arc::new(DynamoDbStore::new(region, table_prefix, endpoint).await?))
        }

        #[cfg(feature = "cassandra")]
        StoreConfig::Cassandra { hosts, keyspace, datacenter } => {
            use crate::adapters::cassandra::CassandraStore;
            Ok(Arc::new(CassandraStore::connect(hosts, keyspace, datacenter).await?))
        }

        #[allow(unreachable_patterns)]
        _ => anyhow::bail!("backend not compiled in — check Cargo features"),
    }
}
```

### 3.5 业务层初始化（palantir-ingest-api）

```rust
// main.rs 启动时
static STORE: OnceLock<Arc<dyn MetadataStore>> = OnceLock::new();

async fn main() {
    let config = StoreConfig::from_env().expect("invalid PALANTIR_DB");
    let store  = build_store(config).await.expect("failed to connect to metadata store");
    STORE.set(store).unwrap();
    // ...
}

// 业务代码完全不知道后端
pub fn db() -> &'static dyn MetadataStore {
    STORE.get().unwrap().as_ref()
}

// 调用方式不变
db().create_entity_type(...).await?;
db().list_datasets(...).await?;
```

---

## 四、各后端 Adapter 设计

### 4.1 SQLite Adapter（第一阶段，现有代码迁移）

**工作量**：最小，将现有 `Db` struct 改名为 `SqliteStore`，实现 `MetadataStore` trait

```rust
// adapters/sqlite.rs
#[cfg(feature = "sqlite")]
pub struct SqliteStore { pool: sqlx::SqlitePool }

#[cfg(feature = "sqlite")]
impl SqliteStore {
    pub async fn open(path: &str) -> anyhow::Result<Self> {
        // 现有 Db::open() 逻辑迁移过来
    }
}

#[cfg(feature = "sqlite")]
#[async_trait]
impl MetadataStore for SqliteStore {
    // 所有方法体直接从现有 db.rs 迁移，无需修改
}
```

**SQL 方言说明**：SQLite adapter 保留现有 SQL，不做任何改动。

### 4.2 PostgreSQL Adapter（第二阶段）

**工作量**：中等，主要处理 SQL 方言差异

| SQLite | PostgreSQL |
|--------|-----------|
| `?` 占位符 | `$1, $2, ...` |
| `strftime('%Y-%m-%dT%H:%M:%SZ', 'now')` | `NOW()::text` |
| `json_patch(a, b)` | `a \|\| b`（jsonb operator）|
| `json_group_array(x)` | `json_agg(x)` |
| `INSERT OR IGNORE` | `INSERT ... ON CONFLICT DO NOTHING` |
| `INSERT OR REPLACE` | `INSERT ... ON CONFLICT DO UPDATE` |

PostgreSQL 的 pool 类型改为 `sqlx::PgPool`，其余逻辑相同。

### 4.3 DynamoDB Adapter（第三阶段）

**工作量**：较大，需要重新设计数据模型

DynamoDB 无 JOIN，采用**单表设计**（Single Table Design），用 `PK + SK` 编码所有实体：

```
PK                        SK                      数据
─────────────────────────────────────────────────────────────────────
PROJECT#{id}              META                    name, created_at
PROJECT#{id}              FOLD#{fold_id}           fold_name, fold_type
FOLD#{fold_id}            ET#{et_id}              display_name, status
ET#{et_id}                FIELD#{field_name}      field_type, required
DATASET#{dataset_id}      META                    name, source_id, fold_id
DATASET#{dataset_id}      MAPPING                 entity_type_id, sync_mode
```

查询模式：

```
"列出某 project 的所有 fold"  → Query PK=PROJECT#{id}, SK begins_with FOLD#
"获取某 ET 的所有字段"         → Query PK=ET#{id}, SK begins_with FIELD#
"跨 fold 查询所有 ET"          → GSI: GSI_PK=ET_TYPE, Scan
```

**关键限制**：
- 不支持任意 JOIN → 部分复杂查询需多次 round-trip 或 GSI
- 事务仅限同一 partition key 内
- 需要预先规划好所有 access pattern

### 4.4 Cassandra Adapter（第四阶段）

**工作量**：最大，需要按查询路径建多张表

Cassandra 的核心原则是**按查询建表**，每种 access pattern 对应一张物理表（允许数据冗余）：

```sql
-- 按 fold 查 Entity Type（主要路径）
CREATE TABLE entity_types_by_fold (
    fold_id      text,
    created_at   timestamp,
    et_id        text,
    display_name text,
    status       text,
    PRIMARY KEY (fold_id, created_at, et_id)
) WITH CLUSTERING ORDER BY (created_at DESC);

-- 全局查 Entity Type
CREATE TABLE entity_types_global (
    et_id        text PRIMARY KEY,
    display_name text,
    fold_id      text,
    status       text
);

-- 写入时需同时写两张表（应用层保证最终一致）
```

**适用场景**：仅推荐在**超高写入量（每秒 10 万+）+ 多数据中心**场景下使用。

**不适用场景**：元数据量级的存储。Cassandra 的复杂性收益在此场景下为负。

---

## 五、后端选型建议

| 场景 | 推荐后端 | 理由 |
|------|---------|------|
| 本地开发 / 单机部署 | **SQLite** | 零配置，文件即数据库 |
| 生产环境（主流） | **PostgreSQL** | 成熟、工具丰富、完整事务支持 |
| 已有 AWS 基础设施 | **DynamoDB** | Serverless、自动扩缩、免运维 |
| 已有 MySQL 基础设施 | **MySQL** | 方言差异最小，迁移成本低 |
| 超高写入 + 多机房 | **Cassandra/ScyllaDB** | 水平扩展，但元数据场景不推荐 |

> **注意**：当前元数据 Schema 是**关系型**的（多个 FK 约束、JOIN 查询），PostgreSQL/MySQL 是阻力最小的生产选型。DynamoDB 和 Cassandra 需要重新建模，适合有强烈基础设施约束的场景。

---

## 六、迁移路径

### 阶段一（立即可做）：抽 trait，不改实现

```
1. 新建 palantir-meta-store crate
2. 将 types.rs（Row 类型）从 palantir-persistence 迁移过来
3. 定义 MetadataStore trait
4. SqliteStore 实现 MetadataStore（方法体直接从 db.rs 复制）
5. palantir-persistence 改为 palantir-meta-store 的薄包装（pub use palantir_meta_store::*）
6. palantir-ingest-api 的 db() 函数返回 Arc<dyn MetadataStore>
7. 业务代码：零改动（方法签名完全一致）
```

**效果**：SQLite 继续工作，同时具备了切换后端的能力。

### 阶段二：添加 PostgreSQL Adapter

```
1. 在 adapters/postgres.rs 实现 PostgresStore
2. 处理约 10 处 SQL 方言差异
3. 通过 PALANTIR_DB=postgresql://... 切换
```

### 阶段三（按需）：DynamoDB / Cassandra

```
1. 设计适合各后端的 access pattern
2. 实现 adapter（数据模型重新建模）
3. 提供数据迁移工具（从 SQLite dump → 新后端 import）
```

---

## 七、palantir-persistence 的处置（v0.2 已完成）

**已执行**（2026-03-25）：

| 决策 | 结果 |
|------|------|
| `palantir-persistence` 重命名 | → `palantir-sqlite`，移至 `crates/store/palantir-sqlite/` |
| Row 类型重复定义 | → 统一到 `palantir-meta-store/src/types.rs`，db.rs 改为 `use palantir_meta_store::*` |
| 循环依赖（palantir-meta-store → palantir-sqlite → palantir-meta-store） | → palantir-meta-store 的 sqlite adapter 改为 stub，真实实现在 palantir-sqlite 独立 crate |
| `palantir-ingest-api` 依赖 | → 直接依赖 `palantir-sqlite`（具体实现），未来可切换为依赖 `palantir-meta-store` + env var |

---

## 八、验收标准

| 阶段 | 验收项 |
|------|-------|
| 阶段一 | `PALANTIR_DB=sqlite:///... cargo run` 行为与当前完全一致 |
| 阶段一 | 编译时不指定 feature 默认使用 sqlite；指定 `--features postgres` 编译通过 |
| 阶段二 | `PALANTIR_DB=postgresql://...` 切换后所有 API 行为一致 |
| 阶段二 | SQLite 和 PostgreSQL 的 integration test 共用同一套测试用例（测 trait，不测 impl）|
| 阶段三 | 新 adapter 实现后，现有 integration test 无需修改即可对新后端运行 |

---

## 九、内存集群扩展（Geode / Hazelcast / Redis）

### 9.1 应用场景

元数据中有**热数据**和**冷数据**之分：

| 类型 | 示例 | 特征 |
|------|------|------|
| 热数据 | Entity Type Schema、Fold 结构、Platform Config | 每次 API 请求都可能读取，变更频率极低 |
| 冷数据 | 历史 Sync Run、旧 Dataset Version、审计日志 | 按需查询，不在关键路径上 |

将热数据装入内存集群，可以将这些路径的延迟从 ms 级降到 μs 级，且在多实例部署时共享一致的缓存。

### 9.2 两种集成方式

#### 方式一：CachedStore 包装层（推荐，当前最可行）

不依赖外部集群，用进程内 `DashMap` 实现，零运维成本：

```rust
pub struct CachedStore<S: MetadataStore> {
    inner: S,                              // 底层 DB（SQLite / PostgreSQL / ...）
    cache: Arc<DashMap<String, CacheEntry>>,
}

// 热路径走缓存，写操作直写 inner 并失效缓存
#[async_trait]
impl<S: MetadataStore> MetadataStore for CachedStore<S> {
    async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>> {
        if let Some(cached) = self.cache.get("entity_types") {
            return Ok(cached.value().clone());
        }
        let result = self.inner.list_entity_types().await?;
        self.cache.insert("entity_types".into(), CacheEntry::new(result.clone()));
        Ok(result)
    }
    async fn create_entity_type(&self, ...) -> Result<EntityTypeRow> {
        let row = self.inner.create_entity_type(...).await?;
        self.cache.remove("entity_types");  // invalidate
        Ok(row)
    }
    // ...
}
```

业务层完全不感知缓存存在：

```rust
// 启动时
let db_store  = build_store(StoreConfig::from_env()?).await?;
let store     = Arc::new(CachedStore::new(db_store));  // 套一层缓存
STORE.set(store).unwrap();
```

#### 方式二：内存集群作为独立 Adapter（分布式场景）

当多个服务实例需要共享缓存时，接入 Hazelcast / Redis / Geode：

```
StoreConfig::Redis     { url: String }          -- 最轻量，适合缓存层
StoreConfig::Hazelcast { cluster: String }       -- 分布式内存网格
StoreConfig::Geode     { locator: String }       -- Apache Geode / GemFire
```

这三者在 `MetadataStore` trait 下的实现策略：
- **Redis**：直接用 Hash / String 存储序列化 JSON，适合 Cache-Aside
- **Hazelcast**：原生支持 Map / MultiMap，可直接存对象，支持 entry processor
- **Geode**：支持 Region 分区，适合超大规模元数据（百万级 Entity Type）

### 9.3 混合架构（推荐的演进路径）

```
请求
  │
  ▼
CachedStore（进程内 DashMap）       ← 第一层：μs 级，零网络
  │ miss
  ▼
Redis / Hazelcast（集群内存）        ← 第二层：ms 级，多实例共享
  │ miss
  ▼
PostgreSQL / SQLite（持久化）        ← 第三层：持久化，truth source
```

### 9.4 实现优先级

| 阶段 | 内容 |
|------|------|
| 现在 | 无缓存，直连 SQLite，够用 |
| 多实例部署时 | 加 `CachedStore<SqliteStore>` 进程内缓存，热数据 TTL 60s |
| 真正分布式时 | 接入 Redis adapter，替换进程内缓存 |
| 超大规模时 | 考虑 Hazelcast/Geode，按需决策 |
