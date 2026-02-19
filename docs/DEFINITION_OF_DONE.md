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

Toolchain:
- mise
- rustup

System:
- Homebrew (formula; cask adapter deferred to 0.14.x per Decision 019)
- macOS softwareupdate

App Store:
- mas (Mac App Store)

Language:
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
- Upgrade preview available before execution.
- Dry-run mode supported.
- Reboot-required state surfaced clearly to UI.

---

## 4. Safety Requirements

- No shell injection vectors
- Structured process invocation
- Timeouts enforced
- Clear error attribution
- Guardrails for OS updates
- OS updates require explicit confirmation.
- Silent system upgrades are disallowed.
- Privileged operations clearly indicated.

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
- Edition boundaries documented (debug/release/free/pro/business)

---

## 9. Self-Update

- Helm can update itself via signed updates.
- Update integrity verified.
- Update interruption recovery tested.

---

## 10. Transparency & Diagnostics

- Per-task logs visible in UI.
- Failure isolation reporting present.
- Manager detection diagnostics accessible.

---

## 11. Editions and Distribution Baseline

- Reproducible debug and release builds are defined.
- Channel-aware release builds include entitlement-aware gating scaffolding.
- Free capabilities remain available under entitlement failure.
- Fleet-only business controls fail closed when entitlement validation fails.
- Centralized business control plane remains post-1.0 scope.

---

## 12. Licensing & Distribution

- License model defined for release builds
- Commercial usage terms defined for Helm (Consumer Free/Pro) and Helm Business (Fleet)
- Contributor License Agreement (CLA) in place for all contributions
- License and usage terms documented in README
- Distribution model aligned with licensing (binary distribution, update mechanism)

---

## Explicit Non-Goals for 1.0

- Dependency graph resolution
- Plugin marketplace
- CLI companion tool
- Telemetry
- Cloud sync
- Centrally hosted enterprise policy control plane
