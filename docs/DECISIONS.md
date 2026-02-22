# Architectural Decisions

This document records key architectural decisions.

It functions as a lightweight ADR (Architecture Decision Record) log.

---

## Decision 001 — Rust Core

**Decision:**
Use Rust for core logic.

**Rationale:**

- Memory safety
- Strong typing
- Performance
- Testability

---

## Decision 002 — XPC Service Boundary

**Decision:**
Use an XPC service for execution.

**Rationale:**

- Sandbox separation
- Crash isolation
- Security boundary

---

## Decision 003 — SwiftUI UI

**Decision:**
Use SwiftUI for macOS UI.

**Rationale:**

- Native look and feel
- Rapid iteration
- Good integration with macOS

---

## Decision 004 — Adapter Architecture

**Decision:**
Use adapter-per-manager model.

**Rationale:**

- Clear separation
- Extensibility
- Capability-driven behavior

---

## Decision 005 — Capability Model

**Decision:**
Managers declare capabilities.

**Rationale:**

- Avoid false assumptions
- Enable UI gating
- Support diverse managers

---

## Decision 006 — Authority Ordering

**Decision:**
Enforce execution ordering.

**Order:**
Authoritative → Standard → Guarded

**Rationale:**

- Toolchains before packages
- System updates last
- Deterministic behavior

---

## Decision 007 — SQLite Persistence

**Decision:**
Use SQLite for persistence.

**Rationale:**

- Local-first design
- Reliability
- Simple deployment

---

## Decision 008 — No Shell Execution

**Decision:**
Never use shell invocation.

**Rationale:**

- Prevent injection vulnerabilities
- Structured execution

---

## Decision 009 — Task-Based Execution

**Decision:**
All operations are tasks.

**Rationale:**

- Observability
- Cancellation
- Persistence

---

## Decision 010 — Localization via Keys

**Decision:**
All UI text uses localization keys.

**Rationale:**

- Internationalization
- Consistency
- Maintainability

---

## Decision 011 — Source-Available Licensing (Pre-1.0)

**Decision:**
Use non-commercial source-available license.

**Rationale:**

- Prevent early commercialization
- Maintain control
- Allow future licensing flexibility

---

## Decision 012 — Edition-Based Future

**Decision:**
Plan a multi-product entitlement future:

- Helm (Consumer): Free + Pro
- Helm Business (Fleet): separate product lifecycle

**Rationale:**

- Sustainable business model
- Enterprise expansion

---

## Decision 013 — Dual-Surface UI Model

**Decision:**
Use a menu bar popover for quick triage and a separate control center window for sustained workflows.

**Rationale:**

- Popover provides at-a-glance status and quick actions
- Control center window supports deep exploration, inspector, and settings
- AppDelegate manages both via `NSStatusItem`, `FloatingPanel`, and `ControlCenterWindow`

---

## Decision 014 — Locale Mirror Architecture

**Decision:**
Maintain locale files in two locations: `locales/` (canonical source) and `apps/macos-ui/Helm/Resources/locales/` (app resource copy), enforced in sync by CI.

**Rationale:**

- `locales/` is the single source of truth for translations
- The app bundle requires resources in its own directory
- CI `diff -ru` enforcement prevents drift

---

## Decision 015 — Post-Upgrade Validation Pattern

**Decision:**
After an upgrade command reports success, re-check `list_outdated` and return `ProcessFailure` if the package remains outdated.

**Rationale:**

- Exit code 0 does not guarantee the package was actually updated
- Silent upgrade failures are a significant usability problem
- Applied to all 11 adapters with per-package upgrade capability

---

## Decision 016 — Onboarding Walkthrough via SpotlightOverlay

**Decision:**
Implement guided onboarding as a spotlight/coach marks system using SwiftUI preference keys for anchor positioning.

**Rationale:**

- Non-intrusive: overlays existing UI rather than blocking it
- Reusable across popover (6 steps) and control center (7 steps)
- Persisted separately from onboarding wizard via UserDefaults

---

## Decision 017 — Universal Binary Distribution

**Decision:**
Build universal (arm64 + x86_64) binaries using `lipo` and distribute via signed DMG.

**Rationale:**

- Single artifact supports Apple Silicon and Intel Macs
- DMG provides familiar macOS installation experience
- GitHub Actions workflow automates signing and notarization

---

## Decision 018 — XPC Timeout and Reconnection Policy

**Decision:**
Enforce timeouts on all XPC calls (30s data fetches, 300s mutations) and use exponential backoff for reconnection (2s base, doubling to 60s cap).

**Rationale:**

- Prevents UI hangs from unresponsive service
- Exponential backoff avoids thundering herd on service restart
- Reset on successful connection restores normal responsiveness

---

## Decision 019 — Homebrew Casks Deferred

**Decision:**
Defer Homebrew Casks adapter to 0.14.x. Originally planned for 0.10.x but dropped from that milestone.

**Rationale:**

- Homebrew formula adapter covers the primary use case
- Cask handling requires different upgrade and detection semantics
- 0.14.x (Platform, Detection & Optional Managers) is the appropriate milestone

---

## Decision 020 — Multi-Channel Distribution and Product Split

**Decision:**
Adopt a multi-channel distribution strategy with two products:

- Helm (Consumer): Free + Pro feature-gated editions
- Helm Business (Fleet): separate binary and lifecycle

Planned artifacts:

1. Helm (Mac App Store)
2. Helm (Developer ID: direct DMG, Homebrew, MacPorts)
3. Helm (Setapp)
4. Helm Business (Fleet PKG)

