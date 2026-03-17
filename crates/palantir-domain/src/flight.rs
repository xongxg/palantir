//! Flight aggregate — Airline Operations Bounded Context (event-sourced).
//!
//! Palantir Ontology mapping (per official docs):
//!   Object Type  Flight     ← this aggregate
//!   Object Type  Aircraft   ← linked via AircraftAssigned event
//!   Object Type  Airport    ← origin/destination
//!   Link Type    Aircraft ──1:many──▶ Flight
//!   Link Type    Flight   ──1:many──▶ Delay    (FlightAlert in Palantir)
//!
//! Lifecycle:
//!   Scheduled → (GateAssigned) → (AircraftAssigned) → Boarding
//!            → Departed → Landed
//!            → Delayed  (can happen before Departed)
//!            → Cancelled (terminal — triggers cross-BC saga)

use std::collections::HashMap;

// ─── Status ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FlightStatus {
    Scheduled,
    Boarding,
    Departed,
    Landed,
    Delayed,
    Cancelled,
}

impl FlightStatus {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Scheduled => "Scheduled",
            Self::Boarding => "Boarding",
            Self::Departed => "Departed",
            Self::Landed => "Landed",
            Self::Delayed => "Delayed",
            Self::Cancelled => "Cancelled",
        }
    }
}

// ─── Domain Events ────────────────────────────────────────────────────────────

/// Every business fact about a Flight captured as an immutable event.
/// This is the event stream Palantir would back with a dataset.
#[derive(Debug, Clone)]
pub enum FlightEvent {
    FlightScheduled {
        flight_id: String,
        flight_code: String,
        aircraft_id: String,
        origin: String, // IATA code
        destination: String,
        departure: String, // ISO 8601
        airline: String,
    },
    GateAssigned {
        flight_id: String,
        gate: String,
    },
    BoardingStarted {
        flight_id: String,
    },
    FlightDelayed {
        flight_id: String,
        delay_minutes: u32,
        reason: String,
    },
    FlightDeparted {
        flight_id: String,
        actual_departure: String,
    },
    FlightLanded {
        flight_id: String,
        actual_arrival: String,
    },
    FlightCancelled {
        flight_id: String,
        reason: String,
    },
}

impl FlightEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::FlightScheduled { .. } => "FlightScheduled",
            Self::GateAssigned { .. } => "GateAssigned",
            Self::BoardingStarted { .. } => "BoardingStarted",
            Self::FlightDelayed { .. } => "FlightDelayed",
            Self::FlightDeparted { .. } => "FlightDeparted",
            Self::FlightLanded { .. } => "FlightLanded",
            Self::FlightCancelled { .. } => "FlightCancelled",
        }
    }

    pub fn flight_id(&self) -> &str {
        match self {
            Self::FlightScheduled { flight_id, .. } => flight_id,
            Self::GateAssigned { flight_id, .. } => flight_id,
            Self::BoardingStarted { flight_id } => flight_id,
            Self::FlightDelayed { flight_id, .. } => flight_id,
            Self::FlightDeparted { flight_id, .. } => flight_id,
            Self::FlightLanded { flight_id, .. } => flight_id,
            Self::FlightCancelled { flight_id, .. } => flight_id,
        }
    }
}

// ─── Aggregate state ──────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FlightState {
    pub flight_id: String,
    pub flight_code: String,
    pub aircraft_id: String,
    pub origin: String,
    pub destination: String,
    pub gate: Option<String>,
    pub airline: String,
    pub departure: String,
    pub actual_departure: Option<String>,
    pub actual_arrival: Option<String>,
    pub total_delay_mins: u32,
    pub status: FlightStatus,
    pub version: u32,
}

impl FlightState {
    pub fn new(flight_id: &str) -> Self {
        Self {
            flight_id: flight_id.to_string(),
            flight_code: String::new(),
            aircraft_id: String::new(),
            origin: String::new(),
            destination: String::new(),
            gate: None,
            airline: String::new(),
            departure: String::new(),
            actual_departure: None,
            actual_arrival: None,
            total_delay_mins: 0,
            status: FlightStatus::Scheduled,
            version: 0,
        }
    }

