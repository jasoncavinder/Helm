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
- Set manager executable selection and install-method preferences
- Manager self-update (where supported)
- Cancel task (best-effort + process-level where possible)

Bulk upgrade workflows are backend-owned. UI may provide manager/package scope as an intent and render workflow/task state, but it must not sequence authority phases or infer task completion to schedule downstream managers. The service returns individual task handles and may return a workflow handle for scoped cancellation; individual task logs and terminal output remain the source of execution transparency.

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
- start/cancel/query scoped upgrade workflows without exposing UI-owned phase state

### 3.3 Data Encoding

Preferred:
- JSON payloads (UTF-8) for domain objects and lists
- explicit error objects (also JSON)

**Constraint:** JSON schemas must remain stable or versioned.

### 3.4 Library Search Result Provenance

`helm_list_installed_packages`, `helm_list_outdated_packages`, and `helm_search_local` keep their existing result fields and add a nested, versioned `provenance` object. The object distinguishes logical result origin (`local`, `local_cache`, `remote`, or `deferred`) from discovery source (`manager_snapshot`, `catalog_sync`, or `manager_search`). Installed/outdated manager snapshots remain `local` when read from their persisted service representation; `local_cache` is reserved for search-cache results. The XPC service forwards these payloads without reclassifying them, and Swift treats the object as additive and optional for compatibility with older service responses.

The core must never describe a persisted cache read as a live remote result, or use a non-empty query as proof that a manager used network discovery. Consumers loss-isolate nested decoding, then validate schema version, canonical source-manager identity, endpoint-specific origin/discovery rules, and query shape before presenting provenance. Unknown, malformed, or future values fail closed at presentation rather than invalidating the enclosing package result.

The normative version 1 fields, combinations, and compatibility rules are defined in `docs/architecture/LIBRARY_RESULT_PROVENANCE.md`.

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

The `0.18.x` doctor/repair contract additionally makes SQLite the canonical store for:

- doctor finding lifecycle and structured evidence
- repair knowledge, import provenance, and repair history

### 5.2 Schema Versioning

- Schema must be versioned.
- Migrations must be explicit and tested.
- Core must handle “missing/empty/malformed” persisted fields gracefully.

### 5.3 Durability Invariants

- Pin state must persist across restarts.
- Task history must persist across restarts.
- Cache corruption must not crash the UI; it must degrade safely (e.g., rebuild cache).

### 5.4 Doctor/Repair Persistence Contract (`0.18.x`, Implemented)

- Equivalent normalized findings use one deterministic, versioned fingerprint across installations.
- Local occurrence state (`first_seen`, `last_seen`, resolution) is separate from shareable fingerprint identity.
- Repair knowledge is versioned data that maps finding selectors to typed core action IDs.
- Stable knowledge-entry IDs, surface option IDs, and core action IDs are distinct; existing public repair option IDs remain compatible across the database migration.
- Knowledge imports are transactional and retain source, schema, checksum/signature, and trust metadata.
- Trust is assigned by the local importer after verification; imported payloads cannot self-assert authority.
- Internal source identity is importer-assigned; unverified imports cannot claim, revise, or tombstone protected bundled/verified source namespaces.
- Each effective `(finding selector, option_id)` has one action binding; imported knowledge cannot rebind protected option IDs.
- Finding resolution applies only to detectors that completed successfully for the same explicit scan scope.
- Monotonic scan generations prevent older late-completing scans from overwriting newer finding state.
- Unknown or malformed knowledge entries fail closed without preventing doctor from reporting findings.
- Stored findings do not authorize mutation; apply revalidates current state before execution.

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
- UI presents the upgrade plan and collects explicit user approval for OS updates.
- UI submits the upgrade request with `allow_os_updates=true` only after approval.
- Service/Core excludes OS updates when that flag is false and rejects them when Safe Mode is enabled.
- The request/response boundary returns structured errors and localization keys for blocked actions.

**Invariant:** Silent OS updates are prohibited.

### 6.3 Safe Mode (Policy)

Safe mode is an app policy flag that:
- blocks `softwareupdate` upgrade execution
- requires explicit disabling before macOS OS updates can proceed

### 6.4 Repair Knowledge Execution Boundary (Invariant)

Repair knowledge may select an existing typed core capability, but it must never carry executable behavior.

