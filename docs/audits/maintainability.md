# Helm Maintainability Review

Date: 2026-02-25  
Scope: Rust core/CLI/FFI and Swift UI core integration layers.  
Goal: reduce change risk without behavior changes.

## Hotspots

### 1) Monolithic CLI command surface
- Area: `core/rust/crates/helm-cli/src/main.rs`
- Risk: High
- Why it is a hotspot:
  - Very large single file with mixed concerns (arg parsing, workflow orchestration, JSON/text output, coordinator transport, self-update policy).
  - Exit-code and error-shaping logic is repeated across command families.
- Suggested refactors:
  - S: centralize repeated failure-exit mapping and invariants around coordinator timeout handling.
  - M: split command handlers into per-domain modules (`commands/refresh.rs`, `commands/managers.rs`, `commands/updates.rs`) while preserving current command contracts.
  - L: move coordinator protocol/types into dedicated module/crate to decouple command dispatch from transport details.

### 2) Stringly-typed FFI error signaling
- Area: `core/rust/crates/helm-ffi/src/lib.rs`
- Risk: High
- Why it is a hotspot:
  - Error keys are mostly raw string literals at callsites.
  - Repeated lock/state/error boilerplate in exported entrypoints.
  - Mixed logging style (`eprintln!` messages vary by function), making triage noisy.
- Suggested refactors:
  - S: centralize high-frequency error keys as constants in touched paths; add shared helpers for repeated refresh/detection gating + manager-failure log format.
  - M: introduce a local typed error-key enum mapped to localization keys at FFI boundary.
  - L: convert FFI exports to typed `Result`-first internal API, with one JSON/error marshalling layer.

### 3) Runtime orchestration complexity concentration
- Area: `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs`
- Risk: Medium
- Why it is a hotspot:
  - Detect/refresh orchestration combines filtering, scheduling, result normalization, persistence/logging concerns in dense functions.
- Suggested refactors:
  - M: extract pure helpers for manager selection, phase execution, and result reduction.
  - L: formalize orchestration stage pipeline types for easier unit testing and regression isolation.

### 4) Swift fetch/action concentration in HelmCore extensions
- Areas:
  - `apps/macos-ui/Helm/Core/HelmCore+Fetching.swift`
  - `apps/macos-ui/Helm/Core/HelmCore+Actions.swift`
  - `apps/macos-ui/Helm/Core/HelmCore+Settings.swift`
- Risk: Medium
- Why it is a hotspot:
  - Repeated decode/log/error-recording patterns across many fetch/action methods.
  - High coupling between UI state mutations and transport/decode details.
- Suggested refactors:
  - S: factor shared decode+error-context helper inside fetching extension.
  - M: introduce typed service-response wrappers and shared action/fetch execution wrapper.
  - L: split `HelmCore` concerns into coordinators (tasks/search/settings/update).

### 5) Adapter duplication (path/env/bootstrap patterns)
- Areas: manager process sources (`*_process.rs`), especially toolchain managers.
- Risk: Medium
- Why it is a hotspot:
  - Similar PATH bootstrapping and executable-root logic repeated with slight variations.
  - Missing-home and path-edge behavior can diverge per adapter.
- Suggested refactors:
  - S: extract tiny local helpers for optional-home path prefix behavior in each touched adapter.
  - M: add shared adapter utility for optional-home path prefix composition + root list construction.

## Implemented Small (S) Refactors

These were applied with near-zero behavioral risk and validated by tests.

### A) Consolidated manager failure-exit mapping reuse in CLI handlers
- Files:
  - `core/rust/crates/helm-cli/src/main.rs`
- Change:
  - Reused shared `manager_operation_failure_error(...)` in `refresh all` and `managers detect all` handlers instead of duplicating failure->exit-code branches.
- Behavior:
  - No contract change; same failure semantics preserved.

### B) Added explicit coordinator timeout invariant comment
- Files:
  - `core/rust/crates/helm-cli/src/main.rs`
- Change:
  - Documented precondition/invariant in `should_launch_coordinator_on_demand(...)` that timeout errors must not trigger launch-on-demand reset path.
- Behavior:
  - Documentation-only; no behavior change.

### C) Reduced duplication in FFI refresh/detection inflight gating
- Files:
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Change:
  - Added `has_recent_refresh_or_detection(...)` helper and reused it in both `helm_trigger_refresh` and `helm_trigger_detection`.
- Behavior:
  - Same gating logic; centralized implementation.

### D) Standardized manager-operation failure logging shape in FFI
- Files:
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Change:
  - Added `log_manager_operation_failure(...)` and used it for refresh/detection async result errors.
- Behavior:
  - Same error visibility; improved consistency of log context (`operation`, `manager`, `error`).

### E) Centralized common internal FFI error key in touched paths
- Files:
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Change:
  - Added `SERVICE_ERROR_INTERNAL` constant and reused it in touched refresh/detection state checks.
- Behavior:
  - No behavior change; reduces literal duplication.

## Remaining Refactor Backlog (Suggested)

### Small (S)
1. Swift fetch decode helper extraction in `HelmCore+Fetching.swift`.
2. Normalize a few additional repeated FFI error keys (`invalid_input`, `process_failure`, `storage_failure`, `unsupported_capability`) to constants.
3. Add a shared CLI helper for `options.json` emit-vs-print pattern in manager subcommands.

### Medium (M)
1. Split `helm-cli` command handling into per-domain modules with unchanged command contract.
2. Add typed FFI error-key enum mapped to localization key strings at export boundary.
3. Extract orchestration stage helpers from `AdapterRuntime` detect/refresh ordered flows.
4. Introduce typed Swift service response wrappers for fetch/action operations.

### Large (L)
1. Isolate coordinator transport protocol/state-machine from CLI command file.
2. Re-architect FFI internal API around typed errors/results with single serialization boundary.
3. Split Swift `HelmCore` into domain coordinators to reduce cross-feature state coupling.

## Validation

Executed:
- `cargo fmt`
- `cargo test -p helm-cli`
- `cargo test -p helm-ffi`

Result: passing.
