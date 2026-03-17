//! Infrastructure: Outbound Export Adapters
//!
//! DDD Layer: Infrastructure — "driving" adapter that serialises the ontology
//! to an external format. Implements the "Published Language" pattern (Evans):
//! a shared, documented contract that decouples bounded contexts without
//! sharing internal domain models.

use std::fs;

use super::pipeline::dataset::Value;
use crate::application::ontology::{
    bounded_context::ContextMap,
    ddd_mapping::DddMapping,
    graph::OntologyGraph,
    relationship::RelationshipKind,
};

pub struct JsonExporter;

impl JsonExporter {
    /// Serialise the full ontology graph into JSON — no external dependencies.
    pub fn export(graph: &OntologyGraph, mapping: &DddMapping, context_map: &ContextMap) -> String {
        let mut out = String::from("{\n");

        // ── entities ──────────────────────────────────────────────────────
        out.push_str("  \"entities\": [\n");
        for (i, obj) in graph.objects.iter().enumerate() {
            let ddd_concept = mapping.objects.iter()
                .find(|c| c.object_type == obj.object_type.0)
                .map(|c| c.concept.label())
                .unwrap_or("Entity");

            out.push_str("    {\n");
            out.push_str(&fmt_kv("id",           &json_str(&obj.id.0),          3, false));
            out.push_str(&fmt_kv("type",          &json_str(&obj.object_type.0), 3, false));
            out.push_str(&fmt_kv("ddd_concept",   &json_str(ddd_concept),        3, false));
            out.push_str(&fmt_kv("label",         &json_str(&obj.label()),       3, false));

            out.push_str("      \"properties\": {");
            let props: Vec<String> = obj.record.fields.iter()
                .filter(|(k, _)| k.as_str() != "id")
                .map(|(k, v)| {
                    let val_json = match v {
                        Value::String(s) => json_str(s),
                        Value::Float(f)  => format!("{:.2}", f),
                        Value::Int(i)    => i.to_string(),
                        Value::Bool(b)   => b.to_string(),
                        Value::Null      => "null".to_string(),
                        Value::Json(j)   => j.to_string(),
                    };
                    format!("\"{}\": {}", k, val_json)
                })
                .collect();
            out.push_str(&props.join(", "));
            let is_last = i == graph.objects.len() - 1;
            out.push_str("}\n");
            out.push_str(if is_last { "    }\n" } else { "    },\n" });
        }
        out.push_str("  ],\n");

        // ── relationships ─────────────────────────────────────────────────
        out.push_str("  \"relationships\": [\n");
        let rels: Vec<_> = graph.relationships.iter()
            .filter(|r| r.kind == RelationshipKind::Has || r.kind == RelationshipKind::BelongsTo)
            .collect();
        for (i, rel) in rels.iter().enumerate() {
            let action = rel.kind.action_category();
            out.push_str("    {\n");
            out.push_str(&fmt_kv("from",            &json_str(&rel.from_id.0),   3, false));
            out.push_str(&fmt_kv("from_type",       &json_str(&rel.from_type.0), 3, false));
            out.push_str(&fmt_kv("to",              &json_str(&rel.to_id.0),     3, false));
            out.push_str(&fmt_kv("to_type",         &json_str(&rel.to_type.0),   3, false));
            out.push_str(&fmt_kv("kind",            &json_str(rel.kind.label()), 3, false));
            out.push_str(&fmt_kv("action_category", &json_str(action),           3, true));
            out.push_str(if i == rels.len() - 1 { "    }\n" } else { "    },\n" });
        }
        out.push_str("  ],\n");

        // ── bounded_contexts ──────────────────────────────────────────────
        out.push_str("  \"bounded_contexts\": [\n");
        for (i, bc) in context_map.contexts.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&fmt_kv("name",           &json_str(&bc.name),            3, false));
            out.push_str(&fmt_kv("cohesion",       &format!("{:.2}", bc.cohesion), 3, false));
            out.push_str(&fmt_kv("internal_links", &bc.internal_links.to_string(), 3, false));
            let types_json = format!(
                "[{}]",
                bc.entity_types.iter().map(|t| json_str(t)).collect::<Vec<_>>().join(", ")
            );
            out.push_str(&fmt_kv("entity_types",   &types_json,                    3, true));
            out.push_str(if i == context_map.contexts.len() - 1 { "    }\n" } else { "    },\n" });
        }
        out.push_str("  ],\n");

        // ── shared_kernel ─────────────────────────────────────────────────
        let sk_json = format!(
            "[{}]",
            context_map.shared_kernel.dimensions.iter()
                .map(|d| json_str(d))
                .collect::<Vec<_>>()
                .join(", ")
        );
        out.push_str(&format!("  \"shared_kernel\": {},\n", sk_json));

        // ── summary ───────────────────────────────────────────────────────
        out.push_str("  \"summary\": {\n");
        out.push_str(&fmt_kv("total_entities",      &graph.objects.len().to_string(),        3, false));
        out.push_str(&fmt_kv("total_relationships", &graph.relationships.len().to_string(),  3, false));
        out.push_str(&fmt_kv("bounded_contexts",    &context_map.contexts.len().to_string(), 3, true));
        out.push_str("  }\n");

        out.push('}');
        out
    }

    pub fn write(json: &str, path: &str) -> Result<(), String> {
        fs::write(path, json).map_err(|e| format!("Cannot write {}: {}", path, e))
    }
}

fn json_str(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

fn fmt_kv(key: &str, val: &str, indent: usize, last: bool) -> String {
    let pad = "  ".repeat(indent);
    let comma = if last { "" } else { "," };
    format!("{}\"{}\":{} {}{}\n", pad, key, "", val, comma)
}
