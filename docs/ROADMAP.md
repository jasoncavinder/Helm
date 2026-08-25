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
- Support & Feedback card in Settings with 5 actions (Support Helm -> GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo; plus Report a Bug, Request a Feature, Send Feedback, Copy Diagnostics)
- Include Diagnostics toggle with clipboard copy before opening GitHub issue templates
- Support Helm destinations across app surfaces now include all six configured channels (GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo)
- GitHub Sponsors and Patreon funding integration (.github/FUNDING.yml), plus direct support links for Buy Me a Coffee, Ko-fi, PayPal, and Venmo
- 11 new L10n keys across all 6 locales
- AppDelegate decomposed to satisfy SwiftLint thresholds

---

## 0.14.x — Platform, Detection & Optional Managers (stable)

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

Delivered (`v0.14.0` checkpoint):
- Manager metadata scaffolding with optional/detection-only status export in FFI and Swift fallback metadata
- Container/VM adapters: Docker Desktop, podman, colima
- Detection-only adapters: Sparkle, Setapp, Parallels Desktop
- Security/Firmware adapters: Xcode Command Line Tools, Rosetta 2, Firmware Updates
- Optional adapters: asdf, MacPorts, nix-darwin
- Homebrew cask status adapter (`homebrew_cask`)
- Manager capability sweep artifact: `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`
- README/website + workspace/app version alignment to `0.14.0`

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

## 0.16.x — Self-Update & Installer Hardening (beta) - Completed

Goal:

- Sparkle integration for the direct Developer ID consumer channel
- Signed update verification
- Full-installer update feed policy (delta payloads deferred beyond `0.16.x`)
- Self-update testing across versions
- Explicit channel boundaries for update systems:
  - Sparkle only in direct Developer ID consumer build
  - no Sparkle in MAS, Setapp, or Helm Business fleet builds

Exit Criteria:

- Helm can update itself safely
- Downgrade handling defined
- Update interruption recovery tested
- Direct channel Sparkle behavior is isolated from non-Sparkle channels

Delivered (`v0.16.0` checkpoint):
- Channel-aware app-update configuration in runtime + build settings (`HelmDistributionChannel`, `HelmSparkleEnabled`, `SUFeedURL`, `SUPublicEDKey`, `SUAllowsDowngrades`)
- Direct-channel Sparkle integration with strict runtime gating (package-manager-managed installs blocked; mounted-DMG/translocated paths blocked)
- Signed appcast generation/publication automation with policy validation (HTTPS + full-installer DMG only)
- Packaged DMG verification for updater invariants, Sparkle linkage, and artifact integrity before notarization/release publication
- Installer/updater interruption-and-recovery validation runbook + idempotent rerun rehearsal

---

## 0.17.x — Diagnostics & Logging (stable) - Completed

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

## v0.17.12 — Upgrade Workflow Authority Correction (released)

Goal:

- Move bulk and scoped upgrade phase sequencing from SwiftUI into the Rust execution boundary.
- Preserve individual task records, `plan_step_id` labels, cancellation, and diagnostics visibility.
- Ensure authoritative manager work reaches terminal state before standard or guarded work is scheduled.

Exit Criteria:

- GUI bulk, scoped, and manager-scoped upgrades use the same backend workflow path.
- Workflow cancellation prevents future authority-phase submission and retains existing in-flight task cancellation.
- Regression coverage verifies phase ordering and scoped package filtering.

---

## 0.18.x — Doctor & Repair Foundation - Released on `main`

Release status: published through final-containment `v0.18.2`; `v0.18.0` remains withdrawn because of its critical SQLite migration defect, and `v0.18.1` remains its corrective successor.

Goal:

- Introduce a dedicated doctor scan pipeline for manager/package-state health findings.
- Add deterministic, versioned finding fingerprints that converge for equivalent normalized problems across installations while keeping local evidence separate.
- Persist finding lifecycle, repair knowledge, import provenance, and repair history in SQLite.
- Add repair planning primitives (plan/apply) backed by portable local knowledge that references only typed Helm repair capabilities.
- Add deterministic knowledge import/export contracts without automatic network lookup.
- Keep repair execution routed through existing task orchestration and safety gates.
- Deliver first concrete remediation path:
  - Homebrew metadata-only manager installs (installed metadata exists without matching executable instance).
- Maintain parity-ready surface contracts for GUI/CLI/TUI, even if some workflows are initially scaffolded.

Exit Criteria:

