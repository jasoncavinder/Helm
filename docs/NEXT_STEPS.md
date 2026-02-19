# Next Steps

This document defines the immediate priorities for Helm development.

It is intentionally tactical.

---

## Current Phase

Helm is in:

```
0.15.x
```

Focus:
- 0.15.x upgrade preview and execution transparency

Current checkpoint:
- `v0.14.0` stable release alignment completed (manager rollout + docs/version alignment)
- `v0.13.0` stable released (website updates, documentation alignment, version bump)
- `v0.13.0-rc.2` released (support & feedback entry points, diagnostics copy, GitHub Sponsors integration)
- `v0.13.0-rc.1` released (inspector sidebar, upgrade reliability, status menu, documentation)
- Full codebase audit completed 2026-02-17 (Rust core, SwiftUI UI, XPC, localization, CI/CD)
- `v0.14.0-alpha.1` completed (manager metadata scaffolding, optional/detection-only status flags, optional-default disable policy for asdf/macports/nix-darwin)
- `v0.14.0-alpha.2` completed (container/VM + detection-only adapters)
- `v0.14.0-alpha.3` completed (security/firmware adapters)
- `v0.14.0-alpha.4` completed (optional managers: asdf/macports/nix-darwin)
- `v0.14.0-alpha.5` completed (homebrew_cask status adapter)
- `v0.14.0` release-readiness alignment completed (README/website status + version artifact bump)
- `v0.14.0` distribution/licensing architecture planning docs aligned (future-state, no implementation changes)

Next release targets:
- `v0.15.x` — Upgrade Preview & Execution Transparency

---

## v0.13.0-beta.3 — Accessibility + CI Foundation (Completed)

### Accessibility QA Pass (Completed)

Delivered:

- ✅ `accessibilityLabel` modifiers on all interactive elements (package rows, task rows, manager items, status badges, menu bar status item)
- ✅ `accessibilityValue` for dynamic content (task status, package counts, manager state)
- ✅ `accessibilityElement(children: .combine)` semantic grouping on composite rows
- ✅ VoiceOver announcements for refresh start/completion, task cancellation, task failures, and refresh failure
- ✅ `accessibilityReduceMotion` respected in overlay transitions

Carry-forward to beta.5:
- Keyboard-only traversal validation (Tab order, Escape behavior, `.focusable()` modifiers not systematically applied)

### Task Cancellation in UI (Completed)

Delivered:

- ✅ XPC `cancelTask` wired to cancel button with optimistic UI state update
- ✅ Cancel button enabled for running tasks
- ✅ Cancellation state transitions verified (Running → Cancelled)
- ✅ VoiceOver announcement on successful cancellation

### CI Test Enforcement (Completed)

Delivered:

- ✅ `ci-test.yml` with `cargo test --workspace` and `xcodebuild test` on PR/push to main/dev
- ✅ `xcodebuild test` gate added to `release-macos-dmg.yml` before signing
- ✅ `check_locale_lengths.sh` added to `i18n-lint.yml` workflow

### Additional Deliverables (Not Originally Planned)

- ✅ HelmCore.swift decomposed into 5 files (originally beta.5 scope)
- ✅ DashboardView.swift decomposed into 4 files (originally beta.5 scope)
- ✅ SwiftLint thresholds tightened (type_body_length: 400/600, file_length: 500/750)
- ✅ Per-manager "Upgrade All" button in Managers view
- ✅ Spanish accent typo fix ("Actualización")

---

## v0.13.0-beta.4 — Localization Parity + Onboarding Walkthrough (Completed)

### Localization Parity (Completed)

Delivered:

- ✅ 31 walkthrough L10n keys added to `en` and translated across all 5 non-English locales
- ✅ All locale integrity and overflow checks passing
- ✅ Spanish accent typo previously fixed in beta.3

### Onboarding Walkthrough Redesign (Completed)

Delivered:

- ✅ Onboarding copy updated across all 6 locales for friendlier tone (warmer subtitle, encouraging detection feedback, reassuring configure fallback)
- ✅ Reusable `SpotlightOverlay` component with anchor preference system, even-odd fill cutout, animated transitions, `accessibilityReduceMotion` support, and VoiceOver compatibility
- ✅ `WalkthroughManager` singleton with UserDefaults persistence (separate from onboarding), step progression, skip, and reset
- ✅ Popover walkthrough (6 steps): health badge, attention banner, active tasks, manager snapshot, footer actions, search field
- ✅ Control center walkthrough (7 steps): sidebar, overview, packages, tasks, managers, settings, updates — with auto-navigation on step advance
- ✅ "Replay Walkthrough" action in Settings advanced grid
- ✅ All walkthrough L10n keys translated across 6 locales with overflow validation passing

