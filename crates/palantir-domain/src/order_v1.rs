//! Event Upcasting — old schema versions → current OrderEvent.
//!
//! As a domain evolves, event schemas change.  Old events stored on disk must
//! still be readable.  Upcasting solves this without altering stored data:
//!
//!   Stored bytes  →  RawEvent (versioned envelope)  →  upcast()  →  OrderEvent (current)
//!
//! Versioning history:
//!   V1  OrderPlaced had no `customer_id` field  (pre-customer-tracking era)
//!   V2  OrderPlaced includes `customer_id`       (current)
//!
//! Upcast chain: V1 → V2 (→ … future versions)
//! Each step is a pure function with no side effects.

use super::order::OrderEvent;

// ─── V1 event schema ─────────────────────────────────────────────────────────

/// Events as they existed before customer tracking was added.
/// `OrderPlaced` had no `customer_id` — it was assumed implicitly from context.
#[derive(Debug, Clone)]
pub enum OrderEventV1 {
    /// V1: no customer_id.
    OrderPlaced {
        order_id: String,
        amount: f64,
    },
    PaymentReceived {
        order_id: String,
        amount: f64,
    },
    ItemShipped {
        order_id: String,
    },
    OrderDelivered {
        order_id: String,
    },
    OrderCancelled {
        order_id: String,
        reason: String,
    },
}

impl OrderEventV1 {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::OrderPlaced { .. } => "OrderPlaced/v1",
            Self::PaymentReceived { .. } => "PaymentReceived/v1",
            Self::ItemShipped { .. } => "ItemShipped/v1",
            Self::OrderDelivered { .. } => "OrderDelivered/v1",
            Self::OrderCancelled { .. } => "OrderCancelled/v1",
        }
    }
}

// ─── Versioned envelope ───────────────────────────────────────────────────────

/// A raw event as it comes off the wire / out of storage.
/// The outer enum carries the schema version so the upcast chain can decide
/// which transforms to apply.
pub enum RawEvent {
    V1(OrderEventV1),
    V2(OrderEvent),
}

impl RawEvent {
    pub fn schema_version(&self) -> u32 {
        match self {
            Self::V1(_) => 1,
            Self::V2(_) => 2,
        }
    }
}

// ─── Upcast chain ─────────────────────────────────────────────────────────────

/// Registry of upcast functions.  Each step upgrades one version to the next.
/// Call `UpcastChain::to_current()` to run all necessary transforms.
pub struct UpcastChain;

impl UpcastChain {
    /// Upcast V1 → V2 (current).
    ///
    /// The only breaking change: `OrderPlaced` gains a `customer_id`.
    /// Since V1 events didn't record it, we substitute a sentinel value that
    /// signals "migrated from legacy data".  Downstream code can detect this
    /// and trigger a backfill workflow if needed.
    fn v1_to_v2(ev: OrderEventV1) -> OrderEvent {
        match ev {
            OrderEventV1::OrderPlaced { order_id, amount } => OrderEvent::OrderPlaced {
                order_id,
                customer_id: "LEGACY_UNKNOWN".to_string(),
                amount,
            },
            // All other V1 events are structurally identical to V2 — passthrough.
            OrderEventV1::PaymentReceived { order_id, amount } => {
                OrderEvent::PaymentReceived { order_id, amount }
            }
            OrderEventV1::ItemShipped { order_id } => OrderEvent::ItemShipped { order_id },
            OrderEventV1::OrderDelivered { order_id } => OrderEvent::OrderDelivered { order_id },
            OrderEventV1::OrderCancelled { order_id, reason } => {
                OrderEvent::OrderCancelled { order_id, reason }
            }
        }
    }

    /// Run the full upcast chain: any version → current `OrderEvent`.
    /// Add new `vN_to_vN1()` steps here as the schema evolves.
    pub fn to_current(raw: RawEvent) -> OrderEvent {
        match raw {
            RawEvent::V1(v1) => Self::v1_to_v2(v1),
            RawEvent::V2(v2) => v2,
        }
    }
}

// ─── CSV loader ───────────────────────────────────────────────────────────────

/// Load versioned (legacy + current) events from a CSV file.
///
/// CSV columns: occurred_at, schema_version, event_type, order_id, amount
///
/// schema_version = 1 → RawEvent::V1(OrderEventV1)
/// schema_version = 2 → RawEvent::V2(OrderEvent)
pub fn load_legacy_csv(path: &str) -> Result<Vec<(String, RawEvent)>, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Cannot read {}: {}", path, e))?;

    let mut events = Vec::new();
    for line in content.lines().skip(1) {
        let cols: Vec<&str> = line.splitn(5, ',').collect();
        if cols.len() < 5 {
            continue;
        }

        let occurred_at = cols[0].trim().to_string();
        let schema_ver: u32 = cols[1].trim().parse().unwrap_or(2);
        let event_type = cols[2].trim();
        let order_id = cols[3].trim().to_string();
        let amount: f64 = cols[4].trim().parse().unwrap_or(0.0);

        let raw = match schema_ver {
            1 => match event_type {
                "OrderPlaced" => RawEvent::V1(OrderEventV1::OrderPlaced { order_id, amount }),
                "PaymentReceived" => {
                    RawEvent::V1(OrderEventV1::PaymentReceived { order_id, amount })
                }
                "ItemShipped" => RawEvent::V1(OrderEventV1::ItemShipped { order_id }),
                "OrderDelivered" => RawEvent::V1(OrderEventV1::OrderDelivered { order_id }),
                _ => continue,
            },
            _ => match event_type {
                "OrderPlaced" => RawEvent::V2(OrderEvent::OrderPlaced {
                    order_id,
                    customer_id: String::new(),
                    amount,
                }),
                "PaymentReceived" => RawEvent::V2(OrderEvent::PaymentReceived { order_id, amount }),
                "ItemShipped" => RawEvent::V2(OrderEvent::ItemShipped { order_id }),
                "OrderDelivered" => RawEvent::V2(OrderEvent::OrderDelivered { order_id }),
                _ => continue,
            },
        };
        events.push((occurred_at, raw));
    }
    Ok(events)
}