- Doctor scan report is available and deterministic from local state.
- Findings include fingerprint, severity, and top evidence factors; fixture tests prove equivalent normalized findings produce the same fingerprint.
- Fingerprints exclude absolute paths, timestamps, raw output, and other sensitive or incidental evidence.
- Repair plan/apply is functional for metadata-only Homebrew manager-install mismatch.
- SQLite-backed planning preserves all currently delivered repair options and stable option IDs for metadata-only installs, post-install setup, and stale selected-executable findings.
- SQLite-backed knowledge can be deterministically imported/exported and maps findings only to allowlisted typed action IDs.
- Knowledge payloads cannot contain executable paths, commands/arguments, shell fragments, scripts, plugins, or arbitrary code.
- Repair apply revalidates the active finding, preserves required confirmation, and verifies the outcome through a follow-up scan.
- Full, scoped, partial, failed, and cancelled scan tests prove only successfully covered detector scope can resolve persisted findings.
- Import tests reject forged trust, policy weakening, unknown actions, malformed envelopes, and nondeterministic conflict outcomes.
- Existing manager install/uninstall UX for metadata-only mismatch is migrated to the repair subsystem path.

Sequencing note:

- This milestone executes first in the `0.18.x` stream before broader local security/advisory groundwork.

---

## Security Staging

Stage 0 (`<=0.16.x`):
- Documentation and planning only
- No security advisory logic implemented

Stage 1 (`0.18.x`):
- Internal local-only groundwork for vulnerability data handling
- Doctor/repair fingerprinting and local remediation scaffolding may share future serialization/provenance/trust infrastructure with advisories while retaining separate domain models, stores, and evaluation semantics
- No public feature exposure
- No Pro gating
- No centralized backend

Stage 2 (`1.3.x`) — Security Advisory System (Pro):
- Local-first CVE/advisory scanning and recommendations
- Optional public vulnerability API queries
- Local cache with TTL-based refresh
- No Helm-operated central database
- No fingerprint sharing
- No App Attest

Stage 3 (`1.4.x`) — Shared Brain:
- Centralized fingerprint database and known-fix lookup
- Postgres-first system-of-record
- Optional edge/serverless API layer (Cloudflare Workers may be used, but is not required)
- Anonymous per-install auth with Apple App Attest
- Request signing, nonce and replay protection, rate limiting, and abuse controls
- Provider portability requirement (no Durable Objects / D1 lock-in as core persistence)

---

## 0.18.x — Local Security Groundwork (second slice) - Released on `main`

Release status: published through final-containment `v0.18.2` after migration remediation, recovery validation, and migration-safety hardening.

Goal:

- Local vulnerability data model abstractions in core
- Manager-agnostic normalization contract for advisory records
- Local cache schema groundwork (TTL-ready metadata)
- Task/orchestration hooks for future advisory refresh/evaluation tasks
- CLI companion planning/specification kickoff with shared GUI+CLI coordinator model
- No UI exposure and no user-facing advisory feature gate yet
- No Pro entitlement gating in this phase
- No centralized Helm backend in this phase

Exit Criteria:

- Core contracts for local advisory data handling are documented and testable
- Advisory groundwork paths preserve deterministic task execution
- CLI specification is published and aligned with architectural invariants
- No user-facing Security Advisory UI shipped in this milestone
- No backend dependency introduced

---

## 0.18.x — Pre-1.0 Experience Definition (post-release planning closure) - Completed

Status: planning/prototype artifact closure completed after corrective `v0.18.1` and is included as planning-only content in the `v0.18.2` containment release alongside migration-safety hardening.

Goal:

- Audit the current menu bar app and Control Center against core user jobs and native macOS behavior.
- Inventory custom controls and record where native controls, windows, menus, toolbars, Settings, lists/tables, focus, and selection should replace or constrain custom presentation.
- Define the owner-run task-based usability/accessibility protocol, complete an expert cognitive walkthrough, and set measurable design-quality budgets; do not claim participant evidence until sessions occur.
- Prototype the menu bar surface, Control Center, Settings, and Project WOW first run at sufficient fidelity to test workflow and platform behavior.
- Define the complete loading/success/empty/partial/failure/offline/blocked/cancellation/recovery state matrix.
- Produce an incremental migration map that preserves service/core business-logic authority.

Exit Criteria:

