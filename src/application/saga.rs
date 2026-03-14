//! Policy / Saga — cross-Bounded-Context business process coordination.
//!
//! DDD concepts implemented here:
//!
//!   Policy (stateless)
//!     A reactive rule: "when event X occurs in BC-A, do Y in BC-B".
//!     Each Policy corresponds to one row in SAGA_LINKS.
//!     Policies have no memory — they fire once per trigger event.
//!
//!   Saga (stateful)
//!     A long-running process that spans multiple BCs.
//!     Tracks a correlation ID (order_id) across BC boundaries.
//!     Each step is driven by a domain event; if a step fails,
//!     previously completed steps are undone via compensation events.
//!
//!   CrossContextLink (structural → event-driven routing)
//!     `CrossContextLink` in bounded_context.rs describes *structural* coupling
//!     (which entity types reference each other across BCs).
//!     `SagaLink` here extends that concept to *event-driven* coupling:
//!     which event type in which BC triggers which action in which other BC.
//!
//! Saga: OrderFulfillmentSaga
//!   Steps:
//!     AwaitingPayment
//!       on Customer.PaymentReceived   → emit Procurement.POCreated
//!       → AwaitingPOApproval
//!     AwaitingPOApproval
//!       on Procurement.POApproved     → emit Customer.ItemShipped
//!       → AwaitingDelivery
//!       on Procurement.POCancelled    → emit Customer.OrderCancelled  ← COMPENSATE
//!       → Compensated
//!     AwaitingDelivery
//!       on Customer.OrderDelivered    → emit Procurement.POFulfilled
//!       → Completed

use std::collections::HashMap;

use crate::{
    domain::{
        order::OrderEvent,
        procurement::ProcurementEvent,
    },
    infrastructure::event_store::EventStore,
};

// ─── Multi-BC event envelope ──────────────────────────────────────────────────

/// Wraps a domain event with its BC of origin so the orchestrator can
/// route it without inspecting the concrete event type directly.
#[derive(Debug, Clone)]
pub enum BcEvent {
    Order(OrderEvent),
    Procurement(ProcurementEvent),
}

impl BcEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::Order(e)       => e.event_type(),
            Self::Procurement(e) => e.event_type(),
        }
    }

    pub fn bc(&self) -> &'static str {
        match self {
            Self::Order(_)       => "Customer",
            Self::Procurement(_) => "Procurement",
        }
    }
}

// ─── Saga routing table ───────────────────────────────────────────────────────

/// One hop in the cross-BC event routing table.
///
/// Analogous to `CrossContextLink` in bounded_context.rs, but for event-driven
/// (reactive) coupling rather than structural (FK) coupling.
///
/// Reading a `SagaLink`:
///   "When `trigger` fires in `from_bc`, the saga emits `action` in `to_bc`."
///   If `is_compensate` is true, the action undoes a previously completed step.
pub struct SagaLink {
    pub from_bc:       &'static str,
    pub trigger:       &'static str,   // event_type() that fires the policy
    pub to_bc:         &'static str,
    pub action:        &'static str,   // event_type() emitted in target BC
    pub is_compensate: bool,
}

/// Complete routing table for the OrderFulfillmentSaga.
/// This is the explicit cross-BC contract — every coupling is listed here.
pub const SAGA_LINKS: &[SagaLink] = &[
    SagaLink {
        from_bc: "Customer",    trigger: "PaymentReceived",
        to_bc:   "Procurement", action:  "POCreated",
        is_compensate: false,
    },
    SagaLink {
        from_bc: "Procurement", trigger: "POApproved",
        to_bc:   "Customer",    action:  "ItemShipped",
        is_compensate: false,
    },
    SagaLink {
        from_bc: "Procurement", trigger: "POCancelled",
        to_bc:   "Customer",    action:  "OrderCancelled",
        is_compensate: true,   // ← compensation: undo the order
    },
    SagaLink {
        from_bc: "Customer",    trigger: "OrderDelivered",
        to_bc:   "Procurement", action:  "POFulfilled",
        is_compensate: false,
    },
];

// ─── Saga step state machine ──────────────────────────────────────────────────

