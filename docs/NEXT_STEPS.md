# Next Steps

This document defines the immediate priorities for Helm development.

It is intentionally tactical.

---

## Current Phase

Helm is in:

```
0.13.x
```

Focus:
- Accessibility QA and VoiceOver/keyboard support
- CI test enforcement (Rust + Swift)
- Onboarding walkthrough (guided spotlight/coach marks tour)
- Localization parity for redesign, onboarding walkthrough, and control center keys
- UI layer purity fixes and architecture cleanup
- Validation and hardening

Current checkpoint:
- `v0.13.0-beta.5` released (architecture cleanup, UI purity, XPC robustness, legacy removal)
- Full codebase audit completed 2026-02-17 (Rust core, SwiftUI UI, XPC, localization, CI/CD)

Next release targets:
- `v0.13.0-beta.6` — Validation + hardening + documentation

---

## v0.13.0-beta.3 — Accessibility + CI Foundation (Completed)

### Accessibility QA Pass (Completed)

Delivered:

- ✅ `accessibilityLabel` modifiers on all interactive elements (package rows, task rows, manager items, status badges, menu bar status item)
- ✅ `accessibilityValue` for dynamic content (task status, package counts, manager state)
- ✅ `accessibilityElement(children: .combine)` semantic grouping on composite rows
- ✅ VoiceOver announcements for refresh start/completion, task cancellation, task failures, and refresh failure
- ✅ `accessibilityReduceMotion` respected in overlay transitions

Carry-forward to beta.5:
- Keyboard-only traversal validation (Tab order, Escape behavior, `.focusable()` modifiers not systematically applied)

### Task Cancellation in UI (Completed)

Delivered:

- ✅ XPC `cancelTask` wired to cancel button with optimistic UI state update
- ✅ Cancel button enabled for running tasks
- ✅ Cancellation state transitions verified (Running → Cancelled)
- ✅ VoiceOver announcement on successful cancellation

### CI Test Enforcement (Completed)

Delivered:

- ✅ `ci-test.yml` with `cargo test --workspace` and `xcodebuild test` on PR/push to main/dev
- ✅ `xcodebuild test` gate added to `release-macos-dmg.yml` before signing
- ✅ `check_locale_lengths.sh` added to `i18n-lint.yml` workflow

### Additional Deliverables (Not Originally Planned)

- ✅ HelmCore.swift decomposed into 5 files (originally beta.5 scope)
- ✅ DashboardView.swift decomposed into 4 files (originally beta.5 scope)
- ✅ SwiftLint thresholds tightened (type_body_length: 400/600, file_length: 500/750)
- ✅ Per-manager "Upgrade All" button in Managers view
- ✅ Spanish accent typo fix ("Actualización")

---

## v0.13.0-beta.4 — Localization Parity + Onboarding Walkthrough (Completed)

### Localization Parity (Completed)

Delivered:

- ✅ 31 walkthrough L10n keys added to `en` and translated across all 5 non-English locales
- ✅ All locale integrity and overflow checks passing
- ✅ Spanish accent typo previously fixed in beta.3

### Onboarding Walkthrough Redesign (Completed)

Delivered:

- ✅ Onboarding copy updated across all 6 locales for friendlier tone (warmer subtitle, encouraging detection feedback, reassuring configure fallback)
- ✅ Reusable `SpotlightOverlay` component with anchor preference system, even-odd fill cutout, animated transitions, `accessibilityReduceMotion` support, and VoiceOver compatibility
- ✅ `WalkthroughManager` singleton with UserDefaults persistence (separate from onboarding), step progression, skip, and reset
- ✅ Popover walkthrough (6 steps): health badge, attention banner, active tasks, manager snapshot, footer actions, search field
- ✅ Control center walkthrough (7 steps): sidebar, overview, packages, tasks, managers, settings, updates — with auto-navigation on step advance
- ✅ "Replay Walkthrough" action in Settings advanced grid
- ✅ All walkthrough L10n keys translated across 6 locales with overflow validation passing

