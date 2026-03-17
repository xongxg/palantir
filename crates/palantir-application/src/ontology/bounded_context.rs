//! B — Bounded Context Detection.
//!
//! DDD Concept: A Bounded Context is an explicit boundary within which a
//! particular domain model (and its Ubiquitous Language) is consistent.
//! Entities inside a BC are highly cohesive; coupling across BCs is minimal
//! and always mediated by an Anti-Corruption Layer.
//!
//! How we detect BCs from the Ontology graph:
//!   1. Entity types connected by HAS (ownership) → same Bounded Context.
//!      The "owns" relationship implies shared lifecycle and invariants.
//!   2. Types that appear ONLY as BELONGS_TO targets (grouping dimensions)
//!      → Shared Kernel: value concepts referenced by multiple BCs.
//!   3. Cohesion  = internal HAS links / total HAS links  (want: high)
//!   4. Coupling  = BELONGS_TO links crossing BC boundaries (want: low)
//!
//! Palantir Concept: A BC maps to a "Logical Group" in Foundry's Ontology,
//! grouping object types that form a coherent business domain.

use std::collections::{HashMap, HashSet};

use super::{graph::OntologyGraph, relationship::RelationshipKind};

// ─── Data structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BoundedContext {
    /// Derived name — uses the Aggregate Root entity type as the BC name.
    pub name:           String,
    pub entity_types:   Vec<String>,
    pub internal_links: usize,  // HAS edges within this BC
    pub cohesion:       f64,    // internal / total HAS links  (0.0 – 1.0)
}

/// Value Object types that are shared across multiple BCs.
/// Evans calls this the "Shared Kernel" — agreed-upon concepts owned by no
/// single BC but referenced by many.
#[derive(Debug, Clone)]
pub struct SharedKernel {
    pub dimensions: Vec<String>,
}

/// A link between two BCs — represents the coupling that ACLs must mediate.
#[derive(Debug, Clone)]
pub struct CrossContextLink {
    pub from_bc:  String,
    pub to_bc:    String,
    pub via_type: String, // the shared dimension type bridging them
    pub count:    usize,
}

#[derive(Debug)]
pub struct ContextMap {
    pub contexts:     Vec<BoundedContext>,
    pub shared_kernel: SharedKernel,
    pub cross_links:  Vec<CrossContextLink>,
}

// ─── Detection engine ─────────────────────────────────────────────────────────

pub struct BoundedContextDetector;

impl BoundedContextDetector {
    pub fn detect(graph: &OntologyGraph) -> ContextMap {
        // Step 1: separate "core" entity types from "dimension" types
        let mut core_types:   HashSet<String> = HashSet::new();
        let mut dim_types:    HashSet<String> = HashSet::new();

        for rel in &graph.relationships {
            match rel.kind {
                RelationshipKind::Has => {
                    core_types.insert(rel.from_type.0.clone());
                    core_types.insert(rel.to_type.0.clone());
                }
                RelationshipKind::BelongsTo => {
                    dim_types.insert(rel.to_type.0.clone());
                }
                _ => {}
            }
        }

        // Pure dimensions: appear only as grouping targets, never as HAS endpoints
        let pure_dims: Vec<String> = dim_types
            .difference(&core_types)
            .cloned()
            .collect();

        // Step 2: Union-Find — cluster core entity types by HAS edges
        let core_list: Vec<String> = core_types.iter().cloned().collect();
        let mut parent: HashMap<String, String> = core_list
            .iter()
            .map(|t| (t.clone(), t.clone()))
            .collect();

        for rel in &graph.relationships {
            if rel.kind == RelationshipKind::Has {
                union(&mut parent, &rel.from_type.0, &rel.to_type.0);
            }
        }

        // Step 3: group by cluster root
        let mut clusters: HashMap<String, Vec<String>> = HashMap::new();
        for t in &core_list {
            let root = find_root(&parent, t);
            clusters.entry(root).or_default().push(t.clone());
        }

        let total_has = graph.relationships.iter()
            .filter(|r| r.kind == RelationshipKind::Has)
            .count();

        let mut contexts: Vec<BoundedContext> = clusters
            .into_values()
            .map(|mut types| {
                types.sort();
                let internal = graph.relationships.iter()
                    .filter(|r| r.kind == RelationshipKind::Has
                        && types.contains(&r.from_type.0)
                        && types.contains(&r.to_type.0))
                    .count();
                let cohesion = if total_has > 0 { internal as f64 / total_has as f64 } else { 0.0 };
                // Name the BC after the true Aggregate Root: owns children AND is not
                // itself owned by any other type in this same cluster.
                let name = types.iter()
                    .find(|t| {
                        let owns = graph.relationships.iter().any(|r| {
                            r.kind == RelationshipKind::Has && r.from_type.0 == **t
                        });
                        let owned_within_bc = graph.relationships.iter().any(|r| {
                            r.kind == RelationshipKind::Has
                                && r.to_type.0 == **t
                                && types.contains(&r.from_type.0)
                        });
                        owns && !owned_within_bc
                    })
                    .cloned()
                    .unwrap_or_else(|| types[0].clone());
                BoundedContext { name, entity_types: types, internal_links: internal, cohesion }
            })
            .collect();
        contexts.sort_by(|a, b| b.internal_links.cmp(&a.internal_links));

        // Step 4: cross-context links via shared dimensions
        // Find which BCs reference the same shared dimension type
        let mut dim_to_bcs: HashMap<String, Vec<String>> = HashMap::new();
        for rel in &graph.relationships {
            if rel.kind == RelationshipKind::BelongsTo && pure_dims.contains(&rel.to_type.0) {
                let bc_name = contexts.iter()
                    .find(|bc| bc.entity_types.contains(&rel.from_type.0))
                    .map(|bc| bc.name.clone())
                    .unwrap_or_else(|| rel.from_type.0.clone());
                dim_to_bcs
                    .entry(rel.to_type.0.clone())
                    .or_default()
                    .push(bc_name);
            }
        }

        let mut cross_links: Vec<CrossContextLink> = Vec::new();
        for (dim, mut bcs) in dim_to_bcs {
            bcs.sort();
            bcs.dedup();
            if bcs.len() > 1 {
                for i in 0..bcs.len() {
                    for j in (i + 1)..bcs.len() {
                        let count = graph.relationships.iter()
                            .filter(|r| r.kind == RelationshipKind::BelongsTo
                                && r.to_type.0 == dim)
                            .count();
                        cross_links.push(CrossContextLink {
                            from_bc:  bcs[i].clone(),
                            to_bc:    bcs[j].clone(),
                            via_type: dim.clone(),
                            count,
                        });
                    }
                }
            }
        }

        let mut sorted_dims = pure_dims;
        sorted_dims.sort();

        ContextMap {
            contexts,
            shared_kernel: SharedKernel { dimensions: sorted_dims },
            cross_links,
        }
    }
}

// ─── Union-Find helpers ───────────────────────────────────────────────────────

fn find_root(parent: &HashMap<String, String>, x: &str) -> String {
    let mut x = x.to_string();
    loop {
        let p = parent[&x].clone();
        if p == x { return x; }
        x = p;
    }
}

fn union(parent: &mut HashMap<String, String>, a: &str, b: &str) {
    let ra = find_root(parent, a);
    let rb = find_root(parent, b);
    if ra != rb {
        parent.insert(ra, rb);
    }
}
