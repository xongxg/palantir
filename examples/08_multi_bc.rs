//! Example 8 — Multiple Bounded Contexts: HR · Project · Customer · Procurement
//!
//! Run:  cargo run --example 08_multi_bc
//!       then  cargo run --bin serve  →  http://localhost:3000
//!
//! Dataset design (data/multi_bc/) ensures 4 distinct FK clusters:
//!
//!   BC 1  HR            Department ──HAS──▶ Employee ──HAS──▶ Contract
//!   BC 2  Project       Project   ──HAS──▶ Milestone ──HAS──▶ Task
//!   BC 3  Customer      Customer  ──HAS──▶ Order     ──HAS──▶ Invoice
//!   BC 4  Procurement   Vendor    ──HAS──▶ PurchaseOrder ──HAS──▶ Payment
//!
//! No FK field crosses BC boundaries → Union-Find produces 4 separate clusters.
//!
//! Shared Kernel (BELONGS_TO value objects visible across BCs):
//!   status   — HR, Project, Customer, Procurement (all 4)
//!   priority — Project, Customer, Procurement (3)
//!   country  — HR, Customer, Procurement (3)
//!   tier     — Customer, Procurement (2)
//!   currency — Customer, Procurement (2)
//!
//! Also exports ontology_graph.json for the D3.js visualizer.

use palantir::{
    application::ontology::{
        bounded_context::BoundedContextDetector,
        ddd_mapping::DddMapping,
        discovery::DiscoveryEngine,
        graph::OntologyGraph,
        relationship::RelationshipKind,
    },
    infrastructure::{datasource::CsvLoader, export::JsonExporter, pipeline::dataset::Dataset},
};

const BASE: &str = "data/multi_bc";

