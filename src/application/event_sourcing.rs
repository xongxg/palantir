//! Event Sourcing application services — projections, time-travel, CSV loader.
//!
//! Three projection types (all built by replaying the event stream):
//!
//!   OrderStatusProjection     — current status of every order
//!   RevenueByDayProjection    — daily revenue from PaymentReceived events
//!   CustomerOrdersProjection  — orders per customer
//!
//! Time-travel query:
//!   Rebuilds aggregate state as it was at an arbitrary past timestamp.
//!
//! Projections are:
//!   · Disposable — can be rebuilt from scratch at any time
//!   · Eventual   — may lag the event stream (catch-up via events_since)
//!   · Read-only  — never write to the EventStore

use std::collections::HashMap;

use crate::{
    domain::order::{OrderEvent, OrderState, OrderStatus},
    infrastructure::event_store::{EventStore, Snapshot, SnapshotStore},
};

// ─── CSV loader ───────────────────────────────────────────────────────────────

/// Load order events from a CSV file into the EventStore.
/// CSV columns: id,order_id,customer_id,event_type,amount,occurred_at
/// Events are appended in the order they appear in the file (may be scrambled).
pub fn load_csv_into_store(path: &str, store: &mut EventStore) -> Result<usize, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path, e))?;

    let mut count = 0usize;
    for line in content.lines().skip(1) {          // skip header
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 6 { continue; }

        let order_id    = cols[1].trim();
        let customer_id = cols[2].trim();
        let event_type  = cols[3].trim();
        let amount: f64 = cols[4].trim().parse().unwrap_or(0.0);
        let occurred_at = cols[5].trim();

        if let Some(event) = OrderEvent::from_csv_row(order_id, customer_id, event_type, amount) {
            store.append(order_id, occurred_at, event);
            count += 1;
        }
    }
    Ok(count)
}

/// Same as `load_csv_into_store` but sorts rows by occurred_at before appending.
/// This produces a chronologically correct event stream.
pub fn load_csv_sorted(path: &str, store: &mut EventStore) -> Result<usize, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Cannot read {}: {}", path, e))?;

    // Parse all rows first
    let mut rows: Vec<(String, String, String, f64, String)> = Vec::new(); // (order_id, customer_id, event_type, amount, occurred_at)
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if cols.len() < 6 { continue; }
        rows.push((
            cols[1].trim().to_string(),
            cols[2].trim().to_string(),
            cols[3].trim().to_string(),
            cols[4].trim().parse().unwrap_or(0.0),
            cols[5].trim().to_string(),
        ));
    }

    // Sort globally by occurred_at (ISO 8601 lex = chronological)
    rows.sort_by(|a, b| a.4.cmp(&b.4));

    let mut count = 0usize;
    for (order_id, customer_id, event_type, amount, occurred_at) in rows {
        if let Some(event) = OrderEvent::from_csv_row(&order_id, &customer_id, &event_type, amount) {
            store.append(&order_id, &occurred_at, event);
            count += 1;
        }
    }
    Ok(count)
}

// ─── Projection: Order Status ─────────────────────────────────────────────────

/// Read model: current status of every order (derived by replaying all events).
pub struct OrderStatusProjection {
    pub orders: HashMap<String, OrderState>,
    /// Violations encountered during replay.
    pub violations: Vec<String>,
    /// Store-level Vec index of the last event processed (used for catch-up).
    /// Per-aggregate sequence numbers are independent, so we track position
    /// in the store's insertion Vec rather than any shared counter.
    pub checkpoint: usize,
}

impl OrderStatusProjection {
    /// Build from scratch by replaying all events in the store.
    pub fn build(store: &EventStore) -> Self {
        let mut orders: HashMap<String, OrderState> = HashMap::new();
        let mut violations = Vec::new();

        for se in store.all() {
            let state = orders
                .entry(se.aggregate_id.clone())
                .or_insert_with(|| OrderState::draft(&se.aggregate_id));

            if let Err(msg) = state.apply(&se.event) {
                violations.push(msg);
            }
        }
        let checkpoint = store.len();   // next catch-up starts here

        Self { orders, violations, checkpoint }
    }

