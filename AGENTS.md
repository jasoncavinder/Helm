# AGENTS.md — Helm Codex Operating Guide (Lean)

This file defines the minimum operating policy for AI agents in Helm.

Keep this file policy-only. Put repeatable procedures in `ops/codex/skills/`.

## 1) Repo Overview

Helm is an infrastructure-grade macOS package/update control center with three layers:

- UI: `apps/macos-ui/` (SwiftUI presentation)
- Service boundary: `service/macos-service/` (process/control boundary)
- Core: `core/rust/` (adapters, orchestration, policy, persistence)

## 2) Component Map

Primary work areas:

- `core/rust/`: core logic, adapters, orchestration, CLI/FFI crates
- `apps/macos-ui/`: macOS UI, bridge code, localized resources, UI scripts
- `service/macos-service/`: background/runtime service boundary
- `scripts/`: release/test/ops automation
- `docs/`: product truth, architecture, decisions, current state, next steps
- `web/`: website/docs site content and build

## 3) Required Read Before Implementation

Read these before code changes:

- `docs/PROJECT_BRIEF.md`
- `docs/CURRENT_STATE.md`
- `docs/NEXT_STEPS.md`
- `docs/architecture/MANAGER_ELIGIBILITY_POLICY.md`

If behavior or policy changes, update the source-of-truth docs that changed reality.

## 4) Safety Rules (Non-Negotiable)

- Never construct shell commands via string concatenation in product code.
- Keep UI presentation-only; business logic belongs in service/core.
- Keep core deterministic and testable.
- Use structured process arguments and task-based execution.
- Respect authority ordering and cancellation expectations.
- Keep user-facing text localized; maintain locale parity between:
  - `locales/`
  - `apps/macos-ui/Helm/Resources/locales/`
- Do not touch secrets/signing/notarization credentials/certificates/profiles.
- Do not run destructive commands (for example `rm -rf`, branch deletion, cache purges) without explicit user approval.
- Do not auto-publish releases/appcasts/website deploys.
- Release and appcast work must remain dry-run/checklist-first unless user explicitly confirms mutation.

Branch safety:

- Never commit directly to long-lived branches: `main`, `dev`, `docs`, `web`.
- Verify branch first (`git branch --show-current`) before edits.
- If on a long-lived branch, create a task branch first.

Sandbox note for macOS tooling:

- If sandboxed `xcodebuild`/`simctl` output appears unreliable, re-run outside sandbox (with approval) before concluding host toolchain failure.

## 5) Skill Trigger Catalog

Use these existing Skills when triggers match:

- `run-quality-gate`
  - Trigger: PR-readiness, CI-like validation, cross-layer refactors.
- `audit-remediation-batch`
  - Trigger: remediation IDs/backlog batches, security/maintainability hardening.
- `sparkle-appcast-checklist`
  - Trigger: updater/appcast/release-readiness checks (dry-run by default).
- `adapter-bug-triage-regression-test`
  - Trigger: manager-specific defects, parser/provenance/policy drift.
- `docs-sync`
  - Trigger: behavior/policy/release-line changes needing source-of-truth doc alignment.
- `skill-generator`
  - Trigger: user asks to convert a repeated workflow/process into a reusable skill.
  - Required flow: workflow -> WORKFLOW SPEC -> user confirmation -> generated skill.

If auto-trigger is missed, users may explicitly request a skill by name.

## 6) When to Propose a New Skill

Propose creating a new skill when any of these conditions are true:

- workflow repeats 3+ times
- workflow is a multi-step checklist
- workflow depends on a fragile command sequence
- workflow is CI-like and repeated
- workflow is a release/packaging process with repeatable gates

When proposing:

- say why the workflow qualifies
- recommend using `skill-generator` to scaffold the new reusable skill
- offer to implement under `ops/codex/skills/<skill-name>/`
- keep AGENTS policy-only and move procedure details into skill docs/scripts

When Codex detects a repeated workflow or multi-step manual procedure that could be reused, it should recommend `skill-generator`.

Trigger signals include:

- repeated commands
- long checklists
- fragile workflows
- CI-like processes
- packaging/release flows

Candidate-mining policy:

- if `ops/codex/docs/SKILL_CANDIDATES.md` exists and is older than 7 days, recommend running `ops/codex/scripts/skill-mine.sh`
- when a task appears repetitive, check `ops/codex/docs/SKILL_CANDIDATES.md` before inventing a new skill

## 7) Notify / MCP / Apps / Multi-Agent Guidance

Notify:

- Recommend notify logging for long-running, multi-step, CI-like, or release-like tasks.
- Use `dev/logs/codex-runs.ndjson` to review repeated workflows and decide which should become skills.

MCP:

- Default to local-first.
- Recommend MCP only when required context is external and repeatedly blocks progress.
- Do not add MCP by default; propose it with rationale.

`/apps`:

Codex should recommend `/apps` when:

- a task depends on external services
- issue tracking or GitHub automation is needed
- a workflow would benefit from integration tooling

Use app integrations only when they reduce manual burden safely.

Multi-agent:

Recommend multi-agent execution when tasks involve:

- large codebase exploration
- refactors across many modules
- implementation + testing + review as distinct lanes
- audit remediation batches

Suggested role pattern:

- Explorer
- Implementer
- Tester
- Reviewer

## 8) Working Style

- Be explicit, deterministic, and minimal.
- Prefer targeted verification before broad sweeps.
- If uncertain, ask instead of guessing.
- Keep docs, code, and tests consistent.
