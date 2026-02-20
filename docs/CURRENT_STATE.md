# Current State

This document describes the current implementation status of Helm.

It reflects reality, not intention.

---

## Version

Current version: **0.16.0-alpha.1** (kickoff in progress on `dev`; latest stable release on `main` is `v0.15.0`)

See:
- CHANGELOG.md

Active milestone:
- 0.16.0 — Self-Update & Installer Hardening (alpha kickoff)
- 0.15.0 — Released on `main` (tag `v0.15.0`)

---

## Completed Milestones

- 0.1.x — Core Foundation
- 0.2.x — Homebrew adapter
- 0.3.x — Orchestration engine
- 0.4.x — SwiftUI shell + XPC bridge
- 0.5.x — Progressive search
- 0.6.x — Toolchain managers (mise, rustup)
- 0.7.x — System & App Store managers (softwareupdate, mas)
- 0.8.x — Pinning & policy
- 0.9.x — Internationalization foundation
- 0.10.x — Core language managers + hardening checkpoint
- 0.11.x — Extended language managers (beta checkpoint)
- 0.12.x — Localization hardening + upgrade transparency (stable checkpoint)
- 0.13.x — UI/UX analysis & redesign (stable checkpoint)
- 0.14.x — Platform, detection & optional managers (stable checkpoint)
- 0.15.x — Upgrade preview & execution transparency (stable checkpoint)

---

## Implemented Managers

Fully functional:

- Homebrew
- mise
- npm (global)
- pnpm (global)
- yarn (global)
- RubyGems
- Poetry (self/plugins)
- Bundler
- pip (`python3 -m pip`, global)
- pipx
- cargo
- cargo-binstall
- rustup
- softwareupdate
- mas

---

## Core Capabilities

- list installed
- list outdated
- install
- uninstall
- upgrade
- upgrade all
- pin / unpin
- progressive search
- task system
- safe mode
- localization system

Localization coverage:

- All 6 locales (en, es, de, fr, pt-BR, ja) have full key parity across app/common/service files
- Locale length audit script added at `apps/macos-ui/scripts/check_locale_lengths.sh` for overflow-risk preflight
- Locale key/placeholder integrity audit script added at `apps/macos-ui/scripts/check_locale_integrity.sh`
- `v0.11.0-beta.2` heuristic overflow audit captured at `docs/validation/v0.11.0-beta.2-l10n-overflow.md` (no high-risk candidates flagged)
- `v0.12.0-beta.1` on-device overflow validation captured at `docs/validation/v0.12.0-beta.1-visual-overflow.md` (Settings surface checks passing)
- Expanded on-device overflow validation coverage for onboarding/navigation/packages/managers captured at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`
- Manager display-name localization keys now cover upgrade-preview/task-fallback manager labels (including software update/app store naming)
- All walkthrough keys (31 keys) translated and validated across all 6 locales

Validation snapshot for `v0.11.0-beta.1` expansion:

- Priority 1 language-manager local smoke matrix captured on a macOS dev host:
  - Detected and smoke-tested: npm, pip (`python3 -m pip`), cargo
  - Not installed in the validation host environment: pipx, cargo-binstall

---

## Architecture Status

- Rust core: stable (198+ unit/integration tests, zero shell injection vectors, structured process invocation throughout, `#[instrument]` tracing spans on adapter execution paths, post-upgrade validation on all 11 adapter upgrade handlers)
- XPC service: stable (code-signing validation, graceful reconnection with exponential backoff, timeout enforcement on all calls)
- FFI boundary: functional (poisoned-lock recovery, JSON interchange, thread-safe static state, lifecycle documented in module-level docs)
- UI: feature-complete for current scope; VoiceOver accessibility labels, semantic grouping, and state-change announcements implemented; HelmCore decomposed into 5 files; UI layer purity cleanup completed (business logic extracted from views to HelmCore/ManagerInfo); inspector sidebar with task/package/manager detail views; keyboard Tab traversal still pending (macOS SwiftUI limitation)

---

## v0.16.0-alpha.1 Status

### Channel-Aware Self-Update Kickoff

Implemented on `feat/v0.16.0-kickoff`:

- Added app-update channel configuration plumbing in macOS UI runtime:
  - `HelmDistributionChannel` (`developer_id`, `app_store`, `setapp`, `fleet`)
  - `HelmUpdateAuthority` mapping derived from channel
  - `HelmSparkleEnabled` gate for direct-channel updater enablement
- Added `AppUpdateCoordinator` with strict channel isolation:
  - only Developer ID channel with Sparkle explicitly enabled can present in-app update checks
  - non-direct channels remain no-op at runtime (MAS/Setapp/Fleet isolation)
