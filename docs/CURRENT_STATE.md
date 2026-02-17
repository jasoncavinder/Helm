# Current State

This document describes the current implementation status of Helm.

It reflects reality, not intention.

---

## Version

Current version: **0.10.0-beta.1**

See:
- CHANGELOG.md

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
- 0.10.0-beta.1 — Priority 1 language-manager checkpoint

---

## Implemented Managers

Fully functional:

- Homebrew
- mise
- npm (global)
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
- Manager display-name localization keys now cover upgrade-preview/task-fallback manager labels (including software update/app store naming)

---

## Architecture Status

- Rust core: stable
- XPC service: stable
- FFI boundary: functional
- UI: feature-complete for current scope

---

## Known Limitations

- Priority 1 language manager coverage is complete for the beta checkpoint:
  - Implemented: npm (global), pip (`python3 -m pip`, global), pipx, cargo, cargo-binstall
  - Pending: none
- Overflow validation is still heuristic/script-based until full on-device visual pass is completed
- Upgrade-all transparency now provides summary counts + top manager breakdown in confirmation flow
- Upgrade-preview filtering/sorting logic now has dedicated macOS UI unit coverage (`HelmTests/UpgradePreviewPlannerTests`)
- No upgrade preview UI
- No dry-run mode exposed in UI
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

Helm is a **functional control plane for 10 managers** with:

- Working orchestration
- Task system
- Pinning and policy
- Localization foundation

The core architecture is in place.

Remaining work is **feature expansion and hardening toward 1.0**.
