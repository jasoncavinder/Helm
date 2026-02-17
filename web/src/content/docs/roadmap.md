---
title: Roadmap
description: Helm milestones from alpha to 1.0.
---

Helm follows feature-driven milestones. Dates are intentionally omitted — milestones ship when they're ready.

## Completed

| Version | Milestone |
|---|---|
| 0.1.x | Core Foundation — Rust workspace, adapter trait, capability model |
| 0.2.x | First Adapter — Homebrew detection, listing, fixture-based tests |
| 0.3.x | Orchestration Engine — task queue, per-manager serialization, cancellation |
| 0.4.x | SwiftUI Shell — menu bar app, XPC service, Rust FFI bridge, live UI |
| 0.5.x | Progressive Search — local-first search, remote search, cache enrichment |
| 0.6.x | Core Toolchain Managers — mise, rustup adapters, authority ordering |
| 0.7.x | System & App Store Managers — softwareupdate, mas, restart detection |
| 0.8.x | Pinning & Policy Enforcement — native/virtual pins, safe mode, guarded updates |
| 0.9.x | Internationalization Foundation — centralized localization system, ICU format |

## In Progress (Beta)

| Version | Milestone |
|---|---|
| 0.10.x | Core Language Package Managers — checkpoint `v0.10.0-beta.1` delivered (npm, pipx, pip, Cargo, cargo-binstall) |
| 0.11.x | Extended Language Package Managers — pnpm, yarn, poetry, RubyGems, bundler |
| 0.12.x | Localization Expansion — non-English locale coverage hardening and overflow validation |

## Planned

| Version | Milestone |
|---|---|
| 0.13.x | Platform, Detection & Optional Managers — Docker, Xcode, Rosetta, Sparkle |
| 0.14.x | Upgrade Preview & Execution Transparency — bulk preview, dry-run, failure isolation |
| 0.15.x | Self-Update & Installer Hardening — Sparkle integration, signed verification |
| 0.16.x | Diagnostics & Logging — log viewer, structured error export, health panel |
| 0.17.x | Stability & Pre-1.0 Hardening — stress tests, crash recovery, memory audit |
| 1.0.0 | Stable Control Plane Release — production-safe execution, full feature set |

## Details

For full milestone definitions, exit criteria, and delivered features, see the [ROADMAP.md on GitHub](https://github.com/jasoncavinder/Helm/blob/main/docs/ROADMAP.md).