- Added optional Sparkle bridge implementation guarded with `#if canImport(Sparkle)` so non-Sparkle builds remain compile-safe
- Wired Sparkle SPM package linkage into the Helm app target for direct-channel runtime update checks
- Added user entry points for manual update checks:
  - status-menu `Check for Updates`
  - popover About overlay `Check for Updates`
- Added localized `app.overlay.about.check_updates` key across canonical and mirrored locale trees
- Added default channel metadata via build settings (`HelmDistributionChannel=developer_id`, `HelmSparkleEnabled=false`) for local/direct builds.
- Added channel profile templates for build-time distribution mapping:
  - `apps/macos-ui/Config/channels/developer_id.xcconfig`
  - `apps/macos-ui/Config/channels/app_store.xcconfig`
  - `apps/macos-ui/Config/channels/setapp.xcconfig`
  - `apps/macos-ui/Config/channels/fleet.xcconfig`
- Build script now emits `apps/macos-ui/Generated/HelmChannel.xcconfig` from `HELM_CHANNEL_PROFILE` (+ env overrides), and Xcode consumes it through checked-in base config include.
- Added shared channel renderer script (`apps/macos-ui/scripts/render_channel_xcconfig.sh`) so all generated channel config paths use one policy-enforced code path.
- Added channel-policy matrix check script (`apps/macos-ui/scripts/check_channel_policy.sh`) and wired it into CI (`.github/workflows/ci-test.yml`) before Xcode build/test.
- Helm target Info.plist keys are now build-setting injected (channel + Sparkle feed/signature metadata) rather than hardcoded plist values.
- Helm target now injects `SUAllowsDowngrades` from build settings and defaults this to disabled (`NO`) across channel profiles/base config.
- Release DMG workflow now injects direct-channel Sparkle metadata at build time and validates required release secrets/packaged plist channel keys.
- Release DMG workflow now also verifies packaged updater invariants in the built artifact:
  - `HelmDistributionChannel=developer_id`
  - `HelmSparkleEnabled` truthy
  - `SUAllowsDowngrades` false
  - `SUFeedURL` uses `https://`
  - `SUPublicEDKey` non-empty
  - Sparkle framework is bundled and linked into the Helm binary
- Added regression coverage for channel config parsing + Sparkle gating behavior (`AppUpdateConfigurationTests`) and extracted shared app-update config model into `Helm/Core/AppUpdateConfiguration.swift`.
- Build script now enforces channel/Sparkle policy at generation time and fails fast on invalid combinations:
  - non-Developer-ID channels cannot enable Sparkle or set Sparkle feed/signature metadata
  - Developer ID channel with Sparkle enabled must provide both `HELM_SPARKLE_FEED_URL` and `HELM_SPARKLE_PUBLIC_ED_KEY`
  - Developer ID channel with Sparkle enabled must use an `https://` Sparkle feed URL
  - Sparkle downgrades are disallowed (`HELM_SPARKLE_ALLOW_DOWNGRADES` cannot be enabled)
- Runtime app-update configuration now requires a secure Sparkle feed URL (`https://`) before enabling Sparkle checks.
- Runtime app-update configuration now also blocks Sparkle checks when downgrades are enabled in metadata.

Validation:

