# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and follows SemVer-compatible Helm versioning.

## [Unreleased]

### Added
- Third-party dependency licensing baseline document:
  - `docs/legal/THIRD_PARTY_LICENSES.md`
  - includes runtime/build/toolchain scope split and release obligations.

### Changed
- Legal notice and licensing strategy docs now explicitly link to third-party dependency obligations:
  - `docs/legal/NOTICE.md`
  - `docs/legal/LICENSING_STRATEGY.md`
- Release process now includes mandatory third-party license compliance gates in:
  - `docs/RELEASE_CHECKLIST.md`
- Project state/planning docs now track this compliance baseline:
  - `docs/CURRENT_STATE.md`
  - `docs/NEXT_STEPS.md`
- ADR log adds third-party license compliance baseline decision:
  - `docs/DECISIONS.md` (Decision 023)

## [0.16.1] - 2026-02-21

### Changed
- Documentation-only release planning update for staged security rollout (no feature implementation changes).
- Revised milestone sequencing:
  - new `0.18.x` local security groundwork milestone
  - prior `0.18.x` stability/hardening milestone moved to `0.19.x`
  - new `1.4.x` Shared Brain milestone inserted
  - prior `1.4.x+` milestones shifted forward by one minor version
- Security architecture clarified as three staged phases:
  - Stage 1 (`0.18.x`) local internal groundwork
  - Stage 2 (`1.3.x`) Security Advisory System (Helm Pro, local-first, optional public API queries, local TTL cache)
  - Stage 3 (`1.4.x`) Shared Brain (fingerprint sharing, known-fix lookup, centralized backend, App Attest request controls)
- Minimum supported macOS baseline documentation updated to `macOS 11+ (Big Sur)` across README and website installation/FAQ surfaces.

## [0.16.0] - 2026-02-21

### Added
- App update channel scaffolding in macOS UI:
  - `HelmDistributionChannel` + `HelmUpdateAuthority` runtime model
  - `AppUpdateCoordinator` with channel-gated manual update checks
  - optional Sparkle bridge guarded by `#if canImport(Sparkle)`
- Sparkle Swift Package Manager dependency linked into the Helm app target for direct-channel runtime update checks.
- Regression tests for app-update config parsing and Sparkle channel gating (`AppUpdateConfigurationTests`).
- Channel profile build templates for distribution/update authority separation:
  - `apps/macos-ui/Config/channels/developer_id.xcconfig`
  - `apps/macos-ui/Config/channels/app_store.xcconfig`
  - `apps/macos-ui/Config/channels/setapp.xcconfig`
  - `apps/macos-ui/Config/channels/fleet.xcconfig`
- Manual `Check for Updates` entry points in both the menu-bar status menu and About popover overlay.
- New localized UI key `app.overlay.about.check_updates` across all supported locales (canonical + mirrored locale trees).
- Upgrade-plan preview model surfaced end-to-end (FFI → XPC → SwiftUI) with stable step IDs, order indices, manager/action/authority context, and localized reason metadata.
- Updates inspector plan-step details with projected runtime status and linked runtime task IDs.
- Scoped Updates controls for manager/package filtering and failed-step retry targeting.
- Failed-task inspector now provides a dedicated task-output sheet with `stderr`/`stdout` tabs backed by on-demand FFI/XPC task-output retrieval.
- Sparkle appcast generation script added (`apps/macos-ui/scripts/generate_sparkle_appcast.sh`) for signing a finalized DMG and emitting `appcast.xml`.
- Website Sparkle feed path scaffolded at `web/public/updates/appcast.xml`.

### Changed
- Added default app update-channel metadata to `Info.plist`:
  - `HelmDistributionChannel=developer_id`
  - `HelmSparkleEnabled=false`
