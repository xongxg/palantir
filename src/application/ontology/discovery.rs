//! Ontology Discovery Engine — scans raw datasets, auto-extracts entities & relationships.
//!
//! Three passes:
//!   1. Materialise entities (one OntologyObject per record)
//!   2. FK detection (*_id fields) → owner HAS child  (Integration)
//!   3. Categorical grouping (repeated string values) → BELONGS_TO  (Logic)

use std::collections::HashMap;

use crate::infrastructure::pipeline::dataset::{Dataset, Value};

use super::{
    entity::{EntityId, ObjectType, OntologyObject},
    relationship::{Relationship, RelationshipKind},
};

pub struct DiscoveryEngine;

impl DiscoveryEngine {
    pub fn discover(datasets: &[Dataset]) -> (Vec<OntologyObject>, Vec<Relationship>) {
        let mut objects: Vec<OntologyObject> = Vec::new();
        let mut relationships: Vec<Relationship> = Vec::new();
        let mut id_to_type: HashMap<String, ObjectType> = HashMap::new();

        // Pass 1: materialise all entities
        for ds in datasets {
            for rec in &ds.records {
                let obj = OntologyObject::new(&ds.object_type, rec.clone());
                id_to_type.insert(obj.id.0.clone(), obj.object_type.clone());
                objects.push(obj);
            }
        }

        // Pass 2: FK detection (*_id fields) → owner HAS child (Integration)
        for ds in datasets {
            for rec in &ds.records {
                for (field, value) in &rec.fields {
                    if field == "id" || !field.ends_with("_id") { continue; }
                    let ref_id = value.to_string();
                    if let Some(owner_type) = id_to_type.get(&ref_id) {
                        relationships.push(Relationship {
                            from_id:   EntityId(ref_id.clone()),
                            from_type: owner_type.clone(),
                            to_id:     EntityId(rec.id.clone()),
                            to_type:   ObjectType(ds.object_type.clone()),
                            kind:      RelationshipKind::Has,
                            via_field: field.clone(),
                        });
                    }
                }
            }
        }

        // Pass 3: categorical grouping → BELONGS_TO (Logic)
        for ds in datasets {
            let mut field_groups: HashMap<String, HashMap<String, Vec<String>>> = HashMap::new();
            for rec in &ds.records {
                for (field, val) in &rec.fields {
                    if field == "id" || field.ends_with("_id") { continue; }
                    if let Value::String(s) = val {
                        field_groups
                            .entry(field.clone())
                            .or_default()
                            .entry(s.clone())
                            .or_default()
                            .push(rec.id.clone());
                    }
                }
            }
            for (field, groups) in &field_groups {
                if !groups.values().any(|ids| ids.len() > 1) { continue; }
                for (group_val, member_ids) in groups {
                    let group_id = format!("{}:{}", field, group_val);
                    for member_id in member_ids {
                        relationships.push(Relationship {
                            from_id:   EntityId(member_id.clone()),
                            from_type: ObjectType(ds.object_type.clone()),
                            to_id:     EntityId(group_id.clone()),
                            to_type:   ObjectType(field.clone()),
                            kind:      RelationshipKind::BelongsTo,
                            via_field: field.clone(),
                        });
                    }
                }
            }
        }

        (objects, relationships)
    }
}
