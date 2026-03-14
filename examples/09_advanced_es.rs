//! Example 9 — Advanced Event Sourcing: Optimistic Concurrency · Upcasting · Read Model Export
//!
//! Run:  cargo run --example 09_advanced_es
//!       then  cargo run --bin serve  →  http://localhost:3000
//!
//! Three acts:
//!
//!   Act 1  Optimistic Concurrency
//!          Writer A and Writer B both read version N.
//!          Writer A appends successfully.
//!          Writer B gets ConcurrencyError — must reload and retry.
//!
//!   Act 2  Event Upcasting
//!          V1 events (no customer_id in OrderPlaced) are upcasted to V2
//!          before being applied to the aggregate.  Stored bytes never change.
//!
//!   Act 3  Read Model → ontology_graph.json
//!          OrderStatusProjection exported as entity graph.
//!          cargo run --bin serve  shows live order states in D3 visualizer.

use palantir::{
    application::event_sourcing::{
        export_order_graph, load_csv_sorted, OrderStatusProjection,
    },
    domain::{
        order::{OrderEvent, OrderState},
        order_v1::{OrderEventV1, RawEvent, UpcastChain},
    },
    infrastructure::event_store::EventStore,
};

const CSV: &str = "data/timeseries/order_events.csv";

fn main() {
    banner("ADVANCED EVENT SOURCING  —  Concurrency · Upcasting · Read Model Export");

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 1 — Optimistic Concurrency
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 1 — OPTIMISTIC CONCURRENCY");
    println!("  Pattern: reader observes version N, includes N as precondition on write.");
    println!("  If another writer advanced to N+1 first → ConcurrencyError → reload + retry.");
    println!();

    let mut store = EventStore::new();
    load_csv_sorted(CSV, &mut store).expect("CSV load failed");

    let target = "o22"; // in-progress order: currently v2 (Placed → Paid)

    let v_before = store.version_of(target);
    println!("  Setup: order {} is at v{} (status: {})",
        target, v_before, rebuild_status(&store, target));
    println!();

    // ── Writer A: correct expected_version ───────────────────────────────────
    println!("  Writer A: append_expected(expected=v{})  →", v_before);
    let result_a = store.append_expected(
        target,
        "2024-01-15T08:00:00",
        OrderEvent::ItemShipped { order_id: target.to_string() },
        v_before,
    );
    match &result_a {
        Ok(new_seq) => println!("    ✓  Ok — {} is now v{}  (status: {})",
            target, new_seq, rebuild_status(&store, target)),
        Err(e) => println!("    ✗  Err: {}", e),
    }
    println!();

    // ── Writer B: same expected_version (stale read — raced with A) ──────────
    let stale_version = v_before;  // B still thinks the version is v2
    println!("  Writer B: append_expected(expected=v{})  →  (stale — A already wrote v{})",
        stale_version, v_before + 1);
    let result_b = store.append_expected(
        target,
        "2024-01-15T09:00:00",
        OrderEvent::OrderDelivered { order_id: target.to_string() },
        stale_version,
    );
    match &result_b {
        Ok(_)  => println!("    ✓  Ok (unexpected)"),
        Err(e) => println!("    ✗  ConcurrencyError: {}", e),
    }
    println!();

    // ── Writer B retries with reloaded version ────────────────────────────────
    let fresh_version = store.version_of(target);
    println!("  Writer B reloads → v{}, retries append_expected(expected=v{})  →",
        fresh_version, fresh_version);
    let result_retry = store.append_expected(
        target,
        "2024-01-15T10:00:00",
        OrderEvent::OrderDelivered { order_id: target.to_string() },
        fresh_version,
    );
    match &result_retry {
        Ok(seq) => println!("    ✓  Ok — {} is now v{}  (status: {})",
            target, seq, rebuild_status(&store, target)),
        Err(e) => println!("    ✗  Err: {}", e),
    }
    println!();

    // Show all events for o22
    println!("  Full event log for {}:", target);
    println!("  {:<6}  {:>3}  {:<18}  {:<20}",
        "order", "seq", "event_type", "occurred_at");
    println!("  {}", "─".repeat(56));
    for se in store.load(target) {
        println!("  {:<6}  {:>3}  {:<18}  {:<20}",
            se.aggregate_id, se.sequence, se.event_type, se.occurred_at);
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 2 — Event Upcasting
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 2 — EVENT UPCASTING  (V1 schema → V2 schema)");
    println!("  Scenario: legacy system stored OrderPlaced without customer_id (V1).");
    println!("  New code needs customer_id.  We upcast on read — stored data unchanged.");
    println!();

    // Simulate a batch of V1 events loaded from legacy storage
    let legacy_events: Vec<(String, RawEvent)> = vec![
        ("2024-02-01T09:00:00".into(), RawEvent::V1(OrderEventV1::OrderPlaced {
            order_id: "o99".to_string(),
            amount:   450.0,
        })),
        ("2024-02-01T09:05:00".into(), RawEvent::V1(OrderEventV1::PaymentReceived {
            order_id: "o99".to_string(),
            amount:   450.0,
        })),
        ("2024-02-03T08:00:00".into(), RawEvent::V2(OrderEvent::ItemShipped {
            order_id: "o99".to_string(),
        })),
        ("2024-02-05T14:00:00".into(), RawEvent::V2(OrderEvent::OrderDelivered {
            order_id: "o99".to_string(),
        })),
    ];

    println!("  Raw events from legacy store:");
    println!("  {:<20}  {:<12}  {:<22}",
        "occurred_at", "schema", "event_type");
    println!("  {}", "─".repeat(58));
    for (ts, raw) in &legacy_events {
        let (schema, etype) = match raw {
            RawEvent::V1(v1) => ("V1 (legacy)", v1.event_type()),
            RawEvent::V2(v2) => ("V2 (current)", v2.event_type()),
        };
        println!("  {:<20}  {:<12}  {:<22}", ts, schema, etype);
    }
    println!();

    // Upcast and replay into a fresh store
    let mut upcast_store = EventStore::new();
    println!("  Upcasting and appending to EventStore:");
    for (ts, raw) in legacy_events {
        let version = raw.schema_version();
        let current = UpcastChain::to_current(raw);
        let etype   = current.event_type();
        let customer_id = match &current {
            OrderEvent::OrderPlaced { customer_id, .. } => format!("  customer_id={}", customer_id),
            _ => String::new(),
        };
        let seq = upcast_store.append("o99", &ts, current);
        println!("    v{}→V2  seq={:<2}  {:<18}{}", version, seq, etype, customer_id);
    }
    println!();

    let (state, violations) = {
        let evts: Vec<_> = upcast_store.load("o99").into_iter().map(|se| &se.event).collect();
        OrderState::from_events("o99", &evts)
    };
    println!("  Rebuilt state for o99:");
    println!("    status      = {}", state.status.label());
    println!("    customer_id = {}  ← sentinel marks migrated event", state.customer_id);
    println!("    amount      = {:.0}", state.amount);
    println!("    version     = {}", state.version);
    if violations.is_empty() {
        println!("    violations  = none ✓");
    }
    println!();
    println!("  Key insight: stored bytes were NEVER modified.");
    println!("  Upcast ran purely in memory at read time.");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 3 — Read Model → ontology_graph.json
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 3 — READ MODEL PERSISTENCE  (Projection → ontology_graph.json)");
    println!("  The D3 visualizer reads ontology_graph.json.");
    println!("  We export the live OrderStatusProjection into that format.");
    println!("  Run `cargo run --bin serve` to see the order graph.");
    println!();

    // Build projection on current store (which includes o22's new events from Act 1)
    let proj = OrderStatusProjection::build(&store);

    println!("  Current OrderStatusProjection (source of truth for export):");
    println!("  {:<6}  {:<10}  {:<12}  {:>8}  {:>5}",
        "order", "customer", "status", "amount", "ver");
    println!("  {}", "─".repeat(50));
    let mut order_ids: Vec<_> = proj.orders.keys().collect();
    order_ids.sort();
    for id in &order_ids {
        let s = &proj.orders[*id];
        println!("  {:<6}  {:<10}  {:<12}  {:>8.0}  {:>5}",
            id, s.customer_id, s.status.label(), s.amount, s.version);
    }
    println!();

    // Export
    export_order_graph(&store, "ontology_graph.json")
        .expect("graph export failed");

    println!("  Written: ontology_graph.json");
    println!();
    println!("  Graph structure exported:");
    println!("    Nodes  — Customer (Aggregate Root, red)");
    println!("           — Order    (Entity, blue)");
    println!("           — status:X (Value Object, green)");
    println!("    Edges  — Customer ──HAS──▶ Order  (solid blue)");
    println!("           — Order ──BELONGS_TO──▶ status:X  (dashed green)");
    println!("    BC     — «OrderFulfillment» hull wrapping Customer + Order nodes");
    println!("    SK     — status shared kernel node outside the hull");
    println!();
    println!("  Projection catch-up demo:");
    println!("  Before exporting, proj was built at checkpoint={}.", proj.checkpoint);
    let mut proj2 = OrderStatusProjection::build(&store);
    // Simulate new event arriving
    store.append("o99", "2024-02-06T10:00:00",
        OrderEvent::OrderPlaced { order_id: "o99".to_string(),
            customer_id: "cu9".to_string(), amount: 99.0 });
    let cp_before = proj2.checkpoint;
    proj2.catch_up(&store);
    println!("  New event appended (o99/OrderPlaced).  catch_up() processed {} new event(s).",
        proj2.checkpoint - cp_before);
    println!("  Projection checkpoint advanced: {} → {}.", cp_before, proj2.checkpoint);
    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    section("SUMMARY");
    println!("  Act 1  Optimistic Concurrency");
    println!("         append_expected(v{}) → Ok", v_before);
    println!("         append_expected(v{}) → ConcurrencyError (stale)", stale_version);
    println!("         reload → append_expected(v{}) → Ok (retry succeeded)", fresh_version);
    println!();
    println!("  Act 2  Event Upcasting");
    println!("         2 V1 events + 2 V2 events → upcast chain → identical replay");
    println!("         customer_id = LEGACY_UNKNOWN marks migrated OrderPlaced events");
    println!("         Stored bytes untouched — upcast is a pure read-time transform");
    println!();
    println!("  Act 3  Read Model → ontology_graph.json");
    println!("         OrderStatusProjection exported as entity graph");
    println!("         Projection catch-up: incremental update via events_after(checkpoint)");
    println!("         Open http://localhost:3000 after `cargo run --bin serve`");
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn rebuild_status(store: &EventStore, order_id: &str) -> &'static str {
    let mut state = OrderState::draft(order_id);
    for se in store.load(order_id) {
        let _ = state.apply(&se.event);
    }
    state.status.label()
}

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
