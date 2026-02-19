# Current State

This document describes the current implementation status of Helm.

It reflects reality, not intention.

---

## Version

Current version: **0.13.0**

See:
- CHANGELOG.md

Active milestone:
- 0.13.x — UI/UX redesign, accessibility, onboarding walkthrough, and hardening (stable)

---

## Completed Milestones

- 0.1.x — Core Foundation
- 0.2.x — Homebrew adapter
- 0.3.x — Orchestration engine
- 0.4.x — SwiftUI shell + XPC bridge
- 0.5.x — Progressive search
- 0.6.x — Toolchain managers (mise, rustup)
- 0.7.x — System & App Store managers (softwareupdate, mas)
- 0.8.x — Pinning & policy
- 0.9.x — Internationalization foundation
- 0.10.x — Core language managers + hardening checkpoint
- 0.11.x — Extended language managers (beta checkpoint)
- 0.12.x — Localization hardening + upgrade transparency (stable checkpoint)

---

## Implemented Managers

Fully functional:

- Homebrew
- mise
- npm (global)
- pnpm (global)
- yarn (global)
- RubyGems
- Poetry (self/plugins)
- Bundler
- pip (`python3 -m pip`, global)
- pipx
- cargo
- cargo-binstall
- rustup
- softwareupdate
- mas

---

## Core Capabilities

- list installed
- list outdated
- install
- uninstall
- upgrade
- upgrade all
- pin / unpin
- progressive search
- task system
- safe mode
- localization system

Localization coverage:

- All 6 locales (en, es, de, fr, pt-BR, ja) have full key parity across app/common/service files
- Locale length audit script added at `apps/macos-ui/scripts/check_locale_lengths.sh` for overflow-risk preflight
- Locale key/placeholder integrity audit script added at `apps/macos-ui/scripts/check_locale_integrity.sh`
- `v0.11.0-beta.2` heuristic overflow audit captured at `docs/validation/v0.11.0-beta.2-l10n-overflow.md` (no high-risk candidates flagged)
- `v0.12.0-beta.1` on-device overflow validation captured at `docs/validation/v0.12.0-beta.1-visual-overflow.md` (Settings surface checks passing)
- Expanded on-device overflow validation coverage for onboarding/navigation/packages/managers captured at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`
- Manager display-name localization keys now cover upgrade-preview/task-fallback manager labels (including software update/app store naming)
- All walkthrough keys (31 keys) translated and validated across all 6 locales

Validation snapshot for `v0.11.0-beta.1` expansion:

- Priority 1 language-manager local smoke matrix captured on a macOS dev host:
  - Detected and smoke-tested: npm, pip (`python3 -m pip`), cargo
  - Not installed in the validation host environment: pipx, cargo-binstall

---

## Architecture Status

- Rust core: stable (198+ unit/integration tests, zero shell injection vectors, structured process invocation throughout, `#[instrument]` tracing spans on adapter execution paths, post-upgrade validation on all 11 adapter upgrade handlers)
- XPC service: stable (code-signing validation, graceful reconnection with exponential backoff, timeout enforcement on all calls)
- FFI boundary: functional (poisoned-lock recovery, JSON interchange, thread-safe static state, lifecycle documented in module-level docs)
- UI: feature-complete for current scope; VoiceOver accessibility labels, semantic grouping, and state-change announcements implemented; HelmCore decomposed into 5 files; UI layer purity cleanup completed (business logic extracted from views to HelmCore/ManagerInfo); inspector sidebar with task/package/manager detail views; keyboard Tab traversal still pending (macOS SwiftUI limitation)

---

## v0.13.0-beta.3 Audit Status

Based on the full codebase audit conducted on 2026-02-17 and subsequent beta.3 remediation work.

### Rust Core

- All architectural invariants pass: no shell injection, structured args, authority ordering, cancelable tasks, deterministic and testable
- 190+ unit/integration tests cover adapters, parsing, orchestration, authority ordering, cancellation, and end-to-end flows
- Package identifier validation on all mutating operations prevents flag injection
- SQLite persistence uses versioned migrations (v1–v3), parameterized queries, and transactional multi-record ops
- No critical or high-priority issues identified

### SwiftUI UI

- All user-facing text uses L10n localization keys — no hardcoded English strings detected
- XPC service boundary has proper code-signing validation, async patterns, and reconnection logic
- State management is sound (single ObservableObject, @Published properties, weak-self captures)
- UI layer purity violations resolved in beta.5: search merge, safe-mode badge filtering, task-to-manager inference, authority/capability/symbol lookups all extracted from views
- HelmCore.swift decomposed from 1,133 lines into HelmCore.swift (314 lines) + 4 extension files (Actions, Fetching, Settings, Dashboard)
- DashboardView.swift decomposed from 1,919 lines into DashboardView.swift + ControlCenterModels, ControlCenterViews, HelmButtonStyles
- Task cancel button now wired and functional via XPC cancelTask method