- `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml` passing
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test` passing
- `swiftlint lint --no-cache apps/macos-ui/Helm/Core/HelmCore.swift apps/macos-ui/Helm/Core/AppUpdateConfiguration.swift apps/macos-ui/Helm/AppDelegate.swift apps/macos-ui/Helm/Views/PopoverOverlayViews.swift apps/macos-ui/Helm/Core/L10n+App.swift` passing (local toolchain warns about one unsupported legacy rule key)
- Channel-policy hardening sanity checks:
  - `CONFIGURATION=Debug HELM_CHANNEL_PROFILE=app_store HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES apps/macos-ui/scripts/build_rust.sh` fails as expected
  - `CONFIGURATION=Debug HELM_CHANNEL_PROFILE=developer_id HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES apps/macos-ui/scripts/build_rust.sh` fails as expected when feed/key are omitted
  - `apps/macos-ui/scripts/check_channel_policy.sh` passes

---

## v0.14.1 Patch-Track Status (Released)

Released in `v0.14.1` (UI/UX slice):

- Onboarding "Finding Your Tools" and "Pick Your Managers" rows now render manager name + version metadata on one line
- Package list now visually highlights the inspector-selected package row
- Control-center package search now uses only the top-right global search field; redundant in-section search chip removed
- Top-right search field now includes an inline clear (`x`) control when query text is present
- Inspector package panel now shows description text (when available) and context actions (Update, Pin/Unpin, View Manager)
- Homebrew manager display names now consistently use `Homebrew (formulae)` and `Homebrew (casks)` across canonical/mirrored locale resources

Released in `v0.14.1` (cache/persistence slice):

- Search-cache persistence now deduplicates records by `(manager, package)` so repeated queries do not accumulate duplicate available-package rows
- Search-cache metadata persistence preserves existing non-empty version/summary when newer remote responses omit those fields
- Control-center available package cache refresh now deduplicates entries and keeps non-empty summaries during merges
- Package aggregation (`allKnownPackages`) now enriches installed/outdated package rows with cached summaries when available
- Package filtering now includes summary text and merges remote-search summary/latest metadata into local rows for fresher inspector/detail context

Released in `v0.14.1` (follow-up stabilization slice):

- Onboarding manager rows now keep manager name + version on a single line in both detection and configure steps
- Task list now deduplicates in-flight rows by `(manager, task_type)` while keeping bounded terminal history
- Task list fetches a wider recent-task window so long-running queued/running entries are less likely to drop out under queue churn
- Task pruning now expires only completed/failed tasks (cancelled tasks are retained)
- Duplicate submission guards now reuse existing queued/running task IDs for identical manager install/update/uninstall and package upgrade actions
- Refresh trigger now skips launching a new sweep while refresh/detection tasks are already in flight
- RubyGems is now included in per-package upgrade eligibility for control-center package actions
- Added regression coverage:
  - FFI unit tests for in-flight deduplication and bounded terminal history behavior
  - SQLite store test validating prune policy keeps cancelled/running rows

Released in `v0.14.1` (adapter behavior slice):

- RubyGems packages are now eligible for per-package update actions in the UI (`canUpgradeIndividually`)
- Manager install preflight now validates Homebrew availability before attempting Homebrew-backed manager installs (`mise`, `mas`)
- When Homebrew is unavailable, install now returns a specific localized service error key (`service.error.homebrew_required`) instead of generic process failure
- Added localized `service.error.homebrew_required` messaging across all supported locales in canonical and mirrored locale trees

Released in `v0.14.1` (search + inspector action slice):

- Remote package search now fans out across all enabled, detected, search-capable managers (instead of a single Homebrew-only path)
- Search task labels now include manager + query context (`Searching {manager} for {query}`) and manager-only warmup labels for empty-query cache refresh tasks
- `Refresh Now` now queues per-manager background search warmup tasks to repopulate the available package cache for supported managers
- Added manager-scoped remote-search FFI/service method used by SwiftUI search orchestration and per-package description refresh
- Package inspector description behavior now supports:
  - immediate rendering of cached descriptions
  - background refresh attempts for newer description data
  - loading and unavailable fallback states when descriptions are missing or unsupported
- Task inspector now shows localized troubleshooting feedback for failed tasks (including Homebrew install-specific guidance)
- Package inspector now surfaces context-appropriate actions (Install/Uninstall/Update/Pin/Unpin + View Manager) based on package status and manager capabilities
- Added package-level install/uninstall FFI + XPC surface methods and wired SwiftUI actions for supported managers

Validation:

- `cargo test -p helm-core -p helm-ffi` passing
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm test -destination 'platform=macOS'` passing

---

## v0.15.0-alpha.1 Status

### Plan Model + Inspector Foundations

Implemented on `feat/v0.15.x-alpha.1-kickoff`:

- Added ordered upgrade-plan preview export at the FFI boundary (`helm_preview_upgrade_plan`) with per-step metadata:
  - stable step ID (`manager:package`)
  - order index
  - manager/action/authority context
  - localized reason label key + args
  - initial status (`queued`)
- Added XPC surface method `previewUpgradePlan(includePinned:allowOsUpdates:)` and wired service forwarding to the new FFI export
- Added Swift `CoreUpgradePlanStep` state in `HelmCore` and plan-refresh helpers for decoding/sorting/localized status + reason rendering
- Updates section now renders the ordered execution plan directly from FFI-backed plan steps and supports plan-step selection
- Inspector now resolves selected plan steps into task-shaped detail rows so reason/status context is visible without executing upgrades
- Plan refresh now re-runs after outdated snapshot refresh (when plan state is active), and also on updates-section appear/toggle changes

Validation:

- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

---

## v0.15.0-alpha.2 Status

### Execution Transparency + Partial Failure Summary

Implemented on `feat/v0.15.x-alpha.1-kickoff`:

- Upgrade task labels now carry explicit `plan_step_id` metadata so runtime task records can be correlated directly to execution-plan steps
- Added plan-step runtime projection in `HelmCore`:
  - queued/running/completed/failed task statuses now project onto existing plan rows
  - projection remains scoped to current visible plan-step IDs
- Added grouped partial-failure summaries in Updates:
  - failures grouped by manager/cause bucket with affected package lists
  - failures are derived from projected plan-step runtime state
