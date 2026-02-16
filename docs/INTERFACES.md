# Interfaces & Contracts

This document defines **stable contracts** between Helm subsystems.

It exists to prevent “accidental coupling” across:
- SwiftUI UI
- XPC service boundary
- Rust core
- Adapter implementations
- Persistence (SQLite)

This is a **contract document**, not a tutorial. If implementation disagrees, either:
1) update the implementation to comply, or
2) record an explicit change in `docs/DECISIONS.md` and update this document.

---

## 0. Contract Status

- Pre-1.0: interfaces may evolve, but changes must be intentional and documented.
- Approaching 1.0: interfaces should converge and become “change-averse.”

---

## 1. System Boundaries

Helm consists of three layers:

1) UI (SwiftUI) — presentation only  
2) Service (XPC) — process boundary + execution host  
3) Core (Rust) — business logic, orchestration, persistence  

**Invariant:** UI never executes system commands. All execution flows through Service → Core.

---

## 2. UI ↔ Service Contract (XPC)

### 2.1 Principles

- XPC APIs are **typed** and versionable.
- UI sends **intents**, service returns **state snapshots** and **task handles**.
- Service must remain resilient: reconnectable and safe under partial failure.

### 2.2 XPC API Shape (Normative)

The XPC layer must support, at minimum:

#### State / Data
- Get managers (installed/enabled/version/capabilities)
- Get packages (installed/outdated/search results)
- Get pins
- Get app settings (safe mode, language override, etc.)
- Get tasks (recent + active, including logs/terminal output if available)

#### Actions (task-producing)
- Refresh (per-manager and refresh-all ordered)
- Search local
- Trigger remote search
- Install package
- Uninstall package
- Upgrade package
- Upgrade all (pin-aware, authority-ordered, guarded actions require confirmation)
- Pin / unpin
- Manager self-update (where supported)
- Cancel task (best-effort + process-level where possible)

### 2.3 Versioning

- If the XPC protocol changes materially, record it in `docs/DECISIONS.md`.
- Prefer additive changes. Breaking changes should be rare and coordinated with UI updates.

---

## 3. Service ↔ Core Contract (FFI)

### 3.1 Principles (Non-negotiable)

- FFI boundary uses a stable ABI (C ABI).
- FFI must avoid “Swift-shaped” types; use JSON strings or flat structs where appropriate.
- FFI calls must be **thread-safe** and must not assume single-threaded UI behavior.
- No shell invocation anywhere in core (including via FFI).

### 3.2 FFI Function Categories (Normative)

FFI must expose functions sufficient for:

#### Manager + Capability Surface
- list managers, versions, install state
- capabilities per manager
- authority level per manager

#### Package Surface
- list installed
- list outdated (including restart_required where applicable)
- search local
- trigger/cancel remote search
- available/search cache access

#### Task Surface
- create tasks (refresh/install/uninstall/upgrade/search/etc.)
- query task status by id
- cancel tasks
- fetch task logs/terminal output

#### Policy/Settings Surface
- get/set safe mode
- get/set language override
- get/set manager enablement
- upgrade-all confirmation token flow (see §6)

### 3.3 Data Encoding

Preferred:
- JSON payloads (UTF-8) for domain objects and lists
- explicit error objects (also JSON)

**Constraint:** JSON schemas must remain stable or versioned.

---

## 4. Core ↔ Adapter Contract

### 4.1 Adapter Responsibilities

Each adapter must:
- declare capabilities explicitly
- declare authority level (Authoritative / Standard / Guarded)
- support structured invocation (argv array, no shell)
- parse output defensively
- provide deterministic results under test with fixture-based parsers

Adapters must NOT:
- mutate project-local dependencies by default
- assume output format stability
- run bulk operations unless explicitly required by orchestration design

### 4.2 Capability Model (Normative)

Capabilities are the gate for UI actions and orchestration actions.

If a capability is not declared, Helm must treat it as unsupported and:
- disable corresponding UI affordances
- avoid attempting the action during orchestration

### 4.3 Authority Ordering (Normative)

Authority phases are executed in this order:

1) Authoritative (toolchains) — e.g., mise, rustup  
2) Standard (language/app managers)  
3) Guarded (system-level) — e.g., Homebrew, softwareupdate  

**Invariant:** Guarded actions execute last and require guardrails (see §6).

---

## 5. Core ↔ Persistence Contract (SQLite)

### 5.1 Persistence Principles

- SQLite is the canonical store for:
  - package cache
  - outdated state
  - search cache
  - pin records
  - tasks history
  - app settings

### 5.2 Schema Versioning

- Schema must be versioned.
- Migrations must be explicit and tested.
- Core must handle “missing/empty/malformed” persisted fields gracefully.

### 5.3 Durability Invariants

- Pin state must persist across restarts.
- Task history must persist across restarts.
- Cache corruption must not crash the UI; it must degrade safely (e.g., rebuild cache).

---

## 6. Safety & Guardrails Contracts

### 6.1 No Shell Injection (Invariant)

All process execution uses:
- executable + argv array
- no string concatenation to form command lines
- sanitized input handling

### 6.2 Guarded Actions Confirmation (Normative)

Guarded actions (notably macOS OS updates) require explicit confirmation.

Contract shape:
- UI requests a guarded operation
- Service/Core returns a “confirmation required” response including:
  - reason code
  - human-readable message key (localized in UI)
  - confirmation token (short-lived)
- UI must resubmit action with the confirmation token to proceed

**Invariant:** Silent OS updates are prohibited.

### 6.3 Safe Mode (Policy)

Safe mode is an app policy flag that:
- blocks guarded upgrade execution
- requires explicit disabling before guarded operations can proceed

---

## 7. Error Contract

### 7.1 Structured Errors (Normative)

Errors returned across any boundary must include attribution:

- manager_id (if applicable)
- task_id (if applicable)
- action (e.g., refresh, install, upgrade_all)
- error_code (stable identifier)
- user_message_key (localization key)
- debug_context (non-localized, for logs)

### 7.2 UI Localization Rule

- Core/service may return localization keys.
- UI is responsible for turning keys into strings.
- Debug logs remain English.

(See `docs/I18N_STRATEGY.md`.)

---

## 8. Task Contract

### 8.1 Task Lifecycle (Normative)

States:
- queued
- running
- completed
- failed
- canceled

Transitions must be consistent and persisted.

### 8.2 Cancellation (Normative)

Cancellation must be:
- process-level where possible (not “UI-only”)
- reflected in task state
- safe under race conditions (cancel during completion, etc.)

---

## 9. Documentation Obligations (Process Contract)

When a contract changes:
- update this document
- record the decision in `docs/DECISIONS.md`
- update `docs/CURRENT_STATE.md` if behavior changed
- update `docs/NEXT_STEPS.md` if priorities shift

---

## 10. Open Items / TODO

- Explicit list of current XPC methods + parameter schemas
- Explicit list of current FFI exports + JSON schemas
- Task log payload schema (terminal output vs structured logs)
- SQLite schema summary (tables + key fields)
- Confirmation token TTL and storage model

These should be filled in as the implementation stabilizes.
