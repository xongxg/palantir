//! Example 10 — Policy / Saga: Cross-BC Business Process Coordination
//!
//! Run:  cargo run --example 10_policy_saga
//!
//! Pattern: OrderFulfillmentSaga
//!
//!   Customer BC                    Procurement BC
//!   ──────────────────────         ──────────────────────
//!   OrderPlaced
//!   PaymentReceived  ──policy──▶   POCreated
//!                    ◀──policy──   POApproved
//!   ItemShipped
//!   OrderDelivered   ──policy──▶   POFulfilled
//!   ══ COMPLETED ═════════════════════════════════════════
//!
//!   Compensation path (PO cancelled before approval):
//!   PaymentReceived  ──policy──▶   POCreated
//!                    ◀──policy──   POCancelled  ← vendor out of stock
//!   OrderCancelled  (compensation — undoes committed step)
//!   ══ COMPENSATED ✗══════════════════════════════════════
//!
//! Key concepts:
//!   SagaLink  ≈ CrossContextLink but for event-driven (not structural) coupling
//!   Policy    = stateless reactive rule (one SagaLink = one Policy)
//!   Saga      = stateful multi-step process with correlation ID + compensation

use palantir::application::saga::{load_saga_csv, BcEvent, SagaOrchestrator, SAGA_LINKS};
use palantir::domain::order::OrderEvent;
use palantir::domain::procurement::ProcurementEvent;

const HAPPY_CSV:        &str = "data/saga/happy_path.csv";
const COMPENSATION_CSV: &str = "data/saga/compensation.csv";

fn main() {
    banner("POLICY / SAGA  —  Cross-BC Business Process Coordination");

    // ══════════════════════════════════════════════════════════════════════════
    //  Routing table: CrossContextLink → event-driven SagaLink
    // ══════════════════════════════════════════════════════════════════════════
    section("ROUTING TABLE  (SagaLink — one row per cross-BC policy)");
    println!(
        "  {:<12}  {:<20}  {:<12}  {:<20}  {}",
        "from_bc", "trigger_event", "to_bc", "action_emitted", "compensate?"
    );
    println!("  {}", "─".repeat(76));
    for link in SAGA_LINKS {
        println!(
            "  {:<12}  {:<20}  {:<12}  {:<20}  {}",
            link.from_bc,
            link.trigger,
            link.to_bc,
            link.action,
            if link.is_compensate { "⚡ YES" } else { "no" }
        );
    }
    println!();
    println!("  Comparison:");
    println!("    CrossContextLink (bounded_context.rs) — structural coupling via shared FK/dim types");
    println!("    SagaLink         (saga.rs)             — behavioural coupling via event triggers");
    println!("  Both describe the same seam between BCs, from different perspectives.");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 1 — Happy Path
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 1 — HAPPY PATH  (Order o30 → complete fulfillment)");

    let mut orch = SagaOrchestrator::new();
    let happy_events = load_saga_csv(HAPPY_CSV).expect("happy_path.csv load failed");

    for (ts, event) in happy_events {
        drive(&mut orch, 0, &ts, event);
    }

    println!();
    print_saga_state(&orch, "o30");
    print_stores(&orch, "o30", "po-001");

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 2 — Compensation Path
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 2 — COMPENSATION PATH  (Order o31 → PO cancelled → rollback)");

    let comp_events = load_saga_csv(COMPENSATION_CSV).expect("compensation.csv load failed");

    for (ts, event) in comp_events {
        drive(&mut orch, 0, &ts, event);
    }

    println!();
    print_saga_state(&orch, "o31");
    print_stores(&orch, "o31", "po-002");

    // ══════════════════════════════════════════════════════════════════════════
    //  Summary
    // ══════════════════════════════════════════════════════════════════════════
    section("SUMMARY");
    println!("  Act 1  Happy Path (o30)");
    println!("         PaymentReceived → POCreated   [Customer → Procurement]");
    println!("         POApproved      → ItemShipped [Procurement → Customer]");
    println!("         OrderDelivered  → POFulfilled [Customer → Procurement]");
    println!("         Saga: Completed ✓");
    println!();
    println!("  Act 2  Compensation (o31)");
    println!("         PaymentReceived → POCreated       [Customer → Procurement]");
    println!("         POCancelled     → OrderCancelled  [Procurement → Customer]  ← COMPENSATE");
    println!("         Saga: Compensated ✗  (no manual undo needed — event drives rollback)");
    println!();
    println!("  Pattern notes:");
    println!("    · Each saga step is driven by a domain event — no polling, no direct calls");
    println!("    · Compensation is just another event; the aggregate applies it like any other");
    println!("    · The correlation ID (order_id) is the only shared concept across the BC boundary");
    println!("    · Adding a new step = add one SagaLink + one match arm — no other code changes");
}

// ─── Event driver ─────────────────────────────────────────────────────────────