    /// Catch-up: apply events appended after our store-index checkpoint.
    pub fn catch_up(&mut self, store: &EventStore) {
        for se in store.events_after(self.checkpoint) {
            let state = self.orders
                .entry(se.aggregate_id.clone())
                .or_insert_with(|| OrderState::draft(&se.aggregate_id));

            let _ = state.apply(&se.event);
        }
        self.checkpoint = store.len();
    }

    pub fn get(&self, order_id: &str) -> Option<&OrderState> {
        self.orders.get(order_id)
    }

    pub fn by_status(&self, status: &OrderStatus) -> Vec<&OrderState> {
        self.orders.values().filter(|s| &s.status == status).collect()
    }
}

// ─── Projection: Revenue by Day ───────────────────────────────────────────────

/// Read model: total revenue (from PaymentReceived events) aggregated by date.
pub struct RevenueByDayProjection {
    /// date string ("2024-01-03") → total amount
    pub daily: HashMap<String, f64>,
    pub total: f64,
}

impl RevenueByDayProjection {
    pub fn build(store: &EventStore) -> Self {
        let mut daily: HashMap<String, f64> = HashMap::new();
        let mut total = 0.0f64;

        for se in store.all() {
            if let OrderEvent::PaymentReceived { amount, .. } = &se.event {
                // Extract date prefix: "2024-01-03T09:20:00" → "2024-01-03"
                let day = &se.occurred_at[..se.occurred_at.find('T').unwrap_or(10).min(10)];
                *daily.entry(day.to_string()).or_default() += amount;
                total += amount;
            }
        }
        Self { daily, total }
    }

    pub fn sorted_days(&self) -> Vec<(&str, f64)> {
        let mut v: Vec<_> = self.daily.iter().map(|(k, v)| (k.as_str(), *v)).collect();
        v.sort_by_key(|(d, _)| *d);
        v
    }
}

// ─── Projection: Customer Orders ──────────────────────────────────────────────

/// Read model: all order IDs placed by each customer, with total spend.
pub struct CustomerOrdersProjection {
    /// customer_id → (order_ids, total_paid)
    pub by_customer: HashMap<String, (Vec<String>, f64)>,
}

impl CustomerOrdersProjection {
    pub fn build(store: &EventStore) -> Self {
        let mut by_customer: HashMap<String, (Vec<String>, f64)> = HashMap::new();

        for se in store.all() {
            match &se.event {
                OrderEvent::OrderPlaced { customer_id, order_id, .. } => {
                    let entry = by_customer.entry(customer_id.clone()).or_default();
                    if !entry.0.contains(order_id) {
                        entry.0.push(order_id.clone());
                    }
                }
                OrderEvent::PaymentReceived { order_id, amount } => {
                    // find which customer owns this order
                    if let Some(customer_id) = store.all().iter()
                        .find_map(|e| {
                            if let OrderEvent::OrderPlaced { order_id: oid, customer_id, .. } = &e.event {
                                if oid == order_id { Some(customer_id.clone()) } else { None }
                            } else { None }
                        })
                    {
                        by_customer.entry(customer_id).or_default().1 += amount;
                    }
                }
                _ => {}
            }
        }
        Self { by_customer }
    }
}

// ─── Time-travel query ────────────────────────────────────────────────────────

/// Reconstruct an order's state as it was at `until_time`.
/// Returns (state, events_applied_count).
pub fn time_travel(
    store:      &EventStore,
    order_id:   &str,
    until_time: &str,
) -> (OrderState, usize) {
    let history = store.load_until_time(order_id, until_time);
    let count   = history.len();
    let mut state = OrderState::draft(order_id);
    for se in history {
        let _ = state.apply(&se.event);
    }
    (state, count)
}

// ─── Snapshot helpers ─────────────────────────────────────────────────────────

// ─── Read Model → ontology_graph.json ────────────────────────────────────────

