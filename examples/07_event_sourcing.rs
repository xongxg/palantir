//! Example 7 — Event Sourcing: Append-only EventStore · State Reconstruction · Projections
//!
//! Run:  cargo run --example 07_event_sourcing
//!
//! Replaces the old EventBus (fire-and-forget Vec) with a proper Event Store.
//!
//! Core principle:
//!   The event stream is the ONLY source of truth.
//!   Current state = fold(events, apply).
//!   Repositories store nothing — state is rebuilt on demand.
//!
//! Five acts:
//!
//!   Act 1  Append   — load scrambled CSV into EventStore (raw order)
//!   Act 2  Disorder — detect state-machine violations caused by out-of-order append
//!   Act 3  Rebuild  — sort + re-ingest; rebuild each Order by replaying clean stream
//!   Act 4  Project  — derive read models: status map, revenue/day, customer history
//!   Act 5  Rewind   — time-travel: reconstruct order state at arbitrary past timestamps
//!          Snapshot — snapshot current state → append new event → rebuild from snapshot+delta

use palantir::{
    application::{
        event_sourcing::{
            load_csv_into_store, load_csv_sorted,
            OrderStatusProjection, RevenueByDayProjection, CustomerOrdersProjection,
            snapshot_all, time_travel,
        },
        saga::load_time_travel_csv,
    },
    domain::order::OrderStatus,
    infrastructure::event_store::{EventStore, SnapshotStore},
};

const CSV:        &str = "data/timeseries/order_events.csv";
const TRAVEL_CSV: &str = "data/timeseries/time_travel_queries.csv";

