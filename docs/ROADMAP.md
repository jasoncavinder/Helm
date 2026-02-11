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

## 0.4.x — SwiftUI Shell (beta) - Active

Goal:
- Menu bar app scaffold
- Task list UI
- Installed packages view
- Refresh action wired

Exit Criteria:
- App launches
- Refresh populates UI
- Tasks update live

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

## 0.6.x — Pinning & Policy (beta)

Goal:
- Native pin support (brew first)
- Virtual pin fallback
- Pin-aware upgrade-all
- Settings controls for pin behavior

Exit Criteria:
- Pinned packages excluded from bulk upgrades
- Pin state persists across restarts

---

## 0.7.x — Guardrails & Safety (beta)

Goal:
- Privilege boundary enforcement
- OS update confirmations
- Timeout tuning
- Retry policies
- Robust error surfacing

Exit Criteria:
- No shell injection paths
- Clear per-manager failure reporting

---

## 0.8.x — Hardening (rc)

Goal:
- Comprehensive test coverage
- Deterministic parsing tests
- Logging refinement
- Crash recovery validation

Exit Criteria:
- All core paths covered by tests
- No known race conditions

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