- Added retry affordances for failed plan steps:
  - retry per failure group
  - retry all failed steps only
  - retries call targeted package upgrade requests (including `softwareupdate` sentinel support) rather than re-running successful steps
- Inspector plan-step details now reflect projected runtime status and linked runtime task IDs when available

Validation:

- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

---

## v0.15.0-alpha.3 Status

### Operator Controls for Large Plans

Implemented on `feat/v0.15.x-alpha.1-kickoff`:

- Added Updates plan-scope controls for manager and package filtering
- Added scoped execution controls:
  - run scoped plan steps
  - cancel remaining scoped queued/running plan tasks
  - retry only failed scoped plan steps
- Enforced stable authority-first plan ordering in shared planner helpers used by execution/scope filtering
- Added planner regression coverage for authority ordering and manager/package scoping
- Fixed scoped-run behavior so initial preview `queued` steps execute (while still skipping already-projected queued/running/completed tasks)

Validation:

- `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`
- `apps/macos-ui/scripts/check_locale_integrity.sh`
- `apps/macos-ui/scripts/check_locale_lengths.sh`

---

## v0.15.0-alpha.4 Status (Completed)

### Final 0.15.0 Cut Readiness

Implemented on `feat/v0.15.x-alpha.1-kickoff` (current progress):

- Consolidated plan-step ID resolution across dashboard and action paths via shared planner helper logic
- Hardened projection/retry/failure-group mapping paths to tolerate duplicate step IDs without dictionary trap risk
- Scoped run execution now progresses by authority phase (authoritative → standard → guarded) instead of submitting all managers at once
- Cancel Remaining now aborts any in-progress scoped run sequencer before cancelling matching in-flight tasks
- Cancel Remaining now also cancels scoped projected in-flight upgrade tasks that have not surfaced in the latest task snapshot yet
- Authority-phase sequencing now waits for submission callbacks before phase polling and preserves newly queued projections until task IDs are observed in snapshots
- Scoped run completion state now ignores stale callbacks from superseded run tokens so newer runs remain accurately marked in progress
- Scoped authority-phase waiting now enforces a bounded timeout and invalidates the active run token when a phase stalls
- Expanded planner regression coverage for:
  - scoped-run eligibility gating (queued/running/completed + safe mode)
  - in-flight status handling for queued-without-projection plan rows
  - explicit and fallback plan-step ID resolution paths
  - projected task-ID extraction for scoped cancellation (status + overflow guardrails)
- Applied Rust formatting-only cleanup updates across adapter/runtime test files (no behavior change)
- Added initial `v0.15.0-alpha.1` pre-release checklist scaffolding in `docs/RELEASE_CHECKLIST.md`
- Added checked-in `apps/macos-ui/Config/HelmVersion.base.xcconfig` with optional include of generated version metadata to avoid clean-checkout build failures
- Updates section content now renders in a scrollable container so large execution-plan lists no longer clip top/bottom content in the control center
- Updates plan rows now use display-order numbering, full-row inspector selection hit targets, and an in-progress scoped-run indicator
- Failed task inspector content now includes manager/task-aware suggested repro command hints and a single `View Diagnostics` action
- Task inspector now surfaces a dedicated `Command` field with the resolved repro command (or unavailable fallback)
- Diagnostics modal now includes three tabs: `diagnostics`, `stderr`, and `stdout`
- Support diagnostics manager rows now render in a stable order (authoritative → standard → guarded, then alphabetical)
- Popover failure banner now shows a `Review` action (instead of `Upgrade All`) while failures exist, opening Control Center Tasks and selecting the first failed task
- Removed redundant Updates `Dry Run` action now that plan/risk context is continuously visible inline
- Added task-output capture plumbing across Rust execution and FFI/XPC:
  - adapter executions now propagate runtime task ID context into process-spawn requests
  - process stdout/stderr is captured per task ID and exposed through `helm_get_task_output`
  - SwiftUI inspector fetches task output on demand via new XPC `getTaskOutput` method

Validation:

- `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`
- `apps/macos-ui/scripts/check_locale_integrity.sh`
- `apps/macos-ui/scripts/check_locale_lengths.sh`

---

## v0.13.0-beta.3 Audit Status

Based on the full codebase audit conducted on 2026-02-17 and subsequent beta.3 remediation work.

### Rust Core

- All architectural invariants pass: no shell injection, structured args, authority ordering, cancelable tasks, deterministic and testable
- 190+ unit/integration tests cover adapters, parsing, orchestration, authority ordering, cancellation, and end-to-end flows
- Package identifier validation on all mutating operations prevents flag injection
- SQLite persistence uses versioned migrations (v1–v3), parameterized queries, and transactional multi-record ops
- No critical or high-priority issues identified

