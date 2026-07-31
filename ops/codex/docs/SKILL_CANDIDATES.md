# Skill Candidates

Generated (UTC): `2026-07-29T22:08:29Z`
Source log: `/Users/jasoncavinder/Projects/Helm/dev/logs/codex-runs.ndjson`

This report proposes reusable skill candidates from telemetry. It does **not** create skills automatically.

Approval-required drafting command:

```bash
ops/codex/scripts/draft-skill-from-candidate.sh <candidate-name>
```

## Top Candidates

| Rank | Candidate | Priority | Repeats | Components |
|---|---|---:|---:|---|
| 1 | `operating-lean-slash` | 7.33 | 2 | skills/docs-sync, skills/audit-remediation-batch, AGENTS.md |

## Candidate: operating-lean-slash

- Rank: 1
- Priority Score: `7.33`
- Repeated Entries: `2`
- Dominant Components: `skills/docs-sync, skills/audit-remediation-batch, AGENTS.md, core/rust`

Example summaries:
- Implemented codex operating model scaffold.
- Refined Codex operating model for lean context and slash commands.

### WORKFLOW SPEC

```text
WORKFLOW SPEC

Name:
operating-lean-slash

Purpose:
Standardize a repeated 'operating lean' workflow observed across skills/docs-sync, skills/audit-remediation-batch, AGENTS.md, core/rust. This candidate is based on 2 similar telemetry entries.

Inputs:
- task objective and expected outcome
- target scope/components (common: skills/docs-sync, skills/audit-remediation-batch, AGENTS.md, core/rust)

Outputs:
- concise execution summary
- touched component/file summary
- verification results and next-step recommendations

Steps:
1. Confirm the workflow scope, boundaries, and desired outcome.
2. Gather required context from dominant components (skills/docs-sync, skills/audit-remediation-batch, AGENTS.md, core/rust).
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
ops/codex/scripts/draft-skill-from-candidate.sh operating-lean-slash
```

