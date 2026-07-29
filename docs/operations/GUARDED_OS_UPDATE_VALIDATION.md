# Guarded OS Update Validation

Status: Active operational runbook  
Owner: Helm Release Engineering  
Last Updated: 2026-02-26

---

## 1. Purpose

This runbook defines the guarded OS update validation contract for high-risk update orchestration paths.

It exists to validate decision behavior for destructive-class operations without mutating production hosts.

Primary goals:

- codify required safety controls before any mutating path is allowed
- define deterministic decision outcomes and operator-observable signals
- provide a repeatable advisory validation lane for regression detection

---

## 2. Required Safety Controls

All guarded update validations must enforce the following controls:

1. Isolated target only:
   - run on disposable VM/test host
   - never run against primary workstation or production fleet host
2. Snapshot required before mutating path:
   - VM snapshot/restore point must exist and be recent
3. Explicit operator confirmation required:
   - mutating operations require an explicit opt-in signal
4. Rollback plan required:
   - restore procedure must be documented before starting validation
5. Abort conditions:
   - immediately halt when isolation, snapshot, or confirmation gates are not satisfied

---

## 3. Decision Matrix

| Scenario | Preconditions | Expected Status | Expected Action | Operator-Observable Signal |
|---|---|---|---|---|
| Read-only refresh validation | operation is non-mutating/read-only | `allowed` | `run` | `guarded_read_only_allowed` |
| Mutating path missing confirmation | mutating operation + isolation/snapshot OK + confirmation not granted | `needs_confirmation` | `halt` | `confirmation_required` |
| Mutating path missing isolation | mutating operation on non-isolated host | `denied` | `halt` | `isolation_required` |
| Mutating path missing snapshot | mutating operation without snapshot/restore point | `denied` | `halt` | `snapshot_required` |
| Mutating path fails after mutation phase | mutating operation allowed and started, then reports post-mutation failure | `rollback_required` | `rollback` | `mutation_failed_rollback_required` |
| Mutating path ready to run | mutating operation + isolation + snapshot + confirmation all satisfied | `allowed` | `run` | `guarded_mutation_allowed` |

---

## 4. Local Contract Invocation

Run deterministic non-destructive contract checks:

```bash
scripts/tests/guarded_os_update_contract.sh \
  --report-path artifacts/guarded-os-update/contract-report-local.json
```

Exit behavior:

- `0`: all scenarios matched expected contract outputs
- non-zero: one or more scenario assertions failed

---

## 5. CI Advisory Invocation

Workflow:

- `.github/workflows/guarded-os-update-contracts.yml`

Triggers:

- `workflow_dispatch`
- scheduled weekly run

Artifacts:

- `guarded-os-update-contract.log`
- `guarded-os-update-contract-report.json`

---

## 6. Triage and Rollback Guidance

If a guarded-update contract run fails:

1. Inspect `scenario_results` in the report and identify mismatched signal/action pairs.
2. Classify failure:
   - gate failure classification drift (isolation/snapshot/confirmation)
   - rollback requirement misclassification
   - report schema or contract-harness regression
3. For mutating-path regressions:
   - stop before any live mutation testing
   - re-validate snapshot/restore workflow in isolated environment
4. Re-run contract lane only after the decision-classification regression is fixed.

---

## 7. Safety Boundary

This validation lane must remain non-destructive:

- fixture-driven decision checks only
- no `softwareupdate` install/download mutation commands
- no system package mutations
- no release/tag/publication mutations
