pub mod db;

// Re-export all Row types from palantir-meta-store (canonical source of truth)
pub use palantir_meta_store::{
    BuildRow, ConnectorRow, DataSourceRow, DatasetRow, DatasetVersionRow,
    EntityFieldRow, EntityRow, EntityTypeRow, FoldRow, LinkTypeMappingInput,
    OntologyLinkRow, OntologyObjectRow, ProjectRow, RelRow, SyncRunRow,
    MetadataStore, StoreConfig, build_store,
};

// Keep Db as the concrete SQLite handle for backwards compatibility
pub use db::Db;