- Build script now emits generated channel settings (`apps/macos-ui/Generated/HelmChannel.xcconfig`) from `HELM_CHANNEL_PROFILE` with optional CI overrides.
- Helm target Info.plist channel/feed/signature keys now come from build settings (`HelmDistributionChannel`, `HelmSparkleEnabled`, `SUFeedURL`, `SUPublicEDKey`) instead of hardcoded plist entries.
- App update channel/config parsing model was extracted to `Helm/Core/AppUpdateConfiguration.swift` so runtime and tests share the same source of truth.
- Release DMG workflow now validates Sparkle feed/signature secrets and injects Developer ID channel Sparkle metadata during signed release builds.
- Build-time channel policy validation now fails fast in `build_rust.sh` when Sparkle metadata/settings violate channel boundaries (non-Developer-ID channels cannot enable Sparkle; Developer ID + Sparkle requires feed URL + public key).
- Build/runtime Sparkle enablement gates now require an `https://` Sparkle feed URL for Developer ID channel update checks.
- Sparkle downgrade policy is now explicitly disabled via build metadata (`SUAllowsDowngrades=NO`) and enforced in both runtime updater gating and release artifact verification.
- Runtime Sparkle gating now requires an eligible install location (not DMG-mounted under `/Volumes/...` and not App Translocation) before enabling in-app update checks.
- Runtime Sparkle gating now also disables in-app updates for package-manager-managed installs (Homebrew Cask receipts and Homebrew/MacPorts path heuristics), with a dedicated localized reason.
- About overlay and status-menu update controls now show localized reasons when update checks are unavailable due to channel/config/install-location policy.
- Channel config rendering/policy enforcement now runs through a shared script (`apps/macos-ui/scripts/render_channel_xcconfig.sh`) reused by build generation and test validation flows.
- CI now runs a channel-policy matrix check (`apps/macos-ui/scripts/check_channel_policy.sh`) before Xcode build/test.
- Release DMG verification now enforces packaged updater invariants (`HelmDistributionChannel`, `HelmSparkleEnabled`, `SUFeedURL`, `SUPublicEDKey`) and validates Sparkle framework bundling/linkage.
- Release DMG workflow now validates final packaged DMG contents (app bundle presence, `/Applications` symlink, background asset, updater invariants, and codesign integrity) via `apps/macos-ui/scripts/verify_release_dmg.sh`.
- Release DMG workflow now generates a signed Sparkle `appcast.xml` from the finalized stapled DMG and uploads it to release assets.
- Build metadata generation now derives a monotonic numeric `CURRENT_PROJECT_VERSION` from semantic versions to keep Sparkle version ordering stable across prerelease/stable builds.
- Runtime upgrade task labels now include `plan_step_id` metadata so task rows can be projected onto execution-plan rows.
- Partial-failure summaries now group failed plan steps by manager and affected package set.
- Scoped plan execution now runs phase-by-phase by authority rank (authoritative → standard → guarded) instead of submitting all manager steps concurrently.
- Cancel Remaining now aborts active scoped-run sequencing before cancelling matching in-flight tasks.
- Scoped run sequencing now ignores stale callbacks from superseded run tokens and bounds phase waiting with timeout-based token invalidation.
- Cancel Remaining now also cancels scoped upgrade tasks that are still projected in-flight even when they have not yet appeared in task snapshots.
- Xcode build settings now resolve through a checked-in base version xcconfig that optionally includes generated version metadata, preventing missing-file failures on clean checkouts.
- Updates section content now scrolls to prevent clipping when execution-plan and failure lists exceed available viewport height.
- Updates plan rows now use display-order numbering and full-row hit targets for inspector selection.
- Updates section now shows an in-progress indicator while scoped plan execution is active.
- Task inspector now shows a `Command` field with the resolved repro command when available.
- Popover failure banner now replaces contradictory `Upgrade All` with `Review` when failures exist, routing directly to Control Center Tasks and selecting the first failed task.
- Failed-task inspector now uses a single `View Diagnostics` action that opens a 3-tab diagnostics view (`diagnostics`, `stderr`, `stdout`).
- Support diagnostics manager listing is now stable (authority order, then alphabetical) to prevent row reordering churn.
- Process-executed adapter tasks now carry task ID context through execution so stdout/stderr can be captured and mapped back to task IDs for diagnostics.
- Removed the redundant `Dry Run` button from Updates now that equivalent plan visibility is always present inline.
- Release-prep metadata now targets `0.16.0` across workspace versioning and status documentation (README/website/release checklist).
- Generated `apps/macos-ui/Generated/HelmVersion.xcconfig` is now ignored and no longer tracked.

## [0.16.0-rc.9] - 2026-02-21

### Changed
- Status-menu `Support Helm` submenu now includes all six support destinations (GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo), matching the Settings support picker.
- About overlay now includes a `Support Helm` button that opens the same multi-channel support picker.
- Status-menu update-item enablement now honors app-managed availability state by disabling Cocoa auto-validation (`menu.autoenablesItems = false`), keeping `Check for Updates` correctly disabled for ineligible installs.
- Release workflow appcast fallback no longer fails the full release job when Actions token permissions block `gh pr create`; it now logs manual compare URL instructions after pushing the fallback branch.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.9-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.9` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.8] - 2026-02-20

### Changed
- Appcast publication logic in `release-macos-dmg.yml` now checks `git status --porcelain -- web/public/updates/appcast.xml` instead of `git diff --quiet` so untracked feed files are published on first write.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.8-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.8` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).


## [0.16.0-rc.7] - 2026-02-20

### Changed
- Sparkle appcast generation now downloads Sparkle's official SPM artifact ZIP (`Sparkle-for-Swift-Package-Manager.zip`) and extracts `bin/sign_update` when no local DerivedData artifact path can be found.
- Release workflow appcast generation step reverted to default script discovery behavior (no forced Sparkle path arguments), letting script-level fallback logic handle runner differences.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.7-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.7` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.6] - 2026-02-20

### Changed
- Sparkle appcast generation now falls back to `swift run --package-path <Sparkle checkout> sign_update` when no prebuilt `sign_update` binary is available from DerivedData artifacts.
- Release workflow now passes Sparkle checkout path (`build/DerivedData/SourcePackages/checkouts/Sparkle`) to appcast generation so fallback signing can run deterministically in CI.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.6-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.6` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.5] - 2026-02-20

