//! Interface Layer
//!
//! DDD Layer: Interface (Presentation) — renders output to the terminal.
//! Pure rendering: no domain logic, no state mutation.
//! Equivalent to a View in MVC or a Presenter in MVP.

use std::collections::HashMap;

use crate::application::action::ActionSummary;
use crate::application::ontology::{
    bounded_context::ContextMap,
    ddd_mapping::{DddLayer, DddMapping},
    graph::OntologyGraph,
    pattern_detector::DetectedPattern,
    relationship::RelationshipKind,
};
use crate::domain::events::{DomainEvent, EventBus};
use crate::infrastructure::pipeline::dataset::Value;

// ─── Public entry point ───────────────────────────────────────────────────────

pub fn render_ontology(graph: &OntologyGraph, actions: &[ActionSummary]) {
    render_entity_summary(graph);
    render_relationship_table(graph);
    render_semantic_graph(graph);
    render_spend_barchart(graph);
    render_action_mapping(actions);
    render_ddd_mapping(&DddMapping::from_graph(graph));
}

pub fn render_csv_concept(path_a: &str, count_a: usize, path_b: &str, count_b: usize) {
    println!("\n╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  A — Data Source: Infrastructure Adapter  (CSV → Dataset)           ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD Concept: Infrastructure adapters (Hexagonal Architecture / Ports & Adapters).");
    println!("  The domain and ontology layers never know WHERE data comes from.");
    println!("  Swap CsvLoader → DatabaseLoader → KafkaLoader → zero changes above.");
    println!();
    println!("  Loaded:");
    println!("    {:<35} → {:>3} objects", path_a, count_a);
    println!("    {:<35} → {:>3} objects", path_b, count_b);
    println!();
    println!("  Flow:");
    println!("    CSV file");
    println!("      └── CsvLoader (Infrastructure)");
    println!("            └── Dataset (Analytics)");
    println!("                  └── DiscoveryEngine (Ontology)  ← same pipeline as in-memory");
    println!("                        └── OntologyGraph");
    println!();
}

