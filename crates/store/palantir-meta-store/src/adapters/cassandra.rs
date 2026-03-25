#![cfg(feature = "cassandra")]
//! CassandraStore — stub adapter, not yet implemented.
//! Set PALANTIR_DB=cassandra://... and implement this module to activate.

use anyhow::Result;
use async_trait::async_trait;
use crate::{store::MetadataStore, types::*};

pub struct CassandraStore;

impl CassandraStore {
    pub async fn connect(_hosts: Vec<String>, _keyspace: &str, _dc: &str) -> Result<Self> {
        anyhow::bail!(
            "CassandraStore is not yet implemented. \
             Contributions welcome — implement crates/palantir-db/src/adapters/cassandra.rs"
        )
    }
}

macro_rules! ni {
    () => { anyhow::bail!("CassandraStore: not yet implemented") };
}

#[async_trait]
impl MetadataStore for CassandraStore {
    async fn create_project(&self, _name: &str) -> Result<ProjectRow> { ni!() }
    async fn list_projects(&self) -> Result<Vec<ProjectRow>> { ni!() }
    async fn get_project(&self, _id: &str) -> Result<Option<ProjectRow>> { ni!() }
    async fn rename_project(&self, _id: &str, _name: &str) -> Result<()> { ni!() }
    async fn delete_project(&self, _id: &str) -> Result<()> { ni!() }
    async fn project_stats(&self, _pid: &str) -> Result<(i64, Option<String>, String)> { ni!() }
    async fn touch_project(&self, _id: &str) -> Result<()> { ni!() }

    async fn create_fold(&self, _pid: &str, _name: &str, _desc: Option<&str>) -> Result<FoldRow> { ni!() }
    async fn list_folds(&self, _pid: &str) -> Result<Vec<FoldRow>> { ni!() }
    async fn get_fold(&self, _id: &str) -> Result<Option<FoldRow>> { ni!() }
    async fn delete_fold(&self, _id: &str) -> Result<()> { ni!() }
    async fn fold_stats(&self, _id: &str) -> Result<(i64, i64, String)> { ni!() }

    async fn create_entity_type(&self, _n: &str, _dn: &str, _c: &str, _i: &str, _fid: Option<&str>, _role: &str, _ns: Option<&str>) -> Result<EntityTypeRow> { ni!() }
    async fn list_entity_types(&self) -> Result<Vec<EntityTypeRow>> { ni!() }
    async fn list_entity_types_for_fold(&self, _fid: &str) -> Result<Vec<EntityTypeRow>> { ni!() }
    async fn update_entity_type_ddd_role(&self, _id: &str, _role: &str) -> Result<()> { ni!() }
    async fn update_entity_type_fold(&self, _id: &str, _fid: Option<&str>) -> Result<()> { ni!() }
    async fn delete_entity_type(&self, _id: &str) -> Result<()> { ni!() }

    async fn add_entity_field(&self, _etid: &str, _n: &str, _dt: &str, _req: bool, _cls: &str) -> Result<EntityFieldRow> { ni!() }
    async fn list_entity_fields(&self, _etid: &str) -> Result<Vec<EntityFieldRow>> { ni!() }
    async fn delete_entity_field(&self, _id: &str) -> Result<()> { ni!() }

    async fn upsert_ontology_object(&self, _etid: &str, _etn: &str, _l: &str, _f: &str, _dsid: &str, _eid: &str, _sm: &str) -> Result<String> { ni!() }
    async fn create_ontology_object_with_lineage(&self, _etid: &str, _etn: &str, _l: &str, _f: &str, _dsid: &str, _srid: &str) -> Result<OntologyObjectRow> { ni!() }
    async fn create_ontology_object(&self, _etid: &str, _etn: &str, _l: &str, _f: &str) -> Result<OntologyObjectRow> { ni!() }
    async fn list_ontology_objects(&self, _etid: Option<&str>, _lim: i64, _off: i64) -> Result<Vec<OntologyObjectRow>> { ni!() }
    async fn get_ontology_object(&self, _id: &str) -> Result<Option<OntologyObjectRow>> { ni!() }
    async fn update_ontology_object(&self, _id: &str, _l: &str, _f: &str) -> Result<()> { ni!() }
    async fn delete_ontology_object(&self, _id: &str) -> Result<()> { ni!() }
    async fn delete_ontology_objects_by_dataset(&self, _dsid: &str) -> Result<()> { ni!() }
    async fn get_ontology_graph(&self, _pid: Option<&str>) -> Result<serde_json::Value> { ni!() }

