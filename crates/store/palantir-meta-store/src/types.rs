// All Row types — pure data structures, no database dependency.
// These are the canonical DTOs returned by every MetadataStore adapter.

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct ConnectorRow {
    pub id: String,
    pub project_id: String,
    pub path: String,
    pub ns: String,
    pub schema_name: String,
    pub headers: Option<String>,
    pub samples: Option<String>,
    pub mapping_config: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EntityRow {
    pub id: String,
    pub project_id: String,
    pub entity_type: String,
    pub ddd_concept: String,
    pub label: String,
    pub properties: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BuildRow {
    pub id: String,
    pub project_id: String,
    pub created_at: String,
    pub entities: i64,
    pub relationships: i64,
    pub bounded_contexts: i64,
    pub applied_events: i64,
}

pub struct RelRow {
    pub project_id: String,
    pub from_id: String,
    pub to_id: String,
    pub kind: String,
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
    /// DDD role: 'aggregate_root' | 'entity' | 'value_object'
    pub ddd_role: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct EntityFieldRow {
    pub id: String,
    pub entity_type_id: String,
    pub name: String,
    pub data_type: String,
    pub is_required: bool,
    pub classification: String,
    pub sort_order: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OntologyObjectRow {
    pub id: String,
    pub entity_type_id: String,
    pub entity_type_name: String,
    pub label: String,
    pub fields: String, // JSON
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OntologyLinkRow {
    pub id: String,
    pub from_id: String,
    pub to_id: String,
    pub rel_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LinkTypeMappingInput {
    pub from_fk_col: String,
    pub to_entity_type_id: String,
    pub rel_type: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FoldRow {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub description: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DataSourceRow {
    pub id: String,
    pub fold_id: String,
    pub name: String,
    pub source_type: String,
    pub config: String,
    pub status: String,
    pub write_lock: Option<String>,
    pub last_sync_at: Option<String>,
    pub record_count: Option<i64>,
    pub created_at: String,
    pub deprecated: bool,
    pub deleted_at: Option<String>,
    pub group_id: Option<String>,
    /// snapshot | append | upsert
    pub sync_mode: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncRunRow {
    pub id: String,
    pub source_id: String,
    pub status: String,
    pub total_records: Option<i64>,
    pub processed: i64,
    pub current_item: Option<String>,
    pub error_message: Option<String>,
    pub error_type: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetRow {
    pub id: String,
    pub source_id: String,
    pub name: String,
    pub entity_type_id: Option<String>,
    pub current_version: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatasetVersionRow {
    pub id: String,
    pub dataset_id: String,
    pub version: i64,
    pub sync_run_id: String,
    pub status: String,
    pub schema_json: String,
    pub schema_change: Option<String>,
    pub total_rows: i64,
    pub is_current: bool,
    pub created_at: String,
    pub manifest_path: Option<String>,
}