/// Export the live EventStore state as `ontology_graph.json` for the D3 visualizer.
///
/// Graph layout:
///   Nodes  — Customer (Aggregate Root), Order (Entity), status values (Value Object)
///   Edges  — Customer HAS Order, Order BELONGS_TO status:X
///   BC     — one "OrderFulfillment" context grouping Customer + Order
///   Shared Kernel — ["status"]
///
/// Run `cargo run --bin serve` after calling this to see the live order graph.
pub fn export_order_graph(store: &EventStore, path: &str) -> Result<(), String> {
    // Build current state for all orders
    let proj = OrderStatusProjection::build(store);

    // Collect unique customers
    let mut customer_orders: HashMap<String, Vec<String>> = HashMap::new();
    for se in store.all() {
        if let OrderEvent::OrderPlaced { customer_id, order_id, .. } = &se.event {
            customer_orders.entry(customer_id.clone()).or_default().push(order_id.clone());
        }
    }

    let mut entities  = Vec::new();
    let mut rels      = Vec::new();

    // Customer nodes
    for (cust_id, order_ids) in &customer_orders {
        entities.push(format!(
            r#"    {{
      "id": {id},
      "type": "Customer",
      "ddd_concept": "Aggregate Root",
      "label": {id},
      "properties": {{"orders": {n}}}
    }}"#,
            id = jstr(cust_id),
            n  = order_ids.len()
        ));
    }

    // Order nodes + HAS edges from customer + BELONGS_TO status
    for (order_id, state) in &proj.orders {
        let cust = &state.customer_id;
        entities.push(format!(
            r#"    {{
      "id": {oid},
      "type": "Order",
      "ddd_concept": "Entity",
      "label": {oid},
      "properties": {{"status": {st}, "amount": {amt:.0}, "version": {ver}}}
    }}"#,
            oid = jstr(order_id),
            st  = jstr(state.status.label()),
            amt = state.amount,
            ver = state.version,
        ));

        // Customer HAS Order
        rels.push(format!(
            r#"    {{"from": {cu}, "from_type": "Customer", "to": {oid}, "to_type": "Order", "kind": "HAS", "action_category": "Integration"}}"#,
            cu  = jstr(cust),
            oid = jstr(order_id),
        ));

        // Order BELONGS_TO status
        let status_id = format!("status:{}", state.status.label());
        rels.push(format!(
            r#"    {{"from": {oid}, "from_type": "Order", "to": {sid}, "to_type": "status", "kind": "BELONGS_TO", "action_category": "Logic"}}"#,
            oid = jstr(order_id),
            sid = jstr(&status_id),
        ));
    }

    // Status value-object nodes (distinct statuses in use)
    let mut statuses: std::collections::HashSet<String> = std::collections::HashSet::new();
    for state in proj.orders.values() {
        statuses.insert(state.status.label().to_string());
    }
    for s in &statuses {
        let sid = format!("status:{}", s);
        entities.push(format!(
            r#"    {{"id": {sid}, "type": "status", "ddd_concept": "Value Object", "label": {s}, "properties": {{}}}}"#,
            sid = jstr(&sid),
            s   = jstr(s),
        ));
    }

    // Single bounded context: OrderFulfillment
    let n_entities  = proj.orders.len() + customer_orders.len();
    let n_rels      = rels.len();
    let json = format!(
        r#"{{
  "entities": [
{entities}
  ],
  "relationships": [
{rels}
  ],
  "bounded_contexts": [
    {{
      "name": "OrderFulfillment",
      "cohesion": 1.0,
      "internal_links": {ilinks},
      "entity_types": ["Customer", "Order"]
    }}
  ],
  "shared_kernel": ["status"],
  "summary": {{
    "total_entities": {ne},
    "total_relationships": {nr},
    "bounded_contexts": 1
  }}
}}"#,
        entities = entities.join(",\n"),
        rels     = rels.join(",\n"),
        ilinks   = customer_orders.values().map(|v| v.len()).sum::<usize>(),
        ne       = n_entities,
        nr       = n_rels,
    );

    std::fs::write(path, json).map_err(|e| format!("Cannot write {}: {}", path, e))
}

fn jstr(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

/// Take a snapshot of every aggregate's current state.
pub fn snapshot_all(store: &EventStore, snap_store: &mut SnapshotStore, taken_at: &str) {
    for id in store.aggregate_ids() {
        let mut state = OrderState::draft(&id);
        for se in store.load(&id) {
            let _ = state.apply(&se.event);
        }
        let version = state.version;
        snap_store.save(Snapshot {
            aggregate_id: id,
            version,
            state,
            taken_at:     taken_at.to_string(),
        });
    }
}