### Changed
- Release DMG workflow no longer passes a fixed Sparkle bin directory to appcast generation; it now relies on script-side auto-discovery for `sign_update` across available DerivedData locations.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.5-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.5` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.4] - 2026-02-20

### Changed
- Release DMG workflow now re-signs Sparkle nested code (`Autoupdate`, `Updater.app`, `Downloader.xpc`, `Installer.xpc`, and `Sparkle.framework`) using the active Developer ID identity with secure timestamps before notarization.
- Release DMG workflow now verifies Sparkle helper signatures include Developer ID authority and timestamps prior to artifact verification/notarization.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.4-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.4` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.3] - 2026-02-20

### Changed
- Release DMG workflow now pre-renders release channel config before build and passes explicit `HELM_*` Sparkle/channel build settings to `xcodebuild` so packaged `Info.plist` metadata is correct in the same invocation.
- Release DMG workflow now logs the generated `apps/macos-ui/Generated/HelmChannel.xcconfig` in CI for easier troubleshooting of release metadata overrides.
- Installer/update interruption runbook version advanced to:
  - `docs/validation/v0.16.0-rc.3-installer-recovery.md`
- Workspace package versioning bumped to `0.16.0-rc.3` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).

## [0.16.0-rc.2] - 2026-02-20

### Added
- Sparkle appcast policy validator script (`apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh`) that enforces direct-channel RC policy:
  - no delta update payloads
  - exactly one full-installer `<enclosure>` payload
  - required Sparkle signature/version metadata
  - HTTPS download URL targeting DMG payloads
- Release workflow now runs appcast policy validation immediately after appcast generation.
- Installer/update interruption and recovery validation runbook added:
  - `docs/validation/v0.16.0-rc.2-installer-recovery.md`

### Changed
- Workspace package versioning bumped to `0.16.0-rc.2` (`core/rust/Cargo.toml`, `core/rust/Cargo.lock` for local crates).
- Sparkle release automation now publishes appcast feed updates to `web/public/updates/appcast.xml` with a PR fallback path when direct pushes to `main` are blocked.
- Helm app Info.plist now includes explicit updater metadata placeholders so packaged-release verification can assert channel/Sparkle keys in artifacts.

## [0.15.0] - 2026-02-20

### Added
- Upgrade plan preview model surfaced end-to-end (FFI → XPC → SwiftUI), with stable step IDs and ordered manager/action context.
- Scoped execution controls for Updates (manager/package filters, failed-step retry, cancel remaining).
- Task-output diagnostics surfaced in Inspector via on-demand stdout/stderr retrieval.

### Changed
- Scoped upgrade execution now runs phase-by-phase by authority order (authoritative → standard → guarded) with stricter stale-callback and timeout handling.
- Updates/Inspector UX refinements:
  - scrollable updates content for long plans/failure sets
  - full-row plan selection hit targets with stable display ordering
  - in-progress feedback for active scoped runs
  - failure banner now routes to review-first flow
  - consolidated diagnostics modal tabs and command visibility in task inspector
- Removed redundant `Dry Run` control in Updates in favor of always-visible inline plan context.

## [0.14.1] - 2026-02-20

### Fixed
- Preserved task label metadata for all fetched tasks (including deduplicated in-flight rows) so labels do not regress to generic fallback text.
- Aligned remote-search and package-action capability gating between SwiftUI and FFI manager status exports.
- Prevented unsupported remote search/install package actions for managers that do not expose those capabilities in the active runtime.

### Changed
- Extended manager status payloads with capability support flags used by UI action gating:
  - `supportsRemoteSearch`
  - `supportsPackageInstall`
  - `supportsPackageUninstall`
  - `supportsPackageUpgrade`
- Bumped workspace/app version artifacts to `0.14.1`.

## [0.14.0] - 2026-02-19

### Changed
- Promoted `0.14.x` alpha and rc delivery slices into the stable `v0.14.0` checkpoint.
- Aligned release status metadata across README, docs, website, and generated version artifacts.

## [0.14.0-rc.1] - 2026-02-19

### Added
- Completed 0.14 manager rollout across alpha.1–alpha.5 slices:
  - container/VM managers: Docker Desktop, podman, colima
  - detection-only managers: Sparkle, Setapp, Parallels Desktop
  - security/firmware managers: Xcode Command Line Tools, Rosetta 2, Firmware Updates
  - optional managers: asdf, MacPorts, nix-darwin
  - app-store status manager: Homebrew casks (`homebrew_cask`)
- Added capability/implementation sweep artifact for 0.14 manager inventory:
  - `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`
- Added distribution/licensing planning documentation for future-state architecture:
  - multi-channel build matrix (MAS, Developer ID, Setapp, Fleet)
  - channel licensing-vs-update authority mapping
  - staged milestone planning for Sparkle, MAS, Setapp, Fleet, PKG/MDM, offline licensing

