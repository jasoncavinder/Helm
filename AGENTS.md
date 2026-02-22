# AGENTS.md — Helm AI Development Guidelines

This file defines how AI agents (Codex, Copilot, Claude, etc.) should behave
when working in the Helm repository.

Helm is an infrastructure-grade macOS application. Code quality, safety,
and architectural clarity are more important than speed or shortcuts.

---

## 1. Source of Truth

Helm's behavior is defined by structured documentation. AI agents must consult these documents 
before making changes.

### Required Reading (Always Load First)

- `docs/PROJECT_BRIEF.md` — authoritative product and architecture specification
- `docs/CURRENT_STATE.md` — current implementation status and known gaps
- `docs/NEXT_STEPS.md` — prioritized upcoming work

Agents must not begin implementation until these documents have been read.

---

### Supporting Documents (Load When Relevant)

- `docs/ARCHITECTURE.md` — canonical system design and invariants
- `docs/ROADMAP.md` — milestone sequencing and feature planning
- `docs/DECISIONS.md` — architectural decisions (ADR-style)
- `docs/DEFINITION_OF_DONE.md` — 1.0 release criteria
- `docs/VERSIONING.md` — semantic versioning strategy and release flow
- `docs/RELEASE_CHECKLIST.md` — required ship checklist and tag steps
- `docs/I18N_STRATEGY.md` — localization requirements
- `docs/INTERFACES.md` — stable contracts between Helm subsystems
- `docs/legal/CLA.md` — contributor license agreement

---

### Precedence Rules

If conflicts occur:

1. `docs/PROJECT_BRIEF.md` (product truth)
2. `docs/ARCHITECTURE.md` (system invariants)
3. `docs/CURRENT_STATE.md` (reality)
4. `docs/DECISIONS.md` (decisions override plans)
5. `docs/ROADMAP.md` (intent)

If unclear:
- Ask for clarification instead of guessing.

---

## 2. Architectural Principles (Non-Negotiable)

### 2.1 Layered Architecture
Helm consists of three distinct layers:

1. **UI (SwiftUI)**
   - Pure presentation.
   - No business logic.
   - Reads state and emits intents.

2. **Service Boundary (macOS background service / XPC-style)**
   - Owns process execution.
   - Handles privilege escalation.
   - Enforces cancellation and exclusivity.

3. **Core (Rust)**
   - Manager adapters.
   - Task orchestration.
   - Parsing and normalization.
   - Persistence API (SQLite).

Do not collapse layers or bypass boundaries for convenience.

---

### 2.2 Rust Core Expectations

The Rust core must:
- Be UI-agnostic.
- Avoid shell invocation strings.
- Use structured command arguments.
- Be deterministic and testable.
- Avoid global mutable state.

Adapters must declare:
- Supported capabilities.
- Authority level.
- Whether actions are mutating or read-only.

---

## 2.3 Architectural Invariants (Non-Negotiable)

The following rules must never be violated:

- No shell command construction via string concatenation
- UI layer contains no business logic
- Rust core is deterministic and testable
- All operations execute through tasks
- Authority ordering is respected
- Tasks must be cancelable at process level
- All user-facing text must be localized (no hardcoded strings)
- Locale files exist in two synced directories: `locales/` (canonical source) and `apps/macos-ui/Helm/Resources/locales/` (app resource mirror). CI enforces parity via `diff -ru`. Always update both when changing locale strings.

If a change would violate these, stop and ask.

---

## 3. Manager Adapter Model

Each package manager is implemented as an adapter module.

Adapters must:
- Implement a shared trait/interface.
- Declare supported capabilities explicitly.
- Gracefully handle missing or malformed output.
- Never assume human-readable output is stable.

Adapters must NOT:
- Assume exclusive ownership of a toolchain.
- Modify project-local dependencies by default.
- Perform bulk operations unless explicitly required.

---

## 4. Task & Concurrency Model

- Tasks across different managers may run in parallel.
- Tasks using the same manager MUST run serially.
- Tasks must be cancelable at the process level.
- Cancellation should be cooperative where possible.

Task types include:
- detection
- refresh
- search
- install
- uninstall
- upgrade
- pin / unpin

Never fake completion or cancellation.

---

## 5. Search Behavior (Critical UX Requirement)

Search must be:
- Local-first.
- Progressive.
- Cancelable.

Rules:
- Local cache search returns instantly.
- Remote search is automatically triggered after a debounce.
- Remote searches must be cancellable when the query changes.
- Remote results enrich the cache and UI incrementally.
- Cached results must record:
  - source manager
  - originating query
  - timestamp

---

## 6. Package Pinning Rules

