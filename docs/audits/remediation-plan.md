# Consolidated Remediation Plan

Date: 2026-02-25  
Backlog source: `docs/audits/remediation-backlog.md`

## Release Gating Policy

### Must-fix before release

Release gate policy:
- All `Critical` and `High` severity backlog items must be fixed before release.
- The only exception is an explicit written deferral with owner, rationale, target milestone, and compensating controls.

Current pre-release must-fix IDs (High/Critical scope):
- `SEC-001`, `SEC-002`, `SEC-003`, `SEC-004`, `SEC-005`
- `COR-001`, `COR-002`
- `REL-001`, `REL-002`, `REL-003`, `REL-004` (delivered through `REL-004A/B/C`)
- `BUILD-001`, `BUILD-002`
- `TEST-002`

Mandatory pre-release requirement from `BUILD-001`:
- Phase 1 immutable SHA pinning (release + security workflows) must be complete before release sign-off.

### Decision alignment (resolved)

DEC-001 through DEC-005 are resolved and converted to implementation requirements in the backlog:
- DEC-001: XPC coordinator transport now canonical (`REL-004*`, `MNT-005`)
- DEC-002: timeout formula `min(policy_timeout, orchestration_cap)` (`COR-001`)
- DEC-003: phased SHA pinning with phase-1 release gate (`BUILD-001`)
- DEC-004: metadata truth on `main`/release only (`REL-001`)
- DEC-005: diagnostics redacted by default, explicit reveal/export for full context (`SEC-003`, `UX-001`)

## Recommended Order of Attack

1. Security and release supply-chain gates (P0)
- Complete `BUILD-001` phase 1 and keep `BUILD-002` green.
- Deliver remaining `SEC-*` High items, prioritizing low-blast-radius changes first.

2. Correctness and release integrity (P0)
- Deliver `COR-001` (timeout policy/cap enforcement) and keep `COR-002` behavior stable.
- Deliver `REL-001`, `REL-002`, `REL-003` to lock branch-aware metadata and publish verification.

3. Coordinator transport migration (P0/P1)
- Execute `REL-004A` first (XPC parity path).
- Execute `REL-004B` only if transitional compatibility is required.
- Execute `REL-004C` to enforce file-IPC removal or explicit one-cycle sunset plan.

4. Test depth for release confidence (P1)
- Keep `TEST-002` as release-gating.
- Pair release and policy changes with targeted contract tests (`TEST-004`, `TEST-005` where relevant).

5. UX/docs and maintainability follow-through (P1/P2)
- Deliver privacy-aligned diagnostics UX (`UX-001`) with `SEC-003`.
- Schedule maintainability items (`MNT-*`) after P0 gates unless they are direct enablers.

## PR-by-PR Definition of Done

Each remediation PR is done only when all checklist items below are satisfied.

### Scope and safety
- [ ] PR maps to 1-3 backlog IDs with clear intent in title/body.
- [ ] No unrelated file churn; no behavior change outside stated scope.
- [ ] Risk/rollback note is included in PR description.

### Validation
- [ ] Unit/integration tests added or updated for changed behavior.
- [ ] Existing relevant test suites pass locally/CI.
- [ ] For CI/workflow changes: dry-run/syntax validation and at least one successful workflow run evidence.

### Contracts and compatibility
- [ ] CLI/JSON/NDJSON output contracts remain stable (or migration documented).
- [ ] FFI/UI schema compatibility preserved (backward-compatible decode where required).
- [ ] Error messages remain actionable and localized where applicable.

### Security and privacy
- [ ] New/changed data paths are reviewed for redaction and secret exposure.
- [ ] Privileged or update-related changes include explicit abuse-case tests.
- [ ] No new network trust assumptions without documented policy.

### Documentation and operations
- [ ] Docs updated in the same PR (or explicitly not needed with justification).
- [ ] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` updated when behavior/process changes.
- [ ] Release/runbook/checklist docs updated for any release-impacting change.

### Merge readiness
- [ ] Required checks are green.
- [ ] Reviewer can verify acceptance criteria directly from diff + tests.
- [ ] Backlog item(s) referenced and status updated post-merge.

## Blocked Items Summary

No remaining decision-blocked items for DEC-001 through DEC-005.