---

## v0.13.0-beta.5 — Architecture Cleanup + UI Purity (Completed)

### UI Layer Purity Fixes (Completed)

Delivered:

- ✅ Search deduplication/merge logic moved from `PackageListView` to `HelmCore.filteredPackages(query:managerId:statusFilter:)`
- ✅ Safe-mode upgrade action badge filtering moved from `SettingsPopoverView` to `HelmCore.upgradeActionManagerIds`
- ✅ Task-to-manager inference removed: `TaskItem` now carries `managerId` directly from `CoreTaskRecord`; `inferManagerId` deleted
- ✅ `authority(for:)` consolidated: computed property on `ManagerInfo`, standalone function delegates to it
- ✅ `capabilities(for:)` moved to `ManagerInfo.capabilities` computed property with `canSearch`/`canPin` helpers
- ✅ `managerSymbol(for:)` moved to `ManagerInfo.symbolName` computed property
- ✅ `health(forManagerId:)` now uses structured `managerId` field instead of localized description matching

### HelmCore Decomposition (Delivered Early in beta.3)

Delivered in beta.3:
- ✅ HelmCore.swift decomposed from 1,133 lines into HelmCore.swift (314 lines) + 4 extension files
- ✅ DashboardView.swift decomposed from 1,919 lines into 4 focused files

Remaining (optional further refinement):
- Extract service coordination into a dedicated `ServiceCoordinator` class if HelmCore extensions grow beyond current thresholds

### Keyboard Traversal (Not Resolved — macOS SwiftUI Limitation)

SwiftUI's `.focusable()` modifier does not integrate with AppKit's key view loop (`nextKeyView` / Tab chain). Tab focus stays trapped in `TextField`. Enabling keyboard traversal requires either:
- NSViewRepresentable bridging to manually wire the key view loop
- A future SwiftUI API that bridges focus scopes to AppKit

Deferred to post-0.13.x:
- Investigate NSViewRepresentable approach for Tab traversal
- Validate Escape key behavior consistent across all overlay states
- Validate Enter/Space activation for focusable elements

### Legacy UI Cleanup (Completed)

Delivered:

- ✅ Legacy redesign scaffold (`apps/macos/`, 18 files) removed entirely
- ✅ No orphaned localization keys (legacy scaffold had its own self-contained resources)

### XPC Robustness (Completed)

Delivered:

- ✅ Timeout enforcement on all XPC service calls (30s data fetches, 300s mutations) via `withTimeout` helper
- ✅ JSON decode error logging enhanced with method name and raw data length context
- ✅ `@Published var lastError` for surfacing decode/timeout failures
- ✅ Exponential backoff on XPC reconnection (2s base, doubling to 60s cap, reset on success)

---

## v0.13.0-beta.6 — Validation + Hardening + Documentation (Completed)

### On-Device Validation (Completed)

Delivered:

- ✅ Validation report template created with test matrices for all 6 locales across loading/success/error/partial-failure/empty states
- ✅ Onboarding walkthrough validation matrix included (6 popover + 7 CC steps, spotlight positioning, tooltip content, step indicators)
- ✅ Validation report captured at `docs/validation/v0.13.0-beta.6-redesign-validation.md`

### Usability Test Plan (Completed)

Delivered:

- ✅ Usability test plan documented with acceptance metrics:
  - Core scenarios: first launch, refresh, search, install, upgrade, upgrade-all, pin/unpin
  - Error scenarios: service crash/reconnection, manager failure, network unavailable
  - Accessibility scenarios: VoiceOver-only, keyboard-only (limitation documented), reduced-motion
  - Locale scenarios: es + ja full flow, de expansion check, fr/pt-BR spot check
- ✅ Pass/fail criteria and acceptance rules documented
- ✅ Test plan captured at `docs/validation/v0.13.0-beta.6-usability-test-plan.md`

### Rust Core Hardening (Completed)

Delivered:

- ✅ Structured `#[instrument]` tracing spans on adapter execution entry points (submit, refresh_all_ordered, submit_refresh_request, submit_refresh_request_response)
- ✅ Unit tests for Homebrew `split_upgrade_target()` with `@@helm.cleanup` marker (4 cases: plain, marker, empty, marker-only)
- ✅ FFI lifecycle documented in module-level docs: no `helm_shutdown()`, process-global state, poisoned-lock recovery, 27 export table
- ✅ `execute_batch_tolerant()` error scope documented: deliberate design choice, narrow tolerance, call sites identified