- Prefer native manager pinning when available.
- Fall back to virtual pinning enforced by Helm.
- Pinned packages:
  - Are excluded from bulk upgrades.
  - Are excluded from automatic mode unless explicitly overridden.
- Pin state must be visible and persisted.

---

## 7. Safety & Error Handling

- Never construct shell commands via string concatenation.
- Always use structured process arguments.
- Timeouts and retries must be reasonable and configurable.
- Errors must be attributed to:
  - manager
  - task
  - action
- Prefer explicit errors over silent failure.

---

## 8. Persistence

- SQLite is the canonical store for state and cache.
- Schema must be versioned.
- Migrations must be explicit and reversible where possible.
- Avoid duplicating derived state that can be recomputed cheaply.

---

## 9. Logging

- Logs must be structured and contextual.
- Include manager name, task ID, and action.
- Logging must not block the UI.

---

## 10. Testing Expectations

- Parsing logic must have unit tests with fixed fixtures.
- Adapter behavior must be unit tested.
- Orchestration logic must have integration tests.
- Tests should favor determinism over realism.

---

## 11. Scope Discipline

AI agents must:
- Work incrementally.
- Commit small, coherent changes.
- Avoid speculative features not described in the brief.
- Ask before introducing new dependencies or frameworks.

AI agents must NOT:
- Rewrite large portions of code without request.
- Introduce UI polish prematurely.
- Add telemetry, analytics, or network services.

### 11.1 Generated Artifacts

- `apps/macos-ui/Generated/HelmVersion.xcconfig` is build-generated (updated by the macOS build flow/scripts).
- Treat changes to this file as incidental build output unless the task is explicitly about version/build metadata generation.
- Do not include this file in feature/fix commits by default.
- If this file changes unexpectedly during work:
  - leave it unstaged, or
  - ask the user before including it in a commit.

---

## 12. Communication Style

When uncertain:
- Ask clarifying questions.
- Explain tradeoffs briefly.
- Prefer correctness over cleverness.

When implementing:
- Be explicit.
- Be boring.
- Be predictable.

Helm is infrastructure. Treat it like one.

---

## 13. Git Workflow (Required)

### Branch model
- `main`: stable/releasable branch; protected.
- `dev`: code integration branch; protected; app/core/runtime work merges here first.
- `docs`: documentation integration branch; protected; docs/policy/licensing docs work merges here first.
- `web`: website integration branch; protected; website work under `web/` merges here first.
- Feature branches:
  - code work: branch from `dev`, open PRs to `dev` (`feat/...`, `fix/...`, `chore/...`, `test/...`, `refactor/...`)
  - docs work: branch from `docs`, open PRs to `docs` (`docs-*`)
  - website work: branch from `web`, open PRs to `web` (`web-*`)
- Hotfixes: branch from `main` as `hotfix/...`, merge to `main`, then back-merge/cherry-pick to each impacted integration branch (`dev`, `docs`, `web` as applicable).

### Rules
- Do not commit directly to long-lived branches: `main`, `dev`, `docs`, `web`.
- Exception: direct commits to `main` are allowed when explicitly instructed by the user/repo owner in the current session.
- Prefer PR-based updates for all long-lived branches.
- Do not rewrite published history (no force-push) unless explicitly instructed.
- Prefer small, coherent commits.

### GitHub Enforcement Model (Rulesets + Checks)

GitHub branch rulesets enforce the branch model. Agents must assume these are active and design PRs accordingly.

- `main` required checks:
  - `Policy Gate`
  - `Rust Core Tests`
  - `Xcode Build Check`
  - `hardcoded-ui-strings`
  - `Semgrep scan`
  - `Lint Swift`
- `dev` required checks:
  - `Policy Gate`
  - `Rust Core Tests`
  - `Xcode Build Check`
  - `hardcoded-ui-strings`
  - `Semgrep scan`
  - `Lint Swift`
- `docs` required checks:
  - `Policy Gate`
  - `Docs Checks`
- `web` required checks:
  - `Policy Gate`
  - `Web Build`

Operational repo settings:
- Auto-merge is enabled.
- Update-branch is enabled.
- Delete branch on merge is disabled (protects primary branches from accidental deletion during promotion PRs).

Agent expectations:
- Treat `Policy Gate` as authoritative for branch/PR target policy.
- Do not rely on direct-push fallback behavior for release metadata publication.
- Release metadata publication to `main` is PR-based via `chore/publish-updates-<tag>`.

### Commit messages
Use prefixes:
- `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`

### Before opening a PR
- Ensure relevant tests pass (e.g., `cargo test`).
- Keep PRs focused; split unrelated changes.

### Branch Targeting Rules

Helm uses different integration branches depending on change type.

