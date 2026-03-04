---
name: run-quality-gate
description: Deterministic local validation sweep for Rust, UI, i18n, and release contracts.
---

# Run Quality Gate

## Purpose

Run merge-readiness checks in a stable order and report exactly what failed.

## When this Skill should trigger

- PR-readiness checks
- CI-like local sweeps
- refactors touching multiple surfaces (`core/rust`, `apps/macos-ui`, release scripts)

## Inputs

- `scope`: `rust`, `i18n`, `ui`, `release-contracts`, or `all`
- optional env: `HELM_SKIP_XCODE=1` to skip `xcodebuild test`

## Outputs

- pass/fail status for requested scope
- first failing command (if any)
- short next-step recommendation

## Safety rules

- never hide or skip failing commands silently
- `release-contracts` remains non-destructive
- do not publish/tag/release from this skill

## Steps executed

1. Run `ops/codex/skills/run-quality-gate/scripts/run-quality-gate.sh <scope>`.
2. Execute checks in deterministic order for the selected scope.
3. Stop on first failure and report failure details.
4. Report success only when all selected checks pass.