- The v0.18 Health/Updates/Packages/Activity/Sources planning model is complete historical input; Original Wayfinder's Dashboard/Plan/Library/Activity plus contextual Environment model now supersedes it for production migration.
- Accessibility, localization, motion, perceived-performance, window, input, and state-quality gates are approved.
- Project WOW and whole-app navigation share one compatible experience model.
- Intentional departures from macOS conventions have documented Helm-specific rationale.
- The design lane does not alter the released Doctor/repair, local-security, or migration-safety behavior.
- Owner-run participant validation remains open and is required before v0.20 workflow sign-off and v0.22 UI lock.

---

## 0.19.x — Native Experience Foundation & First-Run Value (stable)

Status: `v0.19.1` is the current stable maintenance release, and `v0.19.0-rc.7` remains available on the isolated beta/RC channel. The line delivers Dashboard-aware status-item routing, actionable update notifications, selective Plan execution, XPC recovery hardening, corrected Settings behavior in external full-screen Spaces, truthful vendor-app routing for third-party Sparkle updates, Node 24-compatible CI actions, clarified manager and status semantics, first-class offline recovery, and the retained-Homebrew-keg correction in `v0.19.1`. The Environment Brief remains debug-gated pending Issue #388, native privileged execution remains on askpass pending its separate activation gates, and selectable themes remain post-1.0 work.

Goal:

- Complete the manager/updater modernization slice:
  - inventory Sparkle apps in standard application roots while excluding Helm by bundle identifier
  - surface bounded static-HTTPS-appcast update visibility with per-feed failure isolation
  - keep third-party Sparkle updates as truthful vendor-authoritative interactive steps, with Open App as the reliable fallback when Helm cannot safely complete an external installation
  - expose scheduled Sparkle checks for eligible direct-channel Helm builds
  - harden npm multi-install normalization and MAS refresh outcomes
  - adopt macOS 13 Ventura as the minimum supported release
- Establish the native macOS application foundation defined in `docs/app-design/NATIVE_MACOS_EXPERIENCE.md`:
  - implement the approved Original Wayfinder two-surface experience and Course Indicator contract captured by the reproducible, non-shipping SwiftUI design lab
  - native window, titlebar, toolbar, menu-command, sidebar/split-view, selection, and focus behavior
  - standard semantic component layer with documented exceptions for Helm-specific controls
  - first-class macOS Settings window/scene with Command-Comma behavior
  - window restoration, resizing, activation, and multi-display behavior
  - keyboard traversal infrastructure, including AppKit bridges where SwiftUI is insufficient
- Introduce revision-aware snapshot transport and evaluate event delivery only where it reduces polling without weakening reconnect, ordering, replay, or backpressure guarantees.
- Consolidate related XPC reads/actions behind additive, versioned request/response contracts; retain compatibility until all surfaces migrate.
- Begin Project WOW base first-run value delivery on the native experience foundation:
  - staged local discovery and streaming Environment Brief
  - conservative defaults with an inspection receipt
  - contextual guidance replacing mandatory first-run tours
  - reviewed plan foundation using native progress, focus, selection, and sheet behavior
- Implement against `docs/contracts/first-run/`, `docs/architecture/FIRST_RUN_EXPERIENCE_CONTRACTS.md`, and the v0.19 slices in `docs/app-design/NATIVE_MACOS_MIGRATION_MAP.md`; do not place planning, consent, execution, verification, or recovery policy in SwiftUI.
- Finalize production typography direction across app/website.
- Reminder: purchase Neue Haas Grotesk commercial license before shipping it in product branding; the app interface continues to prefer system typography.

Exit Criteria:

- Snapshot revision/reconnect behavior and consolidated XPC contracts are covered by integration tests.
- App shell and first-run foundation are coherent in light/dark appearance at minimum and expanded window sizes.
- Original Wayfinder is recorded as the approved Dashboard/popover foundation, and the Course Indicator uses one backend-projected semantic/progress state with complete accessibility and appearance fixtures.
- Toolbar actions have application-menu command equivalents.
- Settings are reachable through the application menu and Command-Comma and contain durable preferences rather than operational workflows.
- No new surface introduces an ad hoc control or status semantic outside the approved component system.
- Project WOW discovery and Environment Brief perform no unprompted mutation and have no mandatory network dependency.

---

## 0.20.x — Core Workflow & Information Architecture Redesign (rc)

Goal:

- Redesign Dashboard, Plan, Library, Activity, contextual Environment, command-based search, contextual detail, and diagnostics around native macOS patterns.
- Validate the approved Wayfinder information architecture against task frequency and owner-moderated research without reopening it through incidental implementation choices.
- Use native list, table, outline, split-view, toolbar, menu, contextual-menu, and inspector behavior where comparison and density matter.
- Simplify action hierarchy and remove duplicated or misplaced entry points.
- Preserve one continuous plan -> execution -> verification -> recovery presentation.
- Complete content-design passes for labels, empty states, errors, policy blocks, provenance, authority, and recovery.
- Implement the unified Wayfinder status-item popover so it remains glanceable and routes cleanly into Dashboard context.
- Implementation checkpoint: the approved fixed-footprint composition, shared semantic Course projection, environment-domain route, single context row, three fast commands, native utility menu, and Dashboard-owned detail/search/confirmation surfaces are implemented. A debug-only six-state seam renders the production popover without submitting refresh or Helm update checks; the owner state/accessibility matrix is complete; reusable per-session/aggregate records and the validated `v0.20-whole-workflow-v1` synthetic corpus are defined. Task 1's partial source coverage and exact applied-but-unverified npm activity now seed shared status-item, popover, Dashboard-hero, and sidebar Course truth; affected System and Packages routes select MacPorts and npm, and Review Recovery routes to Activity item `7001` with inspector/focus handoff. Valid and selected-invalid research states preserve the fixed footprint, and invalid selection reports unavailable/disconnected truth. Task 2's exact 12-update scenario now projects read-only through the shipping Plan, inspector, selected-row risk model, and bounded confirmation without service startup or mutation. Task 3's exact cached Homebrew and remote Cargo `ripgrep` scenario now projects read-only through the shipping Library, global search, package inspector, bounded install confirmation, and offline-deferred variant. Task 4's exact applied-but-unverified npm activity and unstarted App Store source now project read-only through the shipping Activity destination and contextual inspector, preserving separate apply/verification facts, before/after and rollback-limit context, ordered recovery choices, and strictly redacted synthetic diagnostics without service startup or mutation. Task 5's exact ten-manager corpus now projects read-only through the shipping Environment view, with full-record fail-closed validation, explicit unavailable presentation for selected missing or invalid data, source-state-first health, exact rustup active/manageable user provenance, inactive/policy-blocked system provenance, policy-versus-permission explanation, a reversible fixture-local Keep Multiple acknowledgment, live inspector localization, selected-row `.isSelected` semantics, explicit Active/Inactive instance badges and accessibility values, disclosure-state accessibility, and corrected regression-protected Hungarian health labels. Production and fixture Library rows share a Ventura-compatible native table with exact consolidated-member routing, native selection and keyboard/type navigation, localized context/action commands, and retained status, pin, restart, version, provenance, and recommendation facts. Production remote-search submissions are generation-bound and carry explicit interactive or description request purpose through XPC/FFI into Rust in-flight deduplication, preventing cross-purpose task-ID reuse. Cancellation is limited to interactive task IDs owned by the superseded query, including late stale callbacks, and uses a dedicated local-runtime path whose 500 ms graceful window treats verified already-terminal Search races as benign while surfacing real failures. Snapshot reconciliation retires explicitly owned task IDs on terminal status or after two consecutive authoritative snapshot absences without adopting unrelated work. The Library's automated 20,000-row gate now passes with semantic snapshot invalidation, a bounded identity-preserving manager-search overlay, exact locale ordering, and the shared production row-projection cache: cold indexed/end-to-end p95 are `135.458/195.415 ms`, warm indexed median/p95 are `22.365/24.332 ms`, warm production end-to-end p95 is `30.273 ms`, 117 of 120 AppKit scroll-critical samples remain within `16.7 ms`, and only 13 row views are retained. A unique-artifact, host-readiness-aware arm64 harness preserves the contract while visible-window Instruments evidence remains scheduled for v0.22. The Plan uses a Ventura-compatible native authority-grouped outline with contextual toolbar filters and automated empty/single/125-step, exclusion, stale-selection, stale-confirmation, partial-failure, cancellation, selected-risk, and hidden-selection coverage. The legacy generic Upgrade All preview is removed: Dashboard routes to Plan, notification Upgrade All requests Plan's reviewed flow, and the final sheet snapshots exact order, automatic/interactive eligibility, vendor handoffs, and selected risk before failing closed on drift. Dashboard and Settings navigation now use semantic native sidebar split views with title-free full-size chrome, system-owned resizing, collapse behavior, dividers, and toolbar toggles; a restrained Original Wayfinder pale-blue-to-near-white surface extends beneath the window controls without replacing native navigation behavior, and Dashboard's inspector remains inside the detail column without changing the window frame. Owner Plan and production native-table Task 3 Library/global-search visual/accessibility validation plus the automated Library performance gate are complete. Task 1 automated evidence and its attributable current-head visual/interaction/Full Keyboard Access/VoiceOver/localization owner matrix are complete on `3de57a81`; the retained pre-final-source observations remain directional evidence only. Task 5's `d60aad9b` pass is historical superseded evidence only. The later `d2351667` owner matrix remains attributable for unaffected behavior, Full Keyboard Access, localization, and layout observations, but independent review proved that its no-Hungarian-English-fallback and complete-VoiceOver-semantics claims were inaccurate. An attributable current-source owner matrix on exact app source `9e3fa24a` closes inactive-badge minimum/expanded Light/Dark/+40% layout, selected/inactive VoiceOver semantics, live Hungarian presentation and accessibility-title refresh, locale-specific Settings labels, and independent Settings behavior. Task 6 now projects the canonical disabled launch-at-login preference through shipping General Settings with context-owned fixture-local changes, blocks real `SMAppService` access, fails closed for missing or invalid selected data, and preserves the exact Activity `7001` plus strictly redacted Copy Diagnostics path; owner validation is complete on exact app source `43f15385`. The Task 7 implementation is complete, and recorded exact-source owner checks pass on `e87d30d3`, including detailed VoiceOver state semantics; the participant checkpoint remains open. The moderated participant checkpoint and remaining v0.20 workflow redesign continue before release sign-off.
- Task 5 evidence correction: source through `890cf4fe` corrects Hungarian in-progress Refreshing to `Frissítés folyamatban`, live-refreshes the hidden AppKit Dashboard and Settings accessibility titles, and expands the arm64 app-target gate to 321 tests. The 13:56 HST owner artifact remains bounded unattributed evidence because it predates `061e9fa2` and lacks a retained source mapping. A fresh isolated arm64 Debug app built from exact app source `9e3fa24a` and tracked-clean tree `fdfc45a8` closes the current-source inactive-badge layout/+40%, selected/inactive VoiceOver, complete Hungarian presentation, live accessibility-title, locale-specific Settings-label, and independent Settings behavior checks. Task 6 implementation, its 325-test arm64 macOS 13 app-target gate, and its attributable owner visual, interaction, accessibility, localization, layout, diagnostics, reversibility, and cross-window focus matrix on exact app source `43f15385` are complete. The recorded Task 7 exact-source owner checks pass on `e87d30d3`, including detailed VoiceOver state semantics; the participant checkpoint remains open.
- Task 6 independent-review remediation supersedes the current safety/count implication above while retaining `43f15385` as exact-source canonical-flow evidence: privileged-helper `SMAppService` and host CLI-shim reads/mutations are now fenced for either fixture, blocked states render localized unavailable truth, and three new regressions bring the arm64 app-target gate to 328 tests. An isolated owner pass on exact remediated source `c80d362c` closes the unavailable/safety-blocked Settings visual, helper suppression, CLI fail-closed, minimum/expanded Light/Dark, VoiceOver, and English/Hungarian/Japanese checks before Task 7 and the participant checkpoint.
- Task 7 implementation projects the canonical brief/session/plan/action/receipt chain through the shipping first-run host behind paired Debug-only gates. The local sequence enforces brief -> reviewed plan -> verified progress -> Action Receipt, copies only aggregate strict-summary fields, presents localized unavailable truth for invalid paired-preview data, and neither persists onboarding completion nor starts live work. Focused arm64 contract and locale-parity evidence plus the bounded owner canonical-flow, summary-redaction, and relaunch checks pass on app source `bfbea8c6`; exact-source owner evidence on `e87d30d3` closes direct Brief, Plan, and Receipt dismissal, Light/Dark, bidirectional Full Keyboard Access, VoiceOver stage focus/order/roles and copy-success announcement, live localization, representative +40% expansion, and selected missing-corpus unavailability. The moderated participant checkpoint and the whole-workflow cross-cutting accessibility pass remain open.
- Task 1's automated evidence at this checkpoint is bounded to provider/projection/policy/request-state unit coverage and a successful full app-target build. Production overview/navigation wiring and SwiftUI keyboard/VoiceOver focus transfer have attributable current-host owner evidence on `3de57a81` rather than automated production-integration coverage.
- Continue Project WOW through personalized plan preview and one supported safe, verified improvement.
- Deliver true direct updating for eligible third-party Sparkle apps through a dedicated, uniquely identified, Developer ID-signed and notarized external-updater boundary (or an equivalently reviewed architecture) that preserves Helm's app sandbox, accepts only structured validated requests, and fails safely to Open App when compatibility cannot be established.