### Changed
- FFI manager status now reflects full 0.14 implementation baseline (`isImplemented=true` for all registry managers).
- Optional managers remain default-disabled when no explicit preference is stored (`asdf`, `macports`, `nix_darwin`).
- Swift fallback metadata aligned to 0.14 manager implementation coverage.
- Clarified consumer vs fleet product lifecycle separation in architecture and enterprise planning docs (documentation-only; implementation pending).

## [0.13.0] - 2026-02-19

### Added
- Website: changelog page, visual tour with 10 UI screenshots, FAQ & troubleshooting guide
- Website: Open Graph and Twitter Card meta tags for social sharing
- Website: Starlight `lastUpdated` timestamps on all pages

### Changed
- Promoted `0.13.x` beta and rc slices into the stable `v0.13.0` checkpoint
- Aligned version metadata across Cargo.toml, HelmVersion, docs, README, and website to `0.13.0`

## [0.13.0-rc.2] - 2026-02-19

### Added
- "Support & Feedback" card in control-center Settings with 5 action buttons: Support Helm (GitHub Sponsors), Report a Bug, Request a Feature, Send Feedback (mailto), Copy Diagnostics
- "Include Diagnostics" toggle (default OFF) that copies diagnostics to clipboard before opening GitHub issue templates
- Transient "Copied!" confirmation with animated opacity transition and 2-second auto-dismiss
- "Support Helm" submenu in right-click status menu with GitHub Sponsors and Patreon items
- `.github/FUNDING.yml` for GitHub Sponsors and Patreon integration
- `HelmSupport.reportBug()` and `HelmSupport.requestFeature()` methods with template-specific GitHub URLs
- 11 new L10n keys (`app.settings.support_feedback.*`) across all 6 locales (en, es, de, fr, pt-BR, ja)

### Changed
- Decomposed AppDelegate: extracted `StatusBadge` enum and `drawBadge()` into `AppDelegate+StatusBadge.swift`; extracted inline support submenu into `buildSupportMenuItem()` helper to satisfy SwiftLint file/function length thresholds
- README.md updated with GitHub Sponsors link, Patreon link, and issue template links

## [0.13.0-rc.1] - 2026-02-18

### Added
- Inspector sidebar task detail view with status badge, task type, manager name, label key, and label args display
- Post-upgrade validation across all 11 adapter upgrade handlers: after a successful upgrade command, each adapter re-checks `list_outdated` to verify the package was actually updated; returns `ProcessFailure` error if the package remains outdated (prevents silent upgrade failures)
- Control Center menu item in status menu right-click (opens dashboard overview)
- Overview task rows wired to inspector via tap handling with pointer affordance
- Manager inspector enriched with health badge, installed/outdated package counts, and View Packages navigation button
- Security Advisory System milestone added to ROADMAP.md (1.3.x, Pro edition)
- 16 new L10n keys for inspector task/package/manager detail views across all 6 locales
- 4 new Rust unit tests for RubyGems upgrade validation and 1 for bundler upgrade validation

### Fixed
- Inspector selection clearing: selecting a manager no longer shows a stale package inspector (all selection handlers now clear conflicting selections)
- RubyGems upgrade tasks showing "completed" when packages were not actually updated (root cause: no post-execution verification — same fix applied across all adapters)
- Inspector empty state text updated to include "task" alongside "package" and "manager" in all 12 locale files

### Changed
- Task labels now include package names for upgrade tasks across all managers (e.g., "Upgrading rake" instead of generic task description)
- Roadmap renumbered: Business Policy (1.3.x → 1.4.x), Enterprise Rollout (1.4.x → 1.5.x) to accommodate Security Advisory System (1.3.x)
- InspectorManagerDetailView refactored to accept health, packageCount, outdatedCount, and onViewPackages callback parameters

## [0.13.0-beta.6] - 2026-02-18

### Added
- Structured `#[instrument]` tracing spans on adapter execution entry points (`submit`, `refresh_all_ordered`, `submit_refresh_request`, `submit_refresh_request_response`) for long-running operation visibility in logs
- Unit tests for Homebrew `split_upgrade_target()` function (plain name, cleanup marker, empty string, marker-only)
- On-device validation report template for redesigned states across all 6 locales (`docs/validation/v0.13.0-beta.6-redesign-validation.md`)
- Usability test plan with core, error, accessibility, and locale scenarios plus acceptance criteria (`docs/validation/v0.13.0-beta.6-usability-test-plan.md`)
- INTERFACES.md Section 10 filled with concrete XPC protocol (26 methods), FFI export (27 functions), and SQLite schema (9 tables) inventories

### Changed
- Expanded `execute_batch_tolerant()` documentation to clarify deliberate error tolerance scope for idempotent migration replay
- Added FFI lifecycle module documentation explaining process-global state, no explicit shutdown, and poisoned-lock recovery
- Updated ROADMAP.md 0.13.x section to reflect cumulative beta.2-6 delivery
- Updated CURRENT_STATE.md to reflect beta.6 implementation status

## [0.13.0-beta.5] - 2026-02-18

### Added
- XPC timeout enforcement on all service calls (30s data fetches, 300s mutations) via `withTimeout` helper
- Exponential backoff on XPC reconnection (2s base, doubling to 60s cap, reset on success)
- JSON decode error logging enhanced with method name and raw data length context
- `@Published var lastError` for surfacing decode/timeout failures to the UI
- Task list sorted by status (running first, then queued, then terminal states)

