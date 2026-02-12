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
- Installed and outdated packages views populated from Homebrew adapter
- Refresh action wired end-to-end: UI → XPC → FFI → orchestration → SQLite
- Code signing validation on XPC connections (team ID verification via SecCode)
- Centralized version management (workspace Cargo.toml, auto-generated HelmVersion.swift + xcconfig)
- Visual refresh feedback: spinner in nav bar and footer, button state management
- XPC reconnection logic with automatic retry on service interruption
- Tabbed UI (Dashboard / Packages) with MacPax-inspired design
- Dashboard: app icon, version, package stats, manager grid, recent tasks
- Package list: status filter bar, color-coded status icons, detail popover
- Settings popover with functional Refresh/Quit and disabled future controls
- Task ID persistence across app restarts (seeded from SQLite max ID)
- Process stdin null for XPC service daemon context

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
- Toolchain normalization layer
- Authority ordering engine
- Authority ordering enforced:

  - mise / rustup before package managers
- list_installed
- list_outdated
- upgrade toolchains
- version detection and normalization

Exit Criteria:

- mise detection works
- rustup detection works
- Toolchain upgrades execute before brew/npm/pipx
- Version parsing robust across versions
- Failure isolation verified
- Tests include fixture parsing for version output

---

## 0.7.x — System & App Store Managers (beta)

Goal:

- macOS `softwareupdate` adapter
- `mas` adapter
- Guarded execution model
- Explicit confirmation requirement for OS-level updates
- Reboot-required detection
- Privilege boundary validation
- Elevation flow defined (if needed)

Exit Criteria:

- `softwareupdate --list` parsed correctly and stable across macOS versions
- `mas outdated` detection works and is parsed correctly
- Guardrails block silent OS updates
- Reboot-required surfaced in UI

---

## 0.8.x — Pinning & Policy Enforcement (beta)

Goal:

- Native pin support
- Virtual pin fallback
- Pin-aware upgrade-all
- Timeout enforcement across all adapters
- Manager enable/disable toggles
- Per-manager structured error reporting hardened
- Safe mode execution mode

Exit Criteria:

- All adapters respect pin state
- Safe mode blocks OS updates
- softwareupdate cannot run without explicit confirmation
- Cancellation verified across mise / rustup / mas / brew
- Settings persist reliably

---

## 0.9.x — Upgrade Preview & Execution Transparency (beta)

Goal:

- Bulk upgrade preview modal
- Dry-run execution
- Ordered plan rendering
- Failure isolation reporting

Exit Criteria:

- Users can inspect full execution plan
- Dry-run matches actual execution order
- Partial failure clearly reported

---

## 0.10.x — Self-Update & Installer Hardening (beta)

Goal:

- Sparkle integration (or equivalent)
- Signed update verification
- Delta updates
- Self-update testing across versions

Exit Criteria:

- Helm can update itself safely
- Downgrade handling defined
- Update interruption recovery tested

---

## 0.11.x — Diagnostics & Logging (rc)

Goal:

- Per-task log viewer
- Structured error export
- Service health diagnostics panel
- Manager detection diagnostics

Exit Criteria:

- Logs accessible in UI
- No silent failures
- Support data export works

---

## 0.12.x — Stability & Pre-1.0 Hardening (rc)

Goal:

- Full integration test matrix, especially:

  - multi-manager authority ordering
  - guarded OS update flow
  - failure isolation
- Multi-manager orchestration stress tests
- Cancellation stress tests
- Logging refinement
- Crash recovery validation
- No known race conditions
- Memory safety audit

Exit Criteria:

- All core paths tested
- No known race conditions
- No unhandled panics
- Stable FFI boundary
- Deterministic execution verified

---

## 1.0.0 — Stable Control Plane Release

Goal:

- Stable architecture
- Stable adapter trait
- Stable orchestration semantics
- Production-safe execution
- Self-update operational
- Authority ordering guaranteed
- Guardrails enforced
- Logs and diagnostics present
