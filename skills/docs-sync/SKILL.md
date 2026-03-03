---
name: docs-sync
description: Keep source-of-truth docs aligned with implemented behavior and release-line reality.
---

# Docs Sync

## Purpose

Prevent drift between implementation and Helm source-of-truth documentation.

## When this Skill should trigger

- behavior/policy/workflow changes in code or scripts
- release-line or version-marker changes
- architecture/decision updates requiring documented rationale

## Inputs

- changed files
- behavior delta summary
- release-line impact (`yes/no`)

## Outputs

- list of docs updated
- unresolved ambiguity list (if any)
- docs verification check result

## Safety rules

- prefer factual updates over speculative wording
- do not leave stale status/planning claims
- do not rewrite historical release facts

## Steps executed

1. Determine impacted docs (`CURRENT_STATE`, `NEXT_STEPS`, `DECISIONS`, `ARCHITECTURE` when needed).
2. Apply minimal updates that match implementation reality.
3. Run `skills/docs-sync/scripts/docs-sync-check.sh`.
4. Report final docs delta and any open questions.
