# AGENTS.md — Helm AI Development Guidelines

This file defines how AI agents (Codex, Copilot, Claude, etc.) should behave
when working in the Helm repository.

Helm is an infrastructure-grade macOS application. Code quality, safety,
and architectural clarity are more important than speed or shortcuts.

---

## 1. Source of Truth

- `PROJECT_BRIEF.md` is the authoritative product and architecture specification.
- If instructions in prompts conflict with `PROJECT_BRIEF.md`, the brief wins.
- If something is unclear or underspecified, pause and ask for clarification
  rather than making assumptions.

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
