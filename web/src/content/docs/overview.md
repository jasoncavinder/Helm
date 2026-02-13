---
title: Overview
description: What Helm is and how it works.
---

Helm is a native macOS menu bar app that gives you a single control plane for all your package managers and runtime tools. Instead of switching between `brew`, `mise`, `rustup`, `npm`, and others, Helm lets you see everything in one place — installed packages, available updates, and background tasks — all from your menu bar.

## Who is Helm for?

Developers and power users on macOS who manage software through multiple package managers and want a unified, safe way to keep everything up to date.

## What it does today

Helm v0.6.0 supports three package managers with more planned:

| Manager | Type | Capabilities | Status |
|---------|------|-------------|--------|
| **Homebrew** | System package manager | Detect, list installed, list outdated, search | Active |
| **mise** | Tool/runtime manager | Detect, list installed, list outdated | Active |
| **rustup** | Rust toolchain manager | Detect, list installed, list outdated | Active |

Key features:

- **Menu bar app** — lightweight floating panel, no Dock icon
- **Dashboard** — package stats, manager grid, and recent task activity at a glance
- **Package list** — browse installed, upgradable, and available packages with status filters and manager filter
- **Progressive search** — instant local filtering with debounced remote search
- **Authority-ordered refresh** — toolchain managers (mise, rustup) refresh before package managers (Homebrew)
- **Background tasks** — real-time task tracking with per-manager serial execution

## How it works

Helm has a three-layer architecture:

| Layer | Technology | Role |
|-------|-----------|------|
| **UI** | SwiftUI | Menu bar app with floating panel — reads state, emits intents |
| **Service** | XPC | Hosts Rust core in a separate unsandboxed process for shell access |
| **Core** | Rust | All business logic, adapters, orchestration, and persistence |

The XPC boundary isolates process execution from the sandboxed app. The Rust core is UI-agnostic and fully testable with fixture-based deterministic tests.

Each package manager is implemented as an **adapter** — a self-contained module that knows how to detect, list, search, and manage packages for that specific tool. Adapters declare their capabilities, and the orchestration engine handles scheduling, parallelism, and failure isolation.