### Changed
- Extracted search deduplication/merge logic from `PackageListView` to `HelmCore.filteredPackages(query:managerId:statusFilter:)`
- Removed task-to-manager inference: `TaskItem` now carries `managerId` directly from `CoreTaskRecord`; `inferManagerId` deleted
- Consolidated `authority(for:)` as `ManagerInfo` computed property with `find(byId:)` and `find(byDisplayName:)` lookups
- Moved `capabilities(for:)` to `ManagerInfo.capabilities` computed property with `canSearch`/`canPin` helpers
- Moved `managerSymbol(for:)` to `ManagerInfo.symbolName` computed property
- Changed `health(forManagerId:)` to use structured `managerId` field instead of localized description matching
- Changed Settings Advanced buttons to consistent system style (Refresh Now, Reset, Quit Helm, Replay Walkthrough)

### Removed
- Legacy redesign scaffold (`apps/macos/`, 18 files) removed entirely
- Removed Upgrade All button from Settings Advanced card (available elsewhere in UI)
- Removed manager badge tags from Settings action buttons

### Fixed
- Removed unused `clickInControlCenter` variable in AppDelegate (Xcode warning)
- Reverted ineffective `.focusable()` modifiers (macOS SwiftUI limitation documented)
- Frozen menu bar icon tint to concrete sRGB color to prevent appearance-mode drift

## [0.13.0-beta.4] - 2026-02-18

### Added
- Guided onboarding walkthrough system with SpotlightOverlay component:
  - Reusable anchor preference system for tagging UI elements as walkthrough targets
  - Even-odd fill cutout shape with animated transitions between spotlight targets
  - Tooltip card with step dots, Next/Done button, Skip link, and VoiceOver support
  - `accessibilityReduceMotion` respected in all walkthrough animations
- Popover walkthrough (6 steps): status badge, attention banner, active tasks, manager snapshot, footer actions, search field
- Control center walkthrough (7 steps): sidebar, overview, packages, tasks, managers, settings, updates — with auto-navigation to matching section on step advance
- WalkthroughManager singleton with UserDefaults persistence, step progression, skip, and reset
- "Replay Walkthrough" action button in Settings advanced grid
- 31 walkthrough L10n keys (controls, popover steps, CC steps) across all 6 locales
- Walkthrough files registered in Xcode project under Views/Walkthrough group

### Changed
- Onboarding copy updated across all 6 locales for friendlier tone:
  - Warmer subtitle, "Let's Go" / "All Set!" CTAs
  - Encouraging detection feedback ("— great start!", "— looking good!")
  - More reassuring configure step fallback messages

### Fixed
- Spanish locale overflow for CC walkthrough step 4 description (shortened to pass length threshold)

## [0.13.0-beta.3] - 2026-02-18

### Added
- VoiceOver accessibility support across all interactive UI elements:
  - `accessibilityLabel` on package rows, task rows, manager items, status badges, and menu bar status item
  - `accessibilityElement(children: .combine)` semantic grouping on composite rows (packages, tasks, managers)
  - `accessibilityValue` for dynamic content (task status, package counts, manager state)
  - VoiceOver announcements for refresh start/completion, task cancellation, task failures, and refresh failure
- Task cancel button wired to XPC `cancelTask` method with optimistic UI state update
- Per-manager "Upgrade All" button in control-center Managers view
- CI test enforcement:
  - `ci-test.yml` GitHub Actions workflow with `cargo test --workspace` and `xcodebuild test`
  - `xcodebuild test` gate added to `release-macos-dmg.yml` before signing
  - `check_locale_lengths.sh` added to `i18n-lint.yml` workflow
- L10n key additions for cancel action and status announcements across 6 locales

### Changed
- Refactored `HelmCore.swift` (1,133 lines) into HelmCore.swift (314 lines) plus 4 extension files:
  HelmCore+Actions.swift, HelmCore+Fetching.swift, HelmCore+Settings.swift, HelmCore+Dashboard.swift
- Refactored `DashboardView.swift` (1,919 lines) into DashboardView.swift plus ControlCenterModels.swift,
  ControlCenterViews.swift, HelmButtonStyles.swift, and HelmCore+Dashboard.swift
- Tightened SwiftLint thresholds (type_body_length: 400/600, file_length: 500/750, function_body_length: 80/120)

### Fixed
- Sidebar labels now update immediately on locale change (added missing @ObservedObject)
- Dry-run panel no longer displays literal `\n` (fixed JSON escape sequence handling)
- Spanish locale accent typo: "Actualizacion de software" → "Actualización de software"

## [0.13.0-beta.2] - 2026-02-18

### Added
- Added universal-build support in the macOS Rust bridge script:
  - architecture-aware `helm-ffi` builds for `arm64` and `x86_64`
  - universal static library output via `lipo` when multiple slices are requested
  - optional rustup target auto-install for local non-Release builds