### Documentation Alignment (Completed)

Delivered:

- ✅ INTERFACES.md Section 10 filled with concrete inventories:
  - 26 XPC protocol methods with parameter schemas and reply types
  - 27 FFI exports (referencing module docs)
  - 9 SQLite tables across 5 migrations with primary keys
  - Task log payload status (not persisted, tracked for 0.17.x)
  - Confirmation token model (not used; code-signing + safe mode policy)
- ✅ CURRENT_STATE.md reflects beta.6 reality
- ✅ CHANGELOG.md updated for beta.5 and beta.6 changes
- ✅ ROADMAP.md 0.13.x section updated with cumulative beta.2-6 delivered scope

---

## v0.13.0-rc.1 — Inspector + Upgrade Reliability + Status Menu (Completed)

### Inspector Sidebar (Completed)

Delivered:

- Inspector task detail view with status badge, task type, manager, label key/args
- Inspector manager detail enriched with health badge, package/outdated counts, View Packages navigation
- Selection clearing fixes across all selection handlers (overview, managers, dashboard, popover)
- Overview task rows wired to inspector via tap handling

### Upgrade Reliability (Completed)

Delivered:

- Post-upgrade validation on all 11 adapter upgrade handlers
- After upgrade command succeeds, each adapter re-checks `list_outdated` and returns `ProcessFailure` if the package remains outdated
- 5 new Rust unit tests covering upgrade validation scenarios

### Status Menu (Completed)

Delivered:

- "Control Center" item added to right-click status menu (opens dashboard overview)

### Documentation (Completed)

Delivered:

- Security Advisory System milestone added to ROADMAP.md (1.3.x)
- CHANGELOG.md, CURRENT_STATE.md, NEXT_STEPS.md, ROADMAP.md updated for rc.1

---

## v0.13.0-rc.2 — Support & Feedback Entry Points (Completed)

### Support & Feedback Card (Completed)

Delivered:

- New "Support & Feedback" SettingsCard in control-center Settings surface
- 5 action buttons: Support Helm, Send Feedback, Report a Bug, Request a Feature, Copy Diagnostics
- "Include Diagnostics" toggle (default OFF): copies diagnostics to clipboard before opening GitHub issue template
- Transient "Copied!" confirmation with animated opacity transition
- `HelmSupport` updated with template-specific URLs (`reportBug`, `requestFeature` methods)

### Localization (Completed)

Delivered:

- 9 new L10n keys (`app.settings.support_feedback.*`) added to all 6 locales (en, es, de, fr, pt-BR, ja)
- Canonical and mirror locale files synchronized

### GitHub & Documentation (Completed)

Delivered:

- `.github/FUNDING.yml` created for GitHub Sponsors button
- README.md updated with working sponsor link and issue template links
- CURRENT_STATE.md, NEXT_STEPS.md updated for rc.2

---

## v0.14.0-alpha.1 — Manager Metadata Scaffolding (Completed)

### Delivered

- ✅ FFI manager status payload extended with:
  - `isOptional`
  - `isDetectionOnly`
- ✅ Optional managers default-disabled when no preference record exists:
  - `asdf`
  - `macports`
  - `nix_darwin`
- ✅ Swift manager metadata expanded to full 0.14 inventory with explicit optional/detection-only flags
- ✅ Swift manager filtering now prefers runtime `ManagerStatus.isImplemented` (with metadata fallback) in:
  - managers section grouping
  - onboarding detection/configure flows
  - dashboard/control-center visible manager cards
- ✅ 0.14 capability matrix artifact added:
  - `docs/validation/v0.14.0-alpha.1-manager-capability-matrix.md`
- ✅ `helm-ffi` manager-status policy tests added:
  - optional default-disabled policy validation
  - explicit preference override validation
  - detection-only status export validation
- ✅ Validation run:
  - `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
  - `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

### Alpha.1 Exit

- Metadata scaffolding complete. Move adapter delivery to alpha.2.

---

## v0.14.0-alpha.2 — Container/VM + Detection-Only Slice (Completed)

### Delivered

- ✅ Added `helm-core` adapters:
  - `docker_desktop`
  - `podman`
  - `colima`
- ✅ Added process sources for the new adapters with constrained PATH handling suitable for XPC execution context
- ✅ Implemented adapter capabilities for this slice:
  - detect
  - refresh
  - list_installed
  - list_outdated (upgrade prompting via Homebrew outdated JSON when available)
