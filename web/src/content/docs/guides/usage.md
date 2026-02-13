---
title: Usage
description: How to use Helm to manage your packages.
---

Helm runs as a menu bar app with a floating panel UI. It has two main tabs: **Dashboard** and **Packages**.

## Dashboard

The Dashboard gives you an at-a-glance view of your software environment:

- **Package counts** — installed, upgradable, and available (from search cache)
- **Manager grid** — shows all supported managers and whether they're detected on your system
- **Recent tasks** — the latest refresh, search, and other background operations with their status

## Packages

The Packages tab is where you browse and filter your software:

- **Status filters** — filter by Installed, Upgradable, or Available
- **Manager filter** — narrow the list to a specific manager (Homebrew, mise, rustup, etc.)
- **Search** — type to instantly filter the local list; after a brief pause, Helm searches remote sources and merges results
- **Package details** — click any package to see version info and manager attribution

## Refreshing

Click the refresh button in the navigation bar to update all package data. Helm refreshes managers in **authority order**:

1. **Toolchain managers first** (mise, rustup) — these define the runtime environment
2. **Package managers second** (Homebrew) — these depend on the toolchain state

Within each phase, managers refresh in parallel. If one manager fails, the others continue unaffected.

## Search

Search is progressive and non-blocking:

1. **Instant local filter** — as you type, installed and cached packages are filtered immediately
2. **Debounced remote search** — after you stop typing for 300ms, Helm queries remote sources (e.g., Homebrew formulae) in the background
3. **Cache enrichment** — remote results are cached locally, making future searches faster
4. **Cancellation** — typing again cancels any in-flight remote search

## Background tasks

All operations (refresh, search, install, upgrade) run as background tasks. The task list shows real-time status: Queued, Running, Completed, or Failed. Tasks for the same manager run serially; tasks for different managers run in parallel.
