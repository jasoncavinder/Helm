---
name: sparkle-appcast-checklist
description: Dry-run Sparkle/appcast readiness checks with explicit manual follow-up guidance.
---

# Sparkle Appcast Checklist

## Purpose

Validate updater/appcast readiness without performing release publication actions.

## When this Skill should trigger

- `appcast.xml` or updater policy changes
- release readiness checks for direct-channel builds
- Sparkle recovery/contract validation requests

## Inputs

- optional release tag (`vX.Y.Z` or `vX.Y.Z-rc.N`)
- optional env: `HELM_ENABLE_REHEARSAL=1` to run full rehearsal

## Outputs

- checklist pass/fail status
- blocking checks (if any)
- explicit manual next steps for publish workflows

## Safety rules

- default to dry-run/checklist mode
- never create/publish releases from this skill
- never mutate appcast metadata from this skill
- require explicit user confirmation for any release mutation path

## Steps executed

1. Run `ops/codex/skills/sparkle-appcast-checklist/scripts/run-checklist.sh [tag]`.
2. Execute policy/contract checks in non-destructive mode.
3. Optionally run full rehearsal only when explicitly enabled.
4. Return checklist outcome and manual follow-up steps.