fn main() {
    // ── Load ──────────────────────────────────────────────────────────────────
    let specs: &[(&str, &str)] = &[
        // BC 1 — HR
        ("departments.csv",     "Department"),
        ("employees.csv",       "Employee"),
        ("contracts.csv",       "Contract"),
        // BC 2 — Project
        ("projects.csv",        "Project"),
        ("milestones.csv",      "Milestone"),
        ("tasks.csv",           "Task"),
        // BC 3 — Customer
        ("customers.csv",       "Customer"),
        ("orders.csv",          "Order"),
        ("invoices.csv",        "Invoice"),
        // BC 4 — Procurement
        ("vendors.csv",         "Vendor"),
        ("purchase_orders.csv", "PurchaseOrder"),
        ("payments.csv",        "Payment"),
    ];

    banner("MULTI BOUNDED CONTEXT  —  4-Domain Dataset");
    println!("  Hypothesis: 4 isolated FK clusters → 4 distinct Bounded Contexts.");
    println!("  Shared Kernel: status / priority / country / tier / currency");
    println!();

    let mut datasets: Vec<Dataset> = Vec::new();
    for (file, etype) in specs {
        let path = format!("{}/{}", BASE, file);
        match CsvLoader::load(&path, etype) {
            Ok(ds) => {
                println!("  ✓  {:<24} {:>3} records  ({})", file, ds.len(), etype);
                datasets.push(ds);
            }
            Err(e) => eprintln!("  ✗  {} ERROR: {}", file, e),
        }
    }
    println!();

    // ── Discovery ─────────────────────────────────────────────────────────────
    section("PASS 1 — DISCOVERY  (3-pass schema-free scan)");

    let (objects, relationships) = DiscoveryEngine::discover(&datasets);
    let graph = OntologyGraph::build(objects, relationships);

    let has_total  = graph.relationships.iter().filter(|r| r.kind == RelationshipKind::Has).count();
    let bt_total   = graph.relationships.iter().filter(|r| r.kind == RelationshipKind::BelongsTo).count();

    println!("  Entity types  : {}", graph.type_counts().len());
    println!("  Total entities: {}", graph.objects.len());
    println!("  HAS edges     : {:>3}  (FK-ownership chains within each BC)", has_total);
    println!("  BELONGS_TO    : {:>3}  (shared categorical dimensions)", bt_total);
    println!();

    println!("  Relationship patterns discovered:");
    println!("  {:.<38} {:>5}", "pattern", "count");
    println!("  {}", "─".repeat(45));
    for (from, kind, to, n) in graph.relationship_patterns() {
        println!("  {:<18} ──{:^10}──▶ {:<14} {:>3}", from, kind, to, n);
    }
    println!();

    // ── Bounded Context detection ─────────────────────────────────────────────
    section("PASS 2 — BOUNDED CONTEXT DETECTION  (Union-Find over HAS edges)");
    println!("  Rule: entity types connected by HAS chains → same Bounded Context.");
    println!("  No FK crosses BC boundaries → 4 disjoint clusters expected.");
    println!();

    let ctx = BoundedContextDetector::detect(&graph);

    println!("  ┌─────────────────────────────────────────────────────────────┐");
    for bc in &ctx.contexts {
        println!("  │  BC  {:.<20}  {:>2} types  cohesion {:.0}%  {}",
            bc.name,
            bc.entity_types.len(),
            bc.cohesion * 100.0,
            bc.entity_types.join(" · "));
    }
    println!("  └─────────────────────────────────────────────────────────────┘");
    println!();
    println!("  Result: {} Bounded Contexts detected ✓", ctx.contexts.len());
    println!();

    // ── Shared Kernel ─────────────────────────────────────────────────────────
    section("PASS 3 — SHARED KERNEL  (Value Objects referenced across BCs)");
    println!("  These dimensions carry Ubiquitous Language across all contexts.");
    println!("  They live in no BC exclusively — any BC change requires negotiation.");
    println!();

    println!("  Shared Kernel dimensions: {}", ctx.shared_kernel.dimensions.join(" · "));
    println!();

    // ── Context Map (cross-links) ─────────────────────────────────────────────
    section("PASS 4 — CONTEXT MAP  (cross-BC coupling via Shared Kernel)");
    println!("  Each link = a shared categorical dimension bridging two BCs.");
    println!("  These are the seams where Anti-Corruption Layers (ACLs) are needed.");
    println!();

    if ctx.cross_links.is_empty() {
        println!("  (no cross-context links detected)");
    } else {
        let mut seen_pairs: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut by_pair: std::collections::HashMap<(String,String), Vec<String>> =
            std::collections::HashMap::new();
        for cl in &ctx.cross_links {
            let pair = (cl.from_bc.clone(), cl.to_bc.clone());
            by_pair.entry(pair).or_default().push(cl.via_type.clone());
        }
        let mut pairs: Vec<_> = by_pair.into_iter().collect();
        pairs.sort_by_key(|((a,b),_)| (a.clone(), b.clone()));
        for ((from, to), dims) in &pairs {
            let key = format!("{}|{}", from, to);
            if seen_pairs.insert(key) {
                let mut d = dims.clone();
                d.sort();
                println!("  {:<18} ◀─── {} ───▶  {:<18}  via: {}",
                    from, "shared".to_string(), to, d.join(", "));
            }
        }
    }
    println!();

    // ── DDD classification per entity type ────────────────────────────────────
    section("DDD CLASSIFICATION  (auto-derived from graph topology)");
    println!("  Aggregate Root = owns children via HAS, not owned by any parent.");
    println!("  Entity         = has identity; owned or nested.");
    println!("  Value Object   = identity-free grouping dimension (Shared Kernel).");
    println!();

    let mapping = DddMapping::from_graph(&graph);
    println!("  {:<18} {:<16} {}", "entity type", "ddd concept", "reason");
    println!("  {}", "─".repeat(75));
    for c in &mapping.objects {
        println!("  {:<18} {:<16} {}", c.object_type, c.concept.label(), c.reason);
    }
    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    section("SUMMARY");
    println!("  {} entity types  ·  {} entities  ·  {} HAS  ·  {} BELONGS_TO",
        graph.type_counts().len(),
        graph.objects.len(),
        has_total,
        bt_total);
    println!();
    for bc in &ctx.contexts {
        let agg_roots: Vec<_> = bc.entity_types.iter()
            .filter(|t| {
                mapping.objects.iter().any(|c| &c.object_type == *t
                    && c.concept.label() == "Aggregate Root")
            })
            .cloned()
            .collect();
        let entities: Vec<_> = bc.entity_types.iter()
            .filter(|t| {
                mapping.objects.iter().any(|c| &c.object_type == *t
                    && c.concept.label() != "Aggregate Root")
            })
            .cloned()
            .collect();
        println!("  BC «{}»", bc.name);
        println!("     Aggregate Root : {}", agg_roots.join(", "));
        println!("     Entities       : {}", entities.join(", "));
        println!("     Cohesion       : {:.0}%  ({} internal HAS links)",
            bc.cohesion * 100.0, bc.internal_links);
        println!();
    }
    println!("  Shared Kernel ({} dims): {}",
        ctx.shared_kernel.dimensions.len(),
        ctx.shared_kernel.dimensions.join(", "));
    println!();

    // ── JSON Export for D3 visualizer ─────────────────────────────────────────
    section("JSON EXPORT  →  ontology_graph.json");
    let json = JsonExporter::export(&graph, &mapping, &ctx);
    JsonExporter::write(&json, "ontology_graph.json").expect("JSON write failed");
    println!("  Written: ontology_graph.json");
    println!("  Start visualizer:  cargo run --bin serve");
    println!("  Open browser:      http://localhost:3000");
    println!();
    println!("  The D3 graph will show 4 coloured BC hulls with:");
    println!("  · Aggregate Root nodes (red)  ·  Entity nodes (blue)");
    println!("  · Value Object dims (green)   ·  HAS/BELONGS_TO edge styles");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn banner(title: &str) {
    let line = "═".repeat(title.len() + 6);
    println!("╔{}╗", line);
    println!("║   {}   ║", title);
    println!("╚{}╝", line);
    println!();
}

fn section(title: &str) {
    println!("═══ {} {}", title, "═".repeat(75usize.saturating_sub(title.len() + 5)));
}