pub fn render_bounded_context(ctx: &ContextMap) {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  B — Bounded Context Detection                                       ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD Concept: A Bounded Context is a linguistic + semantic boundary.");
    println!("  Entity types with high HAS-relationship density → same BC.");
    println!("  Value Objects (grouping dims) used by multiple BCs → Shared Kernel.");
    println!();
    println!("  Algorithm:");
    println!("    1. Entity types connected via HAS → Union-Find clustering");
    println!("    2. Each cluster = one Bounded Context");
    println!("    3. BELONGS_TO targets not in any cluster = Shared Kernel");
    println!();

    for bc in &ctx.contexts {
        let filled    = (bc.cohesion * 20.0).round() as usize;
        let coh_bar   = "█".repeat(filled);
        let coh_empty = "░".repeat(20usize.saturating_sub(filled));
        println!("  ┌─────────────────────────────────────────────────────────┐");
        println!("  │  Bounded Context: {:^38}│", format!("\"{}\"", bc.name));
        println!("  ├─────────────────────────────────────────────────────────┤");
        println!("  │  Entity types : {:^39}│",
            bc.entity_types.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "));
        println!("  │  Internal HAS : {:<4}  Cohesion: {}{}  {:.0}% │",
            bc.internal_links, coh_bar, coh_empty, bc.cohesion * 100.0);
        println!("  └─────────────────────────────────────────────────────────┘");
    }

    println!();
    println!("  Shared Kernel (Value Objects referenced across BCs):");
    for dim in &ctx.shared_kernel.dimensions {
        println!("    · {}  — no independent identity; equality by value", dim);
    }

    if ctx.cross_links.is_empty() {
        println!();
        println!("  Cross-context links: none (single BC — tight cohesion, zero coupling)");
    } else {
        println!();
        println!("  Cross-context links (ACL required):");
        for link in &ctx.cross_links {
            println!("    {} ──[{}]──▶ {}  ({} links)",
                link.from_bc, link.via_type, link.to_bc, link.count);
        }
    }

    println!();
    println!("  DDD Rule: Access child Entities ONLY through their Aggregate Root.");
    println!("            Never reference a Transaction without going through Employee.");
    println!();
}

pub fn render_event_loop(patterns: &[DetectedPattern], event_bus: &EventBus) {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  C — Ontology → Domain Event Loop                                   ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD Concept: The Ontology is a semantic observer.");
    println!("  It scans entity graphs for MEANING, not raw values.");
    println!("  Detected patterns emit DomainEvents → EventBus → Application Service.");
    println!();
    println!("  Full loop:");
    println!("    Data ─▶ OntologyGraph ─▶ PatternDetector ─▶ DomainEvent");
    println!("    DomainEvent ─▶ EventBus ─▶ ApplicationService ─▶ Command ─▶ Domain");
    println!();

    if patterns.is_empty() {
        println!("  No patterns detected.");
    } else {
        println!("  Detected Patterns:");
        let mut cur_kind = "";
        for p in patterns {
            let kind_label = p.kind.label();
            if kind_label != cur_kind {
                cur_kind = kind_label;
                println!("\n  [{}]  → triggers: {}", kind_label, p.kind.ddd_trigger());
            }
            println!("    · {}", p.detail);
        }
    }

    let ontology_events: Vec<_> = event_bus.events()
        .iter()
        .filter(|e| e.is_ontology_triggered())
        .collect();

    println!();
    println!("  EventBus received {} ontology-triggered DomainEvent(s):", ontology_events.len());
    for ev in &ontology_events {
        match ev {
            DomainEvent::HighSpendPatternDetected { employee_id, total_spend, department } => {
                println!("    [{}]  emp={} spend=${:.0} dept={}",
                    ev.name(), employee_id, total_spend, department);
            }
            DomainEvent::CategoryConcentrationDetected { employee_id, category, percent } => {
                println!("    [{}]  emp={} cat=\"{}\" {:.0}%",
                    ev.name(), employee_id, category, percent * 100.0);
            }
            _ => {}
        }
    }
    println!();
}

pub fn render_json_export(path: &str, entity_count: usize, rel_count: usize, bc_count: usize, sk: &[String]) {
    println!("╔══════════════════════════════════════════════════════════════════════╗");
    println!("║  D — JSON Export: Published Language Pattern                         ║");
    println!("╚══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  DDD Concept: Published Language (Evans) — a shared, documented format");
    println!("  that decouples BCs without sharing internal domain models.");
    println!("  External consumers (frontend, other BCs) use this JSON contract.");
    println!();
    println!("  Palantir: This mirrors the Foundry Ontology API — language-agnostic");
    println!("  JSON representation of all objects, links, and action types.");
    println!();
    println!("  Written to: {}", path);
    println!();
    println!("  Contents:");
    println!("    entities          : {}", entity_count);
    println!("    relationships     : {}", rel_count);
    println!("    bounded_contexts  : {}", bc_count);
    println!("    shared_kernel     : [{}]", sk.join(", "));
    println!();
    println!("  Consumers:");
    println!("    · Frontend graph viz  → D3.js / Cytoscape (use entities + relationships)");
    println!("    · Other BCs           → read bounded_contexts to know the contract");
    println!("    · Analytics tools     → use relationships for lineage / impact analysis");
    println!();
}

// ─── 1. Entity summary ────────────────────────────────────────────────────────

fn render_entity_summary(graph: &OntologyGraph) {
    println!("\n═══ Ontology: Discovered Entities ══════════════════════════════════");
    println!("  ┌──────────────────────┬────────┐");
    println!("  │ Object Type          │ Count  │");
    println!("  ├──────────────────────┼────────┤");
    for (type_name, count) in graph.type_counts() {
        println!("  │  {:<20} │  {:>4}  │", type_name, count);
    }
    println!("  └──────────────────────┴────────┘");
    println!(
        "  Total entities: {}   Total relationships: {}",
        graph.objects.len(),
        graph.relationships.len()
    );
}

// ─── 2. Relationship pattern table ───────────────────────────────────────────

fn render_relationship_table(graph: &OntologyGraph) {
    println!("\n═══ Ontology: Relationship Patterns ════════════════════════════════");
    println!("  ┌──────────────────────────────────────────────┬───────┬─────────────┐");
    println!("  │ Pattern                                      │ Count │ Action      │");
    println!("  ├──────────────────────────────────────────────┼───────┼─────────────┤");
    for (from_type, kind, to_type, count) in graph.relationship_patterns() {
        let pat    = format!("{} ──{}──▶ {}", from_type, kind, to_type);
        let action = match kind.as_str() {
            "BELONGS_TO" => "Logic",
            "HAS"        => "Integration",
            "LINKED_TO"  => "Workflow",
            "SIMILAR_TO" => "Search",
            _            => "—",
        };
        println!("  │  {:<44} │  {:>4} │  {:<10} │", pat, count, action);
    }
    println!("  └──────────────────────────────────────────────┴───────┴─────────────┘");
}

// ─── 3. Semantic entity tree: Department → Employee ──HAS──▶ Transaction ─────

fn render_semantic_graph(graph: &OntologyGraph) {
    println!("\n═══ Ontology Graph: Semantic Entity Tree ════════════════════════════");
    println!("  Department  ──BELONGS_TO──  Employee  ──HAS──▶  Transaction");
    println!();

    // dept_group_id → [employee_ids]
    let mut dept_employees: HashMap<String, Vec<String>> = HashMap::new();
    for emp in graph.objects_by_type("Employee") {
        for rel in graph.outgoing(&emp.id.0, Some(&RelationshipKind::BelongsTo)) {
            if rel.to_type.0 == "department" {
                dept_employees
                    .entry(rel.to_id.0.clone())
                    .or_default()
                    .push(emp.id.0.clone());
            }
        }
    }

    let mut dept_ids: Vec<&String> = dept_employees.keys().collect();
    dept_ids.sort();

    for dept_group_id in &dept_ids {
        let dept_label = dept_group_id.trim_start_matches("department:");
        let emp_ids    = dept_employees.get(*dept_group_id).unwrap();

        // Department total spend
        let dept_spend: f64 = emp_ids.iter().map(|eid| {
            graph.outgoing(eid, Some(&RelationshipKind::Has))
                .into_iter()
                .filter(|r| r.to_type.0 == "Transaction")
                .filter_map(|r| graph.find_object(&r.to_id.0))
                .filter_map(|tx| tx.get("amount").and_then(Value::as_f64))
                .sum::<f64>()
        }).sum();

        println!("  ┌─ [{}]   spend: ${:.0}", dept_label, dept_spend);

        let last_emp = emp_ids.len().saturating_sub(1);
        for (ei, emp_id) in emp_ids.iter().enumerate() {
            let Some(emp) = graph.find_object(emp_id) else { continue };

            let name   = emp.label();
            let salary = emp.get("salary").and_then(Value::as_f64).unwrap_or(0.0);
            let level  = emp.get("level").and_then(Value::as_str).unwrap_or("?");

            let (branch, vline) = if ei == last_emp {
                ("└──", "   ")
            } else {
                ("├──", "│  ")
            };

            println!("  │  {} [{}] {}  ({}, ${:.0})", branch, emp_id, name, level, salary);

            // Transactions owned by this employee
            let txns: Vec<_> = graph
                .outgoing(emp_id, Some(&RelationshipKind::Has))
                .into_iter()
                .filter(|r| r.to_type.0 == "Transaction")
                .collect();

            let last_tx = txns.len().saturating_sub(1);
            for (ti, rel) in txns.iter().enumerate() {
                let Some(tx) = graph.find_object(&rel.to_id.0) else { continue };
                let amount   = tx.get("amount").and_then(Value::as_f64).unwrap_or(0.0);
                let category = tx.get("category").and_then(Value::as_str).unwrap_or("?");
                let tx_branch = if ti == last_tx { "└─" } else { "├─" };
                println!(
                    "  │  {}  {} ──HAS──▶  [{}]  ${:.0}  {}",
                    vline, tx_branch, rel.to_id.0, amount, category
                );
            }
        }
        println!("  │");
    }
    println!();
}

// ─── 4. Spend bar chart ───────────────────────────────────────────────────────

fn render_spend_barchart(graph: &OntologyGraph) {
    println!("═══ Analysis: Department Spend ══════════════════════════════════════");
    println!();

    let mut dept_spend: HashMap<String, f64> = HashMap::new();
    for emp in graph.objects_by_type("Employee") {
        let Some(dept_rel) = graph
            .outgoing(&emp.id.0, Some(&RelationshipKind::BelongsTo))
            .into_iter()
            .find(|r| r.to_type.0 == "department")
        else {
            continue;
        };
        let dept_id = dept_rel.to_id.0.clone();
        let spend: f64 = graph
            .outgoing(&emp.id.0, Some(&RelationshipKind::Has))
            .into_iter()
            .filter(|r| r.to_type.0 == "Transaction")
            .filter_map(|r| graph.find_object(&r.to_id.0))
            .filter_map(|tx| tx.get("amount").and_then(Value::as_f64))
            .sum();
        *dept_spend.entry(dept_id).or_default() += spend;
    }

    let max_spend = dept_spend.values().cloned().fold(0.0f64, f64::max);
    const BAR: usize = 36;

    let mut entries: Vec<_> = dept_spend.iter().collect();
    entries.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));

    for (dept_id, spend) in &entries {
        let label  = dept_id.trim_start_matches("department:");
        let filled = ((*spend / max_spend) * BAR as f64).round() as usize;
        let bar    = format!("{}{}", "█".repeat(filled), "░".repeat(BAR - filled));
        println!("  {:<14} │{}│  ${:.0}", trunc(label, 14), bar, spend);
    }
    println!();
    println!("  Scale: full bar = ${:.0}", max_spend);
    println!();
}

