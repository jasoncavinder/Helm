# Crash and Error Reporting Policy

Date: 2026-02-26
Applies to: Helm `1.0` release gate and `<=1.0.x` patch line unless superseded by a later ADR.

## 1.0 Posture

- Helm does not ship automatic remote crash reporting for `1.0`.
- Helm remains local-first for failure data collection.
- Any diagnostic sharing remains user-initiated (for example, explicit CLI/UI diagnostics export attached to an issue).

This is an explicit "no background telemetry" policy, not an implementation gap.

## Data Model (Local-Only)

Helm diagnostics are generated from local task/runtime records and exported on demand.

Expected fields in local diagnostics payloads:

- metadata: `schema_version`, `generated_at`, app version/build/channel
- task context: `task_id`, `manager`, `task_type`, `status`, timestamps
- process context: `program_path`, bounded `PATH` snippet, `cwd`, exit metadata, timeout class
- failure context: categorized error kind/code and message
- output context: bounded stdout/stderr excerpts (subject to redaction policy)

These payloads stay on the local machine unless the user explicitly chooses to share them.

## Privacy Constraints

- No automatic upload of diagnostics, package inventory, or environment fingerprints.
- Redaction must run before diagnostics are persisted for long-term inspection/export where redaction hooks exist.
- Diagnostics UI/CLI surfaces must avoid exposing raw secrets (tokens, credentials, auth headers, keychain-like material).
- Any future remote reporting channel must be opt-in and documented via a new ADR before release.

## Operational Ownership

- Policy owner: Helm maintainer/repo owner.
- Operational owner: release operator on duty for each release (defaults to maintainer for current project phase).
- Enforcement points:
  - release checklist review
  - architecture/docs consistency checks
  - remediation audits for diagnostics/redaction behavior

## Change Control

Changes to this policy require:

1. ADR update in `docs/DECISIONS.md`
2. corresponding updates to `docs/ARCHITECTURE.md`
3. release checklist update if release gating changes
