# Palantir — DDD + Ontology in Rust

A learning project that combines **Domain-Driven Design (DDD)** with **Palantir Foundry Ontology** concepts, implemented in Rust.

The core idea: given a dataset, automatically discover entities and their relationships, map those relationships to actionable operations (Logic / Integration / Workflow / Search), and render the result as a visual semantic graph — a "digital twin" of the business domain.

---

## Architecture

```
src/
├── domain/           # DDD — pure business logic, zero I/O
│   ├── organization.rs    Employee aggregate root, EmployeeRepository trait
│   ├── finance.rs         Transaction entity, TransactionRepository trait
│   ├── money.rs           Money value object
│   └── events.rs          Domain events + Ontology-triggered events (HighSpend, CategoryConc.)
│
├── application/      # DDD — orchestrates domain, no business rules
│   ├── commands.rs        Write side: hire_employee, file_transaction, flag_high_value
│   └── queries.rs         Read side (CQRS): dept spend, top earners, high-value txns
│
├── infrastructure/   # DDD — ports & adapters, swappable
│   └── in_memory.rs       InMemoryEmployeeRepo, InMemoryTransactionRepo
│
├── datasource/       # A: Infrastructure adapter — loads datasets from any source
│   └── mod.rs             CsvLoader — CSV → Dataset (same pipeline as in-memory)
│
├── analytics/        # Palantir-style pipeline engine
│   ├── dataset.rs         Dataset, Record, Value (dynamic typing)
│   └── pipeline.rs        Transform trait + Filter/Select/Derive/Aggregate/Join/Sort
│
├── ontology/         # Palantir Ontology — semantic discovery layer
│   ├── entity.rs          OntologyObject, EntityId, ObjectType
│   ├── relationship.rs    Relationship, RelationshipKind (BelongsTo/Has/LinkedTo/SimilarTo)
│   ├── discovery.rs       DiscoveryEngine — auto-extracts entities & relationships (3 passes)
│   ├── graph.rs           OntologyGraph — node/edge query API
│   ├── ddd_mapping.rs     Maps ontology concepts → DDD building blocks
│   ├── bounded_context.rs B: BoundedContextDetector — Union-Find clustering by HAS density
│   └── pattern_detector.rs C: PatternDetector — scans graph → emits DomainEvents
│
├── action/           # Bridge layer — Ontology → DDD operations
│   └── mod.rs             ActionSummary, derive_actions()
│
├── export/           # D: Published Language — serialize ontology to JSON
│   └── mod.rs             JsonExporter — OntologyGraph + DddMapping → ontology_graph.json
│
└── visualization/    # ASCII rendering
    └── mod.rs             8 views: entity table, relationship patterns, semantic tree,
                           spend chart, action mapping, DDD map, BC detection, event loop

data/
├── employees.csv     # Sample dataset (mirrors in-memory data)
└── transactions.csv  # Sample dataset

ontology_graph.json   # Generated — Published Language export (D3.js / Cytoscape ready)
```

---

## Key Concepts

### DDD vs Palantir Ontology

They solve different problems and work together:

| | DDD | Palantir Ontology |
|---|---|---|
| Solves | How to **structure code** (layers, aggregates, events) | What the **data means** (objects, relationships, semantics) |
| Origin | Designed by domain experts | **Auto-discovered** from dataset structure |
| Output | Maintainable code architecture | Actionable digital twin |

> **Ontology = Ubiquitous Language made machine-readable. DDD = the architecture that enforces its boundaries.**

### How They Map

| Palantir Ontology | DDD Concept | Layer |
|---|---|---|
| `OntologyObject` with outgoing HAS | **Aggregate Root** | Domain |
| `OntologyObject` with incoming HAS | **Entity** | Domain |
| Grouping dimension (department, level) | **Value Object** | Domain |
| `BelongsTo` relationship | Aggregate boundary | Domain |
| Logic action | **Domain Service** | Domain |
| Integration action | **Repository + ACL** | Infrastructure |
| Workflow action | **Application Service** | Application |
| Search action | **Query Handler (CQRS)** | Application |
| Ontology itself | **Ubiquitous Language** | — |

---

## How Discovery Works

`DiscoveryEngine::discover()` scans raw datasets in three passes — no schema knowledge required:

**Pass 1 — Entities**
Every record in every dataset becomes an `OntologyObject` typed by its dataset's `object_type`.

**Pass 2 — HAS relationships (Integration)**
Fields whose names end in `_id` are treated as foreign keys. If the referenced ID exists in any dataset, an `Employee ──HAS──▶ Transaction` edge is created.