### SwiftUI UI

- All user-facing text uses L10n localization keys — no hardcoded English strings detected
- XPC service boundary has proper code-signing validation, async patterns, and reconnection logic
- State management is sound (single ObservableObject, @Published properties, weak-self captures)
- UI layer purity violations resolved in beta.5: search merge, safe-mode badge filtering, task-to-manager inference, authority/capability/symbol lookups all extracted from views
- HelmCore.swift decomposed from 1,133 lines into HelmCore.swift (314 lines) + 4 extension files (Actions, Fetching, Settings, Dashboard)
- DashboardView.swift decomposed from 1,919 lines into DashboardView.swift + ControlCenterModels, ControlCenterViews, HelmButtonStyles
- Task cancel button now wired and functional via XPC cancelTask method

### Accessibility (Substantially Resolved)

- Reduced-motion support implemented for overlay transitions
- VoiceOver `accessibilityLabel` on all interactive elements (package rows, task rows, manager items, status badges, menu bar status item)
- `accessibilityValue` for dynamic content (task status, package counts, manager state)
- `accessibilityElement(children: .combine)` semantic grouping on composite rows
- VoiceOver announcements for refresh start/completion, task cancellation, task failures, and refresh failure
- Remaining gap: keyboard Tab traversal does not work — macOS SwiftUI `.focusable()` does not integrate with the AppKit key view loop; requires NSViewRepresentable bridging or a future SwiftUI API

### CI/CD (Resolved)

- `ci-test.yml`: runs `cargo test --workspace` and `xcodebuild test` on PR and push to main/dev
- `i18n-lint.yml`: comprehensive locale parity, hardcoded string detection, mirror sync enforcement, and `check_locale_lengths.sh` overflow validation
- `release-macos-dmg.yml`: signed universal DMG + notarization with Rust and Swift test gates before build

### Localization

- All 6 locales pass key parity, placeholder consistency, and ICU format checks
- `check_locale_lengths.sh` included in CI workflow
- Spanish accent typo in "Actualización" has been corrected

---

## v0.13.0-beta.6 Audit Status

### Rust Core Hardening

- Structured `#[instrument]` tracing spans added to adapter execution entry points (submit, refresh_all_ordered, submit_refresh_request, submit_refresh_request_response)
- Unit tests added for `split_upgrade_target()` with cleanup marker parsing (4 cases)
- FFI lifecycle documented: no explicit shutdown, process-global state, poisoned-lock recovery
- `execute_batch_tolerant()` error scope documented: deliberate design choice for idempotent migration replay

### Documentation Alignment

- INTERFACES.md Section 10 filled with concrete inventories: 25 XPC methods, 27 FFI exports, 9 SQLite tables, task log status, confirmation token model
- On-device validation report template created with test matrices for all 6 locales
- Usability test plan created with core, error, accessibility, and locale scenarios plus acceptance criteria
- ROADMAP.md updated with cumulative beta.2-6 delivered scope
- CHANGELOG.md updated with beta.5 and beta.6 entries

---

## v0.13.0-rc.1 Status

### Inspector Sidebar
- Task detail view with status badge, task type, manager, label key/args
- Package detail view with version, status, pinned/restart-required indicators
- Manager detail view with health badge, installed/outdated counts, View Packages navigation
- Selection clearing fixes: selecting any entity properly clears conflicting selections
- Overview task rows wired to inspector via tap handling

### Upgrade Reliability
- Post-upgrade validation added to all 11 adapter upgrade handlers (Homebrew, RubyGems, npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, bundler, poetry)
- After upgrade command succeeds, each adapter re-checks `list_outdated` to verify the package was actually updated
- Silent upgrade failures now surface as `ProcessFailure` errors instead of being silently marked completed

### Status Menu
- "Control Center" item added to right-click status menu (opens dashboard overview)

### Documentation
- Security Advisory System incorporated into ROADMAP.md as milestone 1.3.x
- CHANGELOG.md, CURRENT_STATE.md, NEXT_STEPS.md updated

---

## v0.13.0-rc.2 Status

### Support & Feedback Entry Points
- New "Support & Feedback" card added to Settings surface with 5 actions:
  - Support Helm (opens GitHub Sponsors)
  - Report a Bug (opens GitHub issue template with optional diagnostics copy)
  - Request a Feature (opens GitHub issue template with optional diagnostics copy)
  - Send Feedback (opens mailto: with structured feedback form)
  - Copy Diagnostics (copies system info to clipboard with transient confirmation)
- "Include Diagnostics" toggle: when enabled, Report a Bug and Request a Feature copy diagnostics to clipboard before opening the issue template
- All 9 new L10n keys translated across all 6 locales (en, es, de, fr, pt-BR, ja)
- `.github/FUNDING.yml` created for GitHub Sponsors integration
- README.md updated with working sponsor and issue template links