- Added release automation workflow for signed DMG packaging:
  - `.github/workflows/release-macos-dmg.yml`
  - release assets: `Helm-<tag>-macos-universal.dmg` and `Helm.dmg`
  - DMG layout includes drag-to-`Applications` alias
- Added beta binary installation guidance to website docs:
  - `web/src/content/docs/guides/installation.md`

### Changed
- Updated macOS project signing defaults:
  - Debug signing identity uses `Apple Development`
  - Release signing identity uses `Developer ID Application`
  - team IDs aligned to current project team settings
- Updated project architecture defaults to keep local Debug builds fast (`ONLY_ACTIVE_ARCH = YES`) while enabling universal release builds.
- Bumped release metadata and docs/website status to `v0.13.0-beta.2`.

## [0.13.0-beta.1] - 2026-02-18

### Added
- Delivered the redesigned menu bar popover shell with:
  - top updates attention banner and custom upgrade-all action
  - layered overlays (search, quick settings, about, quit confirmation) with dimmed underlay
  - right-click status-item quick actions and in-icon status badge indicators
- Delivered the redesigned control-center shell with:
  - titlebar-hidden compact header and integrated global search
  - full-row interactive sidebar with hover/press states
  - redesigned card-based Settings surface and manager-aware action badges
  - keyboard shortcuts (`Cmd+F`, `Esc`, `Cmd+W`) and reduced-motion-aware overlay transitions

### Changed
- Updated menu bar icon rendering to preserve monochrome anchor treatment (black/white by appearance) while keeping status indicators colorized.
- Extended custom Helm primary/secondary button styling across non-destructive workflows.
- Added explicit manager health classification for undetected managers (`Not Installed`) instead of reporting them as `Healthy`/`Attention`.
- Bumped release metadata and documentation/website status to `v0.13.0-beta.1`.

## [0.12.0] - 2026-02-17

### Added
- Completed localization hardening for shipped locales (`en`, `es`, `de`, `fr`, `pt-BR`, `ja`) including:
  - locale key/placeholder integrity checks
  - expanded locale parity CI enforcement
  - on-device visual overflow validation expansion beyond Settings
- Added a dedicated Upgrade Preview UI surface in macOS Settings with execution-plan sections.
- Added dry-run support in the Upgrade Preview flow with simulation results and no task submission.

### Changed
- Promoted `0.12.x` beta slices into the stable `v0.12.0` checkpoint.
- Aligned version metadata and release/docs status to `0.12.0`.

## [0.12.0-beta.4] - 2026-02-17

### Added
- Added dry-run support to the dedicated Upgrade Preview sheet in macOS Settings:
  - localized Dry Run mode toggle
  - dry-run result dialog with planned execution summary
  - no task submission when dry-run mode is enabled
- Added new localized keys for dry-run controls/results across shipped locales (`en`, `es`, `de`, `fr`, `pt-BR`, `ja`).

## [0.12.0-beta.3] - 2026-02-17

### Added
- Added a dedicated Upgrade Preview sheet in macOS Settings that shows:
  - a no-OS-updates execution plan section
  - an optional include-OS-updates section (when Safe Mode is off)
  - manager-level package-count breakdown for each execution mode
- Wired direct execution actions from the preview surface for both upgrade modes.

### Changed
- Replaced the previous Upgrade All confirmation alert with a dedicated preview UI surface.

## [0.12.0-beta.2] - 2026-02-17

### Added
- Expanded `LocalizationOverflowValidationTests` to validate localized width budgets for:
  - onboarding constrained labels/actions
  - navigation tabs and search placeholder
  - package filter controls
  - manager category/state labels