---

## v0.13.0-beta.5 — Architecture Cleanup + UI Purity (Completed)

### UI Layer Purity Fixes (Completed)

Delivered:

- ✅ Search deduplication/merge logic moved from `PackageListView` to `HelmCore.filteredPackages(query:managerId:statusFilter:)`
- ✅ Safe-mode upgrade action badge filtering moved from `SettingsPopoverView` to `HelmCore.upgradeActionManagerIds`
- ✅ Task-to-manager inference removed: `TaskItem` now carries `managerId` directly from `CoreTaskRecord`; `inferManagerId` deleted
- ✅ `authority(for:)` consolidated: computed property on `ManagerInfo`, standalone function delegates to it
- ✅ `capabilities(for:)` moved to `ManagerInfo.capabilities` computed property with `canSearch`/`canPin` helpers
- ✅ `managerSymbol(for:)` moved to `ManagerInfo.symbolName` computed property
- ✅ `health(forManagerId:)` now uses structured `managerId` field instead of localized description matching

### HelmCore Decomposition (Delivered Early in beta.3)

Delivered in beta.3:
- ✅ HelmCore.swift decomposed from 1,133 lines into HelmCore.swift (314 lines) + 4 extension files
- ✅ DashboardView.swift decomposed from 1,919 lines into 4 focused files

Remaining (optional further refinement):
- Extract service coordination into a dedicated `ServiceCoordinator` class if HelmCore extensions grow beyond current thresholds

### Keyboard Traversal (Partially Completed)

Delivered:

- ✅ `.focusable()` modifiers added to task rows, package rows, manager health cards, and PackageRowView component

Carry-forward to beta.6:
- Validate full Tab traversal through popover (banner → task list → managers → footer actions)
- Validate full Tab traversal through control center sidebar and section content
- Validate Escape key behavior consistent across all overlay states
- Validate Enter/Space activation for all focusable elements

### Legacy UI Cleanup (Completed)

Delivered:

- ✅ Legacy redesign scaffold (`apps/macos/`, 18 files) removed entirely
- ✅ No orphaned localization keys (legacy scaffold had its own self-contained resources)

### XPC Robustness (Completed)

Delivered:

- ✅ Timeout enforcement on all XPC service calls (30s data fetches, 300s mutations) via `withTimeout` helper
- ✅ JSON decode error logging enhanced with method name and raw data length context
- ✅ `@Published var lastError` for surfacing decode/timeout failures
- ✅ Exponential backoff on XPC reconnection (2s base, doubling to 60s cap, reset on success)

---

## v0.13.0-beta.6 — Validation + Hardening + Documentation

### On-Device Validation

Implement:

- Full on-device validation sweep for redesigned states across all 6 locales:
  - Loading states (refresh in progress, search in progress)
  - Success states (packages loaded, tasks completed)
  - Error states (service disconnected, task failed, manager not installed)
  - Partial failure states (some managers failed during refresh)
  - Empty states (no packages, no tasks, no managers detected)
- Validate onboarding walkthrough renders correctly in all 6 locales:
  - Spotlight overlay positioning
  - Tooltip/card content fits without truncation
  - Step indicators visible and accessible
- Capture validation report at `docs/validation/v0.13.0-beta.6-redesign-validation.md`

### Usability Test Plan

Implement:

- Document usability test plan and acceptance metrics for redesigned flows:
  - Core scenarios: first launch, refresh, search, install, upgrade, upgrade-all, pin/unpin
  - Error scenarios: service crash/reconnection, manager failure, network unavailable
  - Accessibility scenarios: VoiceOver-only navigation, keyboard-only navigation, reduced-motion
  - Locale scenarios: complete each core flow in at least 2 non-English locales
- Document pass/fail criteria for each scenario

### Rust Core Hardening

Implement:

- Add structured logging spans (`tracing`) in Rust adapter execution paths for long-running operations
- Add unit test for Homebrew `split_upgrade_target()` with `@@helm.cleanup` marker
- Document FFI lifecycle: no explicit `helm_shutdown()`, runtime spans process lifetime
- Document `execute_batch_tolerant()` error swallowing scope in SQLite migration comments

