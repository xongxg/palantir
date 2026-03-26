use async_trait::async_trait;
use anyhow::Result;
use crate::types::*;

/// The single port for all metadata operations.
/// Every backend (SQLite, PostgreSQL, MySQL, DynamoDB, …) implements this trait.
/// Business logic depends only on this trait — never on a concrete adapter.
#[async_trait]
pub trait MetadataStore: Send + Sync {

    // ── Projects ──────────────────────────────────────────────────────────────
    async fn create_project(&self, name: &str) -> Result<ProjectRow>;
    async fn list_projects(&self) -> Result<Vec<ProjectRow>>;
    async fn get_project(&self, id: &str) -> Result<Option<ProjectRow>>;
    async fn rename_project(&self, id: &str, name: &str) -> Result<()>;
    async fn delete_project(&self, id: &str) -> Result<()>;
    async fn project_stats(&self, project_id: &str) -> Result<(i64, Option<String>, String)>;
    async fn touch_project(&self, id: &str) -> Result<()>;

    // ── Folds (Bounded Context boundary) ─────────────────────────────────────
    async fn create_fold(&self, project_id: &str, name: &str, description: Option<&str>, fold_type: Option<&str>) -> Result<FoldRow>;
    async fn list_folds(&self, project_id: &str) -> Result<Vec<FoldRow>>;
    async fn list_shared_kernel_folds(&self) -> Result<Vec<serde_json::Value>>;
    async fn get_fold(&self, id: &str) -> Result<Option<FoldRow>>;
    async fn delete_fold(&self, id: &str) -> Result<()>;
    async fn fold_stats(&self, fold_id: &str) -> Result<(i64, i64, String)>;

    // ── Entity Types (Ontology Schema) ────────────────────────────────────────
    async fn create_entity_type(
        &self, name: &str, display_name: &str, color: &str, icon: &str,
        fold_id: Option<&str>, ddd_role: &str, namespace: Option<&str>,
    ) -> Result<EntityTypeRow>;
    async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>>;
    async fn list_entity_types_for_fold(&self, fold_id: &str) -> Result<Vec<EntityTypeRow>>;
    async fn update_entity_type_ddd_role(&self, et_id: &str, ddd_role: &str) -> Result<()>;
    async fn update_entity_type_fold(&self, et_id: &str, fold_id: Option<&str>) -> Result<()>;
    async fn update_entity_type_bc(&self, et_id: &str, bc_id: Option<&str>) -> Result<()>;
    /// Returns how many ontology objects exist for this ET
    async fn count_objects_for_et(&self, et_id: &str) -> Result<i64>;
    /// P1a: change lifecycle status; returns affected_datasets count
    async fn set_entity_type_status(&self, et_id: &str, status: &str) -> Result<i64>;
    async fn delete_entity_type(&self, id: &str) -> Result<()>;

    // ── Entity Fields ─────────────────────────────────────────────────────────
    async fn add_entity_field(
        &self, entity_type_id: &str, name: &str, data_type: &str,
        is_required: bool, classification: &str, sort_order: i64,
    ) -> Result<EntityFieldRow>;
    async fn list_entity_fields(&self, entity_type_id: &str) -> Result<Vec<EntityFieldRow>>;
    /// P0: check if changing field type/name is breaking; returns BreakingChangeInfo
    async fn check_field_type_change(&self, field_id: &str, new_type: &str) -> Result<BreakingChangeInfo>;
    /// P0: apply field type change with migration strategy ('drop' | 'cast')
    async fn apply_field_type_change(&self, field_id: &str, new_type: &str, strategy: &str) -> Result<SchemaMigrationRow>;
    /// P0: check if deleting a field is breaking
    async fn check_field_delete(&self, field_id: &str) -> Result<BreakingChangeInfo>;
    /// P0: apply field deletion with strategy (always 'drop')
    async fn apply_field_delete(&self, field_id: &str) -> Result<SchemaMigrationRow>;
    async fn delete_entity_field(&self, id: &str) -> Result<()>;
    /// P1b: data lineage — datasets feeding this ET
    async fn get_et_lineage(&self, et_id: &str) -> Result<serde_json::Value>;

    // ── Ontology Objects ──────────────────────────────────────────────────────
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