---

## v0.14.0-alpha.1 Status

### Manager Metadata Scaffolding
- Rust FFI manager status payload now includes:
  - `isOptional` (optional manager marker)
  - `isDetectionOnly` (detection-only manager marker)
- Optional managers now default to disabled when no explicit preference is set:
  - `asdf`
  - `macports`
  - `nix_darwin`
- SwiftUI manager metadata expanded to full 0.14 manager inventory (toolchain, system, language, app, container/VM, security/firmware sets) with explicit `isOptional` and `isDetectionOnly` flags
- Manager implementation visibility in SwiftUI now prefers runtime `ManagerStatus.isImplemented` (with static metadata fallback) for:
  - managers section grouping
  - onboarding detection/configure flows
  - visible manager cards in dashboard/control center
- 0.14 manager capability matrix artifact added at:
  - `docs/validation/v0.14.0-alpha.1-manager-capability-matrix.md`
- `helm-ffi` unit tests added for manager-status policy behavior:
  - optional manager default-disabled policy
  - explicit preference override behavior
  - detection-only status export flags
- Rust `helm-ffi` tests pass and macOS `HelmTests` pass after scaffolding updates

---

## v0.14.0-alpha.2 Status

### Container/VM Adapter Slice
- Added first 0.14 container/VM adapter implementations in `helm-core`:
  - `docker_desktop` (detect, refresh, list_installed, list_outdated)
  - `podman` (detect, refresh, list_installed, list_outdated)
  - `colima` (detect, refresh, list_installed, list_outdated)
- Added process-backed sources for all three adapters with constrained PATH handling for XPC runtime environments
- Added parser + behavior test coverage for each new adapter:
  - version parsing
  - Homebrew outdated JSON parsing (upgrade prompting surface)
  - request-shape assertions
  - adapter execute-flow assertions for detection/installed/outdated behavior
- Added fixture artifacts for adapter parsing:
  - `core/rust/crates/helm-core/tests/fixtures/docker_desktop/outdated_brew.json`
  - `core/rust/crates/helm-core/tests/fixtures/podman/outdated_brew.json`
  - `core/rust/crates/helm-core/tests/fixtures/colima/outdated_brew.json`
- Wired new adapters into FFI runtime initialization and marked them implemented in manager status export:
  - `isImplemented=true` now includes `docker_desktop`, `podman`, `colima`
- Swift fallback metadata updated so container/VM managers reflect implemented state even when runtime status is unavailable

### Detection-Only Manager Slice
- Added detection-only adapters in `helm-core`:
  - `sparkle` (detect Sparkle-based updaters by local app bundle framework presence)
  - `setapp` (detect Setapp app bundle presence/version)
  - `parallels_desktop` (detect app bundle presence/version)
- Added process-backed sources for all three detection-only managers using structured `defaults` reads for version probing
- Added adapter test coverage for each detection-only manager:
  - request-shape assertions
  - detection parsing behavior
  - unsupported action rejection
- Wired detection-only adapters into FFI runtime initialization and manager status export:
  - `isImplemented=true` now includes `sparkle`, `setapp`, `parallels_desktop`
- Swift fallback metadata updated so detection-only managers reflect implemented state when runtime status is unavailable

### Alpha.2 Scope Decision
- Deferred manager self-update action surfacing for container/VM managers to a later milestone.
- Alpha.2 scope finalized as:
  - container/VM status adapters (`docker_desktop`, `podman`, `colima`)
  - detection-only manager surfaces (`sparkle`, `setapp`, `parallels_desktop`)

### Validation
- `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`
- 0.14 manager capability sweep artifact:
  - `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`

---

## v0.14.0-alpha.3 Status

### Security/Firmware Adapter Slice
- Added `helm-core` adapters:
  - `xcode_command_line_tools` (detect, refresh, list_installed, list_outdated, upgrade)
  - `rosetta2` (detect, refresh, install)
  - `firmware_updates` (detect, refresh via `softwareupdate --history`)
- Added process-backed sources for all three adapters with structured command invocation:
  - `pkgutil` receipt/version probing for Xcode CLT + Rosetta 2
  - `softwareupdate --history` probing for firmware history state
  - `softwareupdate --install-rosetta --agree-to-license` install path for Rosetta 2
  - `softwareupdate -i <label>` upgrade path for Xcode CLT
- Added fixtures + adapter tests for:
  - version parsing
  - request-shape/elevation assertions
  - detection/status behavior
  - unsupported-capability rejection
- Wired new adapters into FFI runtime initialization and manager status export:
  - `isImplemented=true` now includes:
    - `xcode_command_line_tools`
    - `rosetta2`
    - `firmware_updates`