Exit Criteria:

- Moderated users complete the primary health, upgrade, install, failure-recovery, and manager-source tasks without assistance or avoidable navigation reversals.
- Destructive, privileged, guarded, and policy-blocked actions remain explicit and attributable.
- Each release-critical domain covers loading, success, empty, partial, failure, offline, blocked, cancellation, and recovery states.
- Dashboard remains responsive and preserves selection/context while tasks and snapshot updates arrive.
- Presentation changes do not move business or orchestration logic out of service/core boundaries.
- Signed installed-candidate QA proves download, installation, relaunch, version verification, cancellation/failure recovery, and unsupported-app fallback across representative third-party Sparkle apps and framework versions without weakening Helm's sandbox or claiming completion before the target version is observed.

---

## 0.21.x — Accessibility, System Integration & Resilience (rc)

Goal:

- Deliver keyboard-only and VoiceOver parity for every release-critical macOS workflow.
- Validate Full Keyboard Access, logical focus order/restoration, conventional shortcuts, contextual menus, command validation, default actions, and cancel actions.
- Harden increased contrast, reduced motion, reduced transparency, localization, and text expansion behavior.
- Complete notification deep links and background/foreground continuity.
- Validate offline, partial-data, interrupted-task, cancellation, and recovery experiences.
- Complete Project WOW persisted setup session, verified improvement, Action Receipt, redacted summary, bounded resume/retry/rollback, and GUI/CLI/TUI contract parity where first-run interaction is supported.

