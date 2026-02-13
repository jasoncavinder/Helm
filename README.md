<p align="center">
  <img src="docs/app-icon.png" width="96" alt="Helm app icon">
</p>

<h1 align="center">Helm</h1>

<p align="center">
  <em>Take the helm.</em>
  <br>
  A native macOS menu bar app for unified package manager control.
  <br>
  <strong>Pre-1.0 &middot; v0.7.0</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%2012%2B-blue" alt="macOS 12+">
  <img src="https://img.shields.io/badge/swift-5.7%2B-orange" alt="Swift 5.7+">
  <img src="https://img.shields.io/badge/rust-2024%20edition-brown" alt="Rust 2024">
  <img src="https://img.shields.io/github/v/tag/jasoncavinder/Helm?label=version" alt="Version">
</p>

---

Helm manages software across multiple package managers (Homebrew, npm, pip, Cargo, etc.) and runtime tools (mise, rustup) from a single menu bar interface. It is designed as infrastructure software: deterministic, safety-first, and explicit about authority, orchestration, and error handling.

> **Status:** Active pre-1.0 development. Five adapters are functional (Homebrew, mise, rustup, softwareupdate, mas) with authority-ordered refresh and restart-required detection. Additional adapters and features are being added milestone by milestone.

## Features

- **Menu bar app** — Lightweight floating panel, no Dock icon
- **Dashboard** — Package stats, manager grid, and recent task activity at a glance
- **Package list** — Browse installed, upgradable, and available packages with status filters
- **Progressive search** — Instant local filtering with debounced remote search and cache enrichment
- **Background tasks** — Real-time task tracking with per-manager serial execution
- **Multi-manager refresh** — Authority-ordered refresh across 5 managers (Homebrew, mise, rustup, softwareupdate, mas)
- **Restart detection** — Surface restart-required updates from macOS softwareupdate

## Architecture

Helm is split into three layers connected via XPC and FFI:

| Layer | Location | Role |
|-------|----------|------|
| **UI** (SwiftUI) | `apps/macos-ui/` | Presentation only — reads state, emits intents |
| **Service** (XPC) | `apps/macos-ui/HelmService/` | Hosts Rust FFI in a separate unsandboxed process |
| **Core** (Rust) | `core/rust/` | All business logic, adapters, orchestration, persistence |

The XPC boundary isolates process execution from the sandboxed app. The Rust core is UI-agnostic and fully testable.

## Getting Started

### Prerequisites

- macOS 12+
- Xcode 14+
- Rust stable toolchain (2024 edition)

### Build & Run

```bash
# Run Rust core tests
cd core/rust
cargo test

# Build the macOS app
cd apps/macos-ui
xcodebuild -project Helm.xcodeproj -scheme Helm -configuration Debug build
```

Or open `apps/macos-ui/Helm.xcodeproj` in Xcode and run the **Helm** scheme. The build script automatically compiles the Rust FFI library and generates version headers.

## Milestones

| Version | Milestone | Status |
|---------|-----------|--------|
| 0.1.x | Core foundation — adapter traits, capability model, SQLite schema | Complete |
| 0.2.x | First adapter — Homebrew detection, listing, fixture-based tests | Complete |
| 0.3.x | Orchestration engine — task queue, per-manager serialization, cancellation | Complete |
| 0.4.x | SwiftUI shell — menu bar app, XPC service, Rust FFI bridge, live UI | Complete |
| 0.5.x | Progressive search — local-first search, remote search, cache enrichment | Complete |
| 0.6.x | Core toolchain managers — mise, rustup adapters, authority ordering | Complete |
| 0.7.x | System & App Store managers — softwareupdate, mas, restart detection | Complete |
| 1.0.0 | Stable control plane release | Planned |

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the full roadmap through 1.0.

## Repository Layout

```
apps/macos-ui/              SwiftUI app + XPC service
core/rust/                   Rust workspace (helm-core, helm-ffi)
docs/                        Roadmap, versioning, release criteria
PROJECT_BRIEF.md             Product and architecture source of truth
AGENTS.md                    Engineering guardrails and constraints
```

## Development

- **`main`** — Stable, releasable. Tags created here.
- **`dev`** — Integration branch. Feature branches merge here via PR.
- **Feature branches** — `feat/`, `fix/`, `chore/`, `docs/`, `test/`, `refactor/`

See [`docs/VERSIONING.md`](docs/VERSIONING.md) for the versioning strategy.

## Documentation

- [Roadmap](docs/ROADMAP.md) — Milestone definitions through 1.0
- [Versioning](docs/VERSIONING.md) — Semantic versioning strategy
- [Release Criteria](docs/DEFINITION_OF_DONE.md) — 1.0 definition of done

## License

Currently unlicensed. All rights reserved.
