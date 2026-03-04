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

## Decision 017 — Ruleset-Enforced Four-Branch Governance

**Decision:**
Enforce the `main`/`dev`/`docs`/`web` branch model with GitHub rulesets and required status checks, and require PR-based publication of release appcast metadata.

**Rationale:**

- Keeps AI agents within explicit branch/scope policy (`Policy Gate` + branch-specific checks)
- Reduces release risk by removing direct-push fallback paths for appcast publication
- Preserves fast release flow via auto-merge/update-branch while keeping auditable PR history

---

## Decision 018 — Universal Binary Distribution

**Decision:**
Build universal (arm64 + x86_64) binaries using `lipo` and distribute via signed DMG.

**Rationale:**

- Single artifact supports Apple Silicon and Intel Macs
- DMG provides familiar macOS installation experience
- GitHub Actions workflow automates signing and notarization

---

## Decision 019 — XPC Timeout and Reconnection Policy

**Decision:**
Enforce timeouts on all XPC calls (30s data fetches, 300s mutations) and use exponential backoff for reconnection (2s base, doubling to 60s cap).

**Rationale:**

- Prevents UI hangs from unresponsive service
- Exponential backoff avoids thundering herd on service restart
- Reset on successful connection restores normal responsiveness

---

## Decision 020 — Homebrew Casks Deferred

**Decision:**
Defer Homebrew Casks adapter to 0.14.x. Originally planned for 0.10.x but dropped from that milestone.

**Rationale:**

- Homebrew formula adapter covers the primary use case
- Cask handling requires different upgrade and detection semantics
- 0.14.x (Platform, Detection & Optional Managers) is the appropriate milestone

---

## Decision 021 — Multi-Channel Distribution and Product Split

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

## Decision 022 — Sparkle Delta Policy for 0.16.x

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

## Decision 023 — Security Rollout Staging and Platform Baseline

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

## Decision 024 — Third-Party License Compliance Baseline

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

## Decision 025 — Website Hosting on Cloudflare Pages

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

## Decision 026 — Shared Brain Backend: Postgres-First and Provider-Portable

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

## Decision 027 — Per-Manager Executable and Install-Method Selection

**Decision:**
Add persisted per-manager selection for:

- selected executable path
- selected install method

Behavioral model:

- Default executable is the first discovered candidate in the user's active command resolution order.
- UI exposes explicit executable/install-method selection with recommended/default indicators.
- Core execution applies an alias-aware executable override before spawn so manager commands run against the selected installation rather than whichever binary appears first at runtime.
- Manager install/update/uninstall routing consults selected install method where implemented.

Persistence/API changes:

- `manager_preferences` schema extended with `selected_executable_path` and `selected_install_method` (migration v7).
- XPC and FFI surfaces extended with setter APIs and expanded manager-status payloads so UI and runtime stay in sync.

**Rationale:**

- Prevents PATH-order ambiguity when multiple installations of the same manager coexist.
- Reduces environment-specific failures caused by invoking the wrong toolchain copy.
- Makes manager control explicit and user-auditable instead of implicit/path-dependent.
- Preserves deterministic execution behavior across different host setups.

---

## Decision 028 — Shared GUI + CLI Coordinator

**Decision:**
The upcoming CLI and existing GUI must share a single per-user coordinator/task authority rather than running independent orchestration runtimes.

**Rationale:**

- Prevents split-brain task state and conflicting manager mutations
- Preserves one source of truth for queueing, cancellation, diagnostics, and task history
- Keeps parity behavior between GUI and CLI without duplicating orchestration policy
- Simplifies enforcement of manager serialization, authority ordering, and enablement rules

**Implementation note (`dev`, post-`v0.17.2`):**

- `helm-ffi` now initializes a coordinator-compatible bridge/host using the same per-user state-dir protocol as `helm-cli`.
- If an external coordinator already exists, GUI mutation/cancellation requests route through that external authority.
- If no coordinator exists, GUI host starts a local coordinator endpoint and CLI launch-on-demand requests connect to it.

---
## Decision 029 — Base-System Manager Policy Gate

**Decision:**
Treat macOS base-system language-manager executables as detectable but not manageable.

**Policy:**

- Affected managers may remain visible in detection and executable-path selection.
- Current blocked executables:
  - RubyGems: `/usr/bin/gem`
  - Bundler: `/usr/bin/bundle`
  - pip: `/usr/bin/python3`, `/usr/bin/pip`, `/usr/bin/pip3`
- Manager enablement is blocked when resolved executable matches blocked criteria.
- No privilege-escalation path is offered for this blocked case.
- Runtime task submission treats policy-ineligible managers as effectively disabled.
- Shared matrix and rule maintenance contract live in `docs/architecture/MANAGER_ELIGIBILITY_POLICY.md`.