### Documentation Alignment

Implement:

- Update `docs/INTERFACES.md` Section 10 open items:
  - Add explicit list of current XPC protocol methods with parameter schemas
  - Add explicit list of current FFI exports with JSON schemas
  - Add SQLite schema summary (tables + key fields)
  - Document confirmation token TTL and storage model
- Final documentation consistency sweep:
  - CURRENT_STATE.md reflects beta.6 reality
  - CHANGELOG.md updated for all beta.3–6 changes
  - ROADMAP.md 0.13.x section updated with delivered scope

---

## Completed Priorities (Pre-0.13.x)

### Priority 1 — Core Language Managers (Completed)

- npm (global) ✅
- pip (`python3 -m pip`, global) ✅
- pipx ✅
- cargo ✅
- cargo-binstall ✅

### Priority 2 — Extended Managers (Completed)

- pnpm (global) ✅
- yarn (global) ✅
- RubyGems ✅
- Poetry (self/plugins) ✅
- Bundler ✅

### Priority 3 — Localization Expansion (Completed)

- All 6 locales (en, es, de, fr, pt-BR, ja) at full key parity ✅
- CI enforcement for locale parity + integrity ✅
- On-device overflow validation ✅

### Priority 4 — Upgrade Transparency (Completed)

- Upgrade preview UI ✅
- Execution plan display ✅
- Dry-run support ✅

### Priority 5 — UI/UX Redesign (Partially Complete)

- Redesign concept + integration into production target ✅
- Remaining items allocated to v0.13.0-beta.3–6 above

### Hardening (Partially Complete)

Completed in `v0.10.0` checkpoint:

- Targeted adapter hardening review for regression/robustness/security risks across Priority 1 language-manager paths
- Package-identifier validation on mutating adapter actions for npm/pip/pipx/cargo/cargo-binstall
- Shared cargo/cargo-binstall outdated synthesis logic to reduce duplication and drift risk
- Replaced panic-prone FFI `lock().unwrap()` usage with poisoned-lock recovery
- Resolved website duplicate docs-id build warnings for overview/roadmap pages

Completed in `v0.11.0-beta.2` stabilization:

- Added bounded retry handling for transient task-store create/update persistence failures in orchestration runtime paths
- Added regression coverage for refresh-response error attribution and transient task-persistence recovery

Remaining (allocated to v0.13.0-beta.6):

- Structured logging in adapter execution paths
- Homebrew upgrade-target encoding test coverage
- FFI lifecycle and migration error documentation

---

## Post-0.13.x Priorities

### Priority 6 — Self Update

Implement:

- Signed updates
- Integrity verification
- Update recovery

### Priority 7 — Diagnostics

Implement:

- Task log viewer
- Error export
- Manager diagnostics panel

### Priority 8 — Hardening (Remaining)

Implement:

- Stress test orchestration
- Cancellation reliability under load
- Memory audit
- FFI stability under extended runtime

---

## Non-Goals (Pre-1.0)

- Plugin system
- CLI tool
- Cloud sync
- Enterprise control plane

---

## Summary

The 0.13.x milestone is structured across two remaining beta releases:

- **beta.3**: Accessibility (VoiceOver, keyboard, semantic grouping), task cancellation UI, CI test enforcement — **completed**
- **beta.4**: Localization parity (redesign + walkthrough keys across 6 locales), onboarding walkthrough with spotlight/coach marks — **completed**
- **beta.5**: Architecture cleanup (UI purity fixes, legacy removal, XPC robustness, keyboard traversal) — **completed**
- **beta.6**: Validation sweep (on-device locale validation, usability test plan, Rust hardening, documentation alignment, remaining keyboard traversal validation)

The goal is **closing 0.13.x as a stable, accessible, well-tested redesign checkpoint** before moving to platform expansion (0.14.x).