- Swift fallback metadata updated so security/firmware managers reflect implemented state when runtime status is unavailable

### Validation
- `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

---

## v0.14.0-alpha.4 Status

### Optional Manager Adapter Slice
- Added `helm-core` adapters:
  - `asdf`
  - `macports`
  - `nix_darwin`
- Added process-backed sources for all three optional managers with XPC-safe PATH handling and structured command invocation
- Hardened optional-manager execution in constrained service environments:
  - `asdf` process source now resolves executable via structured `which` lookup with absolute-path fallback and rewrites request program paths
  - `asdf` outdated scan now tolerates per-tool latest-version probe failures (skips failing tool probes instead of failing the full scan)
- Implemented adapter capabilities for this slice:
  - `asdf`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade (compatibility mode)
  - `macports`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade
  - `nix_darwin`: detect, refresh, search, list_installed, list_outdated, install, uninstall, upgrade (compatibility mode via `nix-env`)
- Added fixtures + adapter tests for:
  - version parsing
  - installed/outdated/search parser behavior
  - request-shape/elevation assertions
  - execute-flow assertions
- Wired new adapters into FFI runtime initialization and manager status export:
  - `isImplemented=true` now includes:
    - `asdf`
    - `macports`
    - `nix_darwin`
- Swift fallback metadata updated so optional managers reflect implemented state when runtime status is unavailable

### Validation
- `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

---

## v0.14.0-alpha.5 Status

### Homebrew Cask Adapter Slice
- Added `helm-core` adapter:
  - `homebrew_cask`
- Added process-backed source with XPC-safe PATH handling and Homebrew environment guards
- Implemented adapter capabilities for this slice:
  - `homebrew_cask`: detect, refresh, list_installed, list_outdated
- Added JSON-backed parsing for installed/outdated casks using structured Homebrew output (`--json=v2`)
- Added fixtures + adapter tests for:
  - request-shape assertions
  - installed/outdated parsing behavior
  - read-only execute flow + unsupported mutating action rejection
- Wired adapter into FFI runtime initialization and manager status export:
  - `isImplemented=true` now includes `homebrew_cask`
- Swift fallback metadata updated so `homebrew_cask` reflects implemented state when runtime status is unavailable

### Validation
- `cargo test -p helm-core --manifest-path core/rust/Cargo.toml`
- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`

---

## v0.14.0 Status

### Stable Release Alignment
- README and website release-status pages aligned to `v0.14.0`
- Workspace version bumped to `0.14.0` in `core/rust/Cargo.toml`
- Rust lockfile workspace package versions aligned:
  - `helm-core` `0.14.0`
  - `helm-ffi` `0.14.0`
- Generated app version artifacts aligned:
  - `apps/macos-ui/Generated/HelmVersion.swift` (`0.14.0`)
  - `apps/macos-ui/Generated/HelmVersion.xcconfig` (`MARKETING_VERSION=0.14.0`, `CURRENT_PROJECT_VERSION=0`)

### Distribution/Licensing Planning Alignment (Documentation-Only)
- Architecture and planning docs now reflect planned multi-channel distribution:
  - MAS consumer build
  - Developer ID consumer build (direct DMG + Homebrew + MacPorts)
  - Setapp consumer build
  - Helm Business fleet build (PKG + MDM)
- Channel licensing authority and update authority are now documented as decoupled.
- Sparkle scope is documented as direct Developer ID consumer build only.
- Fleet lifecycle is documented as separate from consumer lifecycle.
- These are roadmap/planning decisions only; channel-specific implementation work is still pending future milestones.

### Final Release Execution (Completed)
- Completed branch/release flow from `docs/RELEASE_CHECKLIST.md`:
  - merged release finalization branch into `dev`
  - merged `dev` into `main` via PR `#60` after CI completion
  - created/pushed `v0.14.0` annotated tag

---

## UI Redesign Artifacts (Integrated Baseline)

- A complete redesign concept package now exists under `docs/ui/`:
  - `REDESIGN_CONCEPT.md`
  - `INFORMATION_ARCHITECTURE.md`
  - `USER_FLOWS.md`
  - `VISUAL_SYSTEM.md`
  - `SWIFTUI_ARCHITECTURE.md`
  - `MOCKUPS.md`
