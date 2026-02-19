# Helm Roadmap

This roadmap defines capability milestones. Dates are intentionally omitted.
Milestones are feature-driven, not time-driven.

---

## 0.1.x — Core Foundation (alpha) - Completed

Goal:
- Rust workspace initialized
- Manager adapter trait defined
- Capability declaration model
- SQLite schema v1
- Basic logging system

Exit Criteria:
- Compiles
- Unit tests pass
- No real adapters yet

---

## 0.2.x — First Adapter (alpha) - Completed

Goal:
- Homebrew adapter implemented
- list_installed
- list_outdated
- basic search (local)
- Task execution scaffold

Exit Criteria:
- Brew detection works
- Installed packages listed correctly
- Tests include parsing fixtures

---

## 0.3.x — Orchestration Engine (beta) - Completed

Goal:
- Background task queue
- Per-manager locking
- Cross-manager parallelism
- True process cancellation
- Structured error reporting

Exit Criteria:
- Multiple managers can run concurrently
- Same manager tasks are serialized
- Cancellation verified via tests

---

## 0.4.x — SwiftUI Shell (beta) - Completed

Goal:
- Menu bar app scaffold
- Task list UI
- Installed packages view
- Refresh action wired

Exit Criteria:
- App launches
- Refresh populates UI
- Tasks update live

Delivered:
- macOS menu bar app with floating panel UI (no Dock icon)
- XPC service architecture: sandboxed app communicates with unsandboxed service for process execution
- Rust FFI layer (`helm-ffi`) bridging Swift UI to Rust core via C ABI
- Real-time task list with 1-second polling (status transitions: Queued → Running → Completed/Failed)
- Installed and outdated packages views populated from Homebrew adapter
- Refresh action wired end-to-end: UI → XPC → FFI → orchestration → SQLite
- Code signing validation on XPC connections (team ID verification via SecCode)
- Centralized version management (workspace Cargo.toml, auto-generated HelmVersion.swift + xcconfig)
- Visual refresh feedback: spinner in nav bar and footer, button state management
- XPC reconnection logic with automatic retry on service interruption
- Tabbed UI (Dashboard / Packages) with MacPax-inspired design
- Dashboard: app icon, version, package stats, manager grid, recent tasks
- Package list: status filter bar, color-coded status icons, detail popover
- Settings popover with functional Refresh/Quit and disabled future controls
- Task ID persistence across app restarts (seeded from SQLite max ID)
- Process stdin null for XPC service daemon context

---

## 0.5.x — Progressive Search (beta) - Completed

Goal:
- Local-first fuzzy search
- Debounced remote search
- Cancellation semantics
- Cache enrichment model

Exit Criteria:
- Typing cancels remote searches
- Cache updates incrementally
- No UI freezing

Delivered:
- Progressive search across all three architectural layers (SwiftUI → XPC → Rust FFI)
- Local-first search: instant filtering of installed/outdated packages by name, plus SQLite cache query via XPC
- Debounced remote search: 300ms debounce timer triggers Homebrew formulae search as background task
- Cancellation: typing cancels in-flight remote searches with graceful 500ms grace period
- Cache enrichment: remote search results persist to SQLite `search_cache` table, enrich local queries on subsequent polls
- Search bar enabled in navigation bar with live spinner during remote search
- Available packages surfaced in package list and Dashboard stats
- Serde derives on search domain types for FFI JSON transport
- Three new FFI functions: `helm_search_local`, `helm_trigger_remote_search`, `helm_cancel_task`
- Three new XPC protocol methods bridging search FFI to SwiftUI
- Search orchestration in HelmCore: debounce timer, remote task tracking, XPC disconnect cleanup
- Comprehensive Rust integration tests: persistence watcher, cancellation, end-to-end search flows
- Compact Dashboard header layout for improved vertical space usage

---

## 0.6.x — Core Toolchain Managers (beta) - Completed

Goal:

- mise adapter
- rustup adapter
- Toolchain normalization layer
- Authority ordering engine
- Authority ordering enforced:

  - mise / rustup before package managers
- list_installed
- list_outdated
- upgrade toolchains
- version detection and normalization

Exit Criteria:

