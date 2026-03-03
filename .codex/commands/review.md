# /review

Use this template to run a focused code review with actionable findings.

## Inputs

- scope (files/dirs)
- base/head refs or PR number
- risk focus (correctness, regressions, tests, security, release impact)

## Prompt Template

Review the requested change with a code-review mindset.

Scope: <scope>
Refs/PR: <base..head or PR>
Risk focus: <risk focus>

Required output format:
1. Findings (ordered by severity)
2. Open questions/assumptions
3. Suggested fixes
4. Verification gaps/tests to add

Review rules:
- prioritize bugs, behavioral regressions, missing tests, safety risks
- include file references and concrete evidence
- keep summary brief and findings-first
- if no findings, state that explicitly and list residual risk
