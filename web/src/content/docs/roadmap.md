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
| 0.14.x | Platform, Detection & Optional Managers — Docker, Xcode, Rosetta, Sparkle, Setapp, Homebrew casks, optional managers (`v0.14.x` stable, latest patch `v0.14.1`) |
| 0.15.x | Advanced Upgrade Transparency — richer execution-plan visibility, failure isolation, and operator controls (`v0.15.0` released) |
| 0.16.x | Self-Update & Installer Hardening — Sparkle integration for direct Developer ID channel, signed verification (`v0.16.0` released) |

> **Testing:** `v0.16.0` is available. Submit feedback via [GitHub Issues](https://github.com/jasoncavinder/Helm/issues/new/choose).

## Planned

| Version | Milestone |
|---|---|
| 0.17.x | Diagnostics & Logging — log viewer, structured error export, health panel |
| 0.18.x | Local Security Groundwork — local vulnerability abstractions and cache plumbing (internal only) |
| 0.19.x | Stability & Pre-1.0 Hardening — stress tests, crash recovery, memory audit |
| 1.0.0 | Stable Control Plane Release — production-safe execution, full feature set |

## Post-1.0

| Version | Milestone |
|---|---|
| 1.1.x | Globalization Expansion — additional locales (zh-Hans, ko, it, nl), website localization |
| 1.2.x | Editions and Entitlement Foundations — channel-aware build matrix and entitlement boundaries for consumer and fleet products |
| 1.3.x | Security Advisory System (Pro) — local-first CVE scanning, optional public advisory API queries, local TTL cache |
| 1.4.x | Shared Brain — fingerprint sharing, known-fix lookup, centralized Postgres services, App Attest auth |
| 1.5.x | Business Policy and Drift Management — scoped policy, baseline enforcement, compliance reporting |
| 1.6.x | Enterprise Rollout, Approvals, and Audit — ring-based rollout, approval workflows, audit export |
| 1.7.x | Mac App Store Distribution Channel — consumer MAS lifecycle and App Store authority alignment |
| 1.8.x | Setapp Distribution Channel — consumer Setapp lifecycle and channel authority alignment |
| 1.9.x | Helm Business Fleet Product — separate business binary and lifecycle from consumer releases |
| 1.10.x | PKG + MDM Deployment and Offline Licensing — fleet deployment workflow and offline org licensing model |

## Details

For full milestone definitions, exit criteria, and delivered features, see the [ROADMAP.md on GitHub](https://github.com/jasoncavinder/Helm/blob/main/docs/ROADMAP.md).