- ✅ Added adapter tests + fixtures for:
  - version parsing
  - Homebrew outdated payload parsing
  - request builder shape validation
  - execute-flow coverage (detect/installed/outdated)
- ✅ Registered adapters in FFI initialization and marked `docker_desktop`/`podman`/`colima` as implemented in manager status export
- ✅ Added detection-only adapters:
  - `sparkle`
  - `setapp`
  - `parallels_desktop`
- ✅ Added process-backed detection sources + adapter tests for all three detection-only managers
- ✅ Registered detection-only adapters in FFI initialization and marked `sparkle`/`setapp`/`parallels_desktop` as implemented in manager status export
- ✅ Scope decision: defer manager self-update action surfacing for container/VM managers to a later milestone
- ✅ Validation run:
  - `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
  - `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
  - `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`
- ✅ Consolidated manager capability validation artifact:
  - `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`

### Next Up (Post-Alpha.2)

- Implement next 0.14 managers:
  - `xcode_command_line_tools`
  - `rosetta2`
  - `firmware_updates`
  - optional managers (`asdf`, `macports`, `nix_darwin`)

---

## v0.14.0-alpha.3 — Security/Firmware Slice (Completed)

### Delivered

- ✅ Added `helm-core` adapters:
  - `xcode_command_line_tools`
  - `rosetta2`
  - `firmware_updates`
- ✅ Added process sources for all three adapters with structured command invocation
- ✅ Implemented adapter capabilities for this slice:
  - `xcode_command_line_tools`: detect, refresh, list_installed, list_outdated, upgrade
  - `rosetta2`: detect, refresh, install
  - `firmware_updates`: detect, refresh (`softwareupdate --history`)
- ✅ Added fixtures + adapter tests for version parsing, request-shape assertions, detection/status behavior, and unsupported-capability rejection
- ✅ Registered adapters in FFI initialization and marked `xcode_command_line_tools`/`rosetta2`/`firmware_updates` as implemented in manager status export
- ✅ Validation run:
  - `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
  - `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
  - `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

### Next Up (Post-Alpha.3)

- Completed in alpha.4:
  - optional managers (`asdf`, `macports`, `nix_darwin`)

---

## v0.14.0-alpha.4 — Optional Manager Slice (Completed)

### Delivered

- ✅ Added `helm-core` adapters:
  - `asdf`
  - `macports`
  - `nix_darwin`
- ✅ Added process sources for all three adapters with constrained PATH handling for XPC runtime environments
- ✅ Hardening follow-up:
  - `asdf` process source now resolves executable path via structured `which` lookup with absolute-path fallback
  - `asdf` outdated scan now degrades gracefully when individual latest-version probes fail
- ✅ Implemented adapter capabilities for this slice:
  - `asdf`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade (compatibility mode)
  - `macports`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade
  - `nix_darwin`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade (compatibility mode via `nix-env`)
- ✅ Added adapter tests + fixtures for:
  - version parsing
  - installed/outdated/search parsing
  - request builder shape + elevation validation
  - execute-flow coverage for detect/list/search paths
- ✅ Registered adapters in FFI initialization and marked `asdf`/`macports`/`nix_darwin` as implemented in manager status export
- ✅ Swift fallback metadata updated so optional managers reflect implemented state when runtime status is unavailable
- ✅ Validation run:
  - `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
  - `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
  - `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

### Next Up (Post-Alpha.4)

- Completed in alpha.5:
  - `homebrew_cask` included in 0.14

---

## v0.14.0-alpha.5 — Homebrew Cask Slice (Completed)

### Delivered

- ✅ Added `helm-core` adapter:
  - `homebrew_cask`
- ✅ Added process source with constrained PATH handling and Homebrew environment guardrails for XPC runtime contexts
- ✅ Implemented adapter capabilities for this slice:
  - `homebrew_cask`: detect, refresh, list_installed, list_outdated
- ✅ Implemented JSON-backed parsing for installed/outdated state via Homebrew `--json=v2` output
- ✅ Added adapter tests + fixtures for:
  - request-shape assertions
  - parse behavior for installed/outdated payloads
  - read-only execution flow + mutating-action rejection
