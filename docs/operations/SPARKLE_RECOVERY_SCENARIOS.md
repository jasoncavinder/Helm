# Sparkle Recovery Scenarios

Status: Active test contract guide  
Owner: Helm Release Engineering  
Last Updated: 2026-02-26

---

## 1. Scope

This document defines deterministic recovery scenarios for Sparkle update interruption handling.

It is a release-validation contract, not a runtime mutation workflow.

Primary goals:

- define expected recovery decisions for interruption/error paths
- define observable signals for operator triage
- provide fixture-backed contract scenarios for CI/manual advisory runs

---

## 2. Inputs and Signals

Contract decision inputs:

- latest appcast top-item version (`sparkle:shortVersionString`)
- persisted updater state snapshot (`target_version`, `phase`, `interrupted`, `last_error`)

Contract outputs:

- `STATUS`
- `ACTION`
- `SIGNAL`
- `TARGET_VERSION`
- `APPCAST_VERSION`

Signal purpose:

- `SIGNAL` keys are stable machine-readable triage hints for operators and CI artifacts.

---

## 3. Recovery Matrix

| Scenario | Preconditions | Expected Status | Expected Action | Operator-Observable Signal |
|---|---|---|---|---|
| Interrupted download | `phase=download`, `interrupted=true`, appcast target matches state target | `recoverable` | `retry_download` | `interrupted_download_recoverable` |
| Interrupted apply | `phase=apply`, `interrupted=true`, appcast target matches state target | `recoverable` | `retry_apply` | `interrupted_apply_recoverable` |
| Stale appcast after interruption | `interrupted=true`, appcast target does not match state target | `needs_appcast_refresh` | `refresh_then_retry_download` or `refresh_then_retry_apply` | `stale_appcast_after_interrupted_download` or `stale_appcast_after_interrupted_apply` |
| Invalid appcast metadata | missing/invalid `sparkle:shortVersionString` in top item | `invalid` | `halt` | `invalid_appcast_metadata` |
| Invalid update metadata/signature error | `last_error=invalid_metadata` or `signature_mismatch` | `manual_review` | `halt` | `invalid_update_metadata` |

---

## 4. Fixture Contract Coverage

Fixture-backed contract scripts:

- decision script: `scripts/release/sparkle_recovery_decision.sh`
- contract suite: `scripts/release/tests/sparkle_recovery_contract.sh`
- fixtures: `scripts/release/tests/fixtures/sparkle/`

Current deterministic scenarios covered:

- interrupted download (recoverable)
- interrupted apply (recoverable)
- stale appcast with interrupted download
- invalid appcast metadata
- invalid metadata/signature-style state error

---

## 5. Safety Boundary

This contract suite must remain non-mutating:

- no Sparkle network calls
- no application bundle writes
- no release metadata publication
- no tag/release mutations

All scenarios are fixture-driven and hermetic.
