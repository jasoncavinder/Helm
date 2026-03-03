# AGENTS.md — core/rust

This file applies to `core/rust/**`.

## Parent Policy

- Read and follow repository root `AGENTS.md` first.
- Root policy wins on conflicts.

## Scope

Use this subtree guidance for:
- `helm-core`, `helm-ffi`, and `helm-cli` changes
- adapter, orchestration, persistence, policy, and CLI contract work

## Local Working Rules

- Keep core deterministic and UI-agnostic.
- Use structured process args only; no shell string composition.
- Preserve authority ordering and task cancellation behavior.
- Prefer small, targeted changes with regression tests.

## Fast Verification Commands

- `cargo test --workspace --manifest-path core/rust/Cargo.toml`
- `cargo fmt --all --manifest-path core/rust/Cargo.toml -- --check`
- `cargo clippy --workspace --manifest-path core/rust/Cargo.toml -- -D warnings`

For targeted loops, run crate/test-specific commands first, then widen.

## Documentation Sync Triggers

When behavior changes, update relevant docs (at minimum):
- `docs/CURRENT_STATE.md`
- `docs/NEXT_STEPS.md`
- `docs/DECISIONS.md` (for contract/policy shifts)
- `docs/architecture/MANAGER_ELIGIBILITY_POLICY.md` when eligibility rules change