#### Code Changes
- Branch from `dev`
- Open PRs targeting `dev`

#### Documentation-Only Changes
- Use `docs-*` branches
- Branch from `docs`
- Open PRs targeting `docs`

#### Website-Only Changes
- Use `web-*` branches
- Branch from `web`
- Open PRs targeting `web`

Documentation-only changes include:
- README updates
- docs/ directory changes
- licensing, CLA, or policy updates

If a change includes both code and documentation:
- Prefer merging into `dev`
- Or split into separate PRs if appropriate

If a change includes both documentation and website updates (without app code):
- Prefer split PRs (`docs` and `web`) unless explicitly instructed to combine.

Promotion to stable `main`:
- App releases: merge `dev` -> `main`
- Docs publishes: merge `docs` -> `main`
- Website publishes: merge `web` -> `main`

Agents must select the correct base branch before starting work.
If unsure, ask for clarification.

---

## 14. Licensing Constraints

Helm is not open source at this stage.

AI agents must not:
- suggest licensing changes without explicit instruction
- introduce third-party code incompatible with a source-available commercial future
- assume MIT/Apache-style reuse permissions

All contributions are subject to the CLA.

---

## 15. Context Management (Critical)

Helm uses repository documents as persistent memory for AI agents.

Documentation is the system of record.

---

### Required Behavior

When making changes, agents MUST update documentation when relevant:

- `docs/CURRENT_STATE.md` — reflect actual implementation
- `docs/NEXT_STEPS.md` — update priorities and completed work
- `docs/DECISIONS.md` — record architectural decisions
- `docs/ARCHITECTURE.md` — only when system design changes

---

### Consistency Rule

Code, tests, and documentation must remain consistent.

If inconsistencies are found:

1. Prefer updating documentation to match reality
2. If unsure, pause and ask for clarification

---

### Forbidden

- Do not leave documentation stale
- Do not implement features not reflected in NEXT_STEPS or ROADMAP without approval

---

## 16. Worktrees & Multi-Agent Coordination (Critical)

Helm supports multiple AI agents working concurrently using Git worktrees.

Each agent operates in a separate working directory with its own branch.

### 16.1 Worktree Isolation

Agents MUST:
- Only operate within the current working directory
- Never assume control of other worktrees
- Never modify files outside their worktree

Each worktree corresponds to a single agent session.

---

### 16.2 Branch Ownership

Each agent is responsible for its own branch.

Recommended naming:

- `agent/codex/...`
- `agent/claude/...`
- `agent/gemini/...`

Agents MUST NOT:
- Commit to another agent's branch
- Reuse another agent's branch without explicit instruction

---

### 16.3 Synchronization with Base Branch

Before starting work, agents MUST:

1. Fetch latest changes:
   ```bash
   git fetch origin
````

2. Update their branch:

   Preferred default for published/shared branches (no history rewrite):
   ```bash
   # code branches
   git merge origin/dev
   ```

   ```bash
   # documentation branches
   git merge origin/docs
   ```

   ```bash
   # website branches
   git merge origin/web
   ```

   ```bash
   # hotfix/release branches based on stable
   git merge origin/main
   ```

   Rebase is allowed only when safe (typically before first push):

   ```bash
   # code branches
   git rebase origin/dev
   ```

   ```bash
   # documentation branches
   git rebase origin/docs
   ```

   ```bash
   # website branches
   git rebase origin/web
   ```

   ```bash
   # hotfix/release branches based on stable
   git rebase origin/main
   ```

Agents must ensure their branch is up-to-date before making changes.
Agents must not force-push rebased history unless explicitly instructed by the repo owner in the current session.

---

### 16.4 Task Isolation

Agents should avoid modifying the same files concurrently.

If a change requires touching shared or high-risk files:

* Prefer coordination via documentation
* Or ask for clarification

Large overlapping edits increase merge conflict risk.

---

### 16.5 Commit Discipline

Agents MUST:

* Make small, focused commits
* Avoid bundling unrelated changes
* Use clear commit messages

Agents SHOULD:

* Commit frequently during long tasks

---

### 16.6 Pull Requests

Agents should:

* Push their branch to origin
* Open a PR targeting the correct base branch (`dev`, `docs`, `web`, or `main`)
* Keep PRs small and focused

---

### 16.7 Safety Rule

If uncertain about:

* branch selection
* merge target
* overlapping work

Agents must pause and ask instead of guessing.

---

### 16.8 Shared State Coordination

Agents should consult:

- `docs/NEXT_STEPS.md` for task prioritization
- `docs/CURRENT_STATE.md` for current implementation
- `docs/DECISIONS.md` for architectural constraints

Agents should not duplicate work already described as in-progress or completed.
