# 0.3.x Kickoff Plan (Orchestration Engine)

This plan starts milestone `0.3.x` from `docs/ROADMAP.md`.

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

## Versioning Notes

- Continue commits on `dev`.
- Follow `docs/VERSIONING.md`:
  - Milestone completion uses a MINOR bump (`0.3.x` target).
  - Iterations within milestone use PATCH increments.
- Do not tag on `dev`; tag only from `main`.
