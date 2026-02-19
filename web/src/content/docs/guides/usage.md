---
title: Usage
description: How to use Helm to manage your packages.
---

Helm runs as a menu bar app with two surfaces: a **menu bar popover** for quick triage and a separate **Control Center window** for deeper management.

## Menu Bar Popover

The popover opens with a single click on the menu bar icon and provides an at-a-glance summary:

- **Health status** — overall environment posture (healthy, attention, error)
- **Pending updates count** — how many packages have upgrades available
- **Quick actions** — Refresh, Upgrade All, and Open Control Center
- **Live task strip** — currently running tasks with progress

## Control Center Window

The Control Center is a standalone window with sidebar navigation across six domains:

1. **Overview** — health posture, package stats, manager grid, and recent task activity
2. **Updates** — pending upgrades grouped by authority tier with bulk-plan builder
3. **Packages** — browse installed, upgradable, and available packages with status and manager filters
4. **Tasks** — real-time task tracking with lifecycle states (queued, running, completed, failed, canceled)
5. **Managers** — per-manager health, capabilities, and configuration
6. **Settings** — cadence, policy, localization, accessibility, and diagnostics

## Inspector Sidebar

Selecting any task, package, or manager in the Control Center opens the **inspector sidebar** on the right. It shows contextual detail: version delta, pin state, manager attribution, available actions, task logs, or manager capabilities — depending on what is selected.

## Supported Managers

Helm supports fifteen package managers across four categories:

| Category | Managers |
|---------|----------|
| **Toolchain / Runtime** | mise, rustup |
| **System / OS / App Store** | Homebrew, softwareupdate, mas |
| **Core Language** | npm (global), pipx, pip (global), Cargo, cargo-binstall |
| **Extended Language** | pnpm (global), yarn (global), Poetry (self/plugins), RubyGems, Bundler |

## Refreshing

Click the refresh button to update all package data. Helm refreshes managers in a **3-phase authority model**:

1. **Authoritative** (mise, rustup) — toolchain managers that define the runtime environment; refreshed first
2. **Standard** (npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, RubyGems, Poetry, Bundler, mas) — language and app store managers
3. **Guarded** (Homebrew, softwareupdate) — system-level managers that may require privileges or restarts; refreshed last

Within each phase, managers refresh in parallel. If one manager fails, the others continue unaffected.

## Pinning and Safe Mode

- **Pinning** — pin individual packages to prevent them from being included in bulk upgrades. Pinned packages still appear in the updates list but are skipped during Upgrade All.
- **Safe mode** — guarded managers (Homebrew, softwareupdate) require explicit confirmation before executing upgrades, and OS updates that require a restart show a dedicated warning.

## Search

Search is progressive and non-blocking:

1. **Instant local filter** — as you type, installed and cached packages are filtered immediately
2. **Debounced remote search** — after you stop typing for 300ms, Helm queries remote sources (e.g., Homebrew formulae) in the background
3. **Cache enrichment** — remote results are cached locally, making future searches faster
4. **Cancellation** — typing again cancels any in-flight remote search

## Background tasks

All operations (refresh, search, install, upgrade) run as background tasks. The task list shows real-time status: Queued, Running, Completed, or Failed. Tasks for the same manager run serially; tasks for different managers run in parallel.
