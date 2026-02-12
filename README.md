# Helm

Helm is a macOS package and update control plane in active pre-1.0 development.
It is designed as infrastructure software: deterministic, safety-first, and explicit about authority, orchestration, and error handling.

## Current Status

**v0.4.0 — SwiftUI Shell (beta)** is complete on `dev`.

Milestones completed:
- **0.1.x** — Core foundation: Rust workspace, adapter traits, capability model, SQLite schema.
- **0.2.x** — First adapter: Homebrew detection, listing, search with fixture-based tests.
- **0.3.x** — Orchestration engine: background task queue, per-manager serialization, cross-manager parallelism, process cancellation, structured errors, real process execution.
- **0.4.x** — SwiftUI shell: macOS menu bar app with floating panel UI, XPC service architecture, Rust FFI bridge, real-time task and package views, refresh wired end-to-end, code signing validation, app sandbox.

Next: **0.5.x** — Progressive search (local-first fuzzy search, debounced remote search, cancellation).

## Architecture

Helm is intentionally split into three layers:

1. `apps/macos-ui` (SwiftUI)
- Presentation only.
- Reads state, emits intents.

2. `apps/macos-ui/HelmService` (XPC service boundary)
- Hosts Rust FFI in a separate unsandboxed process.
- Owns process execution and privilege boundaries.

3. `core/rust` (Rust core)
- Manager models and adapter contracts.
- Orchestration and persistence contracts.
- Parsing/normalization and storage foundations.

## Repository Layout

- `apps/macos-ui/` — macOS app layer scaffold.
- `service/macos-service/` — service boundary scaffold.
- `core/rust/` — Rust workspace (`helm-core`, `helm-ffi`).
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
- Xcode 14+
- macOS 12+

Run core tests:

```bash
cd core/rust
cargo test
```

Build and run the macOS app:

1. Open `apps/macos-ui/Helm.xcodeproj` in Xcode.
2. Build and run the **Helm** scheme.

The build script (`scripts/build_rust.sh`) compiles the Rust FFI library and generates the version header automatically.

## Documentation

- Product and architecture brief: `PROJECT_BRIEF.md`
- Engineering guardrails: `AGENTS.md`
- Roadmap: `docs/ROADMAP.md`
- Versioning strategy: `docs/VERSIONING.md`
- 1.0 release criteria: `docs/DEFINITION_OF_DONE.md`

## License

Currently marked `UNLICENSED` in the Rust crate metadata.
