//! OntologyGraph — the semantic entity graph.
//! Nodes: OntologyObjects.  Edges: typed Relationships.

use std::collections::HashMap;

use super::{
    entity::OntologyObject,
    relationship::{Relationship, RelationshipKind},
};

pub struct OntologyGraph {
    pub objects:       Vec<OntologyObject>,
    pub relationships: Vec<Relationship>,
}

impl OntologyGraph {
    pub fn build(objects: Vec<OntologyObject>, relationships: Vec<Relationship>) -> Self {
        Self { objects, relationships }
    }

    pub fn find_object(&self, id: &str) -> Option<&OntologyObject> {
        self.objects.iter().find(|o| o.id.0 == id)
    }

    pub fn objects_by_type(&self, type_name: &str) -> Vec<&OntologyObject> {
        self.objects.iter().filter(|o| o.object_type.0 == type_name).collect()
    }

    /// All outgoing edges from `from_id`, optionally filtered by relationship kind.
    pub fn outgoing(&self, from_id: &str, kind: Option<&RelationshipKind>) -> Vec<&Relationship> {
        self.relationships.iter()
            .filter(|r| r.from_id.0 == from_id)
            .filter(|r| kind.map_or(true, |k| &r.kind == k))
            .collect()
    }

    /// Object-type → count, sorted descending.
    pub fn type_counts(&self) -> Vec<(String, usize)> {
        let mut map: HashMap<String, usize> = HashMap::new();
        for obj in &self.objects {
            *map.entry(obj.object_type.0.clone()).or_default() += 1;
        }
        let mut v: Vec<_> = map.into_iter().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1));
        v
    }

    /// Aggregated (from_type, kind_label, to_type, count), sorted by count desc.
    pub fn relationship_patterns(&self) -> Vec<(String, String, String, usize)> {
        let mut map: HashMap<(String, String, String), usize> = HashMap::new();
        for rel in &self.relationships {
            let key = (
                rel.from_type.0.clone(),
                rel.kind.label().to_string(),
                rel.to_type.0.clone(),
            );
            *map.entry(key).or_default() += 1;
        }
        let mut v: Vec<_> = map.into_iter().map(|((a, b, c), n)| (a, b, c, n)).collect();
        v.sort_by(|a, b| b.3.cmp(&a.3));
        v
    }
}