- mise detection works
- rustup detection works
- Toolchain upgrades execute before brew/npm/pipx
- Version parsing robust across versions
- Failure isolation verified
- Tests include fixture parsing for version output

Delivered:
- mise adapter with JSON parsing for `mise ls --json` and `mise outdated --json`
- rustup adapter with text parsing for `rustup toolchain list` and `rustup check`
- Authority ordering engine: `authority_phases()` groups adapters by Authoritative → Standard → Guarded
- `refresh_all_ordered()` executes phased parallel refresh with cross-manager parallelism within phases
- Shared adapter infrastructure: `process_utils::run_and_collect_stdout` and `detect_utils::which_executable`
- PATH injection for XPC context: `$HOME/.local/bin` (mise), `$HOME/.cargo/bin` (rustup)
- FFI registers all three adapters (Homebrew, mise, rustup) with shared TokioProcessExecutor
- Failure isolation: one manager failing does not block others in the same or subsequent phases
- End-to-end integration tests for mise and rustup with fake executors
- Multi-manager orchestration tests verifying cross-manager parallelism and authority ordering
- UI marks mise and rustup as implemented in the manager grid

---

## 0.7.x — System & App Store Managers (beta) - Completed

Goal:

- macOS `softwareupdate` adapter
- `mas` adapter
- Guarded execution model
- Explicit confirmation requirement for OS-level updates
- Reboot-required detection
- Privilege boundary validation
- Elevation flow defined (if needed)

Exit Criteria:

- `softwareupdate --list` parsed correctly and stable across macOS versions
- `mas outdated` detection works and is parsed correctly
- Guardrails block silent OS updates
- Reboot-required surfaced in UI

Delivered:
- softwareupdate adapter with `sw_vers` detection and `softwareupdate -l` parsing
- mas (Mac App Store) adapter with `mas version`, `mas list`, and `mas outdated` parsing
- restart_required field on OutdatedPackage model with schema migration v2
- Full 3-phase authority ordering validated: Authoritative (mise, rustup) → Standard (mas) → Guarded (Homebrew, softwareupdate)
- FFI registration for all 5 adapters with shared TokioProcessExecutor
- Restart-required indicator in PackageRowView and PackageDetailPopover
- End-to-end integration tests for softwareupdate and mas adapters
- 5-adapter authority phase ordering test

### 0.7.0

- Onboarding wizard: 3-step first-run experience (Welcome → Detection → Configure)
- Auto-detect on launch: triggerRefresh on app appear for returning users
- Reset Local Data: full database wipe with onboarding re-entry via Settings
- Manager controls:
  - install: mas, mise
  - update/self-update: Homebrew, mas, mise, rustup
  - uninstall: mas, mise, rustup
- Rustup version detection fix: bypass pipe EOF deadlock from background subprocesses
- Mise version parser: handle new output format without "mise " prefix
- Task auto-pruning: completed/failed/cancelled tasks cleaned after 5 minutes
- Reset lifecycle hardened: stop polling during wipe, delete stale task records
- Manager toggles disabled for non-installed managers (show "Not Installed")
- Dashboard layout: fixed header with scrollable Recent Tasks section
- Onboarding detection: spinner during scan, found-only manager list with versions
- Tab reorder: Dashboard → Packages → Managers
- Refresh ordering hardened:
  - capability-aware ordered refresh (skip unsupported list actions)
  - skip list actions when detection reports manager not installed
- mas parsing corrected to use app names (instead of numeric App Store IDs)
- Registry capability declarations aligned with implemented adapter actions

---

## 0.8.x — Pinning & Policy Enforcement (beta) - Completed

Delivered:

- Native pin support
- Virtual pin fallback
- Pin-aware upgrade-all
- Timeout enforcement across all adapters
- Manager enable/disable toggles
- Per-manager structured error reporting hardened
- Safe mode execution mode

Exit Criteria:

- All adapters respect pin state
- Safe mode blocks OS updates
- softwareupdate cannot run without explicit confirmation
- Cancellation verified across mise / rustup / mas / brew
- Settings persist reliably

---

## 0.9.x — Internationalization Foundation (beta) - Completed

Goal:

- Centralized localization system (`locales/` directory)
- Key-based string architecture (no hardcoded UI strings)
- ICU message format support (plurals, variables)
- Locale detection and override (system + user preference)
- Fallback strategy implementation (locale → language → en)
- Structured error localization (service returns keys, not strings)
- Initial English locale (`en`) fully implemented
  - Localization wrapper APIs for SwiftUI (app), Rust (service/core key-return), and website (i18n utilities + routing conventions)

Exit Criteria:

- No user-facing text is hardcoded
- All UI strings resolved via localization system
- Missing keys visibly fail in development
- Fallback logic verified
- Error messages display localized strings via keys
- Introduce i18n:string-freeze before 0.12.x begins; new UI strings require keys + English text + metadata.
- CI check prevents new hardcoded user-facing strings in UI layer (best-effort heuristic is OK initially)

---

## 0.10.x — Core Language Package Managers (beta) - Completed

Goal:

- npm (global) adapter
- pipx adapter (recommended for Python CLI tools)
- pip adapter (`python3 -m pip`, global packages only)
- Cargo adapter (global installs via `cargo install`)
- cargo-binstall adapter
- All adapters declare capabilities: install, uninstall, list, outdated, search
- Global packages only; project-local dependencies excluded unless explicitly enabled

Note: Homebrew Casks adapter was originally planned for this milestone but deferred to 0.14.x.

Exit Criteria:

- Detection works for all five managers
- list_installed and list_outdated produce correct results
- Search returns results for managers that support it
- Fixture-based parser tests for each adapter's output format
- Authority ordering preserved (language managers execute as Standard authority)

---

## 0.11.x — Extended Language Package Managers (beta) - Completed

Goal:

- pnpm (global) adapter
- yarn adapter (classic + berry)
- poetry adapter
- RubyGems adapter
- bundler adapter
- Same capability declaration requirements as 0.10.x adapters

Exit Criteria:

- Detection works for all five managers
- list_installed and list_outdated produce correct results
- Search returns results for managers that support it
- Fixture-based parser tests for each adapter's output format
- Patterns established in 0.10.x reused consistently

Delivered:
- Implemented `pnpm` (global), `yarn` (global), `poetry` (self/plugins), `RubyGems`, and `bundler` adapters end-to-end.
- Wired all five managers through core adapter registry, FFI runtime registration, upgrade routing, and macOS UI manager metadata.
- Added parser fixtures and adapter unit tests for version/list/search/outdated flows where supported.
- Added bounded retry handling for transient task-store persistence failures in orchestration runtime paths.
- Added regression coverage for refresh-response error attribution and transient task-persistence recovery behavior.
- Added repeatable stabilization artifacts for beta checkpoint validation:
  - `docs/validation/v0.11.0-beta.2-smoke-matrix.md`
  - `docs/validation/v0.11.0-beta.2-l10n-overflow.md`

---

## 0.12.x — Localization + Upgrade Transparency (beta) - Completed

Goal:

- Initial non-English locales:
  - es, fr, de, pt-BR, ja
- Translation coverage for core user flows:
  - onboarding
  - refresh
  - updates
  - errors
- Locale selection UI (Settings)
- Text expansion and layout validation
- Tagline transcreation support
- Dedicated upgrade preview UI
- Dry-run simulation path in upgrade preview

Exit Criteria:

- Core workflows fully usable in all supported languages
- No UI truncation or layout breakage
- Pluralization verified across locales
- Tagline displays correctly per locale strategy
- Upgrade preview exposes execution-mode breakdowns before submission
- Dry-run mode presents planned actions without submitting tasks

Delivered:
- Locale coverage expanded to `en`, `es`, `de`, `fr`, `pt-BR`, and `ja` with full app/common/service key parity.
- Added locale integrity checks and locale mirror parity enforcement in CI.
- Added on-device visual overflow validation for settings plus expanded onboarding/navigation/packages/managers surfaces.
- Added dedicated upgrade preview UI in settings with manager-level package breakdowns for no-OS and with-OS paths.
- Added dry-run mode in upgrade preview with explicit simulation result messaging and no task submission.

---

## 0.13.x — UI/UX Analysis & Redesign (beta/rc)

Goal:

- Full UX audit across onboarding, dashboard, packages, managers, tasks, and settings
- Information architecture redesign for core operator flows:
  - detect and refresh
  - search and package actions
  - upgrade-all confirmation and execution visibility
