use palantir_ontology_manager::{
    mapping::Mapping,
    mapping_toml::TomlMapping,
    model::{CanonicalRecord, OntologySchema},
};
use serde_json::Value as JsonValue;
use time::OffsetDateTime;

pub trait IngestTools {
    fn preview_csv(
        &self,
        path: &str,
        ns: &str,
        schema: &str,
        mapping_toml: &str,
        limit: usize,
    ) -> anyhow::Result<usize>;
    fn apply_csv(
        &self,
        path: &str,
        ns: &str,
        schema: &str,
        mapping_toml: &str,
    ) -> anyhow::Result<usize>;
}

pub struct LocalIngestTools;

impl IngestTools for LocalIngestTools {
    fn preview_csv(
        &self,
        path: &str,
        ns: &str,
        schema: &str,
        mapping_toml: &str,
        limit: usize,
    ) -> anyhow::Result<usize> {
        let mapping = TomlMapping::from_str(mapping_toml)?;
        let mut rdr = csv::Reader::from_path(path)?;
        let headers = rdr.headers()?.clone();
        let ont_schema = OntologySchema {
            version: "v1".into(),
            entities: Default::default(),
        };
        let mut count = 0usize;
        for (i, rec) in rdr.records().enumerate() {
            if i >= limit {
                break;
            }
            let rec = rec?;
            let mut obj = serde_json::Map::new();
            for (k, v) in headers.iter().zip(rec.iter()) {
                obj.insert(k.to_string(), serde_json::json!(v));
            }
            let cr = CanonicalRecord {
                source: "csv".into(),
                ns: ns.to_string(),
                schema: schema.to_string(),
                payload: JsonValue::Object(obj),
                ts: OffsetDateTime::now_utc(),
                cursor: None,
            };
            count += mapping.apply(&cr, &ont_schema)?.len();
        }
        Ok(count)
    }
    fn apply_csv(
        &self,
        path: &str,
        ns: &str,
        schema: &str,
        mapping_toml: &str,
    ) -> anyhow::Result<usize> {
        self.preview_csv(path, ns, schema, mapping_toml, usize::MAX)
    }
}
