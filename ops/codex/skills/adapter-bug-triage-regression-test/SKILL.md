---
name: adapter-bug-triage-regression-test
description: Triage manager adapter bugs and convert findings into deterministic regression coverage.
---

# Adapter Bug Triage and Regression Test

## Purpose

Isolate manager-specific defects quickly and pair each fix with targeted regression tests.

## When this Skill should trigger

- manager-specific failures in detect/list/search/install/uninstall/upgrade
- policy/provenance mismatches for a manager
- parser drift from upstream output changes

## Inputs

- manager ID
- failing command/log snippet
- expected behavior

## Outputs

- layer-level triage summary (parser/policy/runtime)
- recommended target tests
- regression-test plan tied to the fix

## Safety rules

- keep scope to the affected manager/layer
- do not rewrite unrelated adapters
- do not change policy defaults without docs updates

## Steps executed

1. Reproduce with the minimal failing path.
2. Run `ops/codex/skills/adapter-bug-triage-regression-test/scripts/manager-test-targets.sh <manager-id>`.
3. Identify failing layer and add/update deterministic regression tests.
4. Apply minimal fix and rerun targeted tests.
5. Escalate to broader gates only if blast radius expands.