// ─── 5. Action mapping ────────────────────────────────────────────────────────

fn render_action_mapping(actions: &[ActionSummary]) {
    println!("═══ Ontology Actions: Logic / Integration / Workflow / Search ═══════");
    println!();

    let mut cur = "";
    for action in actions {
        if action.category != cur {
            cur = action.category;
            let tag = match cur {
                "Logic"       => "[L] Logic",
                "Integration" => "[I] Integration",
                "Workflow"    => "[W] Workflow",
                "Search"      => "[S] Search",
                _             => cur,
            };
            println!("  {}:", tag);
        }
        println!("      · {}", action.description);
    }
    println!();
}

// ─── 6. DDD architecture mapping ─────────────────────────────────────────────

fn render_ddd_mapping(mapping: &DddMapping) {
    println!("═══ DDD Architecture Mapping ════════════════════════════════════════");
    println!("  (How Palantir Ontology concepts align with DDD building blocks)");
    println!();

    // ── Domain layer ──────────────────────────────────────────────────────────
    let domain_objs: Vec<_> = mapping.objects.iter()
        .filter(|c| c.concept.layer() == DddLayer::Domain)
        .collect();

    println!("  ┌─────────────────────────────────────────────────────────────────┐");
    println!("  │  DOMAIN LAYER   — pure business logic, no I/O                  │");
    println!("  ├──────────────────┬──────────────────┬───────────────────────────┤");
    println!("  │ Object Type      │ DDD Concept      │ Why                       │");
    println!("  ├──────────────────┼──────────────────┼───────────────────────────┤");
    for c in &domain_objs {
        println!(
            "  │  {:<15} │  {:<15} │  {:<26}│",
            trunc(&c.object_type, 15),
            c.concept.label(),
            trunc(c.reason, 26),
        );
    }
    println!("  │                  │                  │                           │");
    println!("  │  Logic actions   │  Domain Service  │  stateless, cross-entity  │");
    println!("  └──────────────────┴──────────────────┴───────────────────────────┘");

    // ── Application layer ─────────────────────────────────────────────────────
    println!("  ┌─────────────────────────────────────────────────────────────────┐");
    println!("  │  APPLICATION LAYER  — orchestrates domain, no business rules    │");
    println!("  ├──────────────────┬──────────────────┬───────────────────────────┤");
    println!("  │ Palantir Action  │ DDD Pattern      │ Mechanism                 │");
    println!("  ├──────────────────┼──────────────────┼───────────────────────────┤");
    for ac in mapping.actions.iter()
        .filter(|a| a.concept.layer() == DddLayer::Application)
    {
        println!(
            "  │  {:<15} │  {:<15} │  {:<26}│",
            ac.palantir_action,
            ac.concept.label(),
            trunc(ac.ddd_pattern, 26),
        );
    }
    println!("  └──────────────────┴──────────────────┴───────────────────────────┘");

    // ── Infrastructure layer ──────────────────────────────────────────────────
    println!("  ┌─────────────────────────────────────────────────────────────────┐");
    println!("  │  INFRASTRUCTURE LAYER  — ports & adapters, swappable            │");
    println!("  ├──────────────────┬──────────────────┬───────────────────────────┤");
    println!("  │ Palantir Action  │ DDD Pattern      │ Mechanism                 │");
    println!("  ├──────────────────┼──────────────────┼───────────────────────────┤");
    for ac in mapping.actions.iter()
        .filter(|a| a.concept.layer() == DddLayer::Infrastructure)
    {
        println!(
            "  │  {:<15} │  {:<15} │  {:<26}│",
            ac.palantir_action,
            ac.concept.label(),
            trunc(ac.ddd_pattern, 26),
        );
    }
    println!("  │  Dataset adapter │  Anti-Corrupt. L │  keeps domain model pure  │");
    println!("  └──────────────────┴──────────────────┴───────────────────────────┘");

    // ── Ubiquitous Language note ──────────────────────────────────────────────
    println!();
    println!("  KEY INSIGHT");
    println!("  ───────────");
    println!("  Ontology  =  the Ubiquitous Language made machine-readable.");
    println!("  DDD       =  the architecture that enforces its boundaries.");
    println!();
    println!("  In this codebase:");
    println!("    src/domain/        → DDD pure domain  (Aggregate Roots, VOs, Events)");
    println!("    src/application/   → DDD application  (Commands + CQRS Queries)");
    println!("    src/infrastructure/→ DDD adapters     (In-memory repos, ACL)");
    println!("    src/analytics/     → Palantir pipeline (Dataset + Transform)");
    println!("    src/ontology/      → Palantir ontology (Discovery + Semantic graph)");
    println!("    src/action/        → Bridge layer     (Ontology → DDD operations)");
    println!();
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn trunc(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}
