---
title: Roadmap
description: Helm milestones from alpha to 1.0.
---

Helm follows feature-driven milestones. Dates are intentionally omitted — milestones ship when they're ready.

## Completed

| Version | Milestone |
|---------|-----------|
| 0.1.x | Core foundation — Rust workspace, adapter traits, capability model, SQLite schema |
| 0.2.x | First adapter — Homebrew detection, listing, fixture-based tests |
| 0.3.x | Orchestration engine — task queue, per-manager serialization, cancellation |
| 0.4.x | SwiftUI shell — menu bar app, XPC service, Rust FFI bridge, live UI |
| 0.5.x | Progressive search — local-first search, remote search, cache enrichment |
| 0.6.x | Core toolchain managers — mise, rustup adapters, authority ordering |

## Upcoming

| Version | Milestone |
|---------|-----------|
| 0.7.x | System & App Store managers — macOS `softwareupdate`, `mas`, guarded execution |
| 0.8.x | Pinning & policy enforcement — native/virtual pins, manager toggles, safe mode |
| 0.9.x | Upgrade preview & execution transparency — bulk preview, dry-run, ordered plans |
| 0.10.x | Self-update & installer hardening — signed updates, delta updates |
| 0.11.x | Diagnostics & logging — per-task log viewer, structured error export |
| 0.12.x | Stability & pre-1.0 hardening — full test matrix, stress tests, crash recovery |
| **1.0.0** | **Stable control plane release** |

## Details

For full milestone definitions, exit criteria, and delivered features, see the [ROADMAP.md on GitHub](https://github.com/jasoncavinder/Helm/blob/main/docs/ROADMAP.md).
