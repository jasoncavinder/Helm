# Next Steps

This document defines the immediate priorities for Helm development.

It is intentionally tactical.

---

## Current Phase

Helm is in:

```

0.10.x – 0.12.x

```

Focus:
- Manager expansion
- Localization expansion

Immediate target after `v0.9.3`:
- `v0.10.0-beta.1` (core language-manager milestone checkpoint)

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

`v0.10.0-beta.1` checkpoint scope:

- End-to-end adapter availability for:
  - npm (global)
  - pipx
  - pip (`python3 -m pip`, global)
  - cargo
  - cargo-binstall
- Registry + FFI + XPC + UI wiring verified for all Priority 1 managers
- Fixture-based parser coverage for list/search/version flows where supported
- Capability declarations aligned with implemented actions

`v0.10.0-beta.1` acceptance criteria:

- `cargo test` passes in `core/rust`
- Existing `HelmTests` suite passes
- Manager detection and package listing validate on at least one local dev environment
- `CHANGELOG.md`, `CURRENT_STATE.md`, and website docs are updated for beta scope

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

---

## Priority 4 — Upgrade Transparency

Implement:

- Upgrade preview UI
- Execution plan display
- Dry-run support

---

## Priority 5 — Self Update

Implement:

- Signed updates
- Integrity verification
- Update recovery

---

## Priority 6 — Diagnostics

Implement:

- Task log viewer
- Error export
- Manager diagnostics panel

---

## Priority 7 — Hardening

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

The goal is **closing 1.0 Definition of Done**.
