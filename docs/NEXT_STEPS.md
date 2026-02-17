# Next Steps

This document defines the immediate priorities for Helm development.

It is intentionally tactical.

---

## Current Phase

Helm is in:

```

0.10.x – 0.13.x

```

Focus:
- Manager expansion
- Localization expansion
- UI/UX redesign planning

Current checkpoint:
- `v0.10.0-beta.1` delivered (Priority 1 language-manager milestone checkpoint + transparency hardening)

Next release target:
- `v0.10.0-beta.2` (hardening/review checkpoint + roadmap sequencing update)

After `v0.10.0-beta.2`:
- `v0.11.0-beta.1` (extended language-manager expansion)

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

`v0.10.0-beta.1` checkpoint scope:

- End-to-end adapter availability for:
  - npm (global) ✅
  - pip (`python3 -m pip`, global) ✅
  - pipx ✅
  - cargo ✅
  - cargo-binstall ✅
- Registry + FFI + XPC + UI wiring verified for all Priority 1 managers
- Fixture-based parser coverage for list/search/version flows where supported
- Capability declarations aligned with implemented actions

`v0.10.0-beta.1` acceptance criteria:

- `cargo test` passes in `core/rust` ✅
- Existing `HelmTests` suite passes ✅
- Manager detection and package listing validate on at least one local dev environment ✅
- `CHANGELOG.md`, `CURRENT_STATE.md`, and website docs are updated for beta scope ✅

---

## Priority 2 — Extended Managers

Implement:

- pnpm
- yarn
- poetry
- RubyGems
- bundler

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

Remaining:

- Validate UI overflow across es, fr, de, pt-BR, ja

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

Remaining:

- Add dedicated upgrade preview UI surface
- Add dry-run support

---

## Priority 5 — UI/UX Analysis & Redesign

Implement:

- Full interaction-flow audit (onboarding, refresh, search, upgrade, error handling)
- Information architecture review for dashboard/packages/tasks/managers/settings
- Visual hierarchy, typography, spacing, and state-feedback redesign proposals
- Usability test plan and acceptance metrics for redesigned flows
- Incremental implementation plan with non-breaking migration checkpoints

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

Completed in `v0.10.0-beta.2`:

- Targeted adapter hardening review for regression/robustness/security risks across Priority 1 language-manager paths
- Package-identifier validation on mutating adapter actions for npm/pip/pipx/cargo/cargo-binstall
- Shared cargo/cargo-binstall outdated synthesis logic to reduce duplication and drift risk

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
- Preparing the UI/UX redesign milestone

The goal is **closing 1.0 Definition of Done**.