Channel rules:

- Update transport is decoupled from licensing.
- Sparkle is only used in the direct Developer ID consumer build.
- Sparkle is not used for MAS, Setapp, or Business fleet builds.
- Homebrew and MacPorts distribution reuse the same Developer ID consumer binary.
- Business lifecycle and release cadence are separate from consumer lifecycle.

**Rationale:**

- Keeps one shared core codebase while supporting channel-specific distribution requirements.
- Preserves clean separation between licensing authority and update authority.
- Aligns enterprise deployment needs (PKG/MDM/offline license files) without coupling to consumer channels.
- Reduces long-term operational risk by making channel behavior explicit in planning before implementation.

---

## Decision 021 — Sparkle Delta Policy for 0.16.x

**Decision:**
For `0.16.x`, direct-channel Sparkle updates ship full signed DMG payloads only. Delta updates are explicitly disabled until a later milestone.

Policy guardrails:

- Appcast contains exactly one release `<item>` and one full-installer `<enclosure>`.
- `sparkle:deltas` blocks and `sparkle:deltaFrom` attributes are rejected.
- Enclosure URL must use `https://` and target a `.dmg` payload.
- Release workflow enforces this policy via `apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh`.

**Rationale:**

- Keeps updater behavior simple and auditable while direct-channel hardening is still in progress.
- Avoids additional signing, generation, and rollback complexity in first Sparkle-enabled release candidates.
- Provides a deterministic release artifact model for RC validation and operator troubleshooting.

---

## Decision 022 — Security Rollout Staging and Platform Baseline

**Decision:**
Adopt a staged security rollout with explicit milestone separation and set platform baseline to macOS 11+ (Big Sur).

Staging:

- `<=0.16.x`: documentation/planning only (no security advisory implementation)
- `0.18.x`: local-only internal groundwork
- `1.3.x`: Security Advisory System (Helm Pro, local-first, optional public advisory API queries)
- `1.4.x`: Shared Brain (centralized fingerprint/fix services with App Attest-backed request controls)

Version restructuring:

- Existing `0.18.x` hardening scope is moved to `0.19.x`
- Existing `1.4.x+` milestones are shifted forward by one minor version

**Rationale:**

- Keeps local advisory capabilities independent from centralized infrastructure
- Reduces coupling and delivery risk by separating local security value from backend-heavy features
- Aligns future Shared Brain auth requirements with modern platform primitives available from macOS 11+

---

## Decision 023 — Third-Party License Compliance Baseline

**Decision:**
Maintain an explicit dependency-license inventory and release-gate checklist for third-party components.

Implementation baseline:

- canonical inventory doc: `docs/legal/THIRD_PARTY_LICENSES.md`
- release gating entries: `docs/RELEASE_CHECKLIST.md` (all releases)
- legal notice cross-reference: `docs/legal/NOTICE.md`

Scope clarifications:

- Runtime app dependencies and build-time dependencies are tracked separately.
- Sparkle attributions remain required for channels that include Sparkle.
- Website toolchain dependencies (including `sharp/libvips`) are tracked with distribution-path-specific obligations.

**Rationale:**

- Helm's project license does not supersede third-party license obligations.
- A documented baseline reduces release risk from dependency changes.
- Explicit runtime-vs-build separation prevents over- or under-scoping compliance actions.

---

## Decision 024 — Website Hosting on Cloudflare Pages

**Decision:**
Helm website hosting is standardized on Cloudflare Pages as the production path.

Operational baseline:

- Root directory: `web/`
- Build command: `npm ci && npm run build`
- Output directory: `dist`
- Framework: Astro (Starlight)
- Deploy model: GitHub-connected automatic deployments from `main`, plus preview deployments for pull requests/branches

**Rationale:**

- Consolidates production and preview deployments under one managed platform.
- Reduces custom deployment glue and keeps website hosting aligned with current operational workflow.
- Supports fast preview validation for docs/website changes.

Repository cleanup follow-up:

- Legacy GitHub Pages workflow/config references are expected to stay removed on `main`; if present on non-main branches, they are treated as temporary branch drift and not production deployment intent.

---

## Decision 025 — Shared Brain Backend: Postgres-First and Provider-Portable

**Decision:**
For `1.4.x` Shared Brain planning, use a Postgres-backed system-of-record with provider-portable API architecture.

Design constraints:

- Core persistence is Postgres (not provider-specific state stores).
- Cloudflare Workers may be used as a stateless edge/API layer, but must remain replaceable.
- Durable Objects / D1 are explicitly out-of-scope as core persistence architecture.
- Large artifacts (if needed later) use S3-compatible object storage with references/metadata persisted in Postgres.

Expected Postgres primitives:

- constraints + UPSERT for dedupe/idempotency
- ranking/selection queries (for example best-fix selection)
- materialized views plus FTS/trigram for aggregation/search
- optional RLS/advisory locks if multi-tenant isolation/coordination is required

**Rationale:**

- Keeps core backend logic portable across cloud providers.
- Uses well-understood relational patterns for ranking, dedupe, and queryability.
- Avoids coupling Shared Brain correctness to a single vendor-specific database model.
- Preserves local-first value delivery: current releases (`<=0.17.x`) remain backend-independent for security/advisory workflows.

---

## Summary

Helm prioritizes:

- Safety
- Determinism
- Transparency
- Extensibility

These decisions should not change without strong justification.
