# Repository Guidelines

## Project Structure & Module Organization
- `src/` — Rust code: `domain/` (business entities, events, value objects), `application/` (services, queries, ontology), `infrastructure/` (adapters, pipeline, export), `interface/` (console renderers), `bin/serve` (D3 static server).
- `examples/` — runnable demos (`cargo run --example 0x_*`) covering DDD, ontology, CSV adapter, workflows, ES, multi-BC, saga, airline, etc.
- `assets/` — D3 visualizer (`index.html`) consuming `ontology_graph.json`.
- `data/` — sample CSVs for demos (core, complex, timeseries, saga, multi_bc, airline).
- Generated artifacts: `ontology_graph.json` and `bc_process.puml` in repo root after running exporters.

## Build, Test, and Development Commands
- Build library & binaries: `cargo build`.
- Run main banner: `cargo run`.
- Run demo: `cargo run --example 08_multi_bc` (exports ontology + UML).
- Serve D3 viewer: `cargo run --bin serve` → open `http://localhost:3000`.
- Format: `cargo fmt`.
- Lint (if needed): `cargo clippy` (not wired in CI; run manually).

## Coding Style & Naming Conventions
- Rust 2024 edition; follow `cargo fmt` defaults (4-space indent).
- Modules and files use snake_case; types use PascalCase; functions/vars snake_case.
- Keep domain purity: domain layer stays I/O free; adapters live in `infrastructure/`.
- Keep D3 assets plain HTML/JS; avoid bundlers; prefer small, focused functions.

## Testing Guidelines
- No dedicated test harness present; demos double as executable specs. Before changes, run representative examples (`cargo run --example 02_ontology`, `08_multi_bc`, or affected scenario).
- If adding tests, place under `tests/` or `src/*/mod.rs` with `#[cfg(test)]` and name cases in snake_case.

## Commit & Pull Request Guidelines
- Commit messages: short imperative summary (e.g., “add bc link colors”); keep scope focused.
- PRs should include: purpose, key changes, how to verify (commands run), and screenshots/GIFs if UI (D3) is affected.
- Regenerate artifacts when relevant: rerun example to refresh `ontology_graph.json` / `bc_process.puml`, and note that in the PR.

## Security & Configuration Tips
- Never commit secrets. D3 server is static; no backend creds required.
- Generated files are safe to commit; avoid editing them manually—use the exporters.
