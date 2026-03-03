---
name: audit-remediation-batch
description: Execute small remediation batches from the audit backlog with verification evidence.
---

# Audit Remediation Batch

## Purpose

Turn remediation backlog items into small, verified, low-risk changes.

## When this Skill should trigger

- remediation IDs are provided (`SEC-*`, `COR-*`, `TEST-*`, `MNT-*`, `REL-*`)
- backlog progress/reporting is requested
- security or maintainability hardening is requested in batches

## Inputs

- remediation IDs (optional)
- risk envelope preference (`S`/`M` effort)

## Outputs

- status table for selected IDs
- per-ID completion state and verification notes
- remediation-log update checklist

## Safety rules

- do not mark items complete without executable verification evidence
- do not bundle unrelated IDs into one diff
- keep changes minimal and scoped to selected IDs

## Steps executed

1. Run `skills/audit-remediation-batch/scripts/remediation-backlog-status.py [ID ...]`.
2. Select a coherent small batch by area/dependency.
3. Implement minimal fixes and run targeted tests.
4. Update `docs/audits/remediation-log.md` and backlog status lines.
5. Report completed items and remaining blockers.
