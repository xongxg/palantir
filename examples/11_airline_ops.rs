//! Example 11 — Palantir Ontology: Airline Operations
//!
//! Run:  cargo run --example 11_airline_ops
//!
//! Validates the framework against a real-world domain from Palantir's own
//! documentation (Airbus/Skywise, official ontology examples).
//!
//! Palantir Object Types modelled:
//!   Aircraft     — tail number, model, capacity, airline
//!   Airport      — IATA code, city, lat/lon, hub type
//!   Flight       — aggregate root, links Aircraft + Airport
//!   Delay        — FlightAlert in Palantir; flight_id FK
//!   Passenger    — frequent flyer tier
//!   Booking      — junction: Passenger ←many:many→ Flight
//!   Crew         — assigned to Flight via flight_id FK
//!
//! Palantir Link Types:
//!   Aircraft ──1:many──▶ Flight      (aircraft_id FK in flights.csv)
//!   Airport  ──1:many──▶ Flight      (origin_id / destination_id FK)
//!   Flight   ──1:many──▶ Delay       (flight_id FK in delays.csv)
//!   Flight   ──1:many──▶ Crew        (flight_id FK in crew.csv)
//!   Passenger──1:many──▶ Booking     (passenger_id FK in bookings.csv)
//!   Booking  ←many:many→ Flight      (flight_code shared value / Booking is the junction)
//!
//! Four acts:
//!   Act 1  Ontology Discovery    — auto-detect object types, links, BCs, Shared Kernel
//!   Act 2  Flight Lifecycle      — event-sourced Flight aggregate (ES)
//!   Act 3  Departure Board       — read-model projection (Workshop widget equivalent)
//!   Act 4  Cancellation Saga     — FlightCancelled → cancel bookings (cross-BC)
//!                                                   → release crew (within-BC)

use std::collections::HashMap;

use palantir_application::ontology::{
    bounded_context::BoundedContextDetector, ddd_mapping::DddMapping, discovery::DiscoveryEngine,
    graph::OntologyGraph, relationship::RelationshipKind,
};
use palantir_domain::flight::{DepartureBoard, FlightEvent, FlightEventStore};
use palantir_infrastructure::{datasource::CsvLoader, export::JsonExporter};
use palantir_pipeline::dataset::Dataset;

const BASE: &str = "data/airline";