- The redesign baseline is integrated into the production macOS target at `apps/macos-ui/Helm/` (legacy scaffold at `apps/macos/` removed in beta.5):
  - redesigned menu bar popover shell
  - top-of-popover updates attention banner with custom-styled upgrade-all action
  - layered overlay panels (search, quick settings, about, quit confirmation) with dimmed-underlay transitions
  - footer utility actions (search/settings/quit) + version-triggered About panel
  - dynamic status-item signal (Helm icon + update/error/running cues)
  - in-icon status badge overlays for update/error/running cues (no numeric title text)
  - status-item anchor glyph now preserves menu-appearance monochrome (black/white) while only badge indicators are colorized
  - right-click status-item quick actions (About, Upgrade All, Basic/Advanced Settings, Refresh, Quit)
  - popover panel now auto-sizes to content height to avoid unnecessary scrollbar appearance in normal active-task states
  - explicit light-mode brightness tuning for popover overlays/cards and control-center background gradients
  - dedicated control-center window (overview/updates/packages/tasks/managers/settings)
  - titlebar-hidden control-center presentation with integrated global search bar
  - control-center keyboard shortcuts (`Cmd+F` global search focus, `Cmd+W` window close)
  - full-row clickable sidebar navigation targets for control-center sections
  - tactile sidebar hover/press states and broader pointer affordance cues for interactive rows/actions
  - seamless full-height sidebar surface treatment with refined top-cap blending and darker sidebar step
  - redesigned control-center Settings surface (card-based layout, policy toggles, and operational actions)
  - manager-aware action badges in Settings, including explicit software-update blocked signal when Safe Mode is enabled
  - redesigned button language now favors custom Helm gradient/secondary controls across primary workflows (system button styling retained for destructive/reset-style actions)
  - manager health state model includes a dedicated gray "Not Installed" badge for undetected managers
  - inspector pane for manager/package context
  - live wiring to `HelmCore` data/actions for refresh, upgrade, package actions, and manager operations
  - guided onboarding walkthrough with SpotlightOverlay system (6 popover steps + 7 control center steps)
  - WalkthroughManager with UserDefaults persistence, skip, and replay from Settings
- Release packaging now includes a GitHub Actions workflow for signed universal DMG artifacts:
  - workflow: `.github/workflows/release-macos-dmg.yml`
  - output assets: versioned `Helm-<tag>-macos-universal.dmg` plus stable `Helm.dmg`
  - DMG layout includes standard drag-to-`Applications` alias

---

## Known Limitations

- Priority 1 language manager coverage is complete for the beta checkpoint:
  - Implemented: npm (global), pip (`python3 -m pip`, global), pipx, cargo, cargo-binstall
  - Pending: none
- Priority 2 extended language-manager expansion is complete at this checkpoint:
  - Implemented: pnpm (global), yarn (global), RubyGems, Poetry (self/plugins), Bundler
  - Pending: none
- Redesign integration is functional with layered popover UX + control-center search; accessibility labels and semantic grouping implemented; onboarding walkthrough delivered; UI layer purity cleanup completed
- Keyboard-only traversal: Tab navigation does not work in macOS SwiftUI (`.focusable()` does not participate in AppKit key view loop); requires NSViewRepresentable bridging approach
- All walkthrough and redesign localization keys have been rolled out to all 6 locales
- XPC call timeout enforcement added (30s data fetches, 300s mutations) with exponential backoff reconnection
- Overflow validation now has both heuristic and on-device executable coverage for Settings, onboarding, navigation, package filters, and manager labels/states
- Upgrade-all transparency now provides summary counts + top manager breakdown in confirmation flow
- Upgrade-preview filtering/sorting logic now has dedicated macOS UI unit coverage (`HelmTests/UpgradePreviewPlannerTests`)
- Dedicated upgrade preview UI surface is implemented in macOS Settings (execution-plan sections with manager breakdown)
- Dry-run mode is exposed in the upgrade preview UI (simulation path with no task submission)
- Onboarding flow updated with friendlier tone; guided walkthrough (spotlight/coach marks) now implemented
- 0.14 manager inventory is scaffolded in metadata; alpha.2/alpha.3/alpha.4 delivered container/VM, detection-only, security/firmware, and optional-manager slices
- Optional-manager compatibility caveats:
  - `asdf` support currently assumes plugin already exists; Helm currently manages install/uninstall/upgrade of tool versions, not plugin bootstrap/removal
  - `nix_darwin` support currently operates through `nix-env` compatibility flows and does not edit declarative nix-darwin configuration files
- No self-update mechanism yet
- Limited diagnostics UI
- No CLI interface

---

## Stability

- Pre-1.0
- Rapid iteration
- Breaking changes still possible

---

## Summary

Helm is a **functional control plane for 28 implemented managers** with:

- Working orchestration
- Task system
- Pinning and policy
- Localization foundation (6 locales at full key parity)
- 0.14 platform/detection/optional manager rollout implemented

The core architecture is in place. The Rust core passed a full audit with no critical issues.

0.13.x and 0.14.x stable checkpoints are complete, with latest stable patch `v0.14.1` merged to `main`, tagged, and released. Next delivery focus is 0.15.x.