### Accessibility (Substantially Resolved)

- Reduced-motion support implemented for overlay transitions
- VoiceOver `accessibilityLabel` on all interactive elements (package rows, task rows, manager items, status badges, menu bar status item)
- `accessibilityValue` for dynamic content (task status, package counts, manager state)
- `accessibilityElement(children: .combine)` semantic grouping on composite rows
- VoiceOver announcements for refresh start/completion, task cancellation, task failures, and refresh failure
- Remaining gap: keyboard Tab traversal does not work — macOS SwiftUI `.focusable()` does not integrate with the AppKit key view loop; requires NSViewRepresentable bridging or a future SwiftUI API

### CI/CD (Resolved)

- `ci-test.yml`: runs `cargo test --workspace` and `xcodebuild test` on PR and push to main/dev
- `i18n-lint.yml`: comprehensive locale parity, hardcoded string detection, mirror sync enforcement, and `check_locale_lengths.sh` overflow validation
- `release-macos-dmg.yml`: signed universal DMG + notarization with Rust and Swift test gates before build

### Localization

- All 6 locales pass key parity, placeholder consistency, and ICU format checks
- `check_locale_lengths.sh` included in CI workflow
- Spanish accent typo in "Actualización" has been corrected

---

## v0.13.0-beta.6 Audit Status

### Rust Core Hardening

- Structured `#[instrument]` tracing spans added to adapter execution entry points (submit, refresh_all_ordered, submit_refresh_request, submit_refresh_request_response)
- Unit tests added for `split_upgrade_target()` with cleanup marker parsing (4 cases)
- FFI lifecycle documented: no explicit shutdown, process-global state, poisoned-lock recovery
- `execute_batch_tolerant()` error scope documented: deliberate design choice for idempotent migration replay

### Documentation Alignment

- INTERFACES.md Section 10 filled with concrete inventories: 25 XPC methods, 27 FFI exports, 9 SQLite tables, task log status, confirmation token model
- On-device validation report template created with test matrices for all 6 locales
- Usability test plan created with core, error, accessibility, and locale scenarios plus acceptance criteria
- ROADMAP.md updated with cumulative beta.2-6 delivered scope
- CHANGELOG.md updated with beta.5 and beta.6 entries

---

## v0.13.0-rc.1 Status

### Inspector Sidebar
- Task detail view with status badge, task type, manager, label key/args
- Package detail view with version, status, pinned/restart-required indicators
- Manager detail view with health badge, installed/outdated counts, View Packages navigation
- Selection clearing fixes: selecting any entity properly clears conflicting selections
- Overview task rows wired to inspector via tap handling

### Upgrade Reliability
- Post-upgrade validation added to all 11 adapter upgrade handlers (Homebrew, RubyGems, npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, bundler, poetry)
- After upgrade command succeeds, each adapter re-checks `list_outdated` to verify the package was actually updated
- Silent upgrade failures now surface as `ProcessFailure` errors instead of being silently marked completed

### Status Menu
- "Control Center" item added to right-click status menu (opens dashboard overview)

### Documentation
- Security Advisory System incorporated into ROADMAP.md as milestone 1.3.x
- CHANGELOG.md, CURRENT_STATE.md, NEXT_STEPS.md updated

---

## v0.13.0-rc.2 Status

### Support & Feedback Entry Points
- New "Support & Feedback" card added to Settings surface with 5 actions:
  - Support Helm (opens GitHub Sponsors)
  - Report a Bug (opens GitHub issue template with optional diagnostics copy)
  - Request a Feature (opens GitHub issue template with optional diagnostics copy)
  - Send Feedback (opens mailto: with structured feedback form)
  - Copy Diagnostics (copies system info to clipboard with transient confirmation)
- "Include Diagnostics" toggle: when enabled, Report a Bug and Request a Feature copy diagnostics to clipboard before opening the issue template
- All 9 new L10n keys translated across all 6 locales (en, es, de, fr, pt-BR, ja)
- `.github/FUNDING.yml` created for GitHub Sponsors integration
- README.md updated with working sponsor and issue template links

---

## UI Redesign Artifacts (Integrated Baseline)

- A complete redesign concept package now exists under `docs/ui/`:
  - `REDESIGN_CONCEPT.md`
  - `INFORMATION_ARCHITECTURE.md`
  - `USER_FLOWS.md`
  - `VISUAL_SYSTEM.md`
  - `SWIFTUI_ARCHITECTURE.md`
  - `MOCKUPS.md`
