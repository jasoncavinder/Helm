---
title: Overview
slug: product-overview
description: What Helm is and how it works.
---

Helm is a native macOS menu bar app that gives you a single control plane for all your package managers and runtime tools. Instead of switching between `brew`, `mise`, `rustup`, `npm`, and others, Helm lets you see everything in one place — installed packages, available updates, and background tasks — all from your menu bar.

## Who is Helm for?

Helm serves two operational audiences:

- **Developers and power users on macOS** who want one local control plane for package and toolchain updates.
- **Platform, IT, and security teams** that need deterministic, auditable workflows for managed development environments.

Helm is planned as two product lifecycles: **Helm (Consumer)** and **Helm Business (Fleet)**.

## What it does today

Helm `v0.17.2` supports twenty-eight managers:

| Category | Managers |
|---------|----------|
| **Toolchain / Runtime** | mise, asdf (optional), rustup |
| **System / OS** | Homebrew (formulae), softwareupdate, MacPorts (optional), nix-darwin (optional) |
| **Language Package Managers** | npm (global), pnpm (global), yarn (global), poetry (self/plugins), RubyGems, bundler, pip (global), pipx, Cargo, cargo-binstall |
| **App / GUI Managers** | mas, Homebrew casks, Sparkle updaters (detection-only), Setapp (detection-only) |
| **Container / VM Managers** | Docker Desktop, podman, colima, Parallels Desktop (detection-only) |
| **Security / Firmware Managers** | Xcode Command Line Tools, Rosetta 2, Firmware Updates |

Key features:

- **Menu bar app** — lightweight floating panel, no Dock icon
- **Control Center window** — dedicated window with sidebar navigation (Overview, Updates, Packages, Tasks, Managers, Settings)
- **Inspector sidebar** — contextual detail panel for selected task, package, or manager
- **Dashboard** — package stats, manager grid, and recent task activity at a glance
- **Package list** — browse installed, upgradable, and available packages with status filters and manager filter
- **Progressive search** — instant local filtering with debounced remote search
- **Pinning and safe mode controls** — pin/unpin support plus guarded OS update policy
- **Authority-ordered refresh** — Authoritative (mise, asdf, rustup) → Standard managers → Guarded (Homebrew, softwareupdate, MacPorts, nix-darwin, Xcode CLT, Rosetta 2, Firmware Updates)
- **Restart detection** — surface restart-required updates from macOS softwareupdate
- **Post-upgrade validation** — verify package state after upgrades complete
- **Background tasks** — real-time task tracking with per-manager serial execution
- **Onboarding walkthrough** — guided first-launch experience with spotlight highlights across popover and control center
- **Localization** — `en`, `es`, `de`, `fr`, `pt-BR`, and `ja` with locale override in Settings
- **Upgrade transparency** — dedicated upgrade preview surface with scoped execution and failure-attribution visibility

> **Current Track:** `v0.17.2` is the latest stable release on `main`; `0.18.x` planning is in progress. Please report issues at [GitHub Issues](https://github.com/jasoncavinder/Helm/issues/new/choose).

## How it works

Helm has a three-layer architecture:

| Layer | Technology | Role |
|-------|-----------|------|
| **UI** | SwiftUI | Menu bar app with floating panel — reads state, emits intents |
| **Service** | XPC | Hosts Rust core in a separate unsandboxed process for shell access |
| **Core** | Rust | All business logic, adapters, orchestration, and persistence |

The XPC boundary isolates process execution from the sandboxed app. The Rust core is UI-agnostic and fully testable with fixture-based deterministic tests.

Each package manager is implemented as an **adapter** — a self-contained module that knows how to detect, list, search, and manage packages for that specific tool. Adapters declare their capabilities, and the orchestration engine handles scheduling, parallelism, and failure isolation.
