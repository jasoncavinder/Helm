# How Jason Should Use Codex in Helm

Date: 2026-03-03

## 1) Start Sessions (Repo-Local)

Use the repo launcher so Codex uses this repository's config and skills:

```bash
ops/codex/scripts/session.sh
```

For scoped requests, include target area and required verification:

```bash
ops/codex/scripts/session.sh "Implement <change> in core/rust; run targeted tests; update source-of-truth docs if behavior changed."
```

## 2) Skill-First Workflow

Use a skill when the task matches a known repeatable flow:

- `run-quality-gate`
- `audit-remediation-batch`
- `sparkle-appcast-checklist`
- `adapter-bug-triage-regression-test`
- `docs-sync`
- `skill-generator`

Prefer skill execution over retyping long procedures in prompts.

## 3) Explicitly Invoking Skills

If Codex does not auto-trigger the right skill, invoke it explicitly:

```text
Use skill: run-quality-gate with scope=rust.
Use skill: sparkle-appcast-checklist in dry-run mode for tag vX.Y.Z-rc.N.
Use skill: docs-sync after these changes.
```

You can also ask:

```text
Auto-trigger missed. Execute this using skill <skill-name> and report outputs.
```

## 4) When Codex Should Propose a New Skill

Codex should recommend converting a workflow into a skill when it is:

- repeated 3+ times
- a multi-step checklist
- a fragile command sequence
- CI-like and frequently repeated
- a release or packaging process

Prompt template:

```text
This flow repeats. Convert it into a skill under ops/codex/skills/<name>/ with SKILL.md and scripts when helpful. Keep release-like paths dry-run/checklist-first.
```

## 5) Creating New Skills

When a workflow should become reusable, tell Codex directly:

```text
Convert this workflow into a skill.
Make this workflow reusable.
Create a skill for this process.
We should automate this task.
```

Codex should use `skill-generator` to scaffold a new skill under `ops/codex/skills/<skill-name>/` with a generated `SKILL.md` and optional `scripts/`/`resources/`.

Required generation flow:

1. extract workflow steps from the conversation,
2. produce a `WORKFLOW SPEC`,
3. show the spec for user confirmation,
4. generate the skill from the confirmed spec.

`WORKFLOW SPEC` format:

```text
WORKFLOW SPEC

Name:
<kebab-case skill name>

Purpose:
One or two sentence description.

Inputs:
List required inputs.

Outputs:
List outputs.

Steps:
Ordered list of workflow steps.

Safety Constraints:
Important safety rules.

Optional Scripts:
Whether scripts are needed.
```

Example explicit request:

```text
Use skill: skill-generator to create a reusable skill for this workflow, then update ops/codex/docs/USAGE.md with how to invoke it.
```

Script usage:

```bash
ops/codex/skills/skill-generator/scripts/create_skill.sh --init-spec /tmp/new-skill-spec.md
ops/codex/skills/skill-generator/scripts/create_skill.sh --spec /tmp/new-skill-spec.md --confirm-name
```

Safety behavior:

- skill generation is refused when spec content includes secret handling,
- release/appcast/website publication automation,
- destructive operations without explicit confirmation safeguards.

## 6) From candidate -> new skill

Run telemetry mining first:

```bash
ops/codex/scripts/skill-mine.sh
```

Then draft an approved candidate:

```bash
ops/codex/scripts/draft-skill-from-candidate.sh <candidate-name>
```

What this does:

1. extracts the candidate `WORKFLOW SPEC` from `ops/codex/docs/SKILL_CANDIDATES.md`,
2. calls the existing `skill-generator` flow to draft `ops/codex/skills/<candidate-name>/`,
3. refuses if candidate is missing,
4. refuses to overwrite existing skills.

## 7) Slash-Command Ergonomics

Reusable prompt templates are stored in:

- `.codex/commands/review.md`
- `.codex/commands/triage.md`
- `.codex/commands/release-check.md`

Usage pattern:

1. choose the closest command template,
2. paste/fill the input fields,
3. run as your prompt.

Suggested aliases in conversation:

- `/review` -> structured findings-first review
- `/triage` -> reproducible bug triage plan
- `/release-check` -> non-destructive release readiness checklist

## 8) Notify Logging

Notify hook writes one structured line per completed turn:

- file: `dev/logs/codex-runs.ndjson`
- schema includes timestamp, branch, working directory, changed files, summary

Use it to:

- inspect long or multi-step session history
- identify repeated workflows to convert into skills
- audit what changed during CI-like local runs

## 9) Using Codex Apps

Current in-session discovery:

- App connector surfaced: GitHub tools
- Installed app accounts returned: none
- Installations returned: none

Use apps when tasks need external integration context, especially:

- PR/issue metadata and discussion history
- commit/branch comparison context
- repo inventory automation

Representative GitHub tools:

- `github_search_prs`
- `github_get_pr_info`
- `github_fetch_pr_patch`
- `github_fetch_pr_comments`
- `github_compare_commits`

If discovery is unavailable in your environment:

1. run `/apps`,
2. paste the result into this section,
3. map each app to allowed Helm workflows before relying on it.

## 10) When to Use MCP

Default is local-first and no MCP.

Recommend MCP only when:

- required data is external and cannot be safely mirrored locally,
- the same external lookup repeatedly blocks progress.

## 11) Multi-Agent Guidance

Use multi-agent mode when work naturally splits into lanes:

- large codebase exploration,
- refactors across many modules,
- implement + test + review workflows,
- remediation batches.

Suggested role pattern:

1. Explorer: map files/risk and propose exact edit plan.
2. Implementer: apply scoped code/docs changes.
3. Tester: run targeted validation and report failures.
4. Reviewer: findings-first regression/safety review.

Avoid multi-agent mode for small single-file changes.

## 12) Safety Defaults

- Keep release/appcast workflows dry-run/checklist-first.
- Require explicit confirmation before any publish/mutation step.
- Keep tasks scoped by component and verification outcome.