**Rationale:**

- Avoid mutating Apple-managed base-system toolchains.
- Prevent non-deterministic failures and environment-specific behavior.
- Keep behavior predictable across user systems while preserving detection transparency.

---
## Decision 030 — CLI First-Run Onboarding Gate

**Decision:**
`helm-cli` enforces first-run onboarding and license acceptance before running normal operational commands.

**Policy:**

- Persist CLI onboarding state and accepted license terms version in Helm settings storage.
- Block command execution until both are satisfied, except for:
  - `help` / `--version`
  - `completion`
  - `onboarding` namespace
- First-run onboarding UX is terminal/menu based (non-TUI).
- On successful onboarding, the original command continues in the same invocation.
- Non-interactive/script-friendly first-run controls:
  - `--accept-license`
  - `--accept-defaults`
- If onboarding is required and machine mode is requested (`--json`/`--ndjson`) without sufficient acceptance flags:
  - return deterministic JSON error with explicit onboarding/license-required guidance.

**Rationale:**

- Aligns CLI legal/onboarding guarantees with GUI onboarding requirements.
- Keeps CI/script bootstrap deterministic while preserving explicit consent semantics.
- Avoids hidden state mutation or partial command execution before prerequisites are met.

---
## Decision 031 — Release Friction Promotion Path

**Decision:**
Capture release-process friction in `TMP_RELEASE_FRICTION` during execution, then promote recurring friction into permanent operational docs and decisions.

**Rationale:**

- Keeps release notes lightweight during execution while preserving actionable context
- Prevents recurring operator pain from staying in temporary files
- Ensures release runbook/checklist/decision docs stay aligned with observed release behavior

---
## Decision 032 — 1.0 Crash Reporting Posture (Local-Only)

**Decision:**
For `1.0`, Helm keeps crash/error reporting local-only and does not ship automatic remote crash telemetry.

**Policy details:**

- Diagnostics remain user-initiated export workflows.
- No automatic upload of diagnostics, package inventory, or environment fingerprints.
- Expected diagnostics payload schema and privacy constraints are documented in:
  - `docs/operations/CRASH_REPORTING_POLICY.md`
- Operational owner is the release operator on duty (maintainer by default in current phase).

**Rationale:**

- Preserves Helm's local-first privacy model for pre-1.0 and 1.0 launch.
- Reduces privacy/compliance risk before a dedicated opt-in telemetry design exists.
- Keeps support workflows functional via explicit diagnostics export without background collection.

---
## Decision 033 — Manager Install-Instance Provenance Model (Phase 1)

**Decision:**
Introduce a dedicated per-manager install-instance model for provenance analysis while preserving existing single-path detection compatibility.

**Policy details:**

- Persist install instances in a dedicated table (`manager_install_instances`, migration v9; `decision_margin` added in migration v10).
- Each instance uses a deterministic identity model with ordered fallback:
  - `DevInode` (`dev:ino`) when available
  - `CanonicalPath` when canonical path is available but inode identity is unavailable
  - `FallbackHash` (canonical/display path + stable file metadata)
- Persist identity metadata (`identity_kind`, `identity_value`) and deterministic `instance_id` for continuity across runs even if alias paths change.
- External ownership evidence (for example `brew`, `pkgutil`) must be:
  - timeout-bounded
  - lazy/invoked only for ambiguity resolution
  - cached per detection run
  - optional (signal boost only; detection must fail closed and continue)
- Persist explainability and policy outputs per instance:
  - `provenance`, `confidence`
  - `decision_margin` between top and competing provenance scores (when a competing score exists)
  - top evidence factors
  - competing provenance and score when relevant
  - derived `automation_level`, `uninstall_strategy`, `update_strategy`, `remediation_strategy`
- Managed-policy controls are evaluated at lifecycle runtime/surface projection time (not persisted into provenance records):
  - install-method policy context: `HELM_MANAGED_INSTALL_METHOD_POLICY`, `HELM_MANAGED_INSTALL_METHOD_POLICY_ALLOW_RESTRICTED`
  - automation ceiling policy context: `HELM_MANAGED_AUTOMATION_POLICY` (`automatic|needs_confirmation|read_only`)
  - managed-policy automation ceilings clamp effective automation/strategy behavior conservatively without rewriting stored provenance evidence.
- Route provenance classification through adapter-level spec hooks:
  - `rustup` uses explicit scoring rules in Phase 2
  - non-rustup managers remain explicit `Unknown` stubs with `TODO(provenance-spec)` markers until adapter rules are implemented
