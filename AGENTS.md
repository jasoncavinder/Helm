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
- `docs/I18N_STRATEGY.md` — localization requirements
- `docs/INTERFACES.md` - stable contracts between Helm subsystems
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
- `main`: stable/releasable; protected; merges come from `dev` (except hotfixes).
- `dev`: integration branch; protected; feature work merges here first.
- Feature branches: branch off `dev`, merge back to `dev` via PR:
  - `feat/...`, `fix/...`, `chore/...`, `docs/...`, `test/...`, `refactor/...`
- Hotfixes: branch off `main` as `hotfix/...`, then merge into both `main` and `dev`.

### Rules
- Do not commit directly to `main`.
- Exception: direct commits to `main` are allowed when explicitly instructed by the user/repo owner in the current session.
- Avoid committing directly to `dev` except trivial docs fixes; prefer PRs.
- Do not rewrite published history (no force-push) unless explicitly instructed.
- Prefer small, coherent commits.

### Commit messages
Use prefixes:
- `feat:`, `fix:`, `chore:`, `docs:`, `test:`, `refactor:`

### Before opening a PR
- Ensure relevant tests pass (e.g., `cargo test`).
- Keep PRs focused; split unrelated changes.

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

