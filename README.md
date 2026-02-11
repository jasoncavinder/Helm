# Helm

Helm is a macOS package and update control plane in active pre-1.0 development.
It is designed as infrastructure software: deterministic, safety-first, and explicit about authority, orchestration, and error handling.

## Current Status

This branch (`main`) currently represents the **0.3.x orchestration engine** stage.

Implemented today on `main`:
- Repository scaffold for the 3-layer architecture.
- Rust core workspace (`core/rust`) with:
  - manager and capability data models,
  - adapter trait/contracts,
  - orchestration contracts + in-memory coordinator,
  - SQLite migration and persistence contracts.
- **Orchestration Engine**:
  - Background task queue with per-manager serialization.
  - Cross-manager parallelism.
  - Process-level cancellation (SIGTERM/SIGKILL).
  - Structured error reporting.
- **Homebrew Adapter**:
  - Full detection, listing, and search capabilities.
  - Safe, structured process execution without shell strings.
- Deterministic unit/integration tests for core contracts.

Not yet implemented on `main`:
- Production UI behavior in `apps/macos-ui`.
- Service-boundary XPC integration.

## Architecture

Helm is intentionally split into three layers:

1. `apps/macos-ui` (SwiftUI)
- Presentation only.
- Reads state, emits intents.

2. `service/macos-service` (service boundary)
- Owns process execution and privilege boundaries.
- Enforces cancellation/exclusivity.

3. `core/rust` (Rust core)
- Manager models and adapter contracts.
- Orchestration and persistence contracts.
- Parsing/normalization and storage foundations.

## Repository Layout

- `apps/macos-ui/` — macOS app layer scaffold.
- `service/macos-service/` — service boundary scaffold.
- `core/rust/` — Rust workspace (`helm-core`).
- `docs/` — roadmap, versioning, and release criteria.
- `PROJECT_BRIEF.md` — product and architecture source of truth.
- `AGENTS.md` — repository engineering and workflow constraints.

## Development Workflow

Branch policy:
- `main`: stable/releasable.
- `dev`: integration branch for active feature work.
- feature branches: merge to `dev` first unless explicitly directed otherwise.

Current roadmap and milestones are tracked in:
- `docs/ROADMAP.md`
- `docs/VERSIONING.md`

## Getting Started

Prerequisites:
- Rust stable toolchain (edition 2024)
- Cargo

Run core tests:

```bash
cd core/rust
cargo test
```

Format Rust code:

```bash
cd core/rust
cargo fmt --all
```

## Documentation

- Product and architecture brief: `PROJECT_BRIEF.md`
- Engineering guardrails: `AGENTS.md`
- Roadmap: `docs/ROADMAP.md`
- Versioning strategy: `docs/VERSIONING.md`
- 1.0 release criteria: `docs/DEFINITION_OF_DONE.md`

## License

Currently marked `UNLICENSED` in the Rust crate metadata.
