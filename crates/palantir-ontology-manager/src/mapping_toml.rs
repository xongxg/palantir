use crate::mapping::Mapping;
use crate::errors::MappingError;
use crate::model::{CanonicalRecord, OntologyEvent, OntologyObject, OntologySchema, OntologyId, TimeBounds, Provenance, Value};
use std::collections::BTreeMap;
use time::OffsetDateTime;

#[derive(Debug, serde::Deserialize)]
struct MappingDoc {
    version: String,
    #[serde(default)]
    from: FromNs,
    entity: String,
    #[serde(default)]
    id: IdSpec,
    #[serde(default)]
    map: BTreeMap<String, String>, // target_attr -> "source|cast"
    #[serde(default)]
    links: Vec<LinkSpec>,
}

#[derive(Debug, Default, serde::Deserialize)]
struct FromNs { #[serde(default)] ns: String }

#[derive(Debug, Default, serde::Deserialize)]
struct IdSpec { #[serde(default)] field: String }

#[derive(Debug, Default, serde::Deserialize)]
struct LinkSpec {
    #[serde(default)] rel: String,
    #[serde(default)] #[allow(dead_code)] to_entity: String,
    #[serde(default)] from_key: String, // if empty → current object's id
    #[serde(default)] to_key: String,   // if empty → current object's id
}

pub struct TomlMapping { doc: MappingDoc }

impl TomlMapping {
    pub fn from_str(s: &str) -> Result<Self, MappingError> {
        let doc: MappingDoc = toml::from_str(s).map_err(|e| MappingError::Message(e.to_string()))?;
        Ok(Self { doc })
    }

    fn cast(val: &str, ty: Option<&str>) -> Value {
        match ty.unwrap_or("") {
            "int" => val.parse::<i64>().map(Value::Int).unwrap_or(Value::Null),
            "float" => val.parse::<f64>().map(Value::Float).unwrap_or(Value::Null),
            "bool" => val.parse::<bool>().map(Value::Bool).unwrap_or(Value::Null),
            _ => Value::Str(val.to_string()),
        }
    }
}

impl Mapping for TomlMapping {
    fn version(&self) -> &str { &self.doc.version }

    fn apply(&self, rec: &CanonicalRecord, _schema: &OntologySchema) -> Result<Vec<OntologyEvent>, MappingError> {
        if !self.doc.from.ns.is_empty() && self.doc.from.ns != rec.ns { return Ok(vec![]); }

        let mut attrs = BTreeMap::new();
        for (target, spec) in &self.doc.map {
            let mut parts = spec.split('|');
            let src = parts.next().unwrap_or("");
            let ty = parts.next();
            let val = rec.payload.get(src).and_then(|v| v.as_str()).unwrap_or("");
            attrs.insert(target.clone(), Self::cast(val, ty));
        }
        let id_field = if self.doc.id.field.is_empty() { "id" } else { &self.doc.id.field };
        let id_val = rec.payload.get(id_field).and_then(|v| v.as_str()).unwrap_or("");
        let object = OntologyObject {
            id: OntologyId(id_val.to_string()),
            entity_type: self.doc.entity.clone(),
            attrs,
            time: TimeBounds { valid_from: None, valid_to: None, tx_time: OffsetDateTime::now_utc() },
            version: 1,
            provenance: Provenance { source: rec.source.clone(), cursor: rec.cursor.clone(), record_id: None },
        };
        let mut out = vec![OntologyEvent::Upsert { object: object.clone() }];
        for l in &self.doc.links {
            let from_id = if l.from_key.is_empty() { object.id.0.clone() } else { rec.payload.get(&l.from_key).and_then(|v| v.as_str()).unwrap_or("").to_string() };
            let to_id = if l.to_key.is_empty() { object.id.0.clone() } else { rec.payload.get(&l.to_key).and_then(|v| v.as_str()).unwrap_or("").to_string() };
            if !from_id.is_empty() && !to_id.is_empty() {
                out.push(OntologyEvent::Link {
                    from: OntologyId(from_id),
                    to: OntologyId(to_id),
                    rel: if l.rel.is_empty() { "HAS".into() } else { l.rel.clone() },
                    attrs: None,
                    time: TimeBounds { valid_from: None, valid_to: None, tx_time: OffsetDateTime::now_utc() },
                    provenance: Provenance { source: rec.source.clone(), cursor: rec.cursor.clone(), record_id: None },
                });
            }
        }
        Ok(out)
    }
}