#[derive(Debug)]
pub enum SagaStep {
    /// Saga created; waiting for the customer to pay.
    AwaitingPayment,
    /// PO raised in Procurement BC; waiting for officer to approve or cancel.
    AwaitingPOApproval { po_id: String },
    /// PO approved and item is being shipped; waiting for delivery.
    AwaitingDelivery   { po_id: String },
    /// All steps completed successfully — terminal state.
    Completed,
    /// A compensation event rolled back the saga — terminal state.
    Compensated        { reason: String },
}

impl SagaStep {
    pub fn label(&self) -> String {
        match self {
            Self::AwaitingPayment              => "AwaitingPayment".to_string(),
            Self::AwaitingPOApproval { po_id } => format!("AwaitingPOApproval{{{po_id}}}"),
            Self::AwaitingDelivery   { po_id } => format!("AwaitingDelivery{{{po_id}}}"),
            Self::Completed                    => "Completed ✓".to_string(),
            Self::Compensated { reason }       => format!("Compensated ✗  ({reason})"),
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(self, Self::Completed | Self::Compensated { .. })
    }
}

// ─── Saga instance ────────────────────────────────────────────────────────────

pub struct SagaInstance {
    pub saga_id:  String,
    /// Correlation ID — the order_id ties all events across both BCs together.
    pub order_id: String,
    pub step:     SagaStep,
    /// Human-readable audit trail of step transitions.
    pub log:      Vec<String>,
}

// ─── Procurement Event Store (append-only, per-PO sequence) ──────────────────

pub struct PORecord {
    pub store_pos:   usize,
    pub sequence:    u32,
    pub po_id:       String,
    pub event_type:  &'static str,
    pub event:       ProcurementEvent,
    pub occurred_at: String,
}

#[derive(Default)]
pub struct ProcurementStore {
    pub records:  Vec<PORecord>,
    counters:     HashMap<String, u32>,
}

impl ProcurementStore {
    pub fn new() -> Self { Self::default() }

    pub fn append(&mut self, po_id: &str, occurred_at: &str, event: ProcurementEvent) -> u32 {
        let store_pos = self.records.len();
        let seq = {
            let c = self.counters.entry(po_id.to_string()).or_default();
            *c += 1;
            *c
        };
        self.records.push(PORecord {
            store_pos,
            sequence:   seq,
            po_id:      po_id.to_string(),
            event_type: event.event_type(),
            event,
            occurred_at: occurred_at.to_string(),
        });
        seq
    }

    pub fn all(&self) -> &[PORecord] { &self.records }

    pub fn load(&self, po_id: &str) -> Vec<&PORecord> {
        self.records.iter().filter(|r| r.po_id == po_id).collect()
    }
}

// ─── Saga Orchestrator ────────────────────────────────────────────────────────

/// Drives the OrderFulfillmentSaga across the Customer and Procurement BCs.
///
/// Responsibilities:
///   · Persist every event into the correct BC's event store
///   · Route events to the saga state machine via SAGA_LINKS
///   · Emit reaction events (policy outputs) back to the caller
///   · Maintain the po_id → order_id correlation table
pub struct SagaOrchestrator {
    /// Customer Bounded Context — Order aggregates.
    pub order_store: EventStore,
    /// Procurement Bounded Context — PurchaseOrder aggregates.
    pub po_store:    ProcurementStore,
    /// Active (and completed) saga instances, keyed by order_id.
    pub sagas:       HashMap<String, SagaInstance>,
    /// Reverse-lookup: po_id → order_id (needed when Procurement events arrive).
    pub po_to_order: HashMap<String, String>,
    next_po:         u32,
}

impl SagaOrchestrator {
    pub fn new() -> Self {
        Self {
            order_store: EventStore::new(),
            po_store:    ProcurementStore::new(),
            sagas:       HashMap::new(),
            po_to_order: HashMap::new(),
            next_po:     1,
        }
    }

    fn fresh_po_id(&mut self) -> String {
        let id = format!("po-{:03}", self.next_po);
        self.next_po += 1;
        id
    }

    /// Look up a saga by the order it tracks.
    pub fn saga_for_order(&self, order_id: &str) -> Option<&SagaInstance> {
        self.sagas.get(order_id)
    }

    /// Look up a saga via a po_id (reverse-lookup through correlation table).
    pub fn saga_for_po(&self, po_id: &str) -> Option<&SagaInstance> {
        self.po_to_order.get(po_id)
            .and_then(|oid| self.sagas.get(oid))
    }