Exit Criteria:

- No known keyboard trap or pointer-only release-critical action.
- Accessibility Inspector and manual VoiceOver validation pass the approved state/workflow matrix.
- All seven current locales pass representative narrow/expanded window, critical-sheet, and first-run validation.
- Project WOW produces useful partial results when managers fail or the network is unavailable.
- Every Project WOW action described as reversible has a verified rollback path.

---

## 0.22.x — Fit, Finish & Pre-1.0 Hardening (rc)

Goal:

- Complete the full integration matrix, especially multi-manager authority ordering, guarded OS updates, failure isolation, cancellation, and crash recovery.
- Run multi-manager orchestration and cancellation stress tests.
- Complete logging refinement, memory-safety audit, race-condition closure, and FFI stability validation.
- Validate i18n key/placeholder/ICU correctness, localization fallback, and UI text expansion.
- Optimize first useful render, window open, section switching, search response, scrolling, progress updates, and idle resource use against documented experience budgets.
- Complete motion, progress, inactive-window, menu, focus, selection, and transition polish.
- Make the presentation layer theme-ready by replacing hard-coded Wayfinder styling with semantic visual tokens, while retaining Wayfinder as the only required 1.0 theme.
- Run final custom-component exception, visual consistency, state-fixture, screenshot-regression, multi-display, appearance, localization, and failure-injection audits.
- Complete final moderated usability and first-run validation before UI lock.

Exit Criteria:

- All core paths tested; no known race conditions, unhandled panics, or high-severity design-system, accessibility, workflow, or perceived-performance defects.
- Stable FFI boundary and deterministic execution verified.
- Users can identify current state, next action, action consequence, and recovery path on release-critical surfaces.
- Design sign-off covers behavior and all state variants rather than only static ideal-state screenshots.
- Wayfinder renders entirely through the semantic theme contract, falls back safely when theme data is unavailable, and remains unchanged as Helm's canonical default.
- Product screenshots and end-user design documentation are updated only after UI lock.

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
- First-run Environment Brief demonstrates personalized value before manual configuration
- First-run plans, actions, verification, recovery, and receipts preserve Helm safety and transparency guarantees
- Native macOS window, menu, toolbar, Settings, navigation, selection, focus, and command behavior is consistent across release-critical surfaces
- Core workflows meet the pre-1.0 native experience, accessibility, localization, failure-state, and perceived-performance bar

Exit Criteria:

- Project WOW base experience meets the pre-1.0 accessibility, offline, failure, and performance gates in `docs/app-design/PROJECT_WOW.md`.
- Moderated users can identify what Helm discovered, changed, and left unchanged.
- The production app meets the `docs/app-design/NATIVE_MACOS_EXPERIENCE.md` 1.0 release gate.
- Core workflows pass moderated usability, keyboard, VoiceOver, localization, failure, and performance validation.
- Important toolbar actions have appropriate application-menu and keyboard equivalents.
- Custom controls have documented product value and complete macOS input/accessibility behavior.

