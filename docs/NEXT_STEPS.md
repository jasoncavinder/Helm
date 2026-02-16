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
- Added service error translations for fr, pt-BR, ja
- Exposed fr, pt-BR, ja in the macOS language picker

Remaining:

- Validate UI overflow across es, fr, de, pt-BR, ja
- Expand fr, pt-BR, ja translations beyond onboarding/errors

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
