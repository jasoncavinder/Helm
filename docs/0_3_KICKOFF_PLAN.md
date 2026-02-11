# 0.3.x Kickoff Plan (Orchestration Engine) - Completed

This plan starts milestone `0.3.x` from `docs/ROADMAP.md`.

**Status: COMPLETED (v0.3.0 released)**

Current released development baseline:
- `helm-core` version: `0.2.0-alpha.1`
- Branch for active work: `dev`

## Scope for 0.3.x

Per roadmap goals:
- Background task queue
- Per-manager locking
- Cross-manager parallelism
- True process cancellation
- Structured error reporting

## Implementation Order

1. Runtime task queue contract
- Add async task queue interfaces in core orchestration module.
- Keep per-manager mutual exclusion explicit and testable.

2. Execution and cancellation plumbing
- Introduce task runtime handles (`spawn`, `cancel`, status snapshots).
- Ensure cancellation is process-level at the service boundary contract.

3. Manager concurrency policy integration
- Enforce serial execution for same manager.
- Allow parallel execution across different managers.

4. Structured failure propagation
- Standardize orchestration errors with manager/task/action attribution.
- Add deterministic integration tests for queue + cancellation + failures.

## Exit Criteria (0.3.x)

- Multiple managers run concurrently.
- Same-manager tasks are serialized.
- Cancellation behavior is verified by tests.
- Structured per-task failures are observable from core contracts.

## Progress Snapshot (February 11, 2026)

Completed in this milestone branch:
- Async runtime queue with per-manager serialization and cross-manager parallelism.
- Immediate and graceful cancellation coverage with deterministic tests.
- Adapter execution runtime with structured terminal states.
- Manager-dispatch adapter runtime with duplicate/unknown manager validation.
- Task persistence hooks for orchestration runtime plus SQLite-backed integration tests.
- Service-boundary process execution contracts (`ProcessSpawnRequest`, `ProcessExecutor`, `RunningProcess`).
- Homebrew structured process request planning (detect/list/search) with no shell-string construction.
- `TokioProcessExecutor`: real process spawning with `process_group(0)`, SIGTERM/SIGKILL termination, timeout enforcement, and 7 integration tests against system binaries.
- `ProcessHomebrewSource`: wires `HomebrewSource` trait through `ProcessExecutor` contract using existing structured request builders.
- End-to-end orchestration tests: full path from `AdapterRuntime` through `HomebrewAdapter<ProcessHomebrewSource>` to a routing fake executor, verifying detect/list/search/failure propagation (5 tests).

All 0.3 exit criteria confirmed:
- Multiple managers run concurrently (tested in `orchestration_runtime_queue`).
- Same-manager tasks are serialized (tested in `orchestration_runtime_queue` and `orchestration_in_memory`).
- Cancellation behavior verified by tests (immediate + graceful, tested in `orchestration_adapter_execution` and `orchestration_runtime_queue`).
- Structured per-task failures observable from core contracts (tested in `orchestration_adapter_execution`, `orchestration_adapter_runtime`, and `end_to_end_homebrew`).
- Process execution wired end-to-end through adapter and orchestration layers (tested in `end_to_end_homebrew`).

## Versioning Notes

- Continue commits on `dev`.
- Follow `docs/VERSIONING.md`:
  - Milestone completion uses a MINOR bump (`0.3.x` target).
  - Iterations within milestone use PATCH increments.
- Do not tag on `dev`; tag only from `main`.