    // ── Ontology Links ────────────────────────────────────────────────────────
    async fn create_link(
        &self, from_id: &str, to_id: &str, rel_type: &str, dataset_id: Option<&str>,
    ) -> Result<OntologyLinkRow>;
    async fn list_links_for_object(&self, object_id: &str) -> Result<Vec<OntologyLinkRow>>;
    async fn list_links_for_object_enriched(&self, object_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn delete_link(&self, id: &str) -> Result<()>;

    // ── Data Sources ──────────────────────────────────────────────────────────
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

    // ── Sync Runs ─────────────────────────────────────────────────────────────
    async fn create_sync_run(&self, source_id: &str) -> Result<SyncRunRow>;
    async fn get_sync_run(&self, id: &str) -> Result<Option<SyncRunRow>>;
    async fn list_sync_runs(&self, source_id: &str) -> Result<Vec<SyncRunRow>>;
    async fn update_sync_run_progress(&self, id: &str, processed: i64, current_item: Option<&str>) -> Result<()>;
    async fn set_sync_run_status(&self, id: &str, status: &str) -> Result<()>;
    async fn finish_sync_run(
        &self, id: &str, status: &str, total_records: i64,
        error_message: Option<&str>, error_type: Option<&str>,
    ) -> Result<()>;

    // ── Datasets ──────────────────────────────────────────────────────────────
    async fn create_dataset(&self, source_id: &str, name: &str) -> Result<DatasetRow>;
    async fn list_all_datasets(&self) -> Result<Vec<serde_json::Value>>;
    async fn list_datasets(&self, source_id: &str) -> Result<Vec<DatasetRow>>;
    async fn list_datasets_with_count(&self, source_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn get_dataset(&self, id: &str) -> Result<Option<DatasetRow>>;

    // ── Dataset Versions ──────────────────────────────────────────────────────
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

    // ── Dataset Mappings ──────────────────────────────────────────────────────
    async fn save_object_type_mapping(
        &self, dataset_id: &str, et_id: &str, pk_col: &str,
        field_mapping: &str, sync_mode: &str,
    ) -> Result<()>;
    async fn update_dataset_sync_mode(&self, dataset_id: &str, sync_mode: &str) -> Result<()>;
    async fn list_mapped_dataset_ids(&self) -> Result<Vec<String>>;
    async fn get_object_type_mapping(&self, dataset_id: &str) -> Result<Option<serde_json::Value>>;

    // ── Link Type Mappings ────────────────────────────────────────────────────
    async fn save_link_type_mappings(&self, dataset_id: &str, links: &[LinkTypeMappingInput]) -> Result<()>;
    async fn get_link_type_mappings(&self, dataset_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn list_schema_links(&self) -> Result<Vec<serde_json::Value>>;
    async fn resolve_links_for_dataset(&self, dataset_id: &str) -> Result<usize>;

    // ── Bounded Contexts ──────────────────────────────────────────────────────
    async fn create_bounded_context(&self, fold_id: &str, name: &str, color: &str, auto_detected: bool) -> Result<BoundedContextRow>;
    async fn list_bounded_contexts(&self, fold_id: &str) -> Result<Vec<BoundedContextRow>>;
    async fn delete_bounded_context(&self, id: &str) -> Result<()>;

    async fn create_bc_relationship(
        &self, from_bc_id: &str, to_bc_id: &str, relationship_type: &str, notes: Option<&str>,
    ) -> Result<BcRelationshipRow>;
    async fn list_bc_relationships(&self, bc_id: &str) -> Result<Vec<BcRelationshipRow>>;
    async fn delete_bc_relationship(&self, id: &str) -> Result<()>;
    /// Context Map: all BC nodes + relationship edges for a project
    async fn get_context_map(&self, project_id: &str) -> Result<serde_json::Value>;

    /// P1: Union-Find BC inference — returns suggestions without writing
    async fn infer_child_bcs(&self, fold_id: &str) -> Result<serde_json::Value>;
    /// P1: Apply accepted suggestions — writes BCs and assigns ETs
    async fn apply_bc_suggestions(&self, fold_id: &str, suggestions: &[serde_json::Value]) -> Result<Vec<serde_json::Value>>;

    // ── System Interfaces (P2c) ───────────────────────────────────────────────
    async fn list_interfaces(&self) -> Result<Vec<serde_json::Value>>;
    async fn create_interface(&self, name: &str, description: Option<&str>) -> Result<InterfaceRow>;
    async fn delete_interface(&self, id: &str) -> Result<()>;
    async fn list_et_interfaces(&self, et_id: &str) -> Result<Vec<serde_json::Value>>;
    async fn add_et_interface(&self, et_id: &str, interface_id: &str) -> Result<()>;
    async fn remove_et_interface(&self, et_id: &str, interface_id: &str) -> Result<()>;

    // ── Platform Config ───────────────────────────────────────────────────────
    async fn get_platform_config(&self, key: &str) -> Result<Option<String>>;
    async fn set_platform_config(&self, key: &str, value: &str) -> Result<()>;
    async fn get_storage_config(&self) -> Result<serde_json::Value>;
    async fn set_storage_config(&self, cfg: &serde_json::Value) -> Result<()>;

    // ── Connectors (legacy graph) ─────────────────────────────────────────────
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
