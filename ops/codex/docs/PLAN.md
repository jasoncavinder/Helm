# Codex Operating System Plan (Helm)

Date: 2026-03-03
Scope: targeted optimization of the existing repo-local Codex system for lean context, higher skill usability, better operator ergonomics, and safer observability.

## 1) Repository Summary (Short)

- `core/rust/`: adapters, orchestration, policy, persistence, CLI/FFI crates.
- `apps/macos-ui/`: SwiftUI UI + bridge logic + localization resources/scripts.
- `service/macos-service/`: execution boundary/service runtime.
- `scripts/`: tests, release contracts, runbooks.
- `docs/`: source-of-truth planning and architecture.
- `web/`: website/docs publishing surface.

## 2) Context Optimization

### 2.1 Lean project-doc budget

Repo-local Codex config now uses a lean default:

- `project_doc_max_bytes = 131072`

Applied globally and for the Helm project stanza in `.codex/config.toml`.

### 2.2 Instruction layering

- Root `AGENTS.md` is now policy-only and compact.
- Procedural workflows are moved to `ops/codex/skills/` or `ops/codex/docs/USAGE.md`.
- Subtree AGENTS remain scoped by component (`core/rust`, `apps/macos-ui`, `web`).

## 3) Skill System (Optimized)

### 3.1 Active skills

- `run-quality-gate`
- `audit-remediation-batch`
- `sparkle-appcast-checklist`
- `adapter-bug-triage-regression-test`
- `docs-sync`

### 3.2 Skill interface contract

Each `SKILL.md` now follows the same sections:

- Purpose
- When this Skill should trigger
- Inputs
- Outputs
- Safety rules
- Steps executed

### 3.3 Promotion triggers for new skills

Codex should propose creating a new skill when a workflow is:

- repeated 3+ times
- a multi-step checklist
- a fragile command sequence
- CI-like and repeated
- release/packaging related

## 4) Slash-Command Ergonomics

Repo-local command templates live in:

- `.codex/commands/review.md`
- `.codex/commands/triage.md`
- `.codex/commands/release-check.md`

These provide reusable structured prompt templates for repeated workflows without expanding `AGENTS.md`.

## 5) Notify and Observability

### 5.1 Hook and log target

- Hook: `agent-turn-complete`
- Script: `ops/codex/scripts/notify-turn-complete.sh`
- Log: `dev/logs/codex-runs.ndjson`

### 5.2 Log schema (structured)

Each record includes:

- `timestamp`
- `git_branch`
- `working_directory`
- `changed_files`
- `summary`

Additional fields (`event`, `repo_root`, `git_head`, counts) are included for diagnostics.

### 5.3 Using logs to identify new skills

Use NDJSON logs to detect repeated patterns:

1. filter by similar summary text/workflow keywords,
2. count repeated sequences over recent sessions,
3. if threshold hits (3+), convert to a skill,
4. update `ops/codex/docs/USAGE.md` with invocation guidance.

## 6) `/apps` Integration

### 6.1 Current discovery in this session

Programmatic app discovery surfaced GitHub integration tools only.

Observed state:

- available connector: GitHub app tools
- installed app accounts: none returned
- installations: none returned

### 6.2 Usage guidance

Use `/apps` recommendations primarily for:

- PR triage and diff analysis
- issue/PR context retrieval
- compare-commits and repo metadata workflows

If additional apps are installed later, re-run `/apps` and extend the task-to-app matrix in `ops/codex/docs/USAGE.md`.

## 7) MCP Strategy

Default: defer MCP additions.

Recommend MCP only when external context is required repeatedly and local-first workflows are insufficient.

## 8) Multi-Agent Guidance

Recommend multi-agent for:

- large exploration tasks
- broad refactors
- implementation + test + review lanes
- remediation batches

Suggested role pattern:

- Explorer
- Implementer
- Tester
- Reviewer

## 9) Phased Rollout

### Implemented now

- lean `project_doc_max_bytes`
- compact policy-only root `AGENTS.md`
- normalized skill interface across existing skills
- slash-command templates under `.codex/commands/`
- notify schema refinement and run-log guidance
- usage docs refresh for explicit skill invocation, `/apps`, and multi-agent pattern

### Defer

- new MCP servers (only when recurring external burden is proven)
- additional skills based on observed run-log frequency

## 10) Optional: GitHub Action (Weekly Skill Candidate Report)

Default posture: **defer** (not enabled by default).

Safe optional automation can regenerate `ops/codex/docs/SKILL_CANDIDATES.md` weekly without creating skills.

Example workflow YAML (optional, not installed by default):

```yaml
name: Weekly Skill Candidate Report

on:
  schedule:
    - cron: "0 14 * * 1"
  workflow_dispatch: {}

jobs:
  skill-candidates:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Generate candidate report
        run: ops/codex/scripts/skill-mine.sh
      - name: Upload candidate report artifact
        uses: actions/upload-artifact@v4
        with:
          name: skill-candidates
          path: ops/codex/docs/SKILL_CANDIDATES.md
```

Notes:

- This optional workflow only regenerates candidate documentation.
- It must not create skills automatically.
- It requires no custom secrets for artifact-only mode.