---

## 1.1.x — Globalization & Theme Expansion

Goal:

- Additional locales:
  - zh-Hans, ko, it, nl
- Localization coverage for all UI surfaces
- Website localization
- Documentation localization (partial)
- Add a durable theme preference in Settings with at least three built-in themes: Wayfinder as the default, Corporate, and a third direction selected after prototype and usability review.
- Keep theme selection independent from system light/dark appearance; every theme must support both appearances plus increased contrast and reduced transparency.
- Limit themes to semantic presentation tokens and theme-owned decorative assets. Themes must not change information architecture, layout behavior, command availability, safety semantics, status meaning, or accessibility structure.
- Persist the selected theme locally and fail safely to Wayfinder if a stored theme identifier is missing or no longer supported.
- Reassess SwiftUI state ownership after 1.0 operational data is available. A reducer/store rewrite is not a pre-1.0 goal; any change must be incremental and preserve the presentation-only boundary.

Exit Criteria:

- Additional locales meet UX and QA standards
- Website supports locale routing
- Translation coverage >80% for supported locales
- Wayfinder, Corporate, and the approved third theme pass the same representative screenshot, localization, contrast, keyboard, VoiceOver, reduced-transparency, and light/dark validation matrix.
- Switching themes updates all active Helm surfaces coherently without restarting, resetting navigation, or changing operational state.

---

## 1.2.x — Editions and Entitlement Foundations

Goal:

- Formalize channel-aware build configurations for planned artifacts (MAS, Developer ID, Setapp, Fleet)
- Implement runtime entitlement model for Helm consumer (Free/Pro) and Helm Business (fleet)
- Add entitlement verification and safe degradation behavior
- Define managed bootstrap configuration contract for fleet MDM deployments
- Keep update authority decoupled from licensing authority by channel
- Formalize how first-run initiative features map onto existing products so base value, safety, transparency, accessibility, offline behavior, and recovery never depend on Pro entitlement
- Define entitlement-safe degradation for Helm Pro first-run enrichments and Helm Fleet managed first-run experiences

Exit Criteria:

- Distribution build matrix and channel authority mapping are documented and approved
- Invalid/expired entitlements degrade predictably without unsafe behavior
- Managed bootstrap configuration is documented and testable
- Capability-gate tests prove base Helm remains a complete first-run experience without Pro or Fleet entitlement

---

## 1.3.x — Security Advisory System (Pro)

Goal:

- Local-first CVE vulnerability awareness for installed packages
- Advisory-only recommendations (no enforcement)
- Optional public advisory API queries (OSV.dev, GitHub Advisory Database, NVD, manager-specific feeds)
- Matching engine: package name + version range -> severity + recommendations
- SQLite-backed advisory cache with TTL-based refresh
- UI: vulnerability status, severity badges, and recommended actions per package
- Helm Pro recommendation enrichment using locally evaluated advisory results
- Offline-capable with cached advisory data
- Non-blocking: advisory checks never delay operations
- No Helm-operated central database required
- No fingerprint sharing in this milestone
- No App Attest in this milestone

Exit Criteria:

- Advisory data ingested and cached locally from at least one source (OSV.dev)
- Matching engine correctly identifies affected packages by name and version range
- Affected packages surfaced in UI with severity and recommended action
- Advisory refresh works offline using cached data
- Advisory evaluation does not block or delay any manager operations
- Pro edition entitlement gate verified (feature unavailable in Free edition)
- Implementation works without any Helm-operated backend dependency

---

## 1.4.x — Shared Brain

Goal:

- Fingerprint sharing for anonymous package/environment signals
- Known-fix lookup and recommendation enrichment
- Optional Helm Pro enrichment for Advanced Conflict Radar, personal insights, and Blueprint recommendations; local base findings remain authoritative and available offline
- Postgres-backed central system-of-record
- Provider-portable HTTP API architecture
- Optional stateless edge/API layer for advisory enrichment queries (Cloudflare Workers is acceptable but replaceable)
- Anonymous per-install authentication via Apple App Attest
- Request signing, nonce handling, and replay protection
- Rate limiting per attested install and abuse-detection controls
- Core data operations handled with standard Postgres patterns (constraints/UPSERT, ranking/selection queries, materialized views, FTS/trigram)
- Large artifacts (if introduced later) stored in S3-compatible object storage; Postgres stores references/metadata