    pub fn apply(&mut self, event: &FlightEvent) -> Result<(), String> {
        match event {
            FlightEvent::FlightScheduled {
                flight_code,
                aircraft_id,
                origin,
                destination,
                departure,
                airline,
                ..
            } => {
                self.flight_code = flight_code.clone();
                self.aircraft_id = aircraft_id.clone();
                self.origin = origin.clone();
                self.destination = destination.clone();
                self.departure = departure.clone();
                self.airline = airline.clone();
                self.status = FlightStatus::Scheduled;
            }
            FlightEvent::GateAssigned { gate, .. } => {
                self.gate = Some(gate.clone());
            }
            FlightEvent::BoardingStarted { .. } => {
                if self.status == FlightStatus::Cancelled {
                    return Err(format!(
                        "[{}] cannot board — flight is cancelled",
                        self.flight_id
                    ));
                }
                self.status = FlightStatus::Boarding;
            }
            FlightEvent::FlightDelayed {
                delay_minutes,
                reason,
                ..
            } => {
                if self.status == FlightStatus::Cancelled || self.status == FlightStatus::Landed {
                    return Err(format!(
                        "[{}] cannot delay — already {}",
                        self.flight_id,
                        self.status.label()
                    ));
                }
                self.total_delay_mins += delay_minutes;
                let _ = reason;
                self.status = FlightStatus::Delayed;
            }
            FlightEvent::FlightDeparted {
                actual_departure, ..
            } => {
                self.actual_departure = Some(actual_departure.clone());
                self.status = FlightStatus::Departed;
            }
            FlightEvent::FlightLanded { actual_arrival, .. } => {
                self.actual_arrival = Some(actual_arrival.clone());
                self.status = FlightStatus::Landed;
            }
            FlightEvent::FlightCancelled { .. } => {
                if self.status == FlightStatus::Landed || self.status == FlightStatus::Departed {
                    return Err(format!(
                        "[{}] cannot cancel — already {}",
                        self.flight_id,
                        self.status.label()
                    ));
                }
                self.status = FlightStatus::Cancelled;
            }
        }
        self.version += 1;
        Ok(())
    }
}

// ─── Event Store (per-flight sequence, no global counter) ─────────────────────

pub struct FlightRecord {
    pub store_pos: usize,
    pub sequence: u32,
    pub flight_id: String,
    pub event_type: &'static str,
    pub event: FlightEvent,
    pub occurred_at: String,
}

#[derive(Default)]
pub struct FlightEventStore {
    pub records: Vec<FlightRecord>,
    counters: HashMap<String, u32>,
}

impl FlightEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn append(&mut self, occurred_at: &str, event: FlightEvent) -> u32 {
        let flight_id = event.flight_id().to_string();
        let store_pos = self.records.len();
        let seq = {
            let c = self.counters.entry(flight_id.clone()).or_default();
            *c += 1;
            *c
        };
        self.records.push(FlightRecord {
            store_pos,
            sequence: seq,
            flight_id: flight_id.clone(),
            event_type: event.event_type(),
            event,
            occurred_at: occurred_at.to_string(),
        });
        seq
    }

    pub fn load(&self, flight_id: &str) -> Vec<&FlightRecord> {
        self.records
            .iter()
            .filter(|r| r.flight_id == flight_id)
            .collect()
    }

    pub fn all(&self) -> &[FlightRecord] {
        &self.records
    }

    pub fn rebuild(&self, flight_id: &str) -> FlightState {
        let mut state = FlightState::new(flight_id);
        for rec in self.load(flight_id) {
            let _ = state.apply(&rec.event);
        }
        state
    }
}

// ─── Departure Board Projection ───────────────────────────────────────────────

/// Read model: current status of all flights — equivalent to an airport
/// departure board.  In Palantir this would be a Workshop widget backed by
/// an object set query on the Flight object type.
pub struct BoardEntry {
    pub flight_code: String,
    pub origin: String,
    pub destination: String,
    pub gate: String,
    pub scheduled_dep: String,
    pub total_delay_mins: u32,
    pub status: String,
    pub airline: String,
}

pub struct DepartureBoard {
    pub entries: Vec<BoardEntry>,
}

impl DepartureBoard {
    pub fn build(store: &FlightEventStore) -> Self {
        // Collect distinct flight IDs in first-seen order
        let mut seen = std::collections::HashSet::new();
        let flight_ids: Vec<String> = store
            .all()
            .iter()
            .filter_map(|r| {
                if seen.insert(r.flight_id.clone()) {
                    Some(r.flight_id.clone())
                } else {
                    None
                }
            })
            .collect();

        let mut entries = Vec::new();
        for fid in flight_ids {
            let state = store.rebuild(&fid);
            entries.push(BoardEntry {
                flight_code: state.flight_code,
                origin: state.origin,
                destination: state.destination,
                gate: state.gate.unwrap_or_else(|| "-".to_string()),
                scheduled_dep: state.departure,
                total_delay_mins: state.total_delay_mins,
                status: state.status.label().to_string(),
                airline: state.airline,
            });
        }
        Self { entries }
    }
}
