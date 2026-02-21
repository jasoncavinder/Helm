<p align="center">
  <img src="docs/app-icon.png" width="96" alt="Helm app icon">
</p>

<h1 align="center">Helm</h1>

<p align="center">
  <em>Take the helm.</em>
  <br>
  A native macOS menu bar app for unified package manager control.
  <br>
  <strong>Pre-1.0 &middot; v0.17.0-rc.2</strong>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/platform-macOS%2011%2B-blue" alt="macOS 11+">
  <img src="https://img.shields.io/badge/swift-5.7%2B-orange" alt="Swift 5.7+">
  <img src="https://img.shields.io/badge/rust-2024%20edition-brown" alt="Rust 2024">
  <img src="https://img.shields.io/github/v/tag/jasoncavinder/Helm?label=version" alt="Version">
</p>

---

Helm manages software across multiple package managers (Homebrew, npm, pip, Cargo, etc.) and runtime tools (mise, rustup) from a single menu bar interface. It is designed as infrastructure software: deterministic, safety-first, and explicit about authority, orchestration, and error handling.

> **Status:** Active pre-1.0 development at `v0.17.0-rc.2` (Diagnostics & Logging RC on `dev`; latest stable on `main` is `v0.16.2`).
>
> **Testing:** Please test `v0.17.0-rc.2` and report issues at [GitHub Issues](https://github.com/jasoncavinder/Helm/issues/new/choose).

## Editions (Beta)

Helm is currently in pre-1.0 beta.

All features are available during beta. Some features are marked **Pro (Beta)** and will require a paid Pro license after version 1.0.

### Planned Editions

- **Helm (Consumer)** — Free + Pro tiers for individual/power-user workflows
- **Helm Business (Fleet)** — Separate product for policy, compliance, and organizational management

We will provide advance notice before introducing paid tiers.

---

## Support Helm

Helm is an independent project.

If you find it useful, consider supporting development:

- [GitHub Sponsors](https://github.com/sponsors/jasoncavinder)
- [Patreon](https://patreon.com/jasoncavinder)
- Early access and lifetime licenses (planned)

You can also help by [reporting bugs](https://github.com/jasoncavinder/Helm/issues/new?template=bug_report.yml) or [requesting features](https://github.com/jasoncavinder/Helm/issues/new?template=feature_request.yml). In-app feedback entry points are available in Settings under "Support & Feedback".

Your support helps fund continued development.

## Beta Download

Install the latest beta DMG from GitHub Releases:
- https://github.com/jasoncavinder/Helm/releases

DMG builds target **Any Mac (Apple Silicon + Intel)** on **macOS 11+ (Big Sur)** and use standard drag-to-`Applications` installation.

## Features

- **Menu bar app** — Lightweight floating panel, no Dock icon
- **Dashboard** — Package stats, manager grid, and recent task activity at a glance
- **Package list** — Browse installed, upgradable, and available packages with status filters
- **Progressive search** — Instant local filtering with debounced remote search and cache enrichment
- **Background tasks** — Real-time task tracking with per-manager serial execution
- **Multi-manager refresh** — Authority-ordered refresh across installed managers with phased execution
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

- macOS 11+ (Big Sur)
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
|---|---|---|
| 0.1.x | Core Foundation — Rust workspace, adapter trait, capability model | Completed |
| 0.2.x | First Adapter — Homebrew detection, listing, fixture-based tests | Completed |
| 0.3.x | Orchestration Engine — task queue, per-manager serialization, cancellation | Completed |
| 0.4.x | SwiftUI Shell — menu bar app, XPC service, Rust FFI bridge, live UI | Completed |
| 0.5.x | Progressive Search — local-first search, remote search, cache enrichment | Completed |
| 0.6.x | Core Toolchain Managers — mise, rustup adapters, authority ordering | Completed |
| 0.7.x | System & App Store Managers — softwareupdate, mas, restart detection | Completed |
| 0.8.x | Pinning & Policy Enforcement — native/virtual pins, safe mode, guarded updates | Completed |
| 0.9.x | Internationalization Foundation — centralized localization system, ICU format | Completed |
| 0.10.x | Core Language Package Managers — npm, pipx, pip, Cargo, cargo-binstall | Completed |
| 0.11.x | Extended Language Package Managers — pnpm, yarn, poetry, RubyGems, bundler | Completed |
| 0.12.x | Localization + Upgrade Transparency — locale hardening, visual validation expansion, upgrade preview, dry-run | Completed (`v0.12.0`) |
| 0.13.x | UI/UX Analysis & Redesign — full UX audit, interaction model, information architecture refresh | Completed (`v0.13.0`) |
| 0.14.x | Platform, Detection & Optional Managers — Docker, Xcode, Rosetta, Sparkle | Completed (`v0.14.x` stable, latest patch `v0.14.1`) |
| 0.15.x | Upgrade Preview & Execution Transparency — bulk preview, scoped execution, failure isolation | Completed (`v0.15.0`) |
| 0.16.x | Self-Update & Installer Hardening — Sparkle integration, signed verification | Completed (`v0.16.0`) |
| 0.16.1 | Documentation, Milestone Restructure & Security Staging Clarification | Completed (documentation-only) |
| 0.16.2 | Sparkle Connectivity + Platform Baseline Alignment — network-client entitlement, feed diagnostics, macOS 11 deployment target enforcement | Completed |
| 0.17.x | Diagnostics & Logging — log viewer, structured error export, health panel | Planned |
| 0.18.x | Local Security Groundwork — local vulnerability abstractions and cache plumbing (no public feature surface) | Planned |
| 0.19.x | Stability & Pre-1.0 Hardening — stress tests, crash recovery, memory audit | Planned |
| 1.0.0 | Stable Control Plane Release — production-safe execution, full feature set | Planned |

See [`docs/ROADMAP.md`](docs/ROADMAP.md) for the full roadmap through 1.x.

## Security Rollout (Planned)

- **Phase 1 (`0.18.x`)**: Local-only security groundwork. Internal abstractions and data-handling preparation only. No Pro gate and no centralized backend.
- **Phase 2 (`1.3.x`, Helm Pro)**: **Security Advisory System**. Local-first CVE/advisory evaluation, optional public API queries (OSV/GitHub Advisory DB/NVD-style sources), local TTL cache, and actionable recommendations.
- **Phase 3 (`1.4.x`)**: **Shared Brain**. Fingerprint sharing, known-fix lookup, centralized Postgres-backed services, and App Attest-based request authentication.

Security Advisory System and Shared Brain are separate systems. Shared Brain is additive and depends on additional infrastructure not required for Phase 2.

## Website Hosting (Current)

Helm documentation/marketing website hosting is on **Cloudflare Pages** (not GitHub Pages).

- Framework: Astro (Starlight)
- Root directory: `web/`
- Build command: `npm ci && npm run build`
- Output directory: `dist`
- Deploy trigger: GitHub-connected auto-deploys from pushes to `main` (plus preview deployments for PRs/branches)

Operational notes:

- GitHub Pages is no longer the production host for Helm docs.
- The legacy GitHub Pages deployment workflow (`.github/workflows/deploy-web.yml`) has been removed from `main`; any appearance on older/non-main branches should be treated as historical residue, not active production deployment.
- Cloudflare Pages deployments are visible in the Cloudflare dashboard under the Helm Pages project.

How to verify Cloudflare hosting:

```bash
dig +short helmapp.dev A
dig +short helmapp.dev AAAA
curl -sI https://helmapp.dev | egrep -i 'cf-ray|server: cloudflare|cf-cache-status'
```

Rollback concept (minimal):

- Re-point DNS to a fallback host if needed.
- Re-enable a fallback static host path (for example GitHub Pages) only as an emergency bridge.
- Restore Cloudflare Pages as primary once the incident is resolved.

## Shared Brain Backend Direction (Planned)

The `1.4.x` Shared Brain milestone is planned as **Postgres-first** and **provider-portable**:

- System-of-record: Postgres
- Core capabilities expected from standard Postgres features:
  - dedupe and idempotency via constraints/UPSERT
  - ranking/selection materialization (for example "best fix")
  - search/aggregation via FTS/trigram/materialized views
  - optional RLS/advisory locks if multi-tenant or stronger coordination is needed
- Cloudflare Workers may be used as a stateless edge/API layer, but Cloudflare-specific data stores are not the core architecture.
- Durable Objects / D1 are not the Shared Brain system-of-record.
- Large artifacts (if introduced later) should live in S3-compatible object storage; Postgres stores references/metadata.

Current releases (`<=0.17.x`) do **not** send package/fingerprint data to a shared backend. Security-advisory value remains local-first until the `1.4.x` Shared Brain milestone.

## Repository Layout

```
apps/macos-ui/              SwiftUI app + XPC service
core/rust/                   Rust workspace (helm-core, helm-ffi)
docs/                        Roadmap, versioning, release criteria
docs/PROJECT_BRIEF.md        Product and architecture source of truth
AGENTS.md                    Engineering guardrails and constraints
```

## Development

- **`main`** — Stable, releasable. Tags created here.
- **`dev`** — Integration branch. Feature branches merge here via PR.
- **Feature branches** — `feat/`, `fix/`, `chore/`, `docs/`, `test/`, `refactor/`

See [`docs/VERSIONING.md`](docs/VERSIONING.md) for the versioning strategy.

## Documentation

- [Roadmap](docs/ROADMAP.md) — Milestone definitions through 1.x
- [Versioning](docs/VERSIONING.md) — Semantic versioning strategy
- [Changelog](CHANGELOG.md) — Versioned release notes
- [Release Checklist](docs/RELEASE_CHECKLIST.md) — Required ship checklist and tag steps
- [Release Criteria](docs/DEFINITION_OF_DONE.md) — 1.0 definition of done
- [Third-Party Licenses](docs/legal/THIRD_PARTY_LICENSES.md) — dependency license inventory and release obligations
- [Enterprise Architecture](docs/enterprise/ENTERPRISE_INTEGRATION_ARCHITECTURE.md) — Integration model for managed enterprise environments
- [Editions and Entitlements](docs/enterprise/EDITIONS_AND_ENTITLEMENTS.md) — Debug/release build strategy and Free/Pro/Business gating
- [Business Central Management Spec](docs/enterprise/BUSINESS_CENTRAL_MANAGEMENT_SPEC.md) — Scoped policy, drift, and compliance model
- [Enterprise GTM Matrix](docs/enterprise/GTM_PERSONA_MATRIX.md) — Persona messaging and pilot KPI framework

## Future Licensing

Helm is currently released under a source-available, non-commercial license.

The licensing model will evolve at or after version 1.0 to support commercial use and additional editions.

See [docs/legal/LICENSING_STRATEGY.md](docs/legal/LICENSING_STRATEGY.md) for details.

## License

Helm is currently released under a **source-available, non-commercial license** (pre-1.0).

- Source code is visible for transparency and evaluation
- Use is permitted for personal and non-commercial purposes
- Commercial use is not permitted before 1.0
- Redistribution of unmodified binaries is allowed for non-commercial use
- Modification and reuse of the source code is not permitted

See [LICENSE](LICENSE) for full terms.

---

## Contributions

Contributions are welcome, but require agreement to the Contributor License Agreement (CLA).

By submitting a contribution, you agree to the terms in [docs/legal/CLA.md](docs/legal/CLA.md).

This ensures Helm can evolve its licensing model in the future (including commercial and open-source options).