- Visual system refresh:
  - typography and spacing scale
  - hierarchy and state affordances
  - accessibility contrast and focus behavior
- Interaction and feedback consistency:
  - loading, error, and empty states
  - long-running task and cancellation visibility
- Validation plan:
  - usability scenario checklist
  - regression-safe incremental rollout strategy

Exit Criteria:

- UX audit findings documented with prioritized redesign decisions
- Updated interaction map and component-level redesign spec approved
- At least one end-to-end flow implemented using redesigned patterns without breaking orchestration invariants
- Accessibility and localization implications captured for subsequent milestone execution

Delivered (`v0.13.0-beta.1` checkpoint):
- Redesigned menu bar popover shell integrated into `apps/macos-ui` with:
  - top updates attention banner + custom upgrade-all action
  - layered overlays for search/settings/about/quit with dimmed underlay
  - right-click status-item quick action menu and in-icon status badges
- Redesigned control-center shell integrated with:
  - compact titlebar-hidden top bar and global search routing
  - full-row sidebar section targets with hover/press tactile states
  - card-based Settings surface and manager-aware action badges
- Accessibility and interaction upgrades:
  - reduced-motion-aware overlay transitions
  - keyboard shortcuts (`Cmd+F`, `Esc`, `Cmd+W`)
  - pointer affordance/hit-testing cleanup for overlay states
- Manager health model refinement:
  - explicit gray `Not Installed` state for undetected managers

Delivered (`v0.13.0-beta.2` checkpoint):
- Universal-build support (arm64/x86_64) for signed DMG packaging
- Release automation workflow for signed artifacts

Delivered (`v0.13.0-beta.3` checkpoint):
- VoiceOver accessibility labels, semantic grouping, and state-change announcements
- Task cancel button wired via XPC
- CI test enforcement (cargo test + xcodebuild test)
- HelmCore and DashboardView decomposition

Delivered (`v0.13.0-beta.4` checkpoint):
- Guided onboarding walkthrough with SpotlightOverlay (6 popover + 7 CC steps)
- WalkthroughManager with persistence, skip, and replay
- 31 walkthrough L10n keys across all 6 locales

Delivered (`v0.13.0-beta.5` checkpoint):
- UI layer purity fixes (business logic extracted from views to HelmCore/ManagerInfo)
- Legacy redesign scaffold removed
- XPC robustness: timeout enforcement, exponential backoff, decode error logging

Delivered (`v0.13.0-beta.6` checkpoint):
- Structured tracing spans on adapter execution paths
- Homebrew split_upgrade_target() unit test coverage
- FFI lifecycle and migration error documentation
- INTERFACES.md Section 10 filled with concrete inventories
- On-device validation report template and usability test plan

Delivered (`v0.13.0-rc.1` checkpoint):
- Inspector sidebar: task detail view with status badge, task type, manager, label key/args
- Inspector selection clearing fixes across overview, managers, dashboard, and popover views
- Inspector manager detail enriched with health badge, package/outdated counts, and View Packages navigation
- Overview task rows wired to inspector via tap handling
- Post-upgrade validation on all 11 adapter upgrade handlers (Homebrew, RubyGems, npm, pnpm, yarn, pip, pipx, cargo, cargo-binstall, bundler, poetry) — prevents silent upgrade failures
- Control Center menu item added to status menu right-click
- Task label support for descriptive upgrade task names (e.g., "Upgrading rake")
- Security Advisory System milestone added to roadmap (1.3.x)

Delivered (`v0.13.0-rc.2` checkpoint):
- Support & Feedback card in Settings with 5 actions (Support Helm, Report a Bug, Request a Feature, Send Feedback, Copy Diagnostics)
- Include Diagnostics toggle with clipboard copy before opening GitHub issue templates
- Support Helm submenu in status menu right-click (GitHub Sponsors, Patreon)
- GitHub Sponsors and Patreon funding integration (.github/FUNDING.yml)
- 11 new L10n keys across all 6 locales
- AppDelegate decomposed to satisfy SwiftLint thresholds

---

## 0.14.x — Platform, Detection & Optional Managers (rc)

Goal:

- Container & VM managers:

  - Docker Desktop adapter (detection and upgrade prompting)
  - podman adapter (detection and upgrade prompting)
  - colima adapter (detection and upgrade prompting)
  - Parallels Desktop adapter (detection only)
- Security, firmware & vendor tools:

  - Xcode Command Line Tools adapter
  - Rosetta 2 adapter (Apple Silicon detection and status)
  - Firmware updates adapter (`softwareupdate --history`)
- App detection managers:

  - Sparkle-based updaters (detection only)
  - Setapp (detection only)
- Optional toolchain managers:

  - asdf adapter (optional / compatibility mode)
  - MacPorts adapter (optional)
  - nix-darwin adapter (optional)

Exit Criteria:

- Detection works for all managers in this milestone
- Full adapter capabilities implemented for non-detection-only managers
- Detection-only managers surface status correctly in UI
- Optional managers clearly marked and disabled by default
- Fixture-based parser tests for each adapter

Delivered (`v0.14.0-rc.1` checkpoint):
- Manager metadata scaffolding with optional/detection-only status export in FFI and Swift fallback metadata
- Container/VM adapters: Docker Desktop, podman, colima
- Detection-only adapters: Sparkle, Setapp, Parallels Desktop
- Security/Firmware adapters: Xcode Command Line Tools, Rosetta 2, Firmware Updates
- Optional adapters: asdf, MacPorts, nix-darwin
- Homebrew cask status adapter (`homebrew_cask`)
- Manager capability sweep artifact: `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`
- README/website + workspace/app version alignment to `0.14.0-rc.1`

---

## 0.15.x — Upgrade Preview & Execution Transparency (beta)

Goal:

- Advanced execution-plan visibility
- Failure isolation reporting improvements
- Operator controls for large-plan workflows
- Ordered plan rendering

Exit Criteria:

- Users can inspect full execution plan with expanded operator context
- Partial failure clearly reported

---

## 0.16.x — Self-Update & Installer Hardening (beta)

Goal:

- Sparkle integration for the direct Developer ID consumer channel
- Signed update verification
- Delta updates
- Self-update testing across versions
- Explicit channel boundaries for update systems:
  - Sparkle only in direct Developer ID consumer build
  - no Sparkle in MAS, Setapp, or Helm Business fleet builds

Exit Criteria:

- Helm can update itself safely
- Downgrade handling defined
- Update interruption recovery tested
- Direct channel Sparkle behavior is isolated from non-Sparkle channels

---

## 0.17.x — Diagnostics & Logging (rc)

Goal:

- Per-task log viewer
- Structured error export
- Service health diagnostics panel
- Manager detection diagnostics

Exit Criteria:

- Logs accessible in UI
- No silent failures
- Support data export works

---

## 0.18.x — Stability & Pre-1.0 Hardening (rc)

Goal:

- Full integration test matrix, especially:

  - multi-manager authority ordering
  - guarded OS update flow
  - failure isolation
- Multi-manager orchestration stress tests
- Cancellation stress tests
- Logging refinement
- Crash recovery validation
- No known race conditions
- Memory safety audit
- i18n validation:
  - key parity across locales
  - placeholder consistency
  - ICU syntax validation
- UI validation for text expansion
- Localization fallback behavior tested

Exit Criteria:

- All core paths tested
- No known race conditions
- No unhandled panics
- Stable FFI boundary
- Deterministic execution verified

---

## 1.0.0 — Stable Control Plane Release

Goal:

- Stable architecture
- Stable adapter trait
- Stable orchestration semantics
- Production-safe execution
- Self-update operational
- Authority ordering guaranteed
- Guardrails enforced
- Logs and diagnostics present

---

## 1.1.x — Globalization Expansion

Goal:

- Additional locales:
  - zh-Hans, ko, it, nl
- Localization coverage for all UI surfaces
- Website localization
- Documentation localization (partial)

Exit Criteria:

- Additional locales meet UX and QA standards
- Website supports locale routing
- Translation coverage >80% for supported locales

---

## 1.2.x — Editions and Entitlement Foundations

Goal:

- Formalize channel-aware build configurations for planned artifacts (MAS, Developer ID, Setapp, Fleet)
- Implement runtime entitlement model for Helm consumer (Free/Pro) and Helm Business (fleet)
- Add entitlement verification and safe degradation behavior
- Define managed bootstrap configuration contract for fleet MDM deployments
- Keep update authority decoupled from licensing authority by channel

Exit Criteria:

- Distribution build matrix and channel authority mapping are documented and approved
- Invalid/expired entitlements degrade predictably without unsafe behavior
- Managed bootstrap configuration is documented and testable

---

## 1.3.x — Security Advisory System (Pro)

Goal:

- CVE vulnerability awareness for installed packages
- Local-first advisory evaluation (advisory only, no enforcement)
- Data sources: OSV.dev, NVD, manager-specific feeds
- Matching engine: package name + version range → severity + recommendations
- SQLite-backed advisory cache with periodic refresh
- UI: vulnerability status, severity badges, and recommended actions per package
- Offline-capable with cached advisory data
- Non-blocking: advisory checks never delay operations

Exit Criteria:

- Advisory data ingested and cached locally from at least one source (OSV.dev)
- Matching engine correctly identifies affected packages by name and version range
- Affected packages surfaced in UI with severity and recommended action
- Advisory refresh works offline using cached data
- Advisory evaluation does not block or delay any manager operations
- Pro edition entitlement gate verified (feature unavailable in Free edition)

---

## 1.4.x — Business Policy and Drift Management

Goal:

- Scoped policy model (org / department / team / environment / device group)
- Baseline profile enforcement for package and toolchain consistency
- Drift detection and compliance reporting
- Policy snapshot persistence for offline-safe enforcement
- Organization-level locale policies
- Locale enforcement for managed environments
- Multi-locale reporting and audit output

Exit Criteria:

- Policy precedence rules are deterministic and tested
- Drift categories and compliance states are surfaced clearly
- Offline behavior uses last valid policy snapshot without UI/executor instability

---

## 1.5.x — Enterprise Rollout, Approvals, and Audit

Goal:

- Ring-based rollout workflow (canary, pilot, broad)
- Policy approval workflow and rollback controls
- Audit/event export integrations (SIEM/webhook/ticketing targets)
- Role-based access model for business operators

Exit Criteria:

- Ring promotion and rollback flow verified end-to-end
- Policy changes and enforcement actions produce attributable audit events
- Enterprise controls preserve Helm safety and orchestration guarantees

---

## 1.6.x — Mac App Store Distribution Channel

Goal:

- Deliver consumer distribution through Mac App Store channel requirements
- Align consumer licensing behavior with App Store commerce/receipt authority
- Preserve core architecture and runtime gating model without Sparkle coupling

Exit Criteria:

- MAS channel lifecycle is documented and operationally separable from direct and fleet channels
- Channel update authority is App Store-managed
- No Sparkle dependency in MAS channel

---

## 1.7.x — Setapp Distribution Channel

Goal:

- Deliver consumer distribution through Setapp channel requirements
- Align licensing behavior with Setapp subscription authority
- Preserve architecture invariants and channel-isolated update behavior

Exit Criteria:

- Setapp channel lifecycle is documented and operationally separable from direct, MAS, and fleet channels
- Channel update authority is Setapp-managed
- No Sparkle dependency in Setapp channel

---

## 1.8.x — Helm Business Fleet Product

Goal:

- Deliver Helm Business as a separate fleet-focused binary and lifecycle
- Keep one shared core codebase while separating consumer and fleet release operations
- Integrate business policy/compliance capabilities without collapsing architecture boundaries

Exit Criteria:

- Fleet product boundaries are explicit and documented
- Fleet release lifecycle is independent from consumer release cadence
- Business operational model aligns with enterprise policy and compliance requirements

---

## 1.9.x — PKG + MDM Deployment and Offline Licensing

Goal:

- Deliver PKG-based enterprise deployment flow for Helm Business
- Deliver MDM-ready managed bootstrap and admin-controlled update workflows
- Deliver offline organizational license-file handling for fleet environments

Exit Criteria:

- PKG + MDM deployment lifecycle is documented and validated as the fleet distribution path
- Offline license file model is documented with fail-safe behavior
- Fleet update flow remains admin-controlled and decoupled from consumer update channels
