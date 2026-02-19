# Architectural Decisions

This document records key architectural decisions.

It functions as a lightweight ADR (Architecture Decision Record) log.

---

## Decision 001 — Rust Core

**Decision:**
Use Rust for core logic.

**Rationale:**

- Memory safety
- Strong typing
- Performance
- Testability

---

## Decision 002 — XPC Service Boundary

**Decision:**
Use an XPC service for execution.

**Rationale:**

- Sandbox separation
- Crash isolation
- Security boundary

---

## Decision 003 — SwiftUI UI

**Decision:**
Use SwiftUI for macOS UI.

**Rationale:**

- Native look and feel
- Rapid iteration
- Good integration with macOS

---

## Decision 004 — Adapter Architecture

**Decision:**
Use adapter-per-manager model.

**Rationale:**

- Clear separation
- Extensibility
- Capability-driven behavior

---

## Decision 005 — Capability Model

**Decision:**
Managers declare capabilities.

**Rationale:**

- Avoid false assumptions
- Enable UI gating
- Support diverse managers

---

## Decision 006 — Authority Ordering

**Decision:**
Enforce execution ordering.

**Order:**
Authoritative → Standard → Guarded

**Rationale:**

- Toolchains before packages
- System updates last
- Deterministic behavior

---

## Decision 007 — SQLite Persistence

**Decision:**
Use SQLite for persistence.

**Rationale:**

- Local-first design
- Reliability
- Simple deployment

---

## Decision 008 — No Shell Execution

**Decision:**
Never use shell invocation.

**Rationale:**

- Prevent injection vulnerabilities
- Structured execution

---

## Decision 009 — Task-Based Execution

**Decision:**
All operations are tasks.

**Rationale:**

- Observability
- Cancellation
- Persistence

---

## Decision 010 — Localization via Keys

**Decision:**
All UI text uses localization keys.

**Rationale:**

- Internationalization
- Consistency
- Maintainability

---

## Decision 011 — Source-Available Licensing (Pre-1.0)

**Decision:**
Use non-commercial source-available license.

**Rationale:**

- Prevent early commercialization
- Maintain control
- Allow future licensing flexibility

---

## Decision 012 — Edition-Based Future

**Decision:**
Plan Free / Pro / Business editions.

**Rationale:**

- Sustainable business model
- Enterprise expansion

---

## Decision 013 — Dual-Surface UI Model

**Decision:**
Use a menu bar popover for quick triage and a separate control center window for sustained workflows.

**Rationale:**

- Popover provides at-a-glance status and quick actions
- Control center window supports deep exploration, inspector, and settings
- AppDelegate manages both via `NSStatusItem`, `FloatingPanel`, and `ControlCenterWindow`

---

## Decision 014 — Locale Mirror Architecture

**Decision:**
Maintain locale files in two locations: `locales/` (canonical source) and `apps/macos-ui/Helm/Resources/locales/` (app resource copy), enforced in sync by CI.

**Rationale:**

- `locales/` is the single source of truth for translations
- The app bundle requires resources in its own directory
- CI `diff -ru` enforcement prevents drift

---

## Decision 015 — Post-Upgrade Validation Pattern

**Decision:**
After an upgrade command reports success, re-check `list_outdated` and return `ProcessFailure` if the package remains outdated.

**Rationale:**

- Exit code 0 does not guarantee the package was actually updated
- Silent upgrade failures are a significant usability problem
- Applied to all 11 adapters with per-package upgrade capability

---

## Decision 016 — Onboarding Walkthrough via SpotlightOverlay

**Decision:**
Implement guided onboarding as a spotlight/coach marks system using SwiftUI preference keys for anchor positioning.

**Rationale:**

- Non-intrusive: overlays existing UI rather than blocking it
- Reusable across popover (6 steps) and control center (7 steps)
- Persisted separately from onboarding wizard via UserDefaults

---

## Decision 017 — Universal Binary Distribution

**Decision:**
Build universal (arm64 + x86_64) binaries using `lipo` and distribute via signed DMG.

**Rationale:**

- Single artifact supports Apple Silicon and Intel Macs
- DMG provides familiar macOS installation experience
- GitHub Actions workflow automates signing and notarization

---

## Decision 018 — XPC Timeout and Reconnection Policy

**Decision:**
Enforce timeouts on all XPC calls (30s data fetches, 300s mutations) and use exponential backoff for reconnection (2s base, doubling to 60s cap).

**Rationale:**

- Prevents UI hangs from unresponsive service
- Exponential backoff avoids thundering herd on service restart
- Reset on successful connection restores normal responsiveness

---

## Decision 019 — Homebrew Casks Deferred

**Decision:**
Defer Homebrew Casks adapter to 0.14.x. Originally planned for 0.10.x but dropped from that milestone.

**Rationale:**

- Homebrew formula adapter covers the primary use case
- Cask handling requires different upgrade and detection semantics
- 0.14.x (Platform, Detection & Optional Managers) is the appropriate milestone

---

## Summary

Helm prioritizes:

- Safety
- Determinism
- Transparency
- Extensibility

These decisions should not change without strong justification.