- ✅ Registered adapter in FFI initialization and marked `homebrew_cask` as implemented in manager status export
- ✅ Swift fallback metadata updated so `homebrew_cask` reflects implemented state when runtime status is unavailable
- ✅ Validation run:
  - `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
  - `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
  - `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

### Next Up (Post-Alpha.5)

- Finalize 0.14 release cut:
  - execute branch/tag release steps from `docs/RELEASE_CHECKLIST.md`

---

## v0.14.0 — Stable Release Cut (In Progress)

### Delivered

- ✅ README release status updated for `0.14.0`
- ✅ Website docs release status updated for `0.14.0` (`index`, `overview`, `roadmap`, `changelog`)
- ✅ Workspace version bumped to `0.14.0` in `core/rust/Cargo.toml`
- ✅ Rust lockfile workspace package versions aligned (`helm-core`, `helm-ffi`)
- ✅ Generated app version artifacts aligned:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`
- ✅ Distribution/licensing planning docs aligned for 0.14 release documentation scope:
  - multi-channel build matrix (MAS, Developer ID, Setapp, Fleet)
  - channel licensing vs update authority mapping
  - consumer vs fleet lifecycle separation
  - roadmap phases for Sparkle, MAS, Setapp, Fleet, PKG/MDM, and offline licensing

### Next Up (Release Execution)

- Branch/PR execution:
  - merge release finalization branch into `dev`
  - open PR from `dev` to `main` and run CI checks
- Release finalization:
  - merge `dev` into `main`
  - create/push annotated tag `v0.14.0`

---

## Completed Priorities (Pre-0.13.x)

### Priority 1 — Core Language Managers (Completed)

- npm (global) ✅
- pip (`python3 -m pip`, global) ✅
- pipx ✅
- cargo ✅
- cargo-binstall ✅

### Priority 2 — Extended Managers (Completed)

- pnpm (global) ✅
- yarn (global) ✅
- RubyGems ✅
- Poetry (self/plugins) ✅
- Bundler ✅

### Priority 3 — Localization Expansion (Completed)

- All 6 locales (en, es, de, fr, pt-BR, ja) at full key parity ✅
- CI enforcement for locale parity + integrity ✅
- On-device overflow validation ✅

### Priority 4 — Upgrade Transparency (Completed)

- Upgrade preview UI ✅
- Execution plan display ✅
- Dry-run support ✅

### Priority 5 — UI/UX Redesign (Completed)

- Redesign concept + integration into production target ✅
- Delivered in v0.13.0-beta.3 through rc.1 above

### Hardening (Partially Complete)

Completed in `v0.10.0` checkpoint:

- Targeted adapter hardening review for regression/robustness/security risks across Priority 1 language-manager paths
- Package-identifier validation on mutating adapter actions for npm/pip/pipx/cargo/cargo-binstall
- Shared cargo/cargo-binstall outdated synthesis logic to reduce duplication and drift risk
- Replaced panic-prone FFI `lock().unwrap()` usage with poisoned-lock recovery
- Resolved website duplicate docs-id build warnings for overview/roadmap pages

Completed in `v0.11.0-beta.2` stabilization:

- Added bounded retry handling for transient task-store create/update persistence failures in orchestration runtime paths
- Added regression coverage for refresh-response error attribution and transient task-persistence recovery

Completed in `v0.13.0-beta.6`:

- ✅ Structured `#[instrument]` tracing spans on adapter execution entry points
- ✅ Homebrew `split_upgrade_target()` unit test coverage (4 cases)
- ✅ FFI lifecycle documented (module-level docs in helm-ffi)
- ✅ `execute_batch_tolerant()` error scope documented (sqlite/store.rs)

---

## Post-0.14.x Priorities

### Priority 6 — Self Update

Implement:

- Signed updates
- Integrity verification
- Update recovery

### Priority 7 — Diagnostics

Implement:

- Task log viewer
- Error export
- Manager diagnostics panel

### Priority 8 — Hardening (Remaining)

Implement:

- Stress test orchestration
- Cancellation reliability under load
- Memory audit
- FFI stability under extended runtime

---

## Non-Goals (Pre-1.0)

- Plugin system
- CLI tool
- Cloud sync
- Enterprise control plane

---

## Summary

- 0.13.x is complete and shipped as stable (`v0.13.0`).
- 0.14.x adapter rollout has delivered alpha.1 through alpha.5:
  - manager metadata scaffolding + optional/detection-only policy
  - container/VM + detection-only managers
  - security/firmware managers
  - optional managers (`asdf`, `macports`, `nix_darwin`)
  - Homebrew cask status manager (`homebrew_cask`)
- Manager capability sweep artifact is now in place for 0.14 release prep (`docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`).
- 0.14 stable release alignment for `v0.14.0` is complete (README/website + version artifacts).
- Distribution/licensing future-state planning documentation is aligned for 0.14 release notes and roadmap planning (no implementation yet).
- Next slice is branch/PR/tag release execution for `v0.14.0`, then 0.15.x delivery.