**Pass 3 — BELONGS_TO relationships (Logic)**
String fields where multiple records share the same value become grouping dimensions (e.g. `department`, `level`, `category`). Each member gets a `BELONGS_TO` edge to its group.

---

## Output

Running the project (`cargo run`) produces five views:

```
╔═══════════════════════════════════════════════════════════════════╗
║              PALANTIR ONTOLOGY  —  DIGITAL TWIN VIEW             ║
╚═══════════════════════════════════════════════════════════════════╝

1. Discovered Entities         — object types and counts
2. Relationship Patterns       — pattern table with action category
3. Semantic Entity Tree        — Dept → Employee ──HAS──▶ Transaction
4. Department Spend Bar Chart  — spend intensity visualised
5. Action Mapping              — Logic / Integration / Workflow / Search
6. DDD Architecture Mapping    — which DDD layer each concept belongs to
```

### Semantic Tree (sample)
```
  ┌─ [Engineering]   spend: $7995
  │  ├── [e7] Grace Nguyen  (Staff, $145000)
  │  │    ├─ ──HAS──▶  [t10]  $5000  Software
  │  │    └─ ──HAS──▶  [t11]  $120   Office Supplies
  │  └── [e1] Alice Chen  (Senior, $120000)
  │       └─ ──HAS──▶  [t01]  $1200  Software
```

### Spend Bar Chart (sample)
```
  Engineering    │████████████████████████████████████│  $7995
  Marketing      │███████████████████░░░░░░░░░░░░░░░░░│  $4250
  Sales          │█████████████████░░░░░░░░░░░░░░░░░░░│  $3730
  Operations     │█████░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░│  $1130
```

---

## Getting Started

**Prerequisites:** Rust 1.85+ (edition 2024)

```bash
git clone <repo>
cd palantir
cargo run
```

No external dependencies — pure Rust standard library.

---

## Four Extension Dimensions

### A — Data Source (Infrastructure Adapter)
`src/datasource/CsvLoader` loads any CSV into a `Dataset`.
The ontology discovery pipeline is identical regardless of source.
```
CSV / Database / Kafka / REST API
  └── (adapter) → Dataset → DiscoveryEngine → OntologyGraph
```
**DDD pattern:** Infrastructure port — swap the adapter, zero changes above.

### B — Bounded Context Detection
`src/ontology/bounded_context::BoundedContextDetector` clusters entity types using Union-Find:
- Types connected by **HAS** (ownership) → same Bounded Context
- Types appearing only as **BELONGS_TO** targets → Shared Kernel (Value Objects)
- Outputs cohesion score (internal HAS / total HAS) and cross-context coupling

**DDD pattern:** Bounded Context + Shared Kernel (Evans).

### C — Ontology → Domain Event Loop
`src/ontology/pattern_detector::PatternDetector` scans the graph for business patterns:
- `HighSpend`: employee total spend > threshold → `HighSpendPatternDetected`
- `CategoryConcentration`: >60% spend in one category → `CategoryConcentrationDetected`

Events are published to the `EventBus` → Application Services respond with Commands.
```
OntologyGraph → PatternDetector → DomainEvent → EventBus → ApplicationService → Command → Domain
```
**DDD pattern:** Domain Event + Application Service (command handler).

### D — JSON Export (Published Language)
`src/export::JsonExporter` serialises the full ontology (entities, relationships, BCs, DDD annotations) to `ontology_graph.json`.
- Frontend: feed to D3.js or Cytoscape for interactive graph visualization
- Other BCs: read `bounded_contexts` as the shared contract
- Analytics: use `relationships` for data lineage analysis

**DDD pattern:** Published Language (Evans) — language-agnostic contract between BCs.

---

## Extending Further

**Add a new entity type**
Add records to a new `Dataset` with a distinct `object_type` and pass it to `DiscoveryEngine::discover()`. Relationships are detected automatically.

**Add a new relationship kind**
Extend `RelationshipKind` in `src/ontology/relationship.rs` and handle it in `DiscoveryEngine`, `action/mod.rs`, and `visualization/mod.rs`.

**Add a new pattern**
Add a detection branch in `src/ontology/pattern_detector.rs` and a matching `DomainEvent` variant in `src/domain/events.rs`.

---

## Design Principles

- **Domain layer stays pure** — no I/O, no framework dependencies, only Rust primitives and business logic.
- **Anti-Corruption Layer** — `application/queries.rs` converts domain objects into analytics `Dataset`s, keeping the two models decoupled.
- **Discovery is schema-free** — the ontology engine infers structure from data patterns, not from hardcoded schemas.
- **Visualization is a separate concern** — all rendering lives in `src/visualization/`, with no display logic leaking into domain or ontology layers.
