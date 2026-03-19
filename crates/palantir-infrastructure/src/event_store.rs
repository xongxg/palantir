//! Append-only Event Store — Infrastructure implementation.
//!
//! DDD / Event Sourcing principle:
//!   The EventStore is the authoritative record.  No event is ever updated
//!   or deleted.  State is derived on demand by replaying events.
//!
//! Sequence numbers are PER-AGGREGATE, not global.
//!   o01 events: seq 1, 2, 3, 4
//!   o04 events: seq 1, 2, 3, 4   ← independent counter, no competition
//!
//! Global ordering for projections uses the Vec insertion index, not a
//! shared counter.  Projections checkpoint with a store-level index (usize).
//!
//! Capabilities:
//!   · append             — add a new event, assign per-aggregate sequence
//!   · load               — all events for an aggregate, in append order
//!   · load_until_time    — events up to a wall-clock timestamp (time-travel)
//!   · load_since_version — events after a snapshot version (snapshot + delta)
//!   · events_after_index — all events after store index N (projection catch-up)

use std::collections::HashMap;

use crate::domain::order::{OrderEvent, OrderState};

// ─── Stored event ─────────────────────────────────────────────────────────────

/// A domain event wrapped with infrastructure metadata.
pub struct StoredEvent {
    /// Position in the store's Vec — purely a Vec index, NOT a shared counter.
    /// Useful for "catch-up" slicing and for reading the global append order.
    /// Unique across ALL aggregates (Vec index), but carries no domain meaning.
    pub store_pos: usize,

    /// Per-aggregate monotonic sequence (1, 2, 3 … for THIS aggregate only).
    /// Each aggregate has its own independent counter — they never compete.
    /// Uniqueness: only within ONE aggregate.  Always use (aggregate_id, sequence)
    /// as a composite key, never sequence alone.
    pub sequence: u32,

    /// The aggregate this event belongs to (e.g. "o01").
    pub aggregate_id: String,
    /// Human-readable event type name.
    pub event_type: &'static str,
    /// The actual domain event.
    pub event: OrderEvent,
    /// ISO 8601 wall-clock time when the business fact occurred.
    pub occurred_at: String,
}

// ─── Event Store ──────────────────────────────────────────────────────────────

#[derive(Default)]
pub struct EventStore {
    events: Vec<StoredEvent>,
    /// Per-aggregate sequence counter — each aggregate owns its own counter.
    counters: HashMap<String, u32>,
}