- The redesign baseline is integrated into the production macOS target at `apps/macos-ui/Helm/` (legacy scaffold at `apps/macos/` removed in beta.5):
  - redesigned menu bar popover shell
  - top-of-popover updates attention banner with custom-styled upgrade-all action
  - layered overlay panels (search, quick settings, about, quit confirmation) with dimmed-underlay transitions
  - footer utility actions (search/settings/quit) + version-triggered About panel
  - dynamic status-item signal (Helm icon + update/error/running cues)
  - in-icon status badge overlays for update/error/running cues (no numeric title text)
  - status-item anchor glyph now preserves menu-appearance monochrome (black/white) while only badge indicators are colorized
  - right-click status-item quick actions (About, Upgrade All, Basic/Advanced Settings, Refresh, Quit)
  - popover panel now auto-sizes to content height to avoid unnecessary scrollbar appearance in normal active-task states
  - explicit light-mode brightness tuning for popover overlays/cards and control-center background gradients
  - dedicated control-center window (overview/updates/packages/tasks/managers/settings)
  - titlebar-hidden control-center presentation with integrated global search bar
  - control-center keyboard shortcuts (`Cmd+F` global search focus, `Cmd+W` window close)
  - full-row clickable sidebar navigation targets for control-center sections
  - tactile sidebar hover/press states and broader pointer affordance cues for interactive rows/actions
  - seamless full-height sidebar surface treatment with refined top-cap blending and darker sidebar step
  - redesigned control-center Settings surface (card-based layout, policy toggles, and operational actions)
  - manager-aware action badges in Settings, including explicit software-update blocked signal when Safe Mode is enabled
  - redesigned button language now favors custom Helm gradient/secondary controls across primary workflows (system button styling retained for destructive/reset-style actions)
  - manager health state model includes a dedicated gray "Not Installed" badge for undetected managers
  - inspector pane for manager/package context
  - live wiring to `HelmCore` data/actions for refresh, upgrade, package actions, and manager operations
  - guided onboarding walkthrough with SpotlightOverlay system (6 popover steps + 7 control center steps)
  - WalkthroughManager with UserDefaults persistence, skip, and replay from Settings
- Release packaging now includes a GitHub Actions workflow for signed universal DMG artifacts:
  - workflow: `.github/workflows/release-macos-dmg.yml`
  - output assets: versioned `Helm-<tag>-macos-universal.dmg` plus stable `Helm.dmg`
  - DMG layout includes standard drag-to-`Applications` alias

---

## Known Limitations

- Priority 1 language manager coverage is complete for the beta checkpoint:
  - Implemented: npm (global), pip (`python3 -m pip`, global), pipx, cargo, cargo-binstall
  - Pending: none
- Priority 2 extended language-manager expansion is complete at this checkpoint:
  - Implemented: pnpm (global), yarn (global), RubyGems, Poetry (self/plugins), Bundler
  - Pending: none
- Redesign integration is functional with layered popover UX + control-center search; accessibility labels and semantic grouping implemented; onboarding walkthrough delivered; UI layer purity cleanup completed
- Keyboard-only traversal: Tab navigation does not work in macOS SwiftUI (`.focusable()` does not participate in AppKit key view loop); requires NSViewRepresentable bridging approach
- All walkthrough and redesign localization keys have been rolled out to all 6 locales
- XPC call timeout enforcement added (30s data fetches, 300s mutations) with exponential backoff reconnection
- Overflow validation now has both heuristic and on-device executable coverage for Settings, onboarding, navigation, package filters, and manager labels/states
- Upgrade-all transparency now provides summary counts + top manager breakdown in confirmation flow
- Upgrade-preview filtering/sorting logic now has dedicated macOS UI unit coverage (`HelmTests/UpgradePreviewPlannerTests`)
- Dedicated upgrade preview UI surface is implemented in macOS Settings (execution-plan sections with manager breakdown)
- Dry-run mode is exposed in the upgrade preview UI (simulation path with no task submission)
- Onboarding flow updated with friendlier tone; guided walkthrough (spotlight/coach marks) now implemented
- No self-update mechanism yet
- Limited diagnostics UI
- No CLI interface

---

## Stability

- Pre-1.0
- Rapid iteration
- Breaking changes still possible

---

## Summary

Helm is a **functional control plane for 15 managers** with:

- Working orchestration
- Task system
- Pinning and policy
- Localization foundation (6 locales at full key parity)

The core architecture is in place. The Rust core passed a full audit with no critical issues.

Architecture cleanup completed in beta.5. Remaining work is **validation, hardening, and documentation toward 0.13.x stable**.