    async fn create_link(&self, _fid: &str, _tid: &str, _rt: &str, _dsid: Option<&str>) -> Result<OntologyLinkRow> { ni!() }
    async fn list_links_for_object(&self, _oid: &str) -> Result<Vec<OntologyLinkRow>> { ni!() }
    async fn list_links_for_object_enriched(&self, _oid: &str) -> Result<Vec<serde_json::Value>> { ni!() }
    async fn delete_link(&self, _id: &str) -> Result<()> { ni!() }

    async fn create_data_source(&self, _fid: &str, _n: &str, _st: &str, _cfg: &str, _gid: Option<&str>, _sm: &str) -> Result<DataSourceRow> { ni!() }
    async fn list_all_sources(&self) -> Result<Vec<DataSourceRow>> { ni!() }
    async fn list_data_sources(&self, _fid: &str) -> Result<Vec<DataSourceRow>> { ni!() }
    async fn get_data_source(&self, _id: &str) -> Result<Option<DataSourceRow>> { ni!() }
    async fn update_data_source(&self, _id: &str, _n: &str, _st: &str, _cfg: &str, _sm: &str) -> Result<()> { ni!() }
    async fn set_source_status(&self, _id: &str, _s: &str) -> Result<()> { ni!() }
    async fn acquire_write_lock(&self, _sid: &str, _rid: &str) -> Result<bool> { ni!() }
    async fn release_write_lock(&self, _sid: &str, _s: &str, _rc: Option<i64>) -> Result<()> { ni!() }
    async fn delete_data_source(&self, _id: &str) -> Result<()> { ni!() }
    async fn deprecate_data_source(&self, _id: &str) -> Result<()> { ni!() }
    async fn activate_data_source(&self, _id: &str) -> Result<()> { ni!() }

    async fn create_sync_run(&self, _sid: &str) -> Result<SyncRunRow> { ni!() }
    async fn get_sync_run(&self, _id: &str) -> Result<Option<SyncRunRow>> { ni!() }
    async fn list_sync_runs(&self, _sid: &str) -> Result<Vec<SyncRunRow>> { ni!() }
    async fn update_sync_run_progress(&self, _id: &str, _p: i64, _ci: Option<&str>) -> Result<()> { ni!() }
    async fn set_sync_run_status(&self, _id: &str, _s: &str) -> Result<()> { ni!() }
    async fn finish_sync_run(&self, _id: &str, _s: &str, _tr: i64, _em: Option<&str>, _et: Option<&str>) -> Result<()> { ni!() }

    async fn create_dataset(&self, _sid: &str, _n: &str) -> Result<DatasetRow> { ni!() }
    async fn list_all_datasets(&self) -> Result<Vec<serde_json::Value>> { ni!() }
    async fn list_datasets(&self, _sid: &str) -> Result<Vec<DatasetRow>> { ni!() }
    async fn list_datasets_with_count(&self, _sid: &str) -> Result<Vec<serde_json::Value>> { ni!() }
    async fn get_dataset(&self, _id: &str) -> Result<Option<DatasetRow>> { ni!() }

