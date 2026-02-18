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
- `v0.13.0-beta.3` released (accessibility, task cancellation, CI test enforcement, HelmCore/DashboardView decomposition)
- Full codebase audit completed 2026-02-17 (Rust core, SwiftUI UI, XPC, localization, CI/CD)

Next release targets:
- `v0.13.0-beta.4` — Localization parity + onboarding walkthrough
- `v0.13.0-beta.5` — Architecture cleanup + UI purity
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

## v0.13.0-beta.4 — Localization Parity + Onboarding Walkthrough

### Localization Parity

Implement:

- Audit all redesign-specific UI surfaces for missing localization keys:
  - Control center sidebar section labels (Overview, Updates, Packages, Tasks, Managers, Settings)
  - Task names and descriptions in task list and popover
  - Inspector pane labels and actions
  - Upgrade preview section headers and dry-run labels
  - Manager health state labels (Healthy, Warning, Error, Not Installed)
  - Status badge descriptions and tooltip text
  - Right-click context menu items
  - About panel content
  - Any other surfaces added or modified in the 0.13.x redesign
- Add missing keys to `en` locale files first, then roll out to all 5 non-English locales (es, de, fr, pt-BR, ja)
- Fix Spanish accent typo: "Actualizacion" → "Actualización" in `locales/es/app.json`
- Run `check_locale_integrity.sh` to verify key parity and placeholder consistency
- Run `check_locale_lengths.sh` to verify no overflow-risk regressions

### Onboarding Walkthrough Redesign

Implement:

- Update existing onboarding steps (Welcome → Detection → Configure) with friendlier tone:
  - Warmer welcome copy that explains Helm's value proposition simply
  - More encouraging detection step with progress feedback
  - Clearer configuration step with sensible defaults and brief explanations
  - Localize all updated copy across all 6 locales

- Add guided walkthrough after onboarding setup completes:

  **Spotlight/Coach Marks System:**
  - Implement a reusable `SpotlightOverlay` view component:
    - Full-screen dimmed overlay with a transparent cutout around the target element
    - Descriptive tooltip/card adjacent to the cutout explaining the focused feature
    - "Next" / "Skip" / step indicator controls
    - Smooth transition between spotlight targets
    - Respect `accessibilityReduceMotion` for transitions
    - VoiceOver-compatible: announce step descriptions, support Skip via accessibility action

  **Popover Walkthrough (4–6 steps):**
  - Step 1: Status icon — "This is your Helm status icon. It shows your system health at a glance."
  - Step 2: Updates banner — "When updates are available, they appear here. Tap to upgrade all at once."
  - Step 3: Active tasks — "Running operations appear here so you always know what Helm is doing."
  - Step 4: Manager summary — "A quick snapshot of your detected package managers and their status."
  - Step 5: Footer actions — "Search packages, open settings, or access the full control center from here."
  - Step 6: Quick search — "Search across all your package managers instantly."

  **Control Center Walkthrough (5–7 steps):**
  - Step 1: Sidebar navigation — "Navigate between sections to manage your entire development environment."
  - Step 2: Overview — "Your system health dashboard. See everything at a glance."
  - Step 3: Packages — "Browse, search, install, and manage packages across all managers."
  - Step 4: Tasks — "Every operation Helm runs is tracked here. Cancel running tasks anytime."
  - Step 5: Managers — "Enable, disable, and monitor your package managers."
  - Step 6: Settings — "Configure safe mode, language, and automatic update policies."
  - Step 7: Upgrade preview — "Preview exactly what will happen before running upgrades."

  **Walkthrough State Management:**
  - Track walkthrough completion in UserDefaults (separate from onboarding completion)
  - Allow skipping at any point
  - Allow re-triggering from Settings ("Replay Walkthrough" action)
  - Do not block app usage — walkthrough is dismissible at every step

  **Localization:**
  - Add all walkthrough step titles and descriptions to `app.json` under `app.walkthrough.*` key namespace
  - Translate across all 6 locales (en, es, de, fr, pt-BR, ja)
  - Run overflow validation for walkthrough tooltip content

---

## v0.13.0-beta.5 — Architecture Cleanup + UI Purity

### UI Layer Purity Fixes

Implement:

- Move search deduplication/merge logic from `PackageListView.swift` to a computed property on HelmCore:
  - `displayedPackages` should combine local matches + remote results with dedup in HelmCore, not the view
- Move safe-mode upgrade action badge filtering from `SettingsPopoverView.swift` to HelmCore:
  - `upgradeActionBadges` should be a computed property on HelmCore
- Move task-to-manager inference from `TaskListView.swift` (`inferManagerId`) to structured task data:
  - Tasks returned from XPC/FFI should include `manager_id` as a field, not parsed from description strings
  - Update `CoreTaskRecord` to include `manager_id` and propagate through XPC protocol
- Consolidate authority/capability lookup functions from `DashboardView.swift` into `ManagerInfo` struct:
  - Remove standalone `authority(for:)` and `capabilities(for:)` functions
  - Query from `ManagerInfo.all` or expose as computed properties on `ManagerInfo`

### HelmCore Decomposition (Delivered Early in beta.3)

Delivered in beta.3:
- ✅ HelmCore.swift decomposed from 1,133 lines into HelmCore.swift (314 lines) + 4 extension files
- ✅ DashboardView.swift decomposed from 1,919 lines into 4 focused files

Remaining (optional further refinement):
- Extract service coordination into a dedicated `ServiceCoordinator` class if HelmCore extensions grow beyond current thresholds

### Keyboard Traversal Validation (Carry-Forward from beta.3)

Implement:

- Add `.focusable()` modifiers to all interactive elements systematically
- Validate full Tab traversal through popover (banner → task list → managers → footer actions)
- Validate full Tab traversal through control center sidebar and section content
- Validate Escape key behavior consistent across all overlay states
- Validate Enter/Space activation for all focusable elements

### Legacy UI Cleanup

Implement:

- Identify and remove or archive legacy UI component paths no longer used by the redesigned shell
- Remove dead code paths for pre-redesign views (if any remain)
- Verify no orphaned localization keys after cleanup

### XPC Robustness

Implement:

- Add timeout enforcement on individual XPC service calls:
  - Wrap XPC calls with a reasonable timeout (e.g., 30s for data fetches, 5min for mutations)
  - Surface timeout errors in UI rather than silently hanging
- Add error feedback for JSON decode failures:
  - Log decode errors with context (which method, what was received)
  - Surface a user-visible error state when decode fails (e.g., "Failed to load packages")
- Add exponential backoff to XPC reconnection (currently flat 2s delay)

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

The 0.13.x milestone is structured across four remaining beta releases:

- **beta.3**: Accessibility (VoiceOver, keyboard, semantic grouping), task cancellation UI, CI test enforcement
- **beta.4**: Localization parity (redesign + walkthrough keys across 6 locales), onboarding walkthrough with spotlight/coach marks
- **beta.5**: Architecture cleanup (UI purity fixes, HelmCore decomposition, legacy removal, XPC robustness)
- **beta.6**: Validation sweep (on-device locale validation, usability test plan, Rust hardening, documentation alignment)

The goal is **closing 0.13.x as a stable, accessible, well-tested redesign checkpoint** before moving to platform expansion (0.14.x).
