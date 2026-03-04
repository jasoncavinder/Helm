# Quality Audit Decisions

Date: 2026-02-25  
Status: Final decisions recorded for DEC-001 through DEC-005.

## DEC-001 - Coordinator IPC Transport and Security Model

Status: Resolved (2026-02-25)

Decision: Helm will migrate coordinator IPC to XPC now. File-based request/response IPC is no longer the primary transport. A file-IPC fallback may exist only as a transitional compatibility path behind an explicit feature flag for at most one release cycle, with removal preferred when compatibility is not required.

Rationale:
- Helm already has an XPC service boundary and trust model on macOS.
- XPC reduces spoofing/tampering risk compared with filesystem request/response files.
- A transitional fallback preserves operational continuity only if migration compatibility is needed.

Consequences / Follow-on Work:
- REL-004 implementation changes from file-IPC nonce hardening to XPC migration delivery.
- MNT-005 focuses on transport separation and coordinator protocol modularization around XPC.
- Release docs and migration docs must define fallback policy, default state, and removal target if fallback is retained.

Acceptance Criteria:
- Coordinator submit/cancel/workflow/wait paths use XPC transport in normal operation.
- File-IPC transport is not used by default.
- If fallback exists, it is explicit, feature-flagged, and time-bounded to one release cycle with a documented removal milestone.
- Transport-layer tests cover request handling, cancellation, and failure/recovery behavior on XPC path.
- Documentation names XPC as the canonical coordinator transport.

## DEC-002 - Orchestration Wait Timeout Policy

Status: Resolved (2026-02-25)

Decision: Coordinator/request-response waits use effective timeout values derived from policy and bounded by orchestration caps. Effective timeout is `min(policy_timeout, orchestration_cap)` with orchestration caps defined by operation class.

Rationale:
- Policy-derived timeouts preserve manager-specific tuning.
- Hard caps prevent unbounded waits and improve system safety.
- Operation-class caps let long-running operations differ from short control operations.

Consequences / Follow-on Work:
- COR-001 must replace any fixed timeout assumptions with policy-plus-cap evaluation.
- Timeout policy must be documented with operation classes and cap values.
- Regression tests must validate both below-cap and above-cap behavior.

Acceptance Criteria:
- A documented timeout-cap table exists for operation classes.
- Wait paths compute effective timeout as `min(policy_timeout, orchestration_cap)`.
- No hidden fixed cap remains in coordinator wait paths.
- Tests cover: policy < cap, policy > cap, and default-policy behavior.
- User-facing diagnostics include timeout class and relevant cap/policy context.

## DEC-003 - Workflow Action SHA Pinning Policy

Status: Resolved (2026-02-25)

Decision: Immutable GitHub Actions SHA pinning will be phased by workflow criticality. Phase 1 (release and security workflows) is immediate and pre-release required; phase 2 covers CI/test/lint workflows; phase 3 covers non-critical automation. Any new workflow must be SHA-pinned at creation.

Rationale:
- Immediate critical-path pinning reduces highest supply-chain risk first.
- Phasing lowers churn while preserving clear security progression.
- New unpinned workflows would reintroduce avoidable risk.

Consequences / Follow-on Work:
- BUILD-001 acceptance criteria now require phase-tracked rollout and phase-1 gating.
- Workflow maintenance docs must include pin rotation/update process.
- CI policy checks should enforce pinning for new workflows and critical modified workflows.

Acceptance Criteria:
- Phase list is documented and mapped to workflow files.
- Release and security workflows are SHA-pinned before release sign-off.
- New workflows are rejected if external actions are not pinned.
- Pin-rotation procedure is documented and repeatable.
- Existing workflow behavior remains green after pin updates.

## DEC-004 - Metadata Truth Branch Policy

Status: Resolved (2026-02-25)

Decision: Publish-ready metadata artifacts are authoritative on `main` and release branches only. `dev` does not carry publish-ready metadata artifacts. Preview/testing metadata must use separate preview channels/paths.

Rationale:
- Keeps publication truth tied to release-producing branches.
- Reduces metadata churn/noise on `dev`.
- Avoids mixing stable publication state with integration branch experiments.

Consequences / Follow-on Work:
- REL-001 and related drift checks must enforce branch-aware metadata expectations.
- Release docs and versioning docs must explicitly state branch truth policy.
- Preview workflows, if used, must write to dedicated preview metadata paths.

Acceptance Criteria:
- Docs explicitly define metadata truth branch policy.
- Stable drift checks validate publish metadata on `main`/release branches.
- `dev` workflows do not require stable publish artifacts.
- Preview metadata paths are separate from stable production paths.
- Release verification reflects the branch-aware policy.

## DEC-005 - Diagnostics Exposure and Redaction Policy

Status: Resolved (2026-02-25)

Decision: Diagnostics context is redacted by default in UI/CLI surfaces. Full diagnostics are available only via explicit user action (advanced/reveal/copy/export). Redaction is centralized and tested. Sensitive environment variables are never surfaced except for strict allowlisted keys.

Rationale:
- Protects users from accidental sensitive data disclosure in normal workflows.
- Keeps deep diagnostics available for support when intentionally requested.
- Centralized redaction reduces inconsistency across core/FFI/UI/CLI surfaces.

Consequences / Follow-on Work:
- SEC-003 and UX-001 become implementation work items with shared redaction contracts.
- Redaction logic must be centralized and reused across diagnostics surfaces.
- Test suites must cover token/header/credential and env-var leakage patterns.

Acceptance Criteria:
- Default diagnostics views redact secrets and sensitive environment context.
- Full-detail diagnostics require explicit user action.
- Environment variable exposure follows a strict allowlist model.
- Redaction tests cover auth headers, API tokens, license keys, and representative command output.
- Export and copy paths apply the same centralized redaction rules by default.
