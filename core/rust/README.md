# Helm Rust Core Workspace

This workspace holds the UI-agnostic core for Helm. It contains all business logic, manager adapters, orchestration, and persistence.

## Crates

| Crate | Role |
|-------|------|
| `helm-core` | Domain models, adapter trait, orchestration engine, SQLite persistence, all manager adapters |
| `helm-ffi` | C ABI FFI boundary for bridging to Swift via XPC service |
| `helm-cli` | CLI and TUI clients using the shared core/coordinator contracts |

## Implemented Managers (28)

| Category | Adapters |
|----------|----------|
| Toolchain / Runtime | mise, asdf, rustup |
| System / OS | Homebrew formulae and casks, MacPorts, softwareupdate, nix-darwin, Xcode Command Line Tools, Rosetta 2, firmware updates |
| Language | npm, pnpm, yarn, pip, pipx, Poetry, RubyGems, Bundler, Cargo, cargo-binstall |
| App / Platform | mas, Sparkle, Setapp, Docker Desktop, podman, colima, Parallels Desktop |

## Key Subsystems

- **Adapter trait** — Capability-driven request/response contracts per manager
- **Authority ordering** — Authoritative → Standard → Guarded phased execution
- **Orchestration engine** — Task queue with per-manager serial execution, cross-manager parallelism, true process cancellation
- **SQLite persistence** — Versioned schema (v1–v16), parameterized queries, transactional operations
- **Post-upgrade validation** — After upgrade succeeds, re-checks `list_outdated` where the manager supports validation
- **Pinning** — Native pin support + virtual pin fallback, pin-aware upgrade-all
- **Progressive search** — Local-first with debounced remote search and cache enrichment
- **Structured tracing** — `#[instrument]` spans on adapter execution entry points

## Tests

Unit and integration tests covering:
- Adapter parsing fixtures across implemented managers
- Orchestration, authority ordering, and cancellation flows
- Post-upgrade validation scenarios
- End-to-end integration tests

## Building

```bash
cargo test --workspace --manifest-path core/rust/Cargo.toml
```

## Architecture

The Rust core is:
- **UI-agnostic** — No dependency on SwiftUI or any UI framework
- **Fully testable** — Fake source traits for deterministic testing
- **Deterministic** — No shell invocation, structured process arguments only
- **Thread-safe** — Static state with poisoned-lock recovery in FFI boundary

See `docs/ARCHITECTURE.md` for the full system architecture.
