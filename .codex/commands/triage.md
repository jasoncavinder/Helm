# /triage

Use this template to triage one bug report into a reproducible, testable plan.

## Inputs

- component (`core/rust`, `apps/macos-ui`, `service/macos-service`, `web`)
- repro steps
- expected behavior
- actual behavior
- logs/errors
- suspected manager/adapter (if relevant)

## Prompt Template

Triage this bug into a minimal reproducible path and deterministic fix plan.

Component: <component>
Repro: <steps>
Expected: <expected>
Actual: <actual>
Logs: <logs>
Context: <additional context>

Required output format:
1. Reproduction quality (confirmed/not confirmed + blockers)
2. Likely failure layer (parser/policy/runtime/UI/service)
3. Minimal failing test to add first
4. Smallest safe fix strategy
5. Verification commands (targeted first)
6. Docs/decision updates required