fn main() {
    banner("EVENT SOURCING  —  Order Lifecycle Domain");
    println!("  Source: {}  (intentionally scrambled row order)", CSV);
    println!();
    println!("  EventStore contract:");
    println!("    · Append-only — events are never updated or deleted");
    println!("    · Ordered     — each aggregate owns its own per-aggregate sequence counter");
    println!("    · Replayable  — any read model can be rebuilt by scanning the stream");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 1 — Append raw (scrambled) events
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 1 — APPEND  (raw CSV order, no sorting)");
    println!("  pos  = Vec insertion index   (global, unique across ALL aggregates)");
    println!("  seq  = per-aggregate counter (local,  unique only within ONE aggregate)");
    println!("  Use (order, seq) as a composite key — seq alone is ambiguous.");
    println!();

    let mut raw_store = EventStore::new();
    let n = load_csv_into_store(CSV, &mut raw_store).expect("CSV load failed");

    println!("  {:>3}  {:<6}  {:>3}  {:<18}  {:<20}",
        "pos", "order", "seq", "event_type", "occurred_at");
    println!("  {}", "─".repeat(62));
    for se in raw_store.all() {
        println!("  {:>3}  {:<6}  {:>3}  {:<18}  {:<20}",
            se.store_pos, se.aggregate_id, se.sequence,
            se.event_type, se.occurred_at);
    }
    println!();
    println!("  {} events appended across {} orders.", n, raw_store.aggregate_ids().len());
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 2 — State-machine violation detection (raw order)
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 2 — VIOLATION DETECTION  (events replayed in raw append order)");
    println!("  Replaying events in wrong order violates the state machine:");
    println!("  e.g. ItemShipped arrives before OrderPlaced → illegal transition.");
    println!();

    let mut violations_found = 0usize;
    for id in raw_store.aggregate_ids() {
        let events: Vec<_> = raw_store.load(&id).into_iter()
            .map(|se| &se.event).collect();
        let (state, violations) = palantir::domain::order::OrderState::from_events(&id, &events);

        if violations.is_empty() {
            println!("  [{}]  ✓  clean replay  →  {} (v{})",
                id, state.status.label(), state.version);
        } else {
            violations_found += violations.len();
            println!("  [{}]  ✗  {} violation(s)  →  {} (v{})",
                id, violations.len(), state.status.label(), state.version);
            for v in &violations {
                println!("        ⚠  {}", v);
            }
        }
    }
    println!();
    if violations_found > 0 {
        println!("  {} state-machine violation(s) detected.", violations_found);
        println!("  Cause: CSV rows were scrambled — events arrived out of chronological order.");
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 3 — Rebuild from chronologically sorted stream
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 3 — REBUILD  (sort by occurred_at → replay clean stream)");
    println!("  ISO 8601 string comparison = chronological order: no parsing needed.");
    println!("  After sort: each aggregate's events arrive in the correct state order.");
    println!();

    let mut store = EventStore::new();
    let n2 = load_csv_sorted(CSV, &mut store).expect("CSV load failed");

    // Show sorted append log
    println!("  pos advances globally; seq resets independently per aggregate:");
    println!("  {:>3}  {:<6}  {:>3}  {:<18}  {:<20}",
        "pos", "order", "seq", "event_type", "occurred_at");
    println!("  {}", "─".repeat(62));
    for se in store.all() {
        println!("  {:>3}  {:<6}  {:>3}  {:<18}  {:<20}",
            se.store_pos, se.aggregate_id, se.sequence,
            se.event_type, se.occurred_at);
    }
    println!();
    println!("  {} events re-appended in chronological order.", n2);
    println!();

    // Rebuild each order
    println!("  Aggregate state after clean replay:");
    println!();
    println!("  {:<6}  {:<10}  {:<8}  {:<8}  {:<10}  events",
        "order", "customer", "status", "amount", "version");
    println!("  {}", "─".repeat(60));
    for id in store.aggregate_ids() {
        let events: Vec<_> = store.load(&id).into_iter().map(|se| &se.event).collect();
        let (state, violations) = palantir::domain::order::OrderState::from_events(&id, &events);
        let ok_marker = if violations.is_empty() { "✓" } else { "✗" };
        println!("  {:<6}  {:<10}  {:<8}  {:>8.0}  v{:<8}  {} {} events",
            id, state.customer_id, state.status.label(),
            state.amount, state.version,
            ok_marker, store.load(&id).len());
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 4 — Projections  (read models derived from the event stream)
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 4 — PROJECTIONS  (read models rebuilt by replaying the event stream)");
    println!("  Projections are disposable — destroy and rebuild any time.");
    println!("  They never hold the canonical truth; the event stream does.");
    println!();

    // ── 4a: Order Status Projection ──────────────────────────────────────────
    println!("  ┌─ Projection: Order Status ─────────────────────────────────┐");
    let status_proj = OrderStatusProjection::build(&store);
    let statuses = [
        OrderStatus::Delivered, OrderStatus::Shipped,
        OrderStatus::Paid,      OrderStatus::Placed,
        OrderStatus::Cancelled,
    ];
    for s in &statuses {
        let orders = status_proj.by_status(s);
        if !orders.is_empty() {
            let ids: Vec<_> = orders.iter().map(|o| o.id.as_str()).collect();
            println!("  │  {:<12}  {} order(s): {}", s.label(), orders.len(), ids.join(", "));
        }
    }
    if !status_proj.violations.is_empty() {
        println!("  │  ⚠  {} projection violation(s)", status_proj.violations.len());
    }
    println!("  └────────────────────────────────────────────────────────────┘");
    println!();

    // ── 4b: Revenue by Day ───────────────────────────────────────────────────
    println!("  ┌─ Projection: Revenue by Day ───────────────────────────────┐");
    let rev_proj = RevenueByDayProjection::build(&store);
    let max_rev = rev_proj.sorted_days().iter().map(|(_, v)| *v as usize).max().unwrap_or(1);
    for (day, amount) in rev_proj.sorted_days() {
        let bar_len = (amount as usize * 20 / max_rev).max(1);
        let bar = "█".repeat(bar_len);
        println!("  │  {}  {}  ${:.0}", day, bar, amount);
    }
    println!("  │  ─────────────────────────────────────────────────────────");
    println!("  │  Total revenue: ${:.0}", rev_proj.total);
    println!("  └────────────────────────────────────────────────────────────┘");
    println!();

    // ── 4c: Customer Order History ───────────────────────────────────────────
    println!("  ┌─ Projection: Customer Orders ──────────────────────────────┐");
    let cust_proj = CustomerOrdersProjection::build(&store);
    let mut customers: Vec<_> = cust_proj.by_customer.iter().collect();
    customers.sort_by_key(|(k, _)| k.as_str());
    for (cust, (orders, paid)) in &customers {
        println!("  │  {:<6}  {} order(s)  {:>4} events  paid: ${:.0}",
            cust, orders.len(),
            store.all().iter().filter(|se| orders.contains(&se.aggregate_id)).count(),
            paid);
    }
    println!("  └────────────────────────────────────────────────────────────┘");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 5a — Time-travel queries
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 5a — TIME-TRAVEL  (reconstruct state at arbitrary past timestamp)");
    println!("  \"What did the system look like at T?\"");
    println!("  Method: load events where occurred_at ≤ T, then replay.");
    println!();

    let queries = load_time_travel_csv(TRAVEL_CSV).expect("time_travel_queries.csv load failed");

    println!("  {:<6}  {:<22}  {:<5}  {:<12}  {}",
        "order", "at time", "evts", "status", "notes");
    println!("  {}", "─".repeat(68));
    for (order_id, until, _desc) in &queries {
        let (state, count) = time_travel(&store, order_id, until);
        let note = match state.status {
            OrderStatus::Draft     => "no events yet",
            OrderStatus::Placed    => "payment not received",
            OrderStatus::Paid      => "awaiting shipment",
            OrderStatus::Shipped   => "in transit",
            OrderStatus::Delivered => "complete",
            OrderStatus::Cancelled => "cancelled",
        };
        println!("  {:<6}  {:<22}  {:>5}  {:<12}  {}",
            order_id, until, count, state.status.label(), note);
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 5b — Snapshots + delta replay
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 5b — SNAPSHOTS  (avoid full replay cost for long-lived aggregates)");
    println!("  Pattern: snapshot at version N → reload only events since N.");
    println!("  Reduces replay cost from O(all_events) to O(events_since_snapshot).");
    println!();

    let mut snap_store = SnapshotStore::new();
    snapshot_all(&store, &mut snap_store, "2024-01-17T00:00:00");

    println!("  Snapshots taken:");
    for id in store.aggregate_ids() {
        if let Some(snap) = snap_store.load(&id) {
            println!("    [{}]  v{}  status={}  amount={:.0}  taken_at={}",
                snap.aggregate_id, snap.version,
                snap.state.status.label(), snap.state.amount,
                snap.taken_at);
        }
    }
    println!();

    // Append a new event AFTER snapshotting — simulates o22 getting shipped
    println!("  Appending post-snapshot event: o22 ItemShipped at 2024-01-15T08:00:00");
    store.append("o22", "2024-01-15T08:00:00",
        palantir::domain::order::OrderEvent::ItemShipped { order_id: "o22".to_string() });
    println!();

    println!("  Rebuild via snapshot + delta  (delta = events after snapshot version):");
    for id in ["o01", "o04", "o09", "o14", "o21", "o22"] {
        let (state, violations) = snap_store.rebuild(id, &store);
        let snap_v = snap_store.load(id).map(|s| s.version).unwrap_or(0);
        let delta_n = store.load_since_version(id, snap_v).len();
        let ok = if violations.is_empty() { "✓" } else { "✗" };
        println!("  {}  [{}]  snapshot v{}  +{}δ  →  {} (v{})",
            ok, id, snap_v, delta_n, state.status.label(), state.version);
    }
    println!();

    // ── Summary ───────────────────────────────────────────────────────────────
    section("SUMMARY");
    println!("  EventStore:  {} events  ·  {} aggregates  ·  {} projections rebuilt",
        store.len(), store.aggregate_ids().len(), 3);
    println!();
    println!("  Act 1  Append   — raw CSV → EventStore with per-aggregate sequence numbers");
    println!("  Act 2  Disorder — {} violations detected from out-of-order append",
        violations_found);
    println!("  Act 3  Rebuild  — chronological sort → clean replay → correct state");
    println!("  Act 4  Project  — 3 read models derived from the same event stream");
    println!("           Status map    : current state of every order");
    println!("           Revenue/day   : financial aggregate from PaymentReceived events");
    println!("           Customer view : per-customer order history + spend");
    println!("  Act 5a Time-travel — 6 past-state queries at arbitrary timestamps");
    println!("  Act 5b Snapshot   — snapshot + 1 delta event → rebuild from δ only");
    println!();
    println!("  Contrast with EventBus (old):");
    println!("    EventBus  — fire-and-forget Vec; no sequence; no replay; no projections");
    println!("    EventStore — append-only; sequenced; replayable; supports time-travel");
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
