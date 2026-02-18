# Current State

This document describes the current implementation status of Helm.

It reflects reality, not intention.

---

## Version

Current version: **0.13.0-beta.2**

See:
- CHANGELOG.md

Active milestone:
- 0.13.x — UI/UX analysis and redesign (beta in progress)

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

- en, es, de: broad app/service coverage
- fr, pt-BR, ja: full app/common/service key coverage
- Locale length audit script added at `apps/macos-ui/scripts/check_locale_lengths.sh` for overflow-risk preflight
- Locale key/placeholder integrity audit script added at `apps/macos-ui/scripts/check_locale_integrity.sh`
- `v0.11.0-beta.2` heuristic overflow audit captured at `docs/validation/v0.11.0-beta.2-l10n-overflow.md` (no high-risk candidates flagged)
- `v0.12.0-beta.1` on-device overflow validation captured at `docs/validation/v0.12.0-beta.1-visual-overflow.md` (Settings surface checks passing)
- Expanded on-device overflow validation coverage for onboarding/navigation/packages/managers captured at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`
- Manager display-name localization keys now cover upgrade-preview/task-fallback manager labels (including software update/app store naming)

Validation snapshot for `v0.11.0-beta.1` expansion:

- Priority 1 language-manager local smoke matrix captured on a macOS dev host:
  - Detected and smoke-tested: npm, pip (`python3 -m pip`), cargo
  - Not installed in the validation host environment: pipx, cargo-binstall

---

## Architecture Status

- Rust core: stable
- XPC service: stable
- FFI boundary: functional
- UI: feature-complete for current scope

---

## UI Redesign Artifacts (Integrated Baseline)

- A complete redesign concept package now exists under `docs/ui/`:
  - `REDESIGN_CONCEPT.md`
  - `INFORMATION_ARCHITECTURE.md`
  - `USER_FLOWS.md`
  - `VISUAL_SYSTEM.md`
  - `SWIFTUI_ARCHITECTURE.md`
  - `MOCKUPS.md`
- A standalone SwiftUI redesign scaffold now exists at `apps/macos/Helm/`:
  - menu bar app entry + popover
  - control-center window with section placeholders
  - shared state store and deterministic mock data
  - localized string resources for scaffold surfaces
- The redesign baseline is now integrated into the production macOS target at `apps/macos-ui/Helm/`:
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
- Redesign integration is functional and now includes layered popover UX + control-center search, but still needs iterative UX polish + accessibility validation
- Popover overlay transitions now respect reduced-motion settings, but a full VoiceOver + keyboard-only redesign sweep is still pending
- New redesign localization keys are currently expanded in `en` and require full non-English locale rollout parity
- Overflow validation now has both heuristic and on-device executable coverage for Settings, onboarding, navigation, package filters, and manager labels/states
- Upgrade-all transparency now provides summary counts + top manager breakdown in confirmation flow
- Upgrade-preview filtering/sorting logic now has dedicated macOS UI unit coverage (`HelmTests/UpgradePreviewPlannerTests`)
- Dedicated upgrade preview UI surface is implemented in macOS Settings (execution-plan sections with manager breakdown)
- Dry-run mode is exposed in the upgrade preview UI (simulation path with no task submission)
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
- Localization foundation

The core architecture is in place.

Remaining work is **feature expansion and hardening toward 1.0**.
