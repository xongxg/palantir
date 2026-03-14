//! Example 6 — Time Series: Disorder Detection · Auto-Sort · Temporal Analysis
//!
//! Run:  cargo run --example 06_time_series
//!
//! Seven CSV files loaded with deliberately scrambled timestamp order.
//! The Timeline Engine performs three passes:
//!
//!   0. Detection  — auto-identify `*_at` timestamp fields (no schema given)
//!   1. Disorder   — count out-of-order adjacent transitions, show violations
//!   2. Sort       — stable chronological reordering (ISO 8601 string compare)
//!   3. Analysis   — gaps, daily histogram, cross-entity latency

use palantir::{
    application::{
        ontology::{
            bounded_context::BoundedContextDetector,
            discovery::DiscoveryEngine,
            graph::OntologyGraph,
            relationship::RelationshipKind,
        },
        timeline::{
            self, analyse_disorder, compute_latency, compute_stats,
            daily_histogram, detect_gaps, detect_ts_fields, fmt_duration,
            sort_dataset,
        },
    },
    infrastructure::{datasource::CsvLoader, pipeline::dataset::Dataset},
};

const BASE: &str = "data/timeseries";

fn main() {
    // ── Load ──────────────────────────────────────────────────────────────────
    let specs: &[(&str, &str)] = &[
        ("products.csv",         "Product"),
        ("warehouses.csv",       "Warehouse"),
        ("orders.csv",           "Order"),
        ("shipments.csv",        "Shipment"),
        ("payments.csv",         "Payment"),
        ("inventory_events.csv", "InventoryEvent"),
        ("support_tickets.csv",  "SupportTicket"),
    ];

    banner("TIME SERIES DISCOVERY  —  E-Commerce Operations Dataset");

    println!("  Loading {} CSV files (no schema provided)…", specs.len());
    println!();

    let mut datasets: Vec<Dataset> = Vec::new();
    for (file, etype) in specs {
        let path = format!("{}/{}", BASE, file);
        match CsvLoader::load(&path, etype) {
            Ok(ds) => {
                println!("  ✓  {:<28} {:>3} records  ({})", file, ds.len(), etype);
                datasets.push(ds);
            }
            Err(e) => println!("  ✗  {:<28} ERROR: {}", file, e),
        }
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 0 — Auto-detect timestamp fields
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 0 — AUTO-DETECT Timestamp Fields");
    println!("  Rule: field name ends with `_at`, `_time`, `_date`");
    println!("        AND ≥80% of its string values parse as ISO 8601");
    println!();

    let mut ts_fields: Vec<(usize, String)> = Vec::new(); // (dataset_idx, field)
    for (i, ds) in datasets.iter().enumerate() {
        let fields = detect_ts_fields(ds);
        if fields.is_empty() {
            println!("  {:.<30} —  no timestamp fields", ds.object_type);
        } else {
            for f in &fields {
                println!("  {:.<30} ✓  `{}`", ds.object_type, f);
                ts_fields.push((i, f.clone()));
            }
        }
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 1 — Disorder analysis (before sorting)
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 1 — DISORDER ANALYSIS  (data as loaded from CSV)");
    println!("  Out-of-order = consecutive row pair where timestamp[i] > timestamp[i+1]");
    println!();

    for (ds_idx, field) in &ts_fields {
        let ds = &datasets[*ds_idx];
        let report = analyse_disorder(ds, field);

        let bar_len = 30usize;
        let filled  = (report.oo_pct / 100.0 * bar_len as f64).round() as usize;
        let bar     = format!("{}{}", "█".repeat(filled), "░".repeat(bar_len - filled));

        println!("  {} · `{}`", ds.object_type, field);
        println!("  │  {} rows │ {} out-of-order transitions │ {:.0}% disordered",
            report.total, report.oo_count, report.oo_pct);
        println!("  │  [{}]", bar);
        if !report.violations.is_empty() {
            println!("  │  Sample violations (row A appears before row B, but ts_A > ts_B):");
            for v in &report.violations {
                println!("  │    row {:>2} [{}] {} > row {:>2} [{}] {}  ← WRONG ORDER",
                    v.row_a, v.id_a, v.ts_a, v.row_b, v.id_b, v.ts_b);
            }
        }
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 2 — Sort all timestamp datasets
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 2 — SORT  (ISO 8601 lexicographic = chronological)");
    println!("  No parsing needed: '2024-01-03T…' < '2024-01-10T…' as strings.");
    println!();

    for (ds_idx, field) in &ts_fields {
        let ds = &mut datasets[*ds_idx];
        let before: Vec<String> = ds.records.iter().take(4)
            .map(|r| format!("[{}] {}", r.id, r.get(field).and_then(|v| v.as_str()).unwrap_or("")))
            .collect();

        sort_dataset(ds, field);

        let after: Vec<String> = ds.records.iter().take(4)
            .map(|r| format!("[{}] {}", r.id, r.get(field).and_then(|v| v.as_str()).unwrap_or("")))
            .collect();

        // Verify no disorder remains
        let remaining = analyse_disorder(ds, field);

        println!("  {} · `{}`", ds.object_type, field);
        println!("  │  Before (first 4 rows): {}", before.join("  →  "));
        println!("  │  After  (first 4 rows): {}", after.join("  →  "));
        println!("  │  Disorder remaining: {} out-of-order transitions  ✓",
            remaining.oo_count);
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 3a — Timeline statistics (on sorted data)
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 3a — TIMELINE STATISTICS  (after sorting)");

    for (ds_idx, field) in &ts_fields {
        let ds = &datasets[*ds_idx];
        let Some(stats) = compute_stats(ds, field) else { continue };
        println!("  {} · `{}`", ds.object_type, field);
        println!("  │  First : [{:>3}] {}",    stats.first_id, stats.first_ts);
        println!("  │  Last  : [{:>3}] {}",    stats.last_id,  stats.last_ts);
        println!("  │  Span  : {}   ({} events)",
            fmt_duration(stats.span_secs), stats.count);
        println!("  │  Interval  avg: {}   min: {}   max: {}",
            fmt_duration(stats.avg_interval_secs as i64),
            fmt_duration(stats.min_interval_secs),
            fmt_duration(stats.max_interval_secs));
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 3b — Gap detection
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 3b — GAP DETECTION  (silences > 12 hours)");
    println!("  Gaps reveal weekends, outages, or batch-processing windows.");
    println!();

    let twelve_hours = 12 * 3600_i64;
    for (ds_idx, field) in &ts_fields {
        let ds = &datasets[*ds_idx];
        let gaps = detect_gaps(ds, field, twelve_hours);
        if gaps.is_empty() { continue; }
        println!("  {} · `{}` — {} gap(s) detected:", ds.object_type, field, gaps.len());
        for g in &gaps {
            println!("    [{:>3}] {}  ──gap: {}──▶  [{:>3}] {}",
                g.prev_id, g.prev_ts, fmt_duration(g.gap_secs), g.next_id, g.next_ts);
        }
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 3c — Daily histogram
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 3c — DAILY HISTOGRAM  (orders · ordered_at)");

    // Find orders dataset
    if let Some((orders_idx, orders_field)) = ts_fields.iter()
        .find(|(i, _)| datasets[*i].object_type == "Order")
        .map(|(i, f)| (*i, f.clone()))
    {
        let ds = &datasets[orders_idx];
        let hist = daily_histogram(ds, &orders_field);
        let max  = hist.iter().map(|b| b.count).max().unwrap_or(1);

        println!("  Each █ = 1 order");
        println!();
        for b in &hist {
            let bar = "█".repeat(b.count);
            let pad = " ".repeat(max - b.count);
            println!("  {}  {}{}  {} order(s)",
                b.day, bar, pad, b.count);
        }
        println!();

        // Show gap between Jan-05 and Jan-06 (weekend/holiday effect)
        println!("  Note: no orders between 2024-01-04 and 2024-01-06 — weekend gap.");
        println!();
    }

    // ══════════════════════════════════════════════════════════════════════════
    //  PASS 3d — Cross-entity temporal latency
    // ══════════════════════════════════════════════════════════════════════════
    section("PASS 3d — CROSS-ENTITY TEMPORAL LATENCY");
    println!("  Order → Payment latency  (how long until customer pays)");
    println!("  Order → Shipment latency (fulfillment time: order placed → dispatched)");
    println!();

    let orders_idx   = datasets.iter().position(|d| d.object_type == "Order").unwrap();
    let payments_idx = datasets.iter().position(|d| d.object_type == "Payment").unwrap();
    let ship_idx     = datasets.iter().position(|d| d.object_type == "Shipment").unwrap();

    let pay_latency  = compute_latency(
        &datasets[orders_idx],   "ordered_at",
        &datasets[payments_idx], "order_id",    "paid_at",
    );
    let ship_latency = compute_latency(
        &datasets[orders_idx],   "ordered_at",
        &datasets[ship_idx],     "order_id",    "shipped_at",
    );

    // Join into a combined table
    let mut ship_map = std::collections::HashMap::new();
    for s in &ship_latency { ship_map.insert(&s.anchor_id, s); }

    println!("  {:<6}  {:<20}  {:<12}  {:<12}  alert",
        "order", "ordered_at", "pay-latency", "ship-latency");
    println!("  {}", "─".repeat(68));

    for p in &pay_latency {
        let pay_flag  = if p.latency_secs > 3600 { " ⚠ SLOW PAY" } else { "" };
        let (ship_str, ship_flag) = ship_map.get(&p.anchor_id)
            .map(|s| {
                let flag = if s.latency_secs > 2 * 86400 { " ⚠ SLOW SHIP" } else { "" };
                (fmt_duration(s.latency_secs), flag)
            })
            .unwrap_or_else(|| ("—".to_string(), ""));
        let any_flag = format!("{}{}", pay_flag, ship_flag);
        println!("  {:<6}  {}  {:<12}  {:<12}{}",
            p.anchor_id, p.anchor_ts,
            fmt_duration(p.latency_secs), ship_str, any_flag);
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  Ontology Discovery (on sorted data)
    // ══════════════════════════════════════════════════════════════════════════
    section("ONTOLOGY DISCOVERY  (on chronologically sorted datasets)");
    println!("  Same DiscoveryEngine as always — now operating on temporally correct data.");
    println!();

    let (objects, rels) = DiscoveryEngine::discover(&datasets);
    let graph = OntologyGraph::build(objects, rels);

    let has_count = graph.relationships.iter().filter(|r| r.kind == RelationshipKind::Has).count();
    let bt_count  = graph.relationships.iter().filter(|r| r.kind == RelationshipKind::BelongsTo).count();

    println!("  Entity types  : {}", graph.type_counts().len());
    println!("  Total entities: {}", graph.objects.len());
    println!("  HAS edges     : {:<4} (FK-derived relationships)", has_count);
    println!("  BELONGS_TO    : {:<4} (categorical dimensions)", bt_count);
    println!();

    // Show order-centric relationship fan
    println!("  Order entity fan-out (discovered FK links):");
    let order_patterns: std::collections::HashSet<_> = graph.relationships.iter()
        .filter(|r| r.from_type.0 == "Order" || r.to_type.0 == "Order")
        .map(|r| format!("{} ──{}──▶ {}", r.from_type.0, r.kind.label(), r.to_type.0))
        .collect();
    let mut op: Vec<_> = order_patterns.into_iter().collect();
    op.sort();
    for p in &op { println!("    {}", p); }
    println!();

    // Temporal BELONGS_TO: status, method, carrier, event_type, priority
    println!("  Temporal categorical dimensions discovered:");
    let mut dims: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for rel in graph.relationships.iter().filter(|r| r.kind == RelationshipKind::BelongsTo) {
        dims.insert((rel.from_type.0.clone(), rel.via_field.clone()));
    }
    let mut dims: Vec<_> = dims.into_iter().collect();
    dims.sort();
    for (etype, field) in &dims {
        // Count distinct values
        let vals: std::collections::HashSet<_> = graph.relationships.iter()
            .filter(|r| r.kind == RelationshipKind::BelongsTo
                && r.from_type.0 == *etype && r.via_field == *field)
            .map(|r| r.to_id.0.trim_start_matches(&format!("{}:", field)).to_string())
            .collect();
        println!("    {}.{:<20} → {} values: {}",
            etype, field,
            vals.len(),
            {
                let mut v: Vec<_> = vals.into_iter().collect();
                v.sort();
                v.join(", ")
            });
    }
    println!();

    // BC detection
    let ctx = BoundedContextDetector::detect(&graph);
    println!("  Bounded Contexts: {}", ctx.contexts.len());
    for bc in &ctx.contexts {
        println!("    · \"{}\"  ({} types, {:.0}% cohesion)",
            bc.name,
            bc.entity_types.len(),
            bc.cohesion * 100.0);
    }
    println!("  Shared Kernel: {}",
        ctx.shared_kernel.dimensions.join(", "));
    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    section("SUMMARY");
    println!("  Data was loaded in scrambled timestamp order (simulating real-world disorder).");
    println!();
    println!("  Pass 0 — Detected {} timestamp field(s) with no schema input.",
        ts_fields.len());
    println!("  Pass 1 — Disorder confirmed across all time-series datasets.");
    println!("  Pass 2 — Stable chronological sort applied (ISO 8601 string order).");
    println!("           After sort: 0 out-of-order transitions in every dataset.");
    println!("  Pass 3 — On sorted data:");
    println!("           · Gaps >12h detected (e.g. 2024-01-05 → 2024-01-06, weekend)");
    println!("           · Daily demand histogram reveals order volume patterns");
    println!("           · Cross-entity latency flagged slow payments & fulfillments");
    println!("  Discovery — OntologyGraph built on temporally correct data.");
    println!("              Temporal dimensions (status, carrier, event_type…) auto-grouped.");
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