- Non-rustup managers default to `Unknown` provenance in Phase 1 with explicit `TODO(provenance-spec)` markers.
- Rollout gate:
  - do not switch manager uninstall routing to provenance-first until instance/provenance stability and multi-install ambiguity tests are validated.
  - phase 3 controlled exception: rustup manager uninstall is now provenance-first in CLI (with structured blast-radius preview, `--yes` confirmation gate, and explicit unknown-provenance override); non-rustup uninstall remains compatibility-routed until adapter specs are implemented.

**Rationale:**

- Decouples install-method preference from actual provenance detection.
- Improves safety for multi-install and ambiguous-manager environments.
- Enables confidence-based automation policy instead of path-only assumptions.
- Keeps adoption low-risk by preserving existing detection compatibility and delaying routing switch-over.

---

## Decision 034 — Local-First Doctor/Repair Architecture (Phase 1)

**Decision:**
Introduce dedicated `doctor` and `repair` subsystems in core, with:

- deterministic local finding fingerprints
- embedded/local knowledge lookup for known remediations
- repair planning + apply primitives routed through existing task orchestration

Phase-1 scope is intentionally narrow and starts with Homebrew metadata-only manager-install mismatch remediation.

**Policy details:**

- Doctor findings must include:
  - `finding_code`
  - `issue_code`
  - deterministic `fingerprint`
  - severity and top evidence factors
- Repair planning maps finding fingerprints to actionable options.
- External/online lookup is deferred; current release uses embedded knowledge data and explicit TODO seams for future remote providers.
- Repair execution must reuse existing manager/package mutation pathways and keep task lifecycle visibility/cancellation semantics intact.
- UI/CLI/TUI should consume the same core finding/repair contracts; surface-level UX can evolve independently.

**Rationale:**

- Creates a stable bridge from current local diagnostics to future online known-fix workflows without hard-coupling current releases to backend availability.
- Consolidates ad hoc one-off remediation logic behind one subsystem contract.
- Preserves user trust through deterministic local-first behavior and explainable reasoning prior to backend rollout.

---
## Decision 035 — Post-Install Setup Is a First-Class Health Gate

**Decision:**
Treat manager post-install shell/setup requirements as explicit doctor/repair findings and as a manageability gate (not a soft warning).

Initial implemented manager scope:

- `rustup`
- `mise`
- `asdf`

**Policy details:**

- Detection/doctor emits `post_install_setup_required` when manager install instances are present but required setup checks are unmet.
- Manager enablement must be blocked when setup-required findings are present.
- Repair planning exposes `apply_post_install_setup_defaults` when safe automation is available.
- GUI/CLI/TUI consume the same issue/repair contract:
  - user-facing guided steps
  - explicit verify/check-again path
  - optional automation path when supported
- Install flow can optionally request automatic post-install setup completion; default remains opt-in (`off`).
- Non-implemented managers remain out of scope until adapter-specific setup requirements are defined.

**Rationale:**

- Prevents false "installed/healthy" states when core shell activation is missing.
- Aligns manager health, enablement policy, and repair UX behind one deterministic contract.
- Preserves trust by surfacing clear guidance and explicit verification rather than silent assumptions.

---
## Decision 036 — Repository-Local Codex Operating System

**Decision:**
Adopt a repository-local Codex operating model with:

- repo-scoped instructions layering (`AGENTS.md` + subtree `AGENTS.md` files)
- reusable workflow Skills under `ops/codex/skills/`
- repo-local Codex config (`.codex/config.toml`) using lean `project_doc_max_bytes` (`131072`)
- reusable slash-command templates under `.codex/commands/`
- structured local notify logging on `agent-turn-complete` to `dev/logs/codex-runs.ndjson`

**Policy details:**

- Keep policy/invariants in root `AGENTS.md`; move procedures into Skills.
- Detect repeated/fragile workflows and promote them into Skills instead of expanding root instructions.
- Keep automation local-first; external MCP integrations remain optional and justified-by-need.
- Release/publish operations remain explicit-confirmation and dry-run/checklist-first by default.
- Session observability must avoid secrets and log only minimal structured run metadata.

**Rationale:**

- Reduces instruction repetition and context-window bloat across sessions.
- Standardizes recurring execution paths (quality gates, remediation batches, updater checks, docs sync).
- Improves traceability for long-running/multi-step Codex work without introducing remote telemetry.
- Preserves Helm safety posture while increasing day-to-day operator efficiency.

---
## Summary

Helm prioritizes:

- Safety
- Determinism
- Transparency
- Extensibility

These decisions should not change without strong justification.
