---
title: Changelog
description: Release notes for Helm.
---

All notable changes to Helm are documented here. The format is based on [Keep a Changelog](https://keepachangelog.com/) and follows SemVer-compatible versioning.

For the full changelog, see [CHANGELOG.md on GitHub](https://github.com/jasoncavinder/Helm/blob/main/CHANGELOG.md).

---

## 0.14.0-rc.1 — 2026-02-19

### Added
- Completed 0.14 manager rollout across alpha.1 through alpha.5:
  - Container/VM managers: Docker Desktop, podman, colima
  - Detection-only managers: Sparkle, Setapp, Parallels Desktop
  - Security/Firmware managers: Xcode Command Line Tools, Rosetta 2, Firmware Updates
  - Optional managers: asdf, MacPorts, nix-darwin
  - Homebrew cask status manager (`homebrew_cask`)
- Added 0.14 manager capability sweep artifact at `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`
- Added future-state distribution/licensing planning docs:
  - multi-channel build matrix (MAS, Developer ID, Setapp, Fleet)
  - channel licensing/update authority mapping
  - staged roadmap phases for Sparkle, MAS, Setapp, Fleet, PKG/MDM, and offline licensing

### Changed
- FFI manager status now reports full 0.14 implementation baseline (`isImplemented=true` for all registry managers)
- Optional managers remain default-disabled until explicitly enabled (`asdf`, `macports`, `nix_darwin`)
- Swift fallback manager metadata aligned to 0.14 implementation coverage
- Clarified consumer vs fleet lifecycle separation in architecture/enterprise planning docs (documentation-only)

---

## 0.13.0 — 2026-02-19

### Added
- Website: changelog page, visual tour with 10 UI screenshots, FAQ & troubleshooting guide
- Website: Open Graph and Twitter Card meta tags for social sharing
- Website: Starlight `lastUpdated` timestamps on all pages

### Changed
- Promoted `0.13.x` beta and rc slices into the stable `v0.13.0` checkpoint

---

## 0.13.0-rc.2 — 2026-02-19

### Added
- "Support & Feedback" card in control-center Settings with 5 action buttons: Support Helm (GitHub Sponsors), Report a Bug, Request a Feature, Send Feedback (mailto), Copy Diagnostics
- "Include Diagnostics" toggle that copies diagnostics to clipboard before opening GitHub issue templates
- Transient "Copied!" confirmation with animated opacity transition
- "Support Helm" submenu in right-click status menu with GitHub Sponsors and Patreon items
- `.github/FUNDING.yml` for GitHub Sponsors and Patreon integration
- 11 new L10n keys across all 6 locales

### Changed
- AppDelegate decomposed to satisfy SwiftLint file/function length thresholds
- README.md updated with funding links and issue template links

---

## 0.13.0-rc.1 — 2026-02-18

### Added
- Inspector sidebar task detail view with status badge, task type, manager name, label key, and label args
- Post-upgrade validation across all 11 adapter upgrade handlers — prevents silent upgrade failures
- Control Center menu item in status menu right-click
- Manager inspector enriched with health badge, installed/outdated counts, and View Packages navigation
- Security Advisory System milestone added to ROADMAP.md (1.3.x)
- 16 new L10n keys for inspector views across all 6 locales

### Fixed
- Inspector selection clearing: selecting a manager no longer shows stale package inspector
- RubyGems upgrade tasks showing "completed" when packages were not actually updated

### Changed
- Task labels now include package names for upgrade tasks across all managers

---

## 0.13.0-beta.6 — 2026-02-18

### Added
- Structured `#[instrument]` tracing spans on adapter execution entry points
- Unit tests for Homebrew `split_upgrade_target()` function
- On-device validation report template and usability test plan
- INTERFACES.md Section 10 filled with concrete XPC, FFI, and SQLite inventories

---

## 0.13.0-beta.5 — 2026-02-18

### Added
- XPC timeout enforcement on all service calls (30s data fetches, 300s mutations)
- Exponential backoff on XPC reconnection (2s base, doubling to 60s cap)

### Changed
- Search deduplication logic extracted from views to HelmCore
- Task-to-manager inference removed: tasks now carry `managerId` directly
- Authority, capability, and symbol lookups consolidated as `ManagerInfo` computed properties
- Legacy redesign scaffold removed (18 files)

---

## 0.13.0-beta.4 — 2026-02-18

### Added
- Guided onboarding walkthrough with SpotlightOverlay component (6 popover + 7 control-center steps)
- WalkthroughManager with UserDefaults persistence, skip, and replay from Settings
- 31 walkthrough L10n keys across all 6 locales

### Changed
- Onboarding copy updated across all 6 locales for friendlier tone

---

## 0.13.0-beta.3 — 2026-02-18

### Added
- VoiceOver accessibility support across all interactive UI elements
- Task cancel button wired to XPC `cancelTask` method
- Per-manager "Upgrade All" button in Managers view
- CI test enforcement (`ci-test.yml` with cargo test + xcodebuild test)

### Changed
- HelmCore decomposed from 1,133 lines into 5 files
- DashboardView decomposed from 1,919 lines into 4 files
- SwiftLint thresholds tightened

---

## 0.13.0-beta.2 — 2026-02-18

### Added
- Universal-build support for Apple Silicon + Intel
- Release automation workflow for signed DMG packaging
- Beta binary installation guidance on website

---

## 0.13.0-beta.1 — 2026-02-18

### Added
- Redesigned menu bar popover with updates attention banner, layered overlays, and right-click quick actions
- Redesigned control-center with titlebar-hidden presentation, integrated global search, and card-based Settings
- Keyboard shortcuts (`Cmd+F`, `Esc`, `Cmd+W`)

---

## 0.12.0 — 2026-02-17

### Added
- Localization hardening across all 6 shipped locales (key parity, placeholder consistency, overflow validation)
- Dedicated Upgrade Preview UI surface with execution-plan sections
- Dry-run support in Upgrade Preview flow

---

## 0.11.0-beta.2 — 2026-02-17

### Added
- Bounded retry handling for transient task-store persistence failures
- Regression coverage for refresh-response error attribution

---

## 0.10.0 — 2026-02-17

### Added
- Priority 1 core language-manager support: npm (global), pipx, pip (global), cargo, cargo-binstall
- Package-identifier validation on mutating adapter actions
- Poisoned-lock recovery at FFI boundary

---

## 0.9.0 — 2026-02-14

### Added
- Internationalization foundation: `LocalizationManager`, type-safe `L10n` accessor, JSON locale architecture
- Language picker in Settings (persisted to UserDefaults)
- All user-facing strings localized

---

## 0.8.0 — 2026-02-14

### Added
- End-to-end pinning and policy controls (native Homebrew pin/unpin, virtual pin fallback, safe mode)
- Individual package upgrade actions for Homebrew, mise, and rustup
- `helm_upgrade_all` orchestration with pin filtering and safe-mode enforcement

---

## 0.7.0 — 2026-02-13

### Added
- Manager update/self-update controls (Homebrew update, mas/mise upgrades, rustup self-update)
- Authority-order regression tests for capability-aware refresh
- `mas` package parsing using app names instead of numeric IDs