fn main() {
    banner("PALANTIR ONTOLOGY — Airline Operations");
    println!("  Reference: Palantir Foundry official docs + Airbus/Skywise case study");
    println!("  Domain: Flight, Aircraft, Airport, Delay, Passenger, Booking, Crew");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 1 — Ontology Discovery
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 1 — ONTOLOGY DISCOVERY  (schema-free 3-pass scan)");

    let specs: &[(&str, &str)] = &[
        // Operations BC
        ("aircraft.csv", "Aircraft"),
        ("airports.csv", "Airport"),
        ("flights.csv", "Flight"),
        ("delays.csv", "Delay"),
        ("crew.csv", "Crew"),
        // Passenger BC
        ("passengers.csv", "Passenger"),
        ("bookings.csv", "Booking"),
    ];

    let mut datasets: Vec<Dataset> = Vec::new();
    println!("  Loading datasets:");
    for (file, etype) in specs {
        let path = format!("{}/{}", BASE, file);
        match CsvLoader::load(&path, etype) {
            Ok(ds) => {
                println!(
                    "    ✓  {:<16} {:>3} records  → ObjectType: {}",
                    file,
                    ds.len(),
                    etype
                );
                datasets.push(ds);
            }
            Err(e) => eprintln!("    ✗  {} ERROR: {}", file, e),
        }
    }
    println!();

    let (objects, relationships) = DiscoveryEngine::discover(&datasets);
    let graph = OntologyGraph::build(objects, relationships);

    // Entity type summary
    println!("  Discovered Object Types (Palantir: object type = backed dataset):");
    println!(
        "  {:<12}  {:>5}  {:<50}",
        "ObjectType", "count", "sample properties"
    );
    println!("  {}", "─".repeat(70));
    let mut type_counts: Vec<_> = graph.type_counts().into_iter().collect();
    type_counts.sort_by(|a, b| b.1.cmp(&a.1));
    for (t, n) in &type_counts {
        let sample = graph
            .objects_by_type(t)
            .into_iter()
            .next()
            .map(|o| {
                let mut props: Vec<String> = o
                    .record
                    .fields
                    .keys()
                    .filter(|k| *k != "id")
                    .take(4)
                    .cloned()
                    .collect();
                props.sort();
                props.join(", ")
            })
            .unwrap_or_default();
        println!("  {:<12}  {:>5}  {}", t, n, sample);
    }
    println!();

    // Link type summary (Palantir: link type = relationship between object types)
    println!("  Discovered Link Types (HAS = ownership / integration):");
    println!(
        "  {:<12}  {:<6}  {:<12}  {}",
        "from_type", "kind", "to_type", "via_field"
    );
    println!("  {}", "─".repeat(54));
    let mut shown: std::collections::HashSet<String> = std::collections::HashSet::new();
    for rel in graph
        .relationships
        .iter()
        .filter(|r| r.kind == RelationshipKind::Has)
    {
        let key = format!("{}->{}", rel.from_type.0, rel.to_type.0);
        if shown.insert(key.clone()) {
            println!(
                "  {:<12}  {:<6}  {:<12}  {}",
                rel.from_type.0, "HAS", rel.to_type.0, rel.via_field
            );
        }
    }
    println!();

    // Bounded Context detection
    let ctx_map = BoundedContextDetector::detect(&graph);

    println!("  Bounded Contexts detected:");
    for bc in &ctx_map.contexts {
        println!(
            "    «{}»  entities: {}  cohesion: {:.0}%",
            bc.name,
            bc.entity_types.join(" · "),
            bc.cohesion * 100.0
        );
    }
    println!();

    println!("  Shared Kernel (value types referenced across BCs):");
    for dim in &ctx_map.shared_kernel.dimensions {
        println!("    {}", dim);
    }
    println!();

    println!("  Cross-Context Links (coupling points between BCs):");
    if ctx_map.cross_links.is_empty() {
        println!("    (none — BCs are fully isolated at the FK level)");
        println!("    Note: flight_code is a Shared Kernel string that logically links");
        println!("    Bookings→Flights, but does not appear as a FK — decoupling is intentional.");
    } else {
        for link in &ctx_map.cross_links {
            println!(
                "    {} → {}  via {}",
                link.from_bc, link.to_bc, link.via_type
            );
        }
    }
    println!();

    // DDD mapping
    let mapping = DddMapping::from_graph(&graph);
    println!("  DDD Classification:");
    println!("  {:<12}  {:<20}  {}", "ObjectType", "DDD Concept", "Layer");
    println!("  {}", "─".repeat(50));
    for c in &mapping.objects {
        println!(
            "  {:<12}  {:<20}  {:?}",
            c.object_type,
            c.concept.label(),
            c.concept.layer()
        );
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 2 — Flight Lifecycle (Event Sourcing)
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 2 — FLIGHT LIFECYCLE  (Event Sourcing — append-only FlightEventStore)");
    println!("  Each business fact = one immutable event.  State = fold(events, apply).");
    println!("  Palantir equivalent: time-series property on Flight object type.");
    println!();

    let mut fstore = FlightEventStore::new();

    // ── FL100 (fl001): Happy path ─────────────────────────────────────────────
    println!("  ── FL100 (JFK → LAX) — happy path ──────────────────────────────────");
    let fl001_events = vec![
        (
            "2024-04-01T06:00:00",
            FlightEvent::FlightScheduled {
                flight_id: "fl001".into(),
                flight_code: "FL100".into(),
                aircraft_id: "ac1".into(),
                origin: "JFK".into(),
                destination: "LAX".into(),
                departure: "2024-04-01T08:00:00".into(),
                airline: "SkyAir".into(),
            },
        ),
        (
            "2024-04-01T06:30:00",
            FlightEvent::GateAssigned {
                flight_id: "fl001".into(),
                gate: "B22".into(),
            },
        ),
        (
            "2024-04-01T07:30:00",
            FlightEvent::BoardingStarted {
                flight_id: "fl001".into(),
            },
        ),
        (
            "2024-04-01T08:05:00",
            FlightEvent::FlightDeparted {
                flight_id: "fl001".into(),
                actual_departure: "2024-04-01T08:05:00".into(),
            },
        ),
        (
            "2024-04-01T11:20:00",
            FlightEvent::FlightLanded {
                flight_id: "fl001".into(),
                actual_arrival: "2024-04-01T11:20:00".into(),
            },
        ),
    ];
    for (ts, ev) in fl001_events {
        print_event_append(&mut fstore, ts, ev);
    }
    print_flight_state(&fstore, "fl001");

    // ── FL200 (fl002): Delayed then departed ──────────────────────────────────
    println!("  ── FL200 (LAX → LHR) — delayed ─────────────────────────────────────");
    let fl002_events = vec![
        (
            "2024-04-01T08:00:00",
            FlightEvent::FlightScheduled {
                flight_id: "fl002".into(),
                flight_code: "FL200".into(),
                aircraft_id: "ac2".into(),
                origin: "LAX".into(),
                destination: "LHR".into(),
                departure: "2024-04-01T10:30:00".into(),
                airline: "SkyAir".into(),
            },
        ),
        (
            "2024-04-01T08:30:00",
            FlightEvent::GateAssigned {
                flight_id: "fl002".into(),
                gate: "D14".into(),
            },
        ),
        (
            "2024-04-01T10:15:00",
            FlightEvent::FlightDelayed {
                flight_id: "fl002".into(),
                delay_minutes: 45,
                reason: "crew late".into(),
            },
        ),
        (
            "2024-04-01T11:00:00",
            FlightEvent::FlightDelayed {
                flight_id: "fl002".into(),
                delay_minutes: 20,
                reason: "air traffic control".into(),
            },
        ),
        (
            "2024-04-01T11:35:00",
            FlightEvent::BoardingStarted {
                flight_id: "fl002".into(),
            },
        ),
        (
            "2024-04-01T11:50:00",
            FlightEvent::FlightDeparted {
                flight_id: "fl002".into(),
                actual_departure: "2024-04-01T11:50:00".into(),
            },
        ),
    ];
    for (ts, ev) in fl002_events {
        print_event_append(&mut fstore, ts, ev);
    }
    print_flight_state(&fstore, "fl002");

    // ── OA300 (fl003): Cancelled — triggers Saga in Act 4 ────────────────────
    println!("  ── OA300 (LHR → CDG) — cancelled ───────────────────────────────────");
    let fl003_events = vec![
        (
            "2024-04-01T10:00:00",
            FlightEvent::FlightScheduled {
                flight_id: "fl003".into(),
                flight_code: "OA300".into(),
                aircraft_id: "ac3".into(),
                origin: "LHR".into(),
                destination: "CDG".into(),
                departure: "2024-04-01T14:00:00".into(),
                airline: "OceanAir".into(),
            },
        ),
        (
            "2024-04-01T10:30:00",
            FlightEvent::GateAssigned {
                flight_id: "fl003".into(),
                gate: "T5-A".into(),
            },
        ),
        (
            "2024-04-01T13:45:00",
            FlightEvent::FlightCancelled {
                flight_id: "fl003".into(),
                reason: "crew shortage".into(),
            },
        ),
    ];
    for (ts, ev) in fl003_events {
        print_event_append(&mut fstore, ts, ev);
    }
    print_flight_state(&fstore, "fl003");

    // ── Event log ─────────────────────────────────────────────────────────────
    println!("  Full event store (all flights, global insertion order):");
    println!(
        "  {:<4}  {:<6}  {:<5}  {:<20}  {:<20}",
        "pos", "flight", "seq", "event_type", "occurred_at"
    );
    println!("  {}", "─".repeat(62));
    for r in fstore.all() {
        println!(
            "  {:>4}  {:<6}  {:>5}  {:<20}  {:<20}",
            r.store_pos, r.flight_id, r.sequence, r.event_type, r.occurred_at
        );
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 3 — Departure Board (Read Model Projection)
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 3 — DEPARTURE BOARD  (Read Model — Palantir Workshop widget)");
    println!("  Projection rebuilt by replaying all FlightEvents.");
    println!("  In Palantir: an Object Set filtered + sorted on the Flight object type.");
    println!();

    let board = DepartureBoard::build(&fstore);
    println!(
        "  {:<8}  {:<5}  {:<5}  {:<5}  {:<20}  {:<6}  {:<12}  {}",
        "code", "from", "to", "gate", "scheduled_dep", "delay", "status", "airline"
    );
    println!("  {}", "─".repeat(80));
    for e in &board.entries {
        let delay_str = if e.total_delay_mins > 0 {
            format!("+{}m", e.total_delay_mins)
        } else {
            "  —  ".to_string()
        };
        println!(
            "  {:<8}  {:<5}  {:<5}  {:<5}  {:<20}  {:<6}  {:<12}  {}",
            e.flight_code,
            e.origin,
            e.destination,
            e.gate,
            e.scheduled_dep,
            delay_str,
            e.status,
            e.airline
        );
    }
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  ACT 4 — Cancellation Saga (Cross-BC)
    // ══════════════════════════════════════════════════════════════════════════
    section("ACT 4 — CANCELLATION SAGA  (FlightCancelled → cross-BC reactions)");

    let cancelled_code = "OA300";
    let cancelled_flight_id = "fl003";

    println!("  Trigger: FlightCancelled  flight={cancelled_flight_id}  code={cancelled_code}");
    println!();
    println!("  Cross-BC routing:");
    println!("    Operations BC  FlightCancelled ──▶  Passenger BC  CancelBookings(OA300)");
    println!("    Operations BC  FlightCancelled ──▶  Operations BC ReleaseCrewAssignment(fl003)");
    println!();

    // Load CSVs for saga processing
    let bookings = load_csv_map(&format!("{}/bookings.csv", BASE));
    let passengers = load_csv_map(&format!("{}/passengers.csv", BASE));
    let crew_data = load_csv_map(&format!("{}/crew.csv", BASE));

    // ── Step 1: Cancel bookings (cross-BC — Passenger BC) ────────────────────
    println!("  Step 1 ▶  Passenger BC — cancel all bookings for {cancelled_code}");
    println!(
        "  {:<8}  {:<12}  {:<10}  {:<10}  {}",
        "booking", "passenger", "name", "class", "action"
    );
    println!("  {}", "─".repeat(58));

    let mut cancelled_bookings = 0;
    for row in &bookings {
        if row.get("flight_code").map(|s| s.as_str()) == Some(cancelled_code) {
            let pid = row.get("passenger_id").map(String::as_str).unwrap_or("?");
            let bid = row.get("id").map(String::as_str).unwrap_or("?");
            let cls = row.get("seat_class").map(String::as_str).unwrap_or("?");
            let name = passengers
                .iter()
                .find(|p| p.get("id").map(String::as_str) == Some(pid))
                .and_then(|p| p.get("name"))
                .map(String::as_str)
                .unwrap_or("Unknown");
            println!(
                "  {:<8}  {:<12}  {:<10}  {:<10}  → BookingCancelled",
                bid, pid, name, cls
            );
            cancelled_bookings += 1;
        }
    }
    println!(
        "  {} booking(s) cancelled — passengers will be notified for rebooking.",
        cancelled_bookings
    );
    println!();

    // ── Step 2: Release crew (within Operations BC) ───────────────────────────
    println!("  Step 2 ▶  Operations BC — release crew assigned to {cancelled_flight_id}");
    println!(
        "  {:<8}  {:<16}  {:<12}  {:<12}  {}",
        "crew", "name", "role", "airline", "action"
    );
    println!("  {}", "─".repeat(64));

    let mut released_crew = 0;
    for row in &crew_data {
        if row.get("flight_id").map(String::as_str) == Some(cancelled_flight_id) {
            let cid = row.get("id").map(String::as_str).unwrap_or("?");
            let name = row.get("name").map(String::as_str).unwrap_or("?");
            let role = row.get("role").map(String::as_str).unwrap_or("?");
            let airline = row.get("airline").map(String::as_str).unwrap_or("?");
            println!(
                "  {:<8}  {:<16}  {:<12}  {:<12}  → CrewReleased (available for rebooking)",
                cid, name, role, airline
            );
            released_crew += 1;
        }
    }
    println!(
        "  {} crew member(s) released — returned to standby pool.",
        released_crew
    );
    println!();

    println!("  Saga result:");
    println!(
        "    {} booking(s) cancelled across Passenger BC",
        cancelled_bookings
    );
    println!(
        "    {} crew member(s) released within Operations BC",
        released_crew
    );
    println!("    FlightCancellationSaga → Completed ✓");
    println!();

    // ══════════════════════════════════════════════════════════════════════════
    //  Summary
    // ══════════════════════════════════════════════════════════════════════════
    section("SUMMARY — Framework validated against Palantir Airline Ontology");
    println!();
    println!("  Palantir Concept          →  Framework Implementation");
    println!("  {}", "─".repeat(60));
    println!("  Object Type               →  domain struct (FlightState, etc.)");
    println!("  Link Type (1:many)        →  FK column (*_id) → DiscoveryEngine HAS");
    println!("  Link Type (many:many)     →  Junction table (Booking = Passenger×Flight)");
    println!("  Object Set query          →  Read Model Projection (DepartureBoard)");
    println!("  Action (modify object)    →  Command + Domain Event");
    println!("  Time-series property      →  FlightEventStore append-only log");
    println!("  Workflow/Function         →  Saga (CancellationSaga)");
    println!("  Bounded Context           →  Operations BC + Passenger BC");
    println!("  Shared Kernel             →  flight_code, airline, status (string values)");
    println!("  Anti-Corruption Layer     →  load_saga_csv + BcEvent envelope");
    println!();
    println!("  Ontology Discovery results:");
    println!(
        "    {} object types auto-detected from CSV schemas",
        type_counts.len()
    );
    println!(
        "    {} link types discovered (FK + categorical)",
        graph.relationships.len()
    );
    println!("    {} bounded contexts detected", ctx_map.contexts.len());
    println!(
        "    {} shared kernel dimensions",
        ctx_map.shared_kernel.dimensions.len()
    );

    // ── JSON Export for D3 visualizer ─────────────────────────────────────────
    section("JSON EXPORT  →  ontology_graph.json");
    let json = JsonExporter::export(&graph, &mapping, &ctx_map);
    JsonExporter::write(&json, "ontology_graph.json").expect("JSON write failed");
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

fn print_event_append(store: &mut FlightEventStore, ts: &str, event: FlightEvent) {
    let et = event.event_type();
    let seq = store.append(ts, event);
    println!("    seq={seq}  {ts}  {et}");
}

fn print_flight_state(store: &FlightEventStore, flight_id: &str) {
    let s = store.rebuild(flight_id);
    println!(
        "    → status={:<12} code={:<8} aircraft={:<5} gate={:<6} delay={}m",
        s.status.label(),
        s.flight_code,
        s.aircraft_id,
        s.gate.unwrap_or_else(|| "-".into()),
        s.total_delay_mins
    );
    println!();
}

/// Load a CSV file as a Vec of field→value maps (simple, no typing needed).
fn load_csv_map(path: &str) -> Vec<HashMap<String, String>> {
    let content =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("Cannot read {path}: {e}"));
    let mut lines = content.lines();
    let headers: Vec<&str> = lines
        .next()
        .unwrap_or("")
        .split(',')
        .map(str::trim)
        .collect();
    let mut rows = Vec::new();
    for line in lines {
        let vals: Vec<&str> = line.split(',').map(str::trim).collect();
        let mut map = HashMap::new();
        for (h, v) in headers.iter().zip(vals.iter()) {
            map.insert(h.to_string(), v.to_string());
        }
        rows.push(map);
    }
    rows
}

fn banner(title: &str) {
    let line = "═".repeat(title.len() + 6);
    println!("╔{}╗", line);
    println!("║   {}   ║", title);
    println!("╚{}╝", line);
    println!();
}

fn section(title: &str) {
    println!(
        "═══ {} {}",
        title,
        "═".repeat(75usize.saturating_sub(title.len() + 5))
    );
}
