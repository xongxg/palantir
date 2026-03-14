//! Procurement Bounded Context — PurchaseOrder aggregate (event-sourced).
//!
//! Lifecycle:
//!   NonExistent → (POCreated) → Pending → (POApproved) → Approved
//!                                        → (POCancelled) → Cancelled
//!                               Approved → (POFulfilled) → Fulfilled
//!                               Approved → (POCancelled) → Cancelled
//!
//! This aggregate lives in a separate BC from Order.  The two BCs are
//! decoupled at runtime — they only communicate through published domain
//! events routed by the Saga orchestrator.

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum POStatus {
    /// Aggregate exists in name only — no POCreated event yet.
    NonExistent,
    /// PO raised, awaiting approval.
    Pending,
    /// Approved by procurement officer — ready for fulfillment.
    Approved,
    /// Goods received / service delivered.
    Fulfilled,
    /// Cancelled before fulfillment (triggers saga compensation).
    Cancelled,
}

impl POStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::NonExistent => "NonExistent",
            Self::Pending     => "Pending",
            Self::Approved    => "Approved",
            Self::Fulfilled   => "Fulfilled",
            Self::Cancelled   => "Cancelled",
        }
    }
}

// ─── Domain Events ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub enum ProcurementEvent {
    /// A new PO was raised, correlated to an Order via `order_id`.
    POCreated   { po_id: String, order_id: String, vendor_id: String, amount: f64 },
    /// Procurement officer approved the PO.
    POApproved  { po_id: String },
    /// Goods received — PO closed successfully.
    POFulfilled { po_id: String },
    /// PO could not be fulfilled (out of stock, vendor issue, etc.).
    POCancelled { po_id: String, reason: String },
}

impl ProcurementEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::POCreated   { .. } => "POCreated",
            Self::POApproved  { .. } => "POApproved",
            Self::POFulfilled { .. } => "POFulfilled",
            Self::POCancelled { .. } => "POCancelled",
        }
    }

    pub fn po_id(&self) -> &str {
        match self {
            Self::POCreated   { po_id, .. } => po_id,
            Self::POApproved  { po_id }     => po_id,
            Self::POFulfilled { po_id }     => po_id,
            Self::POCancelled { po_id, .. } => po_id,
        }
    }
}

// ─── Aggregate state ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct POState {
    pub id:        String,
    /// The Customer BC order that triggered this PO (correlation ID).
    pub order_id:  String,
    pub vendor_id: String,
    pub amount:    f64,
    pub status:    POStatus,
    pub version:   u32,
}

impl POState {
    pub fn new(id: &str) -> Self {
        Self {
            id:        id.to_string(),
            order_id:  String::new(),
            vendor_id: String::new(),
            amount:    0.0,
            status:    POStatus::NonExistent,
            version:   0,
        }
    }

    pub fn apply(&mut self, event: &ProcurementEvent) -> Result<(), String> {
        match (&self.status, event) {
            (POStatus::NonExistent, ProcurementEvent::POCreated { order_id, vendor_id, amount, .. }) => {
                self.order_id  = order_id.clone();
                self.vendor_id = vendor_id.clone();
                self.amount    = *amount;
                self.status    = POStatus::Pending;
            }
            (POStatus::Pending, ProcurementEvent::POApproved { .. }) => {
                self.status = POStatus::Approved;
            }
            (POStatus::Approved, ProcurementEvent::POFulfilled { .. }) => {
                self.status = POStatus::Fulfilled;
            }
            (POStatus::Pending | POStatus::Approved, ProcurementEvent::POCancelled { .. }) => {
                self.status = POStatus::Cancelled;
            }
            (s, e) => {
                return Err(format!(
                    "[{}]  {} → {}  ILLEGAL  (at version {})",
                    self.id, s.label(), e.event_type(), self.version
                ));
            }
        }
        self.version += 1;
        Ok(())
    }
}
