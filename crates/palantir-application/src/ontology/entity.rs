use crate::infrastructure::pipeline::dataset::{Record, Value};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ObjectType(pub String);

/// A discovered ontology object — a record with a typed identity.
#[derive(Debug, Clone)]
pub struct OntologyObject {
    pub id: EntityId,
    pub object_type: ObjectType,
    pub record: Record,
}

impl OntologyObject {
    pub fn new(object_type: impl Into<String>, record: Record) -> Self {
        let id = EntityId(record.id.clone());
        Self {
            id,
            object_type: ObjectType(object_type.into()),
            record,
        }
    }

    pub fn get(&self, field: &str) -> Option<&Value> {
        self.record.get(field)
    }

    pub fn label(&self) -> String {
        for field in ["name", "title", "label"] {
            if let Some(Value::String(s)) = self.record.get(field) {
                return s.clone();
            }
        }
        self.id.0.clone()
    }
}
