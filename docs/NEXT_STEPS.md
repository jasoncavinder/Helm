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
- UI/UX redesign hardening and accessibility validation
- Localization parity for redesign-specific keys
- Release-quality polish and diagnostics follow-through
- Hardening and diagnostics

Current checkpoint:
- `v0.13.0-beta.2` released (redesigned shell + universal build/distribution pipeline checkpoint)

Next release target:
- `v0.13.0-beta.3` (redesign accessibility + locale parity + usability validation checkpoint)

---

## Priority 1 — Core Language Managers

Implement:

- npm (global)
- pipx
- pip
- cargo
- cargo-binstall

Requirements:

- detection
- list_installed
- list_outdated
- search (where possible)
- install / uninstall / upgrade

Completed:

- npm (global) adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- npm parser fixtures and adapter unit tests added for version/list/search/outdated flows
- pip (`python3 -m pip`, global) adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- pip parser fixtures and adapter unit tests added for version/list/search/outdated flows
- pipx adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- pipx parser fixtures and adapter unit tests added for version/list/outdated flows
- cargo adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- cargo parser fixtures and adapter unit tests added for version/list/search/outdated flows
- cargo-binstall adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- cargo-binstall parser fixtures and adapter unit tests added for version/list/search/outdated flows

`v0.10.0` completion scope:

- End-to-end adapter availability for:
  - npm (global) ✅
  - pip (`python3 -m pip`, global) ✅
  - pipx ✅
  - cargo ✅
  - cargo-binstall ✅
- Registry + FFI + XPC + UI wiring verified for all Priority 1 managers
- Fixture-based parser coverage for list/search/version flows where supported
- Capability declarations aligned with implemented actions

`v0.10.0` validation summary:

- `cargo test` passes in `core/rust` ✅
- Existing `HelmTests` suite passes ✅
- Manager detection and package listing validate on at least one local dev environment ✅
- `CHANGELOG.md`, `CURRENT_STATE.md`, and website docs are updated for stable scope ✅

---

## Priority 2 — Extended Managers

Implement:

- pnpm
- yarn
- poetry
- RubyGems
- bundler

Completed (`v0.11.0-beta.1` scope complete):

- pnpm adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- pnpm parser fixtures and adapter unit tests added for version/list/search/outdated flows
- pnpm manager metadata wired through macOS UI + localization keys
- yarn adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- yarn parser fixtures and adapter unit tests added for version/list/search/outdated flows
- yarn manager metadata wired through macOS UI + localization keys
- RubyGems adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- RubyGems parser fixtures and adapter unit tests added for version/list/search/outdated flows
- RubyGems manager metadata wired through macOS UI + localization keys
- poetry adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- poetry parser fixtures and adapter unit tests added for self/plugin version/list/search/outdated flows
- poetry manager metadata wired through macOS UI + localization keys
- bundler adapter implemented end-to-end (core adapter + process source + FFI/runtime wiring)
- bundler parser fixtures and adapter unit tests added for runtime version/list/search/outdated flows
- bundler manager metadata wired through macOS UI + localization keys

---

## Priority 3 — Localization Expansion

Add locales:

- es
- fr
- de
- pt-BR
- ja

Requirements:

- onboarding flows translated
- errors translated
- UI validated for overflow

Completed:

- Added locale scaffolding for fr, pt-BR, ja
- Added onboarding flow translations for fr, pt-BR, ja
- Added common UI label translations for fr, pt-BR, ja
- Added service error translations for fr, pt-BR, ja
- Expanded fr, pt-BR, ja to full app/common/service key coverage
- Exposed fr, pt-BR, ja in the macOS language picker
- Added a locale overflow-risk audit script (`apps/macos-ui/scripts/check_locale_lengths.sh`)
- Increased Settings popover and locale picker widths to reduce language-picker overflow risk
- Added localized manager display-name keys used by upgrade-preview/task-fallback UI text
- Added locale key/placeholder integrity audit script (`apps/macos-ui/scripts/check_locale_integrity.sh`)
- Added CI enforcement for locale parity + locale integrity checks in `.github/workflows/i18n-lint.yml`
- Added `HelmTests`-based visual overflow validation for `SettingsPopoverView` locale-sensitive controls (`LocalizationOverflowValidationTests`)
- Captured `v0.12.0-beta.1` visual overflow validation report at `docs/validation/v0.12.0-beta.1-visual-overflow.md`
- Increased settings popover + language picker widths to resolve validated overflow failures
- Expanded `LocalizationOverflowValidationTests` coverage to onboarding/navigation/package-filter/manager-surface constrained labels
- Captured `v0.12.0-beta.2` overflow-expansion validation report at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`

Remaining:

- None for Priority 3 at this checkpoint

---

## Priority 4 — Upgrade Transparency

Implement:

- Upgrade preview UI
- Execution plan display
- Dry-run support

Completed:

- Added a localized execution-plan summary in the Upgrade All confirmation alert (no-OS vs with-OS counts)
- Added manager-level package-count breakdown (top managers) in the Upgrade All confirmation alert
- Localized manager labels used in the Upgrade All breakdown output
- Added focused unit tests for upgrade-preview filtering and breakdown ordering (`UpgradePreviewPlannerTests`)
- Added a dedicated Upgrade Preview UI surface in macOS Settings with execution-plan sections and manager-level package breakdowns for both no-OS and with-OS modes
- Added dry-run mode support in the upgrade-preview UI with explicit simulation results and no task submission

Remaining:

- None for Priority 4 at this checkpoint

---

## Priority 5 — UI/UX Analysis & Redesign

Completed:

- Created redesign concept, IA, flows, visual system, SwiftUI architecture proposal, and annotated mockups under `docs/ui/`
- Added standalone SwiftUI redesign scaffold under `apps/macos/Helm/` with:
  - menu bar integration and status popover
  - control-center window + section placeholders
  - deterministic mock state/data for previews and iteration
  - localization-first string resources for user-facing scaffold text
- Integrated redesign baseline into `apps/macos-ui` target with:
  - redesigned menu bar popover and control-center window
  - section surfaces for overview/updates/packages/tasks/managers/settings
  - inspector pane and manager/package/task interaction wiring
  - live `HelmCore`-backed actions for refresh/upgrade/package pin-update and manager operations
- Refined redesigned shell behavior in `apps/macos-ui` with:
  - top attention banner in popover for pending updates + custom update-all affordance
  - layered popover panels (search/settings/about/quit confirmation) with dimmed-underlay transitions
  - compact utility footer actions (search/settings/quit) and version-triggered About panel
  - dynamic menu-bar status signal (Helm icon with update/error/running cues)
  - in-icon status-item badge overlay and right-click quick action menu
  - status-item icon treatment updated to monochrome anchor + colorized state badge indicators
  - popover outside-click dismissal hardening + overlay cursor/hit-testing cleanup
  - auto-sized popover height to reduce avoidable scrollbar churn at active-task peaks
  - reduced-motion-aware overlay transitions and keyboard shortcuts (`Cmd+F`, `Esc`, `Cmd+W`)
  - titlebar-hidden control-center window chrome with integrated global search routing into Packages
  - compacted control-center top bar alignment with window controls and fixed-size non-resizable window behavior
  - tuned light-mode visual balance for popover overlays/cards and control-center gradients
  - full-row clickable control-center sidebar navigation behavior
  - tactile sidebar hover/press states and broadened pointer affordance cues
  - seamless/darker full-height sidebar surface integration through the titlebar cap region
  - redesigned control-center Settings section to card-based production-quality layout
  - manager-aware Settings action badges (including software-update blocked signaling under Safe Mode)
  - migrated core non-destructive workflows to custom Helm primary/secondary button styles
  - introduced explicit gray "Not Installed" manager health badge state
- Added release artifact pipeline for macOS beta distribution:
  - signed universal build intent (`arm64` + `x86_64`) in Xcode/Rust build flow
  - GitHub Actions DMG packaging with drag-to-`Applications` installer UX

Implement (remaining):

- Usability test plan and acceptance metrics for redesigned flows
- On-device validation sweep for redesigned states (loading/success/error/partial failure) across supported locales
- Localization parity update for redesign-specific keys across shipped non-English locales
- Remove or archive legacy UI component paths no longer used by the redesigned shell
- Accessibility QA pass for keyboard traversal and VoiceOver labels in redesigned surfaces

---

## Priority 6 — Self Update

Implement:

- Signed updates
- Integrity verification
- Update recovery

---

## Priority 7 — Diagnostics

Implement:

- Task log viewer
- Error export
- Manager diagnostics panel

---

## Priority 8 — Hardening

Completed in `v0.10.0` checkpoint:

- Targeted adapter hardening review for regression/robustness/security risks across Priority 1 language-manager paths
- Package-identifier validation on mutating adapter actions for npm/pip/pipx/cargo/cargo-binstall
- Shared cargo/cargo-binstall outdated synthesis logic to reduce duplication and drift risk
- Replaced panic-prone FFI `lock().unwrap()` usage with poisoned-lock recovery
- Resolved website duplicate docs-id build warnings for overview/roadmap pages

Completed in `v0.11.0-beta.2` stabilization:

- Added bounded retry handling for transient task-store create/update persistence failures in orchestration runtime paths
- Added regression coverage for refresh-response error attribution and transient task-persistence recovery

Remaining:

- Stress test orchestration
- Cancellation reliability
- Memory audit
- FFI stability

---

## Non-Goals (Pre-1.0)

- Plugin system
- CLI tool
- Cloud sync
- Enterprise control plane

---

## Summary

Next steps are focused on:

- Expanding manager coverage
- Improving transparency
- Hardening reliability
- Integrating and validating the UI/UX redesign baseline

The goal is **closing 1.0 Definition of Done**.