Exit Criteria:

- Shared Brain API contract is documented and versioned
- App Attest-based request validation flow is testable end-to-end
- Security controls (nonce/replay/rate-limit/abuse) are enforced and observable
- Shared Brain enrichments are additive and do not block local advisory behavior
- Cloudflare-specific stores (Durable Objects / D1) are not required for correctness of core Shared Brain data paths

---

## 1.5.x — Business Policy and Drift Management

Goal:

- Scoped policy model (org / department / team / environment / device group)
- Baseline profile enforcement for package and toolchain consistency
- Drift detection and compliance reporting
- Policy snapshot persistence for offline-safe enforcement
- Organization-level locale policies
- Locale enforcement for managed environments
- Multi-locale reporting and audit output
- Helm Fleet Environment/Compliance Brief built from the same local finding contracts as base Helm
- Organization baselines and managed first-run receipts that distinguish organization-controlled from user-controllable state
- External management authority attribution for MDM/software-distribution-owned state

Exit Criteria:

- Policy precedence rules are deterministic and tested
- Drift categories and compliance states are surfaced clearly
- Offline behavior uses last valid policy snapshot without UI/executor instability
- Fleet findings and drift identify the controlling external authority and never weaken MDM or core safety policy

---

## 1.6.x — Enterprise Rollout, Approvals, and Audit

Goal:

- Ring-based rollout workflow (canary, pilot, broad)
- Policy approval workflow and rollback controls
- Audit/event export integrations (SIEM/webhook/ticketing targets)
- Role-based access model for business operators
- Management-tool integration kits for PKG/configuration deployment, inventory/compliance collection, and audit export
- Initial workflow certification targets: Jamf Pro, Microsoft Intune, Kandji, and Munki

Exit Criteria:

- Ring promotion and rollback flow verified end-to-end
- Policy changes and enforcement actions produce attributable audit events
- Enterprise controls preserve Helm safety and orchestration guarantees
- Certified integrations preserve vendor assignment, rollout, and software-distribution authority without competing background mutation

---

## 1.7.x — Mac App Store Distribution Channel

Goal:

- Deliver consumer distribution through Mac App Store channel requirements
- Align consumer licensing behavior with App Store commerce/receipt authority
- Preserve core architecture and runtime gating model without Sparkle coupling

Exit Criteria:

- MAS channel lifecycle is documented and operationally separable from direct and fleet channels
- Channel update authority is App Store-managed
- No Sparkle dependency in MAS channel

---

## 1.8.x — Setapp Distribution Channel

Goal:

- Deliver consumer distribution through Setapp channel requirements
- Align licensing behavior with Setapp subscription authority
- Preserve architecture invariants and channel-isolated update behavior

Exit Criteria:

- Setapp channel lifecycle is documented and operationally separable from direct, MAS, and fleet channels
- Channel update authority is Setapp-managed
- No Sparkle dependency in Setapp channel

---

## 1.9.x — Helm Business Fleet Product

Goal:

- Deliver Helm Business as a separate fleet-focused binary and lifecycle
- Keep one shared core codebase while separating consumer and fleet release operations
- Integrate business policy/compliance capabilities without collapsing architecture boundaries
- Deliver the managed first-run experience under the Helm Fleet capability model
- Preserve coexistence with incumbent MDM, package distribution, self-service, inventory, and compliance systems

Exit Criteria:

- Fleet product boundaries are explicit and documented
- Fleet release lifecycle is independent from consumer release cadence
- Business operational model aligns with enterprise policy and compliance requirements
- Fleet onboarding can be preconfigured by management authority while still providing employees a readable local receipt

---

## 1.10.x — PKG + MDM Deployment and Offline Licensing

Goal:

- Deliver PKG-based enterprise deployment flow for Helm Business
- Deliver MDM-ready managed bootstrap and admin-controlled update workflows
- Deliver offline organizational license-file handling for fleet environments
- Validate vendor-neutral managed preferences/ManagedApp configuration, CLI JSON/exit-code, inventory attribute, compliance discovery, and local audit-export contracts

Exit Criteria:

- PKG + MDM deployment lifecycle is documented and validated as the fleet distribution path
- Offline license file model is documented with fail-safe behavior
- Fleet update flow remains admin-controlled and decoupled from consumer update channels
- Jamf Pro, Intune, Kandji, and Munki deployment/configuration/inventory paths have documented validation matrices
