# Helm Roadmap

This roadmap defines capability milestones. Dates are intentionally omitted.
Milestones are feature-driven, not time-driven.

---

## 0.1.x — Core Foundation (alpha) - Completed

Goal:
- Rust workspace initialized
- Manager adapter trait defined
- Capability declaration model
- SQLite schema v1
- Basic logging system

Exit Criteria:
- Compiles
- Unit tests pass
- No real adapters yet

---

## 0.2.x — First Adapter (alpha) - Completed

Goal:
- Homebrew adapter implemented
- list_installed
- list_outdated
- basic search (local)
- Task execution scaffold

Exit Criteria:
- Brew detection works
- Installed packages listed correctly
- Tests include parsing fixtures

---

## 0.3.x — Orchestration Engine (beta) - Completed

Goal:
- Background task queue
- Per-manager locking
- Cross-manager parallelism
- True process cancellation
- Structured error reporting

Exit Criteria:
- Multiple managers can run concurrently
- Same manager tasks are serialized
- Cancellation verified via tests

---

## 0.4.x — SwiftUI Shell (beta) - Completed

Goal:
- Menu bar app scaffold
- Task list UI
- Installed packages view
- Refresh action wired

Exit Criteria:
- App launches
- Refresh populates UI
- Tasks update live

Delivered:
- macOS menu bar app with floating panel UI (no Dock icon)
- XPC service architecture: sandboxed app communicates with unsandboxed service for process execution
- Rust FFI layer (`helm-ffi`) bridging Swift UI to Rust core via C ABI
- Real-time task list with 1-second polling (status transitions: Queued → Running → Completed/Failed)
- Installed packages view populated from Homebrew adapter
- Refresh action wired end-to-end: UI → XPC → FFI → orchestration → SQLite
- Code signing validation on XPC connections (team ID verification via SecCode)
- Centralized version management (workspace Cargo.toml, auto-generated HelmVersion.swift)

Known gaps deferred to 0.4.1:
- Outdated packages not yet surfaced in UI (backend persists them, no FFI/UI exposure)
- No visual refresh feedback (spinner, button state, completion notification)
- No XPC reconnection logic on service interruption

---

## 0.5.x — Progressive Search (beta)

Goal:
- Local-first fuzzy search
- Debounced remote search
- Cancellation semantics
- Cache enrichment model

Exit Criteria:
- Typing cancels remote searches
- Cache updates incrementally
- No UI freezing

---

## 0.6.x — Core Toolchain Managers (beta)

Goal:

- mise adapter
- rustup adapter
- Authority ordering enforced:

  - mise / rustup before package managers
- list_installed
- list_outdated
- upgrade toolchains
- version detection and normalization

Exit Criteria:

- mise detection works
- rustup detection works
- Toolchain upgrades execute before brew/npm/pipx in bulk upgrade
- Tests include fixture parsing for version output

---

## 0.7.x — System & App Store Managers (beta)

Goal:

- macOS `softwareupdate` adapter
- `mas` adapter
- Guarded execution model
- Explicit confirmation requirement for OS-level updates
- Reboot-required detection
- Elevation flow defined (if needed)

Exit Criteria:

- `softwareupdate --list` parsed correctly
- `mas outdated` parsed correctly
- Guardrails block silent OS updates
- Reboot-required surfaced in UI

---

## 0.8.x — Pinning & Policy (beta)

Goal:
- Native pin support (brew first)
- Virtual pin fallback
- Pin-aware upgrade-all
- Settings controls for pin behavior

Exit Criteria:
- All adapters respect pin state
- Pinned packages excluded from bulk upgrades
- Pin state persists across restarts

---

## 0.9.x — Guardrails & Safety (beta)

Goal:
- Privilege boundary enforcement
- OS update confirmations
- Timeout enforcement across all adapters
- Timeout tuning
- Retry policies
- Robust error surfacing
- Per-manager structured error reporting hardened

Exit Criteria:
- No shell injection paths
- Clear per-manager failure reporting
- softwareupdate cannot run without explicit confirmation
- Cancellation verified across mise / rustup / mas / brew

---

## 0.10.x — Hardening (rc)

Goal:
- Integration tests covering:

  - multi-manager authority ordering
  - guarded OS update flow
  - failure isolation
- Deterministic parsing tests
- Logging refinement
- Crash recovery validation
- No known race conditions

Exit Criteria:
- All core paths covered by tests
- No unhandled panics
- Stable FFI boundary

---

## 1.0.0 — Stable Release

Goal:
- Stable architecture
- Stable adapter trait
- Stable orchestration semantics
- Usable UI
- Documentation complete
- Known limitations documented

See: docs/DEFINITION_OF_DONE.md
