# Skill Candidates

Generated (UTC): `2026-03-03T20:47:44Z`
Source log: `/Users/jasoncavinder/Projects/Helm/dev/logs/codex-runs.ndjson`

This report proposes reusable skill candidates from telemetry. It does **not** create skills automatically.

Approval-required drafting command:

```bash
ops/codex/scripts/draft-skill-from-candidate.sh <candidate-name>
```

## Top Candidates

| Rank | Candidate | Priority | Repeats | Components |
|---|---|---:|---:|---|
| 1 | `operating-lean-commands` | 9.32 | 2 | core/rust, apps/macos-ui, dev/logs |

## Candidate: operating-lean-commands

- Rank: 1
- Priority Score: `9.32`
- Repeated Entries: `2`
- Dominant Components: `core/rust, apps/macos-ui, dev/logs, ops/codex/skills/audit-remediation-batch`

Example summaries:
- Implemented codex operating model scaffold.
- Refined Codex operating model for lean context and slash commands.

### WORKFLOW SPEC

```text
WORKFLOW SPEC

Name:
operating-lean-commands

Purpose:
Standardize a repeated 'operating lean' workflow observed across core/rust, apps/macos-ui, dev/logs, ops/codex/skills/audit-remediation-batch. This candidate is based on 2 similar telemetry entries.

Inputs:
- task objective and expected outcome
- target scope/components (common: core/rust, apps/macos-ui, dev/logs, ops/codex/skills/audit-remediation-batch)

Outputs:
- concise execution summary
- touched component/file summary
- verification results and next-step recommendations

Steps:
1. Confirm the workflow scope, boundaries, and desired outcome.
2. Gather required context from dominant components (core/rust, apps/macos-ui, dev/logs, ops/codex/skills/audit-remediation-batch).
3. Execute the recurring core actions inferred from similar summaries.
4. Run targeted validation aligned to impacted components.
5. Record outcomes and capture concise telemetry-style summary.

Safety Constraints:
- no secrets, credentials, signing keys, or provisioning material
- no automatic release/appcast/website publication
- no destructive operations without explicit confirmation

Optional Scripts:
no

Suggested Split:
No split suggested (<=25 inferred steps).
```

Draft command (approval required):

```bash
ops/codex/scripts/draft-skill-from-candidate.sh operating-lean-commands
```