    /// Ingest one event from any BC.
    ///
    /// The event is persisted and the saga state machine is advanced.
    /// Returns any reaction events that policies/saga steps emitted.
    /// The caller must feed those reactions back through `process()`.
    pub fn process(&mut self, occurred_at: &str, event: BcEvent) -> Vec<(String, BcEvent)> {
        let mut out = Vec::new();

        match event {
            // ── Customer BC ──────────────────────────────────────────────────
            BcEvent::Order(ev) => {
                let order_id = ev.order_id().to_string();

                // Ensure a saga instance exists for this order
                self.sagas.entry(order_id.clone()).or_insert_with(|| SagaInstance {
                    saga_id:  format!("saga-{}", order_id),
                    order_id: order_id.clone(),
                    step:     SagaStep::AwaitingPayment,
                    log:      Vec::new(),
                });

                // Persist
                self.order_store.append(&order_id, occurred_at, ev.clone());

                // Advance saga — move instance out to avoid borrow conflict
                let mut saga = self.sagas.remove(&order_id).unwrap();
                let step     = std::mem::replace(&mut saga.step, SagaStep::AwaitingPayment);

                saga.step = match step {
                    SagaStep::AwaitingPayment => match &ev {
                        OrderEvent::PaymentReceived { order_id: oid, amount } => {
                            let po_id = self.fresh_po_id();
                            saga.log.push(format!("PaymentReceived → POCreated({po_id})"));
                            out.push((occurred_at.to_string(), BcEvent::Procurement(
                                ProcurementEvent::POCreated {
                                    po_id:     po_id.clone(),
                                    order_id:  oid.clone(),
                                    vendor_id: "ve1".to_string(),
                                    amount:    *amount,
                                }
                            )));
                            SagaStep::AwaitingPOApproval { po_id }
                        }
                        _ => SagaStep::AwaitingPayment,
                    },

                    SagaStep::AwaitingDelivery { po_id } => match &ev {
                        OrderEvent::OrderDelivered { .. } => {
                            saga.log.push(format!("OrderDelivered → POFulfilled({po_id})"));
                            out.push((occurred_at.to_string(), BcEvent::Procurement(
                                ProcurementEvent::POFulfilled { po_id: po_id.clone() }
                            )));
                            SagaStep::Completed
                        }
                        _ => SagaStep::AwaitingDelivery { po_id },
                    },

                    other => other,
                };

                self.sagas.insert(order_id, saga);
            }

            // ── Procurement BC ───────────────────────────────────────────────
            BcEvent::Procurement(ev) => {
                let po_id = ev.po_id().to_string();

                // Record correlation key when the PO is first created
                if let ProcurementEvent::POCreated { order_id, .. } = &ev {
                    self.po_to_order.insert(po_id.clone(), order_id.clone());
                }

                // Persist
                self.po_store.append(&po_id, occurred_at, ev.clone());

                // Advance saga via reverse-lookup
                if let Some(order_id) = self.po_to_order.get(&po_id).cloned() {
                    if let Some(mut saga) = self.sagas.remove(&order_id) {
                        let step = std::mem::replace(&mut saga.step, SagaStep::AwaitingPayment);

                        saga.step = match step {
                            SagaStep::AwaitingPOApproval { po_id: pid } => match &ev {
                                ProcurementEvent::POApproved { .. } => {
                                    saga.log.push(format!(
                                        "POApproved({pid}) → ItemShipped({})", saga.order_id
                                    ));
                                    out.push((occurred_at.to_string(), BcEvent::Order(
                                        OrderEvent::ItemShipped { order_id: saga.order_id.clone() }
                                    )));
                                    SagaStep::AwaitingDelivery { po_id: pid }
                                }
                                ProcurementEvent::POCancelled { reason, .. } => {
                                    let reason = reason.clone();
                                    saga.log.push(format!(
                                        "POCancelled({pid}) → COMPENSATE OrderCancelled({})",
                                        saga.order_id
                                    ));
                                    out.push((occurred_at.to_string(), BcEvent::Order(
                                        OrderEvent::OrderCancelled {
                                            order_id: saga.order_id.clone(),
                                            reason:   format!("PO cancelled: {}", reason),
                                        }
                                    )));
                                    SagaStep::Compensated { reason }
                                }
                                _ => SagaStep::AwaitingPOApproval { po_id: pid },
                            },

                            other => other,
                        };

                        self.sagas.insert(order_id, saga);
                    }
                }
            }
        }

        out
    }
}