- Added visual validation artifact at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`.

### Changed
- Promoted Priority 3 localization-overflow validation from Settings-only coverage to broader high-constrained app surfaces.

## [0.12.0-beta.1] - 2026-02-17

### Added
- Added locale integrity validation script at `apps/macos-ui/scripts/check_locale_integrity.sh` to enforce:
  - key parity against base `en` locale
  - placeholder token parity for localized strings
- Added locale integrity validation to CI (`.github/workflows/i18n-lint.yml`).
- Added `LocalizationOverflowValidationTests` in `HelmTests` for locale-aware width checks on constrained `SettingsPopoverView` controls.
- Added visual overflow validation artifact at `docs/validation/v0.12.0-beta.1-visual-overflow.md`.

### Changed
- Expanded i18n locale mirror parity checks to include `en`, `es`, `de`, `fr`, `pt-BR`, and `ja`.
- Included locale integrity validation in `apps/macos-ui/scripts/run_v0110b2_stabilization_checks.sh`.
- Increased `SettingsPopoverView` width and language picker width to clear validated locale overflow cases.

## [0.11.0-beta.2] - 2026-02-17

### Added
- Added repeatable stabilization and validation artifacts for `v0.11.0-beta.2`, including:
  - Priority 2 manager smoke matrix (`docs/validation/v0.11.0-beta.2-smoke-matrix.md`)
  - Localization overflow heuristic report (`docs/validation/v0.11.0-beta.2-l10n-overflow.md`)
- Added bounded retry handling for transient task-store persistence failures in orchestration runtime paths.
- Added regression coverage for refresh-response error attribution and transient task-persistence recovery behavior.

### Changed
- Updated release metadata and docs for the `v0.11.0-beta.2` stabilization checkpoint.
- Clarified localization overflow status as heuristic-pass complete with on-device visual validation still pending.

## [0.10.0] - 2026-02-17

### Added
- Delivered Priority 1 core language-manager support end-to-end for `npm` (global), `pipx`, `pip` (`python3 -m pip`, global), `cargo`, and `cargo-binstall`.
- Added fixture-based parser and adapter coverage for language-manager version/list/search/outdated flows where supported.
- Added a dedicated roadmap milestone for full UI/UX analysis and redesign (`0.13.x`) before later pre-1.0 platform/hardening phases.

### Changed
- Hardened mutating language-manager adapter paths with package-identifier validation to reject invalid or option-like package names.
- Consolidated cargo/cargo-binstall outdated synthesis logic into a shared helper to reduce duplication and drift risk.
- Replaced panic-prone FFI mutex lock unwrap paths with poisoned-lock recovery to avoid lock-poison panics at the FFI boundary.
- Resolved website duplicate docs-id warnings by moving overview/roadmap pages to unique slugs (`/product-overview/`, `/product-roadmap/`).
- Promoted release metadata from beta to stable `v0.10.0` across core/app/docs/website references.

## [0.10.0-beta.2] - 2026-02-17

### Added
- Added adapter input hardening for language-manager mutating actions to reject invalid package identifiers (empty/whitespace, option-like `-` prefixes, oversized identifiers).
- Added regression tests for invalid package-identifier handling in npm and pip adapters.
- Added a new roadmap milestone for full UI/UX analysis and redesign in the pre-1.0 sequence.

### Changed
- Refactored duplicated cargo/cargo-binstall outdated synthesis into shared logic to reduce drift risk and unnecessary duplicate probes.
- Removed unused pip search query environment propagation from process command specs.
- Updated release metadata and docs for `v0.10.0-beta.2`, including roadmap resequencing and website docs alignment.

## [0.10.0-beta.1] - 2026-02-17

### Added
- Added end-to-end adapter implementations for Priority 1 language managers: `npm` (global), `pipx`, `pip` (`python3 -m pip`, global), `cargo`, and `cargo-binstall`.
- Added parser fixtures and adapter unit coverage for version/list/search/outdated flows across the Priority 1 manager set where supported.
- Added manager wiring across runtime boundaries (registry, FFI, XPC/UI metadata) so the new managers are exposed in app manager status and task routing.

### Changed
- Updated release metadata and docs for the `v0.10.0-beta.1` checkpoint.
- Updated website status/overview/roadmap content to reflect current pre-1.0 manager coverage and beta milestone progress.

## [0.9.3] - 2026-02-16

### Added
- Added localized manager-name keys used by upgrade-preview and task-fallback UI text across `en`, `es`, `de`, `fr`, `pt-BR`, and `ja`.
- Added dedicated unit coverage for upgrade-preview filtering and breakdown sorting with a new `HelmTests` target and `UpgradePreviewPlannerTests`.
- Added `UpgradePreviewPlanner` to centralize upgrade-preview inclusion and manager breakdown logic.

### Changed
- Localized manager display-name resolution in `HelmCore` so upgrade-plan summaries and fallback task text no longer show hardcoded English manager labels.
- Updated release metadata and documentation for `v0.9.3`.

## [0.9.2] - 2026-02-14

### Added
- Added `es` (Spanish) and `de` (German) locale bundles for app/common/service strings in both source locale assets and macOS app resources.
- Added language-picker options for Spanish and German in Settings.

### Changed
- Expanded localization keyset with language display-name keys used by the picker (`app.settings.label.language.spanish`, `app.settings.label.language.german`).

## [0.9.1] - 2026-02-14

### Fixed
- Resolved placeholder localization text caused by folder-reference bundle layout differences by supporting both:
  - `locales/<locale>/<file>.json`
  - `Resources/locales/<locale>/<file>.json`
- Kept Xcode `Resources` folder-reference wiring so locale JSON files are copied without flattening.

## [0.9.0] - 2026-02-14

### Added
- **Internationalization (i18n) Foundation**:
  - Centralized `LocalizationManager` for loading and resolving locale strings.
  - Type-safe `L10n` accessor struct for all UI keys.
  - JSON-based locale architecture (`locales/en/*.json`) supporting future language expansion.
  - Language picker in Settings (persisted to UserDefaults).
- **UI Localization**:
  - All user-facing strings in Dashboard, Package List, Tasks, and Settings are now localized.
  - Structured error keys defined for service layer communication.

### Changed
- Refactored `SettingsPopoverView` to use localized labels and grouped components to respect SwiftUI view limits.
- Updated `LocalizationManager` to robustly handle both folder-referenced and flattened resource bundles.

## [0.8.0] - 2026-02-14

### Added
- End-to-end pinning and policy controls across core/FFI/XPC/UI, including native Homebrew pin/unpin support, virtual pin fallback APIs, and safe mode persistence.
- Individual package upgrade actions for outdated `homebrew_formula`, `mise`, and `rustup` package entries.

### Changed
- `helm_upgrade_all` now queues outdated `homebrew_formula`, `mise`, and `rustup` targets (plus optional macOS updates), with pin filtering, de-duplication, and safe-mode enforcement.
- Homebrew upgrade execution now verifies the target package is no longer outdated after `brew upgrade`, failing tasks when the upgrade is ineffective.
- Homebrew version probing/persistence and settings surfaces were hardened for stripped runtime environments.

### Fixed
- Task terminal persistence now treats missing or malformed terminal payloads as explicit failures instead of leaving stale `running` tasks.
- Detection persistence now normalizes empty-string manager versions as missing values (`NULL`) to prevent blank-version regressions.

## [0.8.0-rc.2] - 2026-02-14

### Added
- Individual package upgrade support for outdated `mise` and `rustup` entries through adapter, FFI, and UI action wiring.

### Changed
- Homebrew upgrade flow now verifies target formula is no longer listed as outdated after `brew upgrade` and fails the task when upgrade was ineffective.
- Individual package upgrade actions are now available for `homebrew_formula`, `mise`, and `rustup` managers in the package list UI.
- Homebrew version probing and persistence were hardened for onboarding/managers visibility in stripped XPC environments.

### Fixed
- Task terminal persistence now handles adapter panic/missing terminal payload cases as explicit failures instead of leaving stale `running` states.
- Detection persistence now treats empty-string manager versions as missing values (`NULL`) to prevent blank version regressions in UI.

## [0.8.0-rc.1] - 2026-02-14

### Added
- End-to-end safe-mode orchestration tests for `softwareupdate` upgrade submission behavior (blocked when safe mode is enabled, allowed when disabled with explicit confirmation token).

### Changed
- `helm_upgrade_all` now skips queuing `softwareupdate` upgrades when safe mode is enabled instead of attempting submission and relying on runtime rejection.

## [0.8.0-beta.1] - 2026-02-14

### Added
- Safe mode persistence and control surfaces across FFI/XPC/UI (`helm_get_safe_mode`, `helm_set_safe_mode`) to block macOS software update upgrades by policy.
- Upgrade-all orchestration entrypoint (`helm_upgrade_all`) with explicit OS-update confirmation gating.
- `softwareupdate` adapter upgrade execution path (`softwareupdate -i -a`) with explicit confirmation token validation.

### Changed
- Runtime submission now enforces safe-mode policy for `softwareupdate` upgrade actions.
- Settings UI now exposes Safe Mode and an operational Upgrade All flow (with and without OS updates).
- SQLite schema adds `app_settings` to persist cross-session application policy flags.

## [0.8.0-alpha.2] - 2026-02-14

### Added
- Native Homebrew pin/unpin adapter actions (`brew pin`, `brew unpin`) with structured command specs and adapter tests.

### Changed
- Pin/unpin FFI path now uses native manager execution for Homebrew and keeps virtual pin fallback for managers without native pin support.
- Homebrew adapter capabilities now explicitly declare `Pin` and `Unpin`.

## [0.8.0-alpha.1] - 2026-02-14

### Added
- Virtual pin APIs in FFI/XPC/UI:
  - list pin records (`helm_list_pins`)
  - pin package (`helm_pin_package`)
  - unpin package (`helm_unpin_package`)
- Package-level pin/unpin controls in the package detail popover.
- Pin indicator in package rows and pin metadata in package detail.

### Changed
- Installed/outdated package queries now overlay persisted pin records so pin state is reflected consistently in UI package lists.

## [0.7.1] - 2026-02-14

### Changed
- Expanded the `README.md` milestone table to mirror the active roadmap through `0.15.x`.
- Updated `PROJECT_BRIEF.md` implementation phases to reflect completed `0.1.x-0.7.x` delivery and upcoming `0.8.x-1.0.0` sequencing.
- Marked all `v0.7.0` branch/merge/tag/publish items complete in `docs/RELEASE_CHECKLIST.md`, including the recorded release SHA.

## [0.7.0] - 2026-02-13

### Added
- Manager update/self-update controls end to end (Rust core, FFI, XPC service, SwiftUI):
  - Homebrew update (`brew update`)
  - Homebrew-managed manager upgrades (`mas`, `mise`)
  - Rustup self-update (`rustup self update`)
- Manager capability model in UI expanded to distinguish install/update/uninstall support.
- Authority-order regression tests for capability-aware refresh behavior.

### Changed
- Refresh orchestration now gates list actions by declared capability and detect state:
  - skips unsupported list actions
  - skips list actions when manager is not installed
- `mas` package parsing now uses app names instead of numeric App Store IDs.
- Registry capability declarations aligned to implemented adapter behavior.

### Fixed
- Prevented refresh failures for managers that do not implement `ListInstalled` (for example `softwareupdate`).
- Removed capability drift between registry metadata and runtime adapter behavior.