    async fn create_dataset_version(&self, _dsid: &str, _srid: &str, _schema: &str) -> Result<DatasetVersionRow> { ni!() }
    async fn commit_dataset_version(&self, _vid: &str, _rows: i64, _mp: Option<&str>) -> Result<()> { ni!() }
    async fn abort_dataset_version(&self, _vid: &str) -> Result<()> { ni!() }
    async fn update_version_manifest_path(&self, _vid: &str, _p: &str) -> Result<()> { ni!() }
    async fn list_dataset_versions(&self, _dsid: &str) -> Result<Vec<DatasetVersionRow>> { ni!() }
    async fn rollback_dataset_version(&self, _dsid: &str, _v: i64) -> Result<()> { ni!() }
    async fn get_prev_committed_schema(&self, _dsid: &str, _bv: i64) -> Result<Option<String>> { ni!() }
    async fn set_version_schema_change(&self, _vid: &str, _c: &str) -> Result<()> { ni!() }
    async fn old_dataset_versions(&self, _dsid: &str, _keep: i64) -> Result<Vec<DatasetVersionRow>> { ni!() }
    async fn gc_version(&self, _vid: &str) -> Result<()> { ni!() }
    async fn get_current_dataset_version(&self, _dsid: &str) -> Result<Option<DatasetVersionRow>> { ni!() }
    async fn list_dataset_records(&self, _dsid: &str, _lim: i64, _off: i64) -> Result<Vec<OntologyObjectRow>> { ni!() }
    async fn count_dataset_records(&self, _dsid: &str) -> Result<i64> { ni!() }

    async fn save_object_type_mapping(&self, _dsid: &str, _etid: &str, _pk: &str, _fm: &str, _sm: &str) -> Result<()> { ni!() }
    async fn update_dataset_sync_mode(&self, _dsid: &str, _sm: &str) -> Result<()> { ni!() }
    async fn list_mapped_dataset_ids(&self) -> Result<Vec<String>> { ni!() }
    async fn get_object_type_mapping(&self, _dsid: &str) -> Result<Option<serde_json::Value>> { ni!() }

    async fn save_link_type_mappings(&self, _dsid: &str, _links: &[LinkTypeMappingInput]) -> Result<()> { ni!() }
    async fn get_link_type_mappings(&self, _dsid: &str) -> Result<Vec<serde_json::Value>> { ni!() }
    async fn list_schema_links(&self) -> Result<Vec<serde_json::Value>> { ni!() }
    async fn resolve_links_for_dataset(&self, _dsid: &str) -> Result<usize> { ni!() }

    async fn get_platform_config(&self, _key: &str) -> Result<Option<String>> { ni!() }
    async fn set_platform_config(&self, _key: &str, _val: &str) -> Result<()> { ni!() }
    async fn get_storage_config(&self) -> Result<serde_json::Value> { ni!() }
    async fn set_storage_config(&self, _cfg: &serde_json::Value) -> Result<()> { ni!() }

    async fn save_connector(&self, _c: &ConnectorRow) -> Result<()> { ni!() }
    async fn update_connector_metadata(&self, _id: &str, _h: &str, _s: &str) -> Result<()> { ni!() }
    async fn save_connector_mapping(&self, _id: &str, _cfg: &str) -> Result<()> { ni!() }
    async fn load_connectors(&self, _pid: &str) -> Result<Vec<ConnectorRow>> { ni!() }
    async fn delete_connector(&self, _id: &str) -> Result<()> { ni!() }
    async fn upsert_entity(&self, _e: &EntityRow) -> Result<()> { ni!() }
    async fn upsert_relationship(&self, _r: &RelRow) -> Result<()> { ni!() }
    async fn load_entities(&self, _pid: &str) -> Result<Vec<EntityRow>> { ni!() }
    async fn load_relationships(&self, _pid: &str) -> Result<Vec<RelRow>> { ni!() }
    async fn clear_project_graph(&self, _pid: &str) -> Result<()> { ni!() }
    async fn save_build(&self, _b: &BuildRow) -> Result<()> { ni!() }
    async fn list_builds(&self, _pid: &str) -> Result<Vec<BuildRow>> { ni!() }
}