Knowledge payloads cannot contain executable paths, command names/arguments, shell fragments, scripts, plugins, arbitrary code, or unvalidated substitutions. Trusted core code owns action registration, parameter derivation, structured process construction, policy checks, confirmation, orchestration, cancellation, and post-action verification.

Imported knowledge cannot register actions or weaken the safety policy of an installed action. Unknown action IDs are non-executable.

Registry policy is the immutable minimum. Knowledge may only add restrictions; effective confirmation and automation use the most restrictive applicable registry, managed-environment, and knowledge policy.

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

## 10. Concrete Interface Inventories

### 10.1 XPC Protocol Methods (65 methods)

Source: `apps/macos-ui/Helm/Shared/HelmServiceProtocol.swift`

All methods use asynchronous `withReply` closures. The protocol covers package and pin operations, Rustup toolchain controls, task lifecycle/output/timeout handling, discovery and search, manager lifecycle/provenance/repair controls, onboarding and settings, upgrade planning, and local-data reset. Connection security is enforced via code-signing team ID validation at `NSXPCListener` acceptance.

The Swift protocol declaration is the canonical method-by-method inventory; it is intentionally not duplicated here to prevent contract drift.

Client-side timeout enforcement: 30s for data fetch calls, 300s for mutation calls. Exponential backoff reconnection on invalidation/interruption (2s base, doubling to 60s cap).

### 10.2 FFI Exports (27 functions)

Source: `core/rust/crates/helm-ffi/src/lib.rs`

See the module-level documentation in `lib.rs` for the full export table with categories. String payloads use JSON-encoded UTF-8 `*mut c_char` values, freed via `helm_free_string`; control operations use their declared scalar return types. The FFI layer has no explicit shutdown; runtime state spans the XPC service process lifetime.

### 10.3 SQLite Schema Summary (14 application tables, 16 migrations)

Source: `core/rust/crates/helm-core/src/sqlite/migrations.rs`

| Table | Migration | Primary Key | Purpose |
|-------|-----------|-------------|---------|
| `installed_packages` | v1 | `(manager_id, package_name)` | Legacy cached installed package state |
| `outdated_packages` | v1 (+v3, v14, v16) | `(manager_id, package_name)` | Cached outdated package state |
| `installed_package_versions` | v13 (+v16 identifiers) | `(manager_id, package_name, installed_version)` | Version-scoped installed package state |
| `pin_records` | v1 (+v15 version scope) | version-scoped manager/package identity | Native and virtual pin records |
| `search_cache` | v1 | none (indexed on `originating_query` + `cached_at_unix`) | Package-search cache for catalog and manager-query results |
| `task_records` | v1 | `task_id INTEGER` | Task execution history |
| `task_log_records` | v6 | `log_id INTEGER` | Persisted structured task logs |
| `manager_detection` | v2 | `manager_id` | Manager install detection state |
| `manager_preferences` | v2 (+v7/v8) | `manager_id` | Per-manager enablement, selection, and timeout preferences |
| `manager_install_instances` | v9 (+v10 adds `decision_margin`) | `(manager_id, instance_id)` | Per-manager install-instance identity, provenance confidence/margin, explainability, and strategy metadata |
| `manager_multi_instance_ack` | v11 | `manager_id` | Acknowledgement state for multi-instance attention |
| `package_manager_preferences` | v12 | `package_name` | Preferred manager for a package family |
| `app_settings` | v4 | `key` | App-level key-value settings |
| `package_keg_policies` | v5 | `(manager_id, package_name)` | Homebrew keg cleanup policy overrides |

Migrations are applied idempotently via `execute_batch_tolerant()` (see `sqlite/store.rs`).

### 10.4 Task Log Payload

Structured task logs persist in `task_log_records`; task lifecycle state persists in `task_records`. Raw command, stdout, and stderr buffers are retained in bounded runtime memory for live diagnostics rather than persisted indefinitely. Adapter responses persist to their domain tables (installed, outdated, search, and detection).

### 10.5 Guarded-Action Confirmation Model

Confirmation tokens are **not used**. Security is enforced at XPC connection acceptance through code-signing team ID verification (`SecCode` + `SecRequirement`). The upgrade boundary accepts boolean `include_pinned` and `allow_os_updates` parameters; `allow_os_updates` records explicit approval, while Safe Mode blocks `softwareupdate` upgrades in core before task submission.