/// Feed one event into the orchestrator and immediately drive any reactions.
/// `depth` controls visual indentation (0 = external input, 1+ = policy reaction).
fn drive(orch: &mut SagaOrchestrator, depth: usize, ts: &str, event: BcEvent) {
    let pad = "  ".repeat(depth);
    let bc  = event.bc();
    let et  = event.event_type();

    // Print the event
    if depth == 0 {
        println!("{pad}  [{bc:<12}]  {}", format_event(&event));
    } else {
        println!("{pad}  ↳ [{bc:<12}]  {}  (policy reaction)", format_event(&event));
    }

    // Annotate any matching policy
    for link in SAGA_LINKS {
        if link.from_bc == bc && link.trigger == et {
            let marker = if link.is_compensate { "⚡ COMPENSATE" } else { "▶ " };
            println!(
                "{pad}    policy {marker}  {}.{} → {}.{}",
                link.from_bc, link.trigger, link.to_bc, link.action
            );
        }
    }

    // Capture the correlation key BEFORE process() consumes the event
    let order_key: Option<String> = match &event {
        BcEvent::Order(e) => Some(e.order_id().to_string()),
        BcEvent::Procurement(e) => {
            let po_id = e.po_id().to_string();
            orch.po_to_order.get(&po_id).cloned()
        }
    };

    // Process (persists + advances saga + emits reactions)
    let reactions = orch.process(ts, event);

    // Print saga state after transition (top-level events only, to reduce noise)
    if depth == 0 {
        if let Some(oid) = &order_key {
            if let Some(saga) = orch.saga_for_order(oid) {
                println!("    saga   {}  [{}]", saga.step.label(), saga.saga_id);
            }
        }
    }

    // Recurse on reactions
    for (rts, reaction) in reactions {
        drive(orch, depth + 1, &rts, reaction);
    }
}

// ─── Display helpers ──────────────────────────────────────────────────────────

fn format_event(event: &BcEvent) -> String {
    match event {
        BcEvent::Order(e) => match e {
            OrderEvent::OrderPlaced { order_id, customer_id, amount } =>
                format!("OrderPlaced        order={order_id}  customer={customer_id}  ${amount:.2}"),
            OrderEvent::PaymentReceived { order_id, amount } =>
                format!("PaymentReceived    order={order_id}  ${amount:.2}"),
            OrderEvent::ItemShipped { order_id } =>
                format!("ItemShipped        order={order_id}"),
            OrderEvent::OrderDelivered { order_id } =>
                format!("OrderDelivered     order={order_id}"),
            OrderEvent::OrderCancelled { order_id, reason } =>
                format!("OrderCancelled     order={order_id}  reason=\"{reason}\""),
        },
        BcEvent::Procurement(e) => match e {
            ProcurementEvent::POCreated { po_id, order_id, vendor_id, amount } =>
                format!("POCreated          po={po_id}  order={order_id}  vendor={vendor_id}  ${amount:.2}"),
            ProcurementEvent::POApproved { po_id } =>
                format!("POApproved         po={po_id}"),
            ProcurementEvent::POFulfilled { po_id } =>
                format!("POFulfilled        po={po_id}"),
            ProcurementEvent::POCancelled { po_id, reason } =>
                format!("POCancelled        po={po_id}  reason=\"{reason}\""),
        },
    }
}

fn print_saga_state(orch: &SagaOrchestrator, order_id: &str) {
    if let Some(saga) = orch.saga_for_order(order_id) {
        println!("  Saga ({}):", saga.saga_id);
        println!("    final step : {}", saga.step.label());
        println!("    audit log  :");
        for (i, entry) in saga.log.iter().enumerate() {
            println!("      {}. {}", i + 1, entry);
        }
        println!();
    }
}

fn print_stores(orch: &SagaOrchestrator, order_id: &str, po_id: &str) {
    // Customer BC event log
    println!("  Customer BC — event log for {order_id}:");
    println!("  {:<4}  {:<4}  {:<20}  {:<20}", "pos", "seq", "event_type", "occurred_at");
    println!("  {}", "─".repeat(54));
    for se in orch.order_store.load(order_id) {
        println!(
            "  {:>4}  {:>4}  {:<20}  {:<20}",
            se.store_pos, se.sequence, se.event_type, se.occurred_at
        );
    }
    println!();

    // Procurement BC event log
    println!("  Procurement BC — event log for {po_id}:");
    println!("  {:<4}  {:<4}  {:<20}  {:<20}", "pos", "seq", "event_type", "occurred_at");
    println!("  {}", "─".repeat(54));
    for r in orch.po_store.load(po_id) {
        println!(
            "  {:>4}  {:>4}  {:<20}  {:<20}",
            r.store_pos, r.sequence, r.event_type, r.occurred_at
        );
    }
    println!();
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
