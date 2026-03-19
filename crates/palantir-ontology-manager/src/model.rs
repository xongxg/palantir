use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use time::OffsetDateTime;
use uuid::Uuid;

pub type SourceId = String;
pub type SchemaId = String;
pub type Cursor = serde_json::Value;
pub type AttrId = String;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanonicalRecord {
    pub source: SourceId,
    pub ns: String,
    pub schema: SchemaId,
    pub payload: serde_json::Value,
    pub ts: OffsetDateTime,
    pub cursor: Option<Cursor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Time(OffsetDateTime),
    Decimal(String),
    Json(serde_json::Value),
    Bytes(Vec<u8>),
    Null,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeBounds {
    pub valid_from: Option<OffsetDateTime>,
    pub valid_to: Option<OffsetDateTime>,
    pub tx_time: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
pub struct OntologyId(pub String);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    pub source: SourceId,
    pub cursor: Option<Cursor>,
    pub record_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologyObject {
    pub id: OntologyId,
    pub entity_type: String,
    pub attrs: BTreeMap<AttrId, Value>,
    pub time: TimeBounds,
    pub version: i64,
    pub provenance: Provenance,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkAttrs(pub BTreeMap<AttrId, Value>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OntologyEvent {
    Upsert {
        object: OntologyObject,
    },
    Delete {
        id: OntologyId,
    },
    Link {
        from: OntologyId,
        to: OntologyId,
        rel: String,
        attrs: Option<LinkAttrs>,
        time: TimeBounds,
        provenance: Provenance,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OntologySchema {
    pub version: String,
    pub entities: BTreeMap<String, EntitySchema>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    pub attributes: BTreeMap<String, String>,
}
