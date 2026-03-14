//! Example 5 — Complex Discovery: 10 entity types across a full company dataset
//!
//! Run:  cargo run --example 05_complex_discovery
//!
//! Loads 9 CSV files with no pre-defined schema.
//! The DiscoveryEngine runs its 3-pass scan and automatically identifies:
//!   · Entity types (one per CSV file)
//!   · HAS relationships  (fields ending in `_id` that match existing entity IDs)
//!   · BELONGS_TO groups  (string fields with repeated values)
//!
//! Original dataset (examples 01–04): 2 types · 23 entities · 46 relationships
//! This dataset                      : 10 types · N entities · M relationships

use std::collections::HashMap;
use palantir::{
    application::ontology::{
        bounded_context::BoundedContextDetector,
        discovery::DiscoveryEngine,
        graph::OntologyGraph,
        relationship::RelationshipKind,
    },
    infrastructure::datasource::CsvLoader,
    infrastructure::pipeline::dataset::{Dataset, Value},
};

const BASE: &str = "data/complex";

fn main() {
    // ── Load all 9 CSV files ──────────────────────────────────────────────────
    let files: &[(&str, &str)] = &[
        ("regions.csv",     "Region"),
        ("divisions.csv",   "Division"),
        ("departments.csv", "Department"),
        ("offices.csv",     "Office"),
        ("employees.csv",   "Employee"),
        ("vendors.csv",     "Vendor"),
        ("projects.csv",    "Project"),
        ("assignments.csv", "Assignment"),
        ("contracts.csv",   "Contract"),
        ("expenses.csv",    "Expense"),
    ];

    println!("╔═══════════════════════════════════════════════════════════════════════╗");
    println!("║       DISCOVERY ENGINE  —  Complex Company Operations Dataset        ║");
    println!("╚═══════════════════════════════════════════════════════════════════════╝");
    println!();
    println!("  Loading {} CSV files (no pre-defined schema)...", files.len());
    println!();

    let mut datasets: Vec<Dataset> = Vec::new();
    let mut total_records = 0usize;

    println!("  {:>3}  {:<28} {:<16} {:>7}", "#", "file", "entity type", "records");
    println!("  {}", "─".repeat(58));
    for (i, (file, entity_type)) in files.iter().enumerate() {
        let path = format!("{}/{}", BASE, file);
        match CsvLoader::load(&path, entity_type) {
            Ok(ds) => {
                let n = ds.len();
                total_records += n;
                println!("  {:>3}  {:<28} {:<16} {:>7}", i + 1, file, entity_type, n);
                datasets.push(ds);
            }
            Err(e) => {
                println!("  {:>3}  {:<28} ERROR: {}", i + 1, file, e);
            }
        }
    }
    println!("  {}", "─".repeat(58));
    println!("  {:>3}  {:<28} {:<16} {:>7}", "", "TOTAL", "", total_records);
    println!();

    // ── Run Discovery Engine ──────────────────────────────────────────────────
    println!("  Running 3-pass discovery...");
    let (objects, relationships) = DiscoveryEngine::discover(&datasets);
    let graph = OntologyGraph::build(objects, relationships);

    let has_count = graph.relationships.iter()
        .filter(|r| r.kind == RelationshipKind::Has).count();
    let bt_count = graph.relationships.iter()
        .filter(|r| r.kind == RelationshipKind::BelongsTo).count();

    println!("  Done.");
    println!();
    println!("  ┌───────────────────────────────────────────────────────────┐");
    println!("  │  Original (2 types):   23 entities    46 relationships    │");
    println!("  │  This dataset          {:>2} types  {:>4} entities  {:>4} relationships  │",
        graph.type_counts().len(), graph.objects.len(), graph.relationships.len());
    println!("  │    ├─ HAS (Integration):  {:>4} edges (FK-derived)         │", has_count);
    println!("  │    └─ BELONGS_TO (Logic): {:>4} edges (categorical)        │", bt_count);
    println!("  └───────────────────────────────────────────────────────────┘");
    println!();

    // ── Pass 1 result: Entity types ───────────────────────────────────────────
    section("PASS 1 — Entities: one OntologyObject per record");
    println!("  {:<16} {:>8}   fields auto-inferred", "entity type", "count");
    println!("  {}", "─".repeat(48));
    for (type_name, count) in graph.type_counts() {
        // Show which fields were discovered for this type
        let sample = graph.objects_by_type(&type_name).first().cloned();
        let fields: Vec<_> = sample.iter()
            .flat_map(|o| o.record.fields.keys().cloned())
            .filter(|k| k != "id")
            .collect();
        let field_str = fields.join(", ");
        println!("  {:<16} {:>8}   [{}]", type_name, count, trunc_str(&field_str, 52));
    }
    println!();

    // ── Pass 2 result: HAS relationships (FK detection) ───────────────────────
    section("PASS 2 — HAS Relationships: auto-detected from *_id fields");
    println!("  How it works: any field ending in `_id` whose value matches an");
    println!("  existing entity ID → that entity HAS the current record.");
    println!();

    let mut has_patterns: HashMap<(String, String, String), (usize, Vec<String>)> = HashMap::new();
    for rel in graph.relationships.iter().filter(|r| r.kind == RelationshipKind::Has) {
        let key = (rel.from_type.0.clone(), rel.to_type.0.clone(), rel.via_field.clone());
        let entry = has_patterns.entry(key).or_insert((0, Vec::new()));
        entry.0 += 1;
    }
    let mut has_sorted: Vec<_> = has_patterns.into_iter().collect();
    has_sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));

    println!("  {:<14} ──HAS──▶ {:<14} {:<20} {:>5}",
        "from (owner)", "to (owned)", "via field", "count");
    println!("  {}", "─".repeat(60));
    for ((from, to, via), (count, _)) in &has_sorted {
        println!("  {:<14} ──HAS──▶ {:<14} .{:<19} {:>5}", from, to, via, count);
    }
    println!();
    println!("  Notable discoveries:");
    println!("    · Employee ──manager_id──▶ Employee   (self-referential org chart)");
    println!("    · Employee ──approver_id──▶ Expense   (dual role: submitter + approver)");
    println!("    · Project  ──lead_employee_id──▶ ...  (non-obvious FK via field naming)");
    println!();

    // ── Pass 3 result: BELONGS_TO (categorical grouping) ─────────────────────
    section("PASS 3 — BELONGS_TO Dimensions: auto-detected from repeated string values");
    println!("  How it works: any string field where at least one value repeats");
    println!("  across records → all values become grouping dimensions.");
    println!();

    let mut bt_dims: HashMap<(String, String), HashMap<String, usize>> = HashMap::new();
    for rel in graph.relationships.iter().filter(|r| r.kind == RelationshipKind::BelongsTo) {
        let dim_val = rel.to_id.0.trim_start_matches(&format!("{}:", rel.via_field)).to_string();
        bt_dims
            .entry((rel.from_type.0.clone(), rel.via_field.clone()))
            .or_default()
            .entry(dim_val)
            .and_modify(|c| *c += 1)
            .or_insert(1);
    }
    let mut bt_sorted: Vec<_> = bt_dims.into_iter().collect();
    bt_sorted.sort_by_key(|((t, _), _)| t.clone());

    println!("  {:<14} {:<18} {:>5}   distinct values", "entity", "dimension", "edges");
    println!("  {}", "─".repeat(65));
    for ((entity, dim), vals) in &bt_sorted {
        let total: usize = vals.values().sum();
        let val_list: Vec<String> = {
            let mut v: Vec<_> = vals.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            v.iter().map(|(k, n)| format!("{}({})", k, n)).collect()
        };
        println!("  {:<14} {:<18} {:>5}   {}",
            entity, dim, total, trunc_str(&val_list.join(", "), 40));
    }
    println!();

    // ── Multi-level hierarchy visualization ───────────────────────────────────
    section("MULTI-LEVEL ENTITY HIERARCHY  (auto-discovered from FK chains)");
    println!("  Region ──HAS──▶ Division ──HAS──▶ Department ──HAS──▶ Employee");
    println!("                                         └──HAS──▶ Project ──HAS──▶ Assignment");
    println!("  Region ──HAS──▶ Office ──HAS──▶ Employee");
    println!("  Vendor ──HAS──▶ Contract ◀──HAS── Department");
    println!();

    for region in graph.objects_by_type("Region") {
        let region_name = region.get("name").and_then(Value::as_str).unwrap_or(&region.id.0);
        let region_currency = region.get("currency").and_then(Value::as_str).unwrap_or("?");
        println!("  ┌── Region: {} [{}]", region_name, region_currency);

        // Divisions in this region
        let divisions: Vec<_> = graph.outgoing(&region.id.0, Some(&RelationshipKind::Has))
            .into_iter()
            .filter(|r| r.to_type.0 == "Division")
            .collect();

        for (di, div_rel) in divisions.iter().enumerate() {
            let is_last_div = di == divisions.len() - 1;
            let div_branch = if is_last_div { "└─" } else { "├─" };
            let Some(div) = graph.find_object(&div_rel.to_id.0) else { continue };
            let div_name = div.get("name").and_then(Value::as_str).unwrap_or(&div.id.0);
            let div_pfx = if is_last_div { "   " } else { "│  " };
            println!("  │  {} Division: {}", div_branch, div_name);

            // Departments in this division
            let depts: Vec<_> = graph.outgoing(&div.id.0, Some(&RelationshipKind::Has))
                .into_iter()
                .filter(|r| r.to_type.0 == "Department")
                .collect();

            for (di2, dept_rel) in depts.iter().enumerate() {
                let is_last_dept = di2 == depts.len() - 1;
                let dept_branch = if is_last_dept { "└─" } else { "├─" };
                let dept_pfx = if is_last_dept { "   " } else { "│  " };
                let Some(dept) = graph.find_object(&dept_rel.to_id.0) else { continue };
                let dept_name = dept.get("name").and_then(Value::as_str).unwrap_or(&dept.id.0);
                let dept_budget = dept.get("budget").and_then(Value::as_f64).unwrap_or(0.0);
                let dept_status = dept.get("status").and_then(Value::as_str).unwrap_or("?");
                println!("  │  {}  {} Dept: {} (budget: ${:.0}, status: {})",
                    div_pfx, dept_branch, dept_name, dept_budget, dept_status);

                // Employees in this department
                let emps: Vec<_> = graph.outgoing(&dept.id.0, Some(&RelationshipKind::Has))
                    .into_iter()
                    .filter(|r| r.to_type.0 == "Employee")
                    .collect();

                for (ei, emp_rel) in emps.iter().enumerate() {
                    let is_last_emp = ei == emps.len() - 1;
                    let emp_branch = if is_last_emp { "└─" } else { "├─" };
                    let Some(emp) = graph.find_object(&emp_rel.to_id.0) else { continue };
                    let emp_name = emp.label();
                    let emp_level = emp.get("level").and_then(Value::as_str).unwrap_or("?");
                    let emp_salary = emp.get("salary").and_then(Value::as_f64).unwrap_or(0.0);

                    // Projects led by this employee
                    let led_projects: Vec<_> = graph
                        .outgoing(&emp.id.0, Some(&RelationshipKind::Has))
                        .into_iter()
                        .filter(|r| r.to_type.0 == "Project" && r.via_field == "lead_employee_id")
                        .filter_map(|r| graph.find_object(&r.to_id.0))
                        .collect();

                    let proj_label = if led_projects.is_empty() {
                        String::new()
                    } else {
                        let names: Vec<_> = led_projects.iter()
                            .filter_map(|p| p.get("name").and_then(Value::as_str))
                            .collect();
                        format!("  leads: [{}]", names.join(", "))
                    };

                    println!("  │  {}  {}  {} {:<15} ({}, ${:.0}){}",
                        div_pfx, dept_pfx, emp_branch,
                        emp_name, emp_level, emp_salary, proj_label);
                }
                // Projects in this department
                let dept_projects: Vec<_> = graph.outgoing(&dept.id.0, Some(&RelationshipKind::Has))
                    .into_iter()
                    .filter(|r| r.to_type.0 == "Project")
                    .filter_map(|r| graph.find_object(&r.to_id.0))
                    .collect();

                if !dept_projects.is_empty() {
                    println!("  │  {}  {}  Projects: {}",
                        div_pfx, dept_pfx,
                        dept_projects.iter()
                            .filter_map(|p| p.get("name").and_then(Value::as_str))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
        println!("  │");
    }
    println!();

    // ── Vendor ↔ Contract ↔ Department ───────────────────────────────────────
    section("CROSS-CUTTING: Vendor → Contract ← Department (auto-discovered)");
    println!("  {:<22} {:<10} {:<14} {:>8}  status",
        "vendor", "contract", "department", "value");
    println!("  {}", "─".repeat(62));

    for vendor in graph.objects_by_type("Vendor") {
        let v_name = vendor.get("name").and_then(Value::as_str).unwrap_or(&vendor.id.0);
        for c_rel in graph.outgoing(&vendor.id.0, Some(&RelationshipKind::Has))
            .iter()
            .filter(|r| r.to_type.0 == "Contract")
        {
            let Some(contract) = graph.find_object(&c_rel.to_id.0) else { continue };
            let value = contract.get("value").and_then(Value::as_f64).unwrap_or(0.0);
            let status = contract.get("status").and_then(Value::as_str).unwrap_or("?");
            // Find which department this contract belongs to
            let dept_name = graph.outgoing(&c_rel.to_id.0, Some(&RelationshipKind::Has))
                .is_empty()
                .then_some("—")
                .unwrap_or("—");
            // The department → contract HAS edge is stored as dept outgoing, not contract outgoing
            // Find department by scanning HAS edges that point to this contract
            let dept_name = {
                let mut d = "—".to_string();
                for dept in graph.objects_by_type("Department") {
                    let has_this = graph.outgoing(&dept.id.0, Some(&RelationshipKind::Has))
                        .iter()
                        .any(|r| r.to_id.0 == c_rel.to_id.0);
                    if has_this {
                        d = dept.get("name").and_then(Value::as_str)
                            .unwrap_or(&dept.id.0).to_string();
                        break;
                    }
                }
                d
            };
            println!("  {:<22} {:<10} {:<14} {:>8.0}  {}",
                trunc_str(v_name, 22), c_rel.to_id.0, dept_name, value, status);
        }
    }
    println!();

    // ── Bounded Context detection ─────────────────────────────────────────────
    section("BOUNDED CONTEXT DETECTION  (Union-Find on HAS edges)");
    let context_map = BoundedContextDetector::detect(&graph);

    println!("  Detected {} Bounded Context(s):", context_map.contexts.len());
    println!();
    for bc in &context_map.contexts {
        let filled    = (bc.cohesion * 30.0).round() as usize;
        let coh_bar   = "█".repeat(filled);
        let coh_empty = "░".repeat(30usize.saturating_sub(filled));
        println!("  ┌── BC: \"{}\"", bc.name);
        println!("  │   Entity types: {}", bc.entity_types.iter()
            .map(|t| t.as_str()).collect::<Vec<_>>().join(", "));
        println!("  │   Cohesion: {}{}  {:.0}%  ({} internal HAS edges)",
            coh_bar, coh_empty, bc.cohesion * 100.0, bc.internal_links);
        println!("  └──");
        println!();
    }

    println!("  Shared Kernel (Value Objects — no independent identity, equality by value):");
    for dim in &context_map.shared_kernel.dimensions {
        println!("    · {}", dim);
    }
    println!();
    if !context_map.cross_links.is_empty() {
        println!("  Cross-context links:");
        for link in &context_map.cross_links {
            println!("    {} ──[{}]──▶ {}  ({} links)", link.from_bc, link.via_type, link.to_bc, link.count);
        }
        println!();
    }

    // ── Summary ───────────────────────────────────────────────────────────────
    section("DISCOVERY SUMMARY");
    println!("  What the engine found with ZERO pre-defined schema:");
    println!();
    println!("  Entity types    : {:<4} (vs 2 in the original dataset)",
        graph.type_counts().len());
    println!("  Total entities  : {:<4} (vs 23)", graph.objects.len());
    println!("  HAS edges       : {:<4} (vs 15)  — FK chains across {} type pairs",
        has_count, has_sorted.len());
    println!("  BELONGS_TO edges: {:<4} (vs 31)  — {} categorical dimensions discovered",
        bt_count, bt_sorted.len());
    println!("  Total relations : {:<4} (vs 46)", graph.relationships.len());
    println!();
    println!("  Self-referential: Employee ──manager_id──▶ Employee  (org chart)");
    println!("  Dual role edge  : Employee ──employee_id──▶ Expense  (submitter)");
    println!("                    Employee ──approver_id──▶ Expense  (approver)");
    println!("  3-level FK chain: Region → Division → Department → Employee");
    println!("  Cross-cutting   : Vendor → Contract ← Department  (many-to-many via contracts)");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn section(title: &str) {
    println!("═══ {} {}", title, "═".repeat(75usize.saturating_sub(title.len() + 5)));
}

fn trunc_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max - 1).collect::<String>())
    }
}