impl EventStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a single domain event.
    /// Returns the per-aggregate sequence number assigned to this event.
    pub fn append(&mut self, aggregate_id: &str, occurred_at: &str, event: OrderEvent) -> u32 {
        let store_pos = self.events.len(); // Vec index before push — no shared counter

        let seq = {
            let counter = self.counters.entry(aggregate_id.to_string()).or_default();
            *counter += 1;
            *counter
        };

        self.events.push(StoredEvent {
            store_pos,
            sequence: seq,
            aggregate_id: aggregate_id.to_string(),
            event_type: event.event_type(),
            event,
            occurred_at: occurred_at.to_string(),
        });
        seq
    }

    /// All stored events in append order (Vec index = insertion order).
    pub fn all(&self) -> &[StoredEvent] {
        &self.events
    }

    /// All events for a given aggregate, in the order they were appended.
    pub fn load(&self, aggregate_id: &str) -> Vec<&StoredEvent> {
        self.events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id)
            .collect()
    }

    /// Events for `aggregate_id` with `occurred_at` ≤ `until_time`.
    /// Used for time-travel: "what was state at time T?"
    pub fn load_until_time(&self, aggregate_id: &str, until_time: &str) -> Vec<&StoredEvent> {
        self.events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id && e.occurred_at.as_str() <= until_time)
            .collect()
    }

    /// Events for `aggregate_id` with per-aggregate sequence > `after_seq`.
    /// Used to load the delta on top of a snapshot.
    pub fn load_since_version(&self, aggregate_id: &str, after_seq: u32) -> Vec<&StoredEvent> {
        self.events
            .iter()
            .filter(|e| e.aggregate_id == aggregate_id && e.sequence > after_seq)
            .collect()
    }

    /// All events appended after store index `after_index` (0-based).
    /// Used by projections to catch up incrementally without a global counter.
    pub fn events_after(&self, after_index: usize) -> &[StoredEvent] {
        let start = after_index.min(self.events.len());
        &self.events[start..]
    }

    /// All distinct aggregate IDs in first-seen order.
    pub fn aggregate_ids(&self) -> Vec<String> {
        let mut seen = std::collections::HashSet::new();
        self.events
            .iter()
            .filter_map(|e| {
                if seen.insert(e.aggregate_id.clone()) {
                    Some(e.aggregate_id.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Current per-aggregate sequence (= total events stored for that aggregate).
    pub fn version_of(&self, aggregate_id: &str) -> u32 {
        self.counters.get(aggregate_id).copied().unwrap_or(0)
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

// ─── Optimistic Concurrency ───────────────────────────────────────────────────

/// Returned when a concurrent writer appended before us.
///
/// Pattern:
///   Writer A reads version 3, prepares event, calls append_expected(3) → Ok
///   Writer B reads version 3, prepares event, calls append_expected(3) → Err
///   Writer B must reload, re-validate, and retry.
#[derive(Debug)]
pub struct ConcurrencyError {
    pub aggregate_id: String,
    /// The version the caller assumed was current.
    pub expected: u32,
    /// The version actually in the store (someone else wrote first).
    pub actual: u32,
}

impl std::fmt::Display for ConcurrencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] concurrency conflict: expected v{} but store is at v{} — reload and retry",
            self.aggregate_id, self.expected, self.actual
        )
    }
}

impl EventStore {
    /// Append with an optimistic concurrency guard.
    ///
    /// `expected_version` must equal the current per-aggregate sequence;
    /// otherwise returns `Err(ConcurrencyError)` and nothing is written.
    ///
    /// Use this whenever two writers may race on the same aggregate.
    pub fn append_expected(
        &mut self,
        aggregate_id: &str,
        occurred_at: &str,
        event: OrderEvent,
        expected_version: u32,
    ) -> Result<u32, ConcurrencyError> {
        let actual = self.version_of(aggregate_id);
        if actual != expected_version {
            return Err(ConcurrencyError {
                aggregate_id: aggregate_id.to_string(),
                expected: expected_version,
                actual,
            });
        }
        Ok(self.append(aggregate_id, occurred_at, event))
    }
}

// ─── Snapshot Store ───────────────────────────────────────────────────────────

/// A point-in-time snapshot of an aggregate's state.
/// Allows skipping replaying all events from the beginning — load snapshot
/// then replay only events with per-aggregate sequence > snapshot.version.
pub struct Snapshot {
    pub aggregate_id: String,
    /// The per-aggregate sequence of the last event included in this snapshot.
    pub version: u32,
    /// The reconstructed state at that version.
    pub state: OrderState,
    /// When the snapshot was taken (wall-clock).
    pub taken_at: String,
}

#[derive(Default)]
pub struct SnapshotStore {
    snapshots: HashMap<String, Snapshot>,
}

impl SnapshotStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn save(&mut self, snap: Snapshot) {
        self.snapshots.insert(snap.aggregate_id.clone(), snap);
    }

    pub fn load(&self, aggregate_id: &str) -> Option<&Snapshot> {
        self.snapshots.get(aggregate_id)
    }

    /// Rebuild an Order — uses snapshot if available, then replays delta from EventStore.
    /// Delta = events whose per-aggregate sequence > snapshot.version.
    pub fn rebuild(&self, aggregate_id: &str, store: &EventStore) -> (OrderState, Vec<String>) {
        let (mut state, after_version) = match self.load(aggregate_id) {
            Some(snap) => (snap.state.clone(), snap.version),
            None => (OrderState::draft(aggregate_id), 0),
        };

        let mut violations = Vec::new();
        for se in store.load_since_version(aggregate_id, after_version) {
            if let Err(msg) = state.apply(&se.event) {
                violations.push(msg);
            }
        }
        (state, violations)
    }
}
