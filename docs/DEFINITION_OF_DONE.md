# Helm 1.0 Definition of Done

This document defines the criteria required before Helm can be released as 1.0.0.

It is intentionally strict.

This document may only be modified via PR targeting main.

---

## 1. Architectural Stability

- Adapter trait considered stable.
- Orchestration model finalized.
- No breaking schema changes expected.
- Service boundary contract documented and versioned.

---

## 2. Functional Completeness

Helm must support:

Core managers (required for 1.0):

Toolchain Managers:
- mise
- rustup

System & OS Managers:
- Homebrew (formula + cask)
- macOS softwareupdate

App Store:
- mas (Mac App Store)

Language Managers:
- npm (global)
- pipx
- Cargo

Capabilities:
- list installed
- list outdated
- search (local + progressive remote)
- install
- uninstall
- upgrade
- pin / unpin
- upgrade all (pin-aware)

---

## 3. Orchestration Guarantees

- Cross-manager parallelism
- Per-manager serialization
- True process-level cancellation
- Authority ordering respected
- Pin enforcement during bulk upgrades
- Toolchain managers execute before downstream package managers.
- System-level updates require explicit confirmation.
- Reboot-required state surfaced to UI.

---

## 4. Safety Requirements

- No shell injection vectors
- Structured process invocation
- Timeouts enforced
- Clear error attribution
- Guardrails for OS updates

---

## 5. Persistence Guarantees

- SQLite schema versioned
- Migration tested
- Pin state durable
- Cache corruption does not crash app

---

## 6. UI Requirements

- Menu bar utility
- Installed view
- Search view
- Task panel
- Settings panel
- Non-blocking UI
- Pin indicator visible

---

## 7. Quality Bar

- Unit tests for:
  - adapters
  - parsing
  - orchestration
- Integration tests for:
  - refresh
  - cancellation
  - multi-manager behavior
- No known race conditions
- No unhandled panics in Rust core

---

## 8. Documentation

- README complete
- Supported managers listed
- Known limitations documented
- Architecture overview included

---

## Explicit Non-Goals for 1.0

- Dependency graph resolution
- Plugin marketplace
- CLI companion tool
- Telemetry
- Cloud sync
