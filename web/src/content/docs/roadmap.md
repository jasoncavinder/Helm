---
title: Roadmap
slug: product-roadmap
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
| 0.10.x | Core Language Package Managers — npm, pipx, pip, Cargo, cargo-binstall |
| 0.11.x | Extended Language Package Managers — pnpm, yarn, poetry, RubyGems, bundler |
| 0.12.x | Localization + Upgrade Transparency — locale hardening, overflow validation expansion, dedicated upgrade preview, dry-run |

## Completed (Recent)

| Version | Milestone |
|---|---|
| 0.13.x | UI/UX Analysis & Redesign — full UX audit, interaction model, visual system refresh, accessibility, onboarding walkthrough, inspector sidebar, support & feedback entry points (`v0.13.0` stable released) |
| 0.14.x | Platform, Detection & Optional Managers — Docker, Xcode, Rosetta, Sparkle, Setapp, Homebrew casks, optional managers (`v0.14.0-rc.1` release candidate) |

> **Testing:** `v0.14.0-rc.1` is available. Submit feedback via [GitHub Issues](https://github.com/jasoncavinder/Helm/issues/new/choose).

## Planned

| Version | Milestone |
|---|---|
| 0.15.x | Advanced Upgrade Transparency — richer execution-plan visibility, failure isolation, and operator controls |
| 0.16.x | Self-Update & Installer Hardening — Sparkle integration for direct Developer ID channel, signed verification |
| 0.17.x | Diagnostics & Logging — log viewer, structured error export, health panel |
| 0.18.x | Stability & Pre-1.0 Hardening — stress tests, crash recovery, memory audit |
| 1.0.0 | Stable Control Plane Release — production-safe execution, full feature set |

## Post-1.0

| Version | Milestone |
|---|---|
| 1.1.x | Globalization Expansion — additional locales (zh-Hans, ko, it, nl), website localization |
| 1.2.x | Editions and Entitlement Foundations — channel-aware build matrix and entitlement boundaries for consumer and fleet products |
| 1.3.x | Security Advisory System (Pro) — CVE awareness, local-first advisory evaluation |
| 1.4.x | Business Policy and Drift Management — scoped policy, baseline enforcement, compliance reporting |
| 1.5.x | Enterprise Rollout, Approvals, and Audit — ring-based rollout, approval workflows, audit export |
| 1.6.x | Mac App Store Distribution Channel — consumer MAS lifecycle and App Store authority alignment |
| 1.7.x | Setapp Distribution Channel — consumer Setapp lifecycle and channel authority alignment |
| 1.8.x | Helm Business Fleet Product — separate business binary and lifecycle from consumer releases |
| 1.9.x | PKG + MDM Deployment and Offline Licensing — fleet deployment workflow and offline org licensing model |

## Details

For full milestone definitions, exit criteria, and delivered features, see the [ROADMAP.md on GitHub](https://github.com/jasoncavinder/Helm/blob/main/docs/ROADMAP.md).
