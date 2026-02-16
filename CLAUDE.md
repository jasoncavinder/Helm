# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

Helm is a native macOS menu bar utility for centralized package manager control. It manages software across multiple package managers (Homebrew, npm, pip, Cargo, etc.) and runtime tools (mise, asdf, rustup). Pre-1.0, currently at v0.7.0-alpha.1 (Homebrew, mise, and rustup adapters with authority-ordered refresh, two-phase detection, and enhanced dashboard UI).

## Build & Test Commands

Rust commands run from `core/rust/`:

```bash
cargo build                              # build
cargo test --lib                         # unit tests
cargo test --test '*'                    # integration tests
cargo test <test_name>                   # single test by name
cargo test --lib -- --nocapture          # tests with stdout
RUST_LOG=debug cargo test <name>         # tests with tracing output
cargo clippy --all-targets               # lint
cargo fmt                                # format
cargo fmt -- --check                     # format check (CI-style)
```

Xcode build from `apps/macos-ui/`:

```bash
xcodebuild -project Helm.xcodeproj -scheme Helm -configuration Debug build
```

## Architecture

Three-layer architecture — do not collapse or bypass boundaries:

1. **UI (SwiftUI)** — `apps/macos-ui/` — Pure presentation, no business logic. Menu bar app with floating panel.
2. **Service (XPC)** — `apps/macos-ui/HelmService/` — Hosts Rust FFI in a separate process. Unsandboxed for process execution. Code-signed connection validation.
3. **Core (Rust)** — `core/rust/crates/helm-core/` — All business logic, adapters, orchestration, persistence. Exposed to Swift via `helm-ffi` C ABI.

### Rust Core Modules (`core/rust/crates/helm-core/src/`)

- **`models/`** — Domain types: `ManagerId` (28 managers), `PackageRef`, `InstalledPackage`, `OutdatedPackage`, `TaskRecord`, `CoreError`, `PinRecord`, `SearchQuery`. Errors carry attribution (manager/task/action).
- **`adapters/`** — Package manager adapters implementing `ManagerAdapter` trait. Each adapter uses a trait-based source for dependency injection and testability. Homebrew, mise, and rustup are implemented. Shared utilities in `process_utils.rs` and `detect_utils.rs`.
- **`orchestration/`** — Task execution engine. `InMemoryTaskCoordinator` (sync state machine), `InMemoryAsyncTaskQueue` (async runtime with Tokio), `AdapterExecutionRuntime` (wraps adapter calls). Per-manager mutex enforces serial execution; cross-manager parallelism allowed.
- **`persistence/`** — Abstract store traits: `PackageStore`, `PinStore`, `SearchCacheStore`, `TaskStore`, `MigrationStore`.
- **`sqlite/`** — SQLite implementation of all store traits. Schema v1 with versioned migrations.
- **`registry.rs`** — Manager descriptors with capability declarations, authority levels, and categories.

### Key Patterns

- **Adapter trait**: `ManagerAdapter` with `execute(AdapterRequest) -> AdapterResult<AdapterResponse>`. Request/response are typed enums, not free-form.
- **Capability gating**: Adapters declare supported capabilities; requests are checked before execution.
- **Concurrency**: `SerialPerManagerPolicy` — same manager tasks serial, different managers parallel. Enforced via per-manager mutex locks.
- **Cancellation**: `TaskCancellationToken` for cooperative cancellation, Tokio `AbortHandle` for process-level.
- **Authority ordering**: `authority_phases()` groups adapters by `ManagerAuthority` (Authoritative → Standard → Guarded). `refresh_all_ordered()` executes phases sequentially with cross-manager parallelism within each phase. Failure in one manager does not block others.

## Authority Documents

In order of precedence:

1. **`docs/PROJECT_BRIEF.md`** — Authoritative product and architecture spec. Wins over conflicting instructions.
2. **`AGENTS.md`** — Non-negotiable development constraints and principles.
3. **`docs/ROADMAP.md`** — Milestone definitions (0.1 through 1.0).

## Git Workflow

- **`main`**: stable/releasable, protected. Merges from `dev` only (except hotfixes).
- **`dev`**: integration branch. Feature branches merge here via PR.
- **Feature branches**: `feat/`, `fix/`, `chore/`, `docs/`, `test/`, `refactor/` — branch off `dev`.
- **Commit prefixes**: `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`
- Tags only from `main`. Version bumps follow `docs/VERSIONING.md`.

## Non-Negotiable Constraints

- No shell injection — use structured process arguments, never string concatenation.
- No global mutable state in Rust core.
- Rust core must be UI-agnostic, deterministic, and testable.
- Same-manager tasks must run serially; cross-manager parallelism is allowed.
- Errors must be attributed to manager, task, and action.
- Schema migrations must be explicit and reversible.
- Tests favor determinism over realism; parsers use fixed fixtures in `tests/fixtures/`.
- Work incrementally with small, coherent commits. No speculative features beyond the brief.

## Licensing

Helm is source-available and not open source.

Contributions are governed by the CLA.
