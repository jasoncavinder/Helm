# Next Steps

This document defines the immediate priorities for Helm development.

It is intentionally tactical.

---

## Current Phase

Helm is in:

```
0.18.x planning kickoff (post-v0.17.6 stable)
```

Focus:
- keep `main`/`dev`/`docs`/`web` publication docs and version markers aligned for `v0.17.6`
- maintain release-process hardening guardrails now that phases 1-5 are complete (preflight, publish verification, drift prevention)
- begin execution planning and branch setup for `0.18.x` local security groundwork after stable publication
- keep launch-at-login scoped to GUI only (no CLI/TUI parity target)

Current checkpoint:
- `v0.17.6` is the current stable release on `main`; post-`v0.17.5` refresh reliability and diagnostics hardening is now included in stable publication:
  - fallback publish PRs merged: `#204` (CLI metadata), `#206` (Sparkle appcast + release notes)
  - latest successful release workflows:
    - `Release CLI Direct Installer` run `22376375473` (`release`, success)
    - `Release macOS DMG` run `22376375456` (`release`, success)
  - post-`0.17.3` `0.17.4` TUI planning slice delivered: detailed ratatui implementation plan documented at `docs/architecture/HELM_TUI_IMPLEMENTATION_PLAN.md` (keyboard model, parity matrix, branding constraints, and ASCII splash-screen contract).
  - post-`0.17.3` `0.17.4` TUI implementation slice delivered: no-arg TTY now launches the ratatui TUI with branded ASCII splash (`logo` + `Helm` + `Take the helm.`), keyboard navigation, command palette/help/confirm overlays, read-only parity panes (updates/packages/tasks/managers/settings/diagnostics), and direct mutation hooks for common manager/package/task actions.
  - post-`0.17.3` `0.17.4` TUI parity-expansion slice delivered: managers pane now supports selected-manager detect/executable/method/priority controls via keyboard, updates pane now supports include-pinned + allow-OS-updates toggles for upgrade workflows, diagnostics pane supports one-key export snapshot writes, task-log detail follows selection movement immediately, and settings pane now exposes integrated self-update status/check/apply controls honoring provenance/channel policy semantics.
  - post-`0.17.3` `0.17.4` GUI↔CLI/TUI parity-closure slice delivered: CLI search now supports progressive local+remote orchestration with manager scoping; CLI updates preview/run now support manager-scoped bulk execution; CLI now exposes per-package Homebrew keg-policy commands; and TUI packages now include progressive available-package rows with install action + Homebrew keg-policy controls.
  - post-`0.17.3` `0.17.4` kickoff slice delivered: app now bundles `helm-cli` and Settings includes install/remove controls for a managed `~/.local/bin/helm` shim with app-bundle provenance marker writes.
  - post-`v0.17.6` settings CLI-shim follow-up delivered on `dev`:
    - `Helm.entitlements` and `HelmRelease.entitlements` now include home-relative read/write exceptions for `~/.local/bin/` and `~/.config/helm/` so sandboxed app builds can install/remove the managed CLI shim and marker at the real user-home paths.
  - `#93` `feat/v0.17-log-foundation`
  - `#95` `feat/v0.17-structured-error-export`
  - `#96` `feat/v0.17-service-health-panel`
  - `#97` `feat/v0.17-task-log-viewer`
  - `#98` `feat/v0.17-manager-detection-diagnostics`
  - `#99` `feat/v0.17-diagnostics-hardening`
  - updater/install hardening for Sparkle sandboxed flows and prerelease appcast short-version labeling
  - post-rc.2 follow-up delivered for `rc.3`:
    - preserve prerelease bundle short version in Sparkle "up to date" messaging
    - add running-task inline expand/collapse command/live-output panel
    - consolidate same-name packages across managers in package/search UI surfaces while preserving manager-scoped actions
    - render HTML package descriptions in inspector with safe-link policy and readable fallback
    - keep inspector detail text containers full-width with leading alignment in the side panel
    - harden package-consolidation row selection policy and task-output buffer caps
    - add prerelease updater bundle-metadata sanity checks and Hungarian translation follow-through for new UI strings
  - post-rc.3 release automation follow-up delivered on `dev`:
    - generate and publish website-hosted Sparkle release-notes pages from `CHANGELOG.md` under `web/public/updates/release-notes/<tag>.html`
    - point appcast `sparkle:releaseNotesLink` to hosted website release notes instead of GitHub release pages
  - post-rc.3 onboarding/legal follow-up delivered on `dev`:
    - require first-run license-terms acceptance for `developer_id` channel before onboarding can proceed
    - persist accepted license version + timestamp and re-prompt automatically when tracked license version changes
    - expose `View License Terms` in About overlay for post-onboarding re-review
  - post-rc.3 control-center/popover workflow follow-up delivered on `dev`:
    - suppress popover while Control Center is open and focus Control Center on status-item clicks during that state
    - make popover health + metric cards and overview metric cards route directly to their Control Center sections
    - extend top-bar drag surface to match the full visible top bar
  - post-rc.3 manager-priority/inspector follow-up delivered on `dev`:
    - replace alphabetical manager ordering with priority ordering (installed first), add intra-authority drag reordering, and expose restore-default-priority action in advanced settings
    - expand manager inspector to show full executable-path discovery set with active-path emphasis and install-method metadata (recommended/preferred tagging)
    - expand manager install-method catalog coverage across implemented managers and improve About overlay diagnostics metadata (build/channel/update authority/last-check)
  - post-rc.3 control-center polish follow-up delivered on `dev`:
    - reset-local-data now clears onboarding license-acceptance state in addition to cached runtime data
    - running-task rows now toggle expand/collapse from whole-row taps (not only indicator affordances)
    - Control Center drag-to-move now applies across the full window background (interactive controls still take precedence)
    - settings top metric cards now deep-link to Managers/Updates/Tasks
    - inspector selection now clears when sections change and selected rows/cards are visually highlighted
    - launch-at-login setting added for supported systems (macOS 13+), with localized unsupported messaging on older systems
    - manager/popover count rendering paths precompute per-manager counts to reduce repeated filtering work in hot UI update loops
  - pre-rc.4 stabilization follow-up delivered on `dev`:
    - popover outside-click close handling now only reacts to click events (not hover/drag movement)
    - floating-panel cursor forcing removed so interactive controls retain expected hover affordances
    - consolidated package default-manager ordering now respects authority-aware manager priority
    - executable-path discovery now skips undetected managers and caches detected-manager discovery results
    - targeted regression coverage added for priority-ranked consolidation behavior and manager-status executable-path behavior
  - post-rc.4 issue-remediation follow-up delivered on `dev`:
    - softwareupdate manager icon mapping corrected to valid SF Symbol naming (`applelogo`)
    - manager-priority drag interactions now take precedence over full-window drag-to-move in the Managers section
    - inflight task dedupe now prefers running/newest IDs so command/stdout panes stay populated when backend output exists
    - Packages now includes localized `Pinned` filtering, excludes pinned packages from `Upgradable`, and uses horizontal filter-chip scrolling to preserve localization fit
  - post-rc.4 UX/task-diagnostics follow-up delivered on `dev`:
    - popover package search rows now expose icon quick actions (install/uninstall/update/pin) without opening Control Center
    - package inspector actions now use icon + tooltip controls to preserve usability in narrow inspector widths
    - manager inspector executable-path lists become scrollable when long, and managers in error state now expose `View Diagnostics`
    - failed tasks now support inline expand/collapse command+output panes, with single-selected expansion behavior
    - task-pruning retention windows now start at completion/failure timestamp rather than original queue/start time
  - post-rc.4 privileged-auth follow-up delivered on `dev`:
    - adapter operations marked `requires_elevation` now execute via structured `sudo -A -- <program> <args...>` wrapping in the core process executor
    - executor auto-provisions a local askpass helper script (or honors `HELM_SUDO_ASKPASS` override) so privileged tasks trigger administrator authentication prompts in-app flow
    - task command/output capture remains active for elevated runs, preserving diagnostics transparency for auth-denied/privileged-failure cases
  - post-rc.4 responsiveness follow-up delivered on `dev`:
    - overview/managers/popover/settings now read section-scoped derived state (manager health/count maps, top task slices) instead of repeatedly recomputing per-render dictionaries
    - polling cadence now adapts to interactive surface visibility (popover/control-center visible vs background), with lifecycle visibility hooks in `AppDelegate`
    - inspector package-description rendering now goes through a bounded core-level LRU render cache
    - scroll-heavy managers/overview/updates/settings/popover-search sections now use lazy stack containers where applicable
  - post-`0.17.x` manager-selection execution follow-up delivered on `dev`:
    - manager inspector executable-path and install-method menus are now actionable and persist explicit selections
    - executable selection now supports an explicit PATH-default mode and recommended/default tagging for discovered paths
    - selected executable/install-method preferences are persisted in SQLite (`manager_preferences` migration v7) and exposed through XPC/FFI manager-status payloads
    - core process execution now routes commands through selected manager executables; manager install/update/uninstall flows honor selected install method where implemented (`mise`, `mas`, `rustup`)
  - post-`0.17.x` manager-enablement enforcement follow-up delivered on `dev`:
    - disabled managers are now excluded from installed/outdated/search/task snapshot surfaces and package/dropdown filters
    - runtime task submission now rejects disabled managers centrally; disabling a manager now cancels in-flight tasks for that manager
    - package/update manager-scope selections now normalize away disabled manager IDs to prevent stale disabled-manager targeting
  - post-`0.17.4` manager eligibility-policy hardening delivered on `dev`:
    - macOS base-system RubyGems/Bundler/pip executables are now explicitly supported as `detected-but-not-manageable`
    - enabling affected managers while mapped to system executables is blocked with structured, localized remediation guidance
    - runtime hard-stops task submission when eligibility is false, and startup/status sync self-heals stale enabled state by auto-disabling
    - shared policy matrix + lessons learned are now documented at `docs/architecture/MANAGER_ELIGIBILITY_POLICY.md`
  - post-`0.17.x` detection/onboarding follow-up delivered on `dev`:
    - onboarding detection now calls a detection-only trigger instead of full refresh, avoiding immediate list-installed/list-outdated work during first-run detection
    - detection trigger pre-seeds manager presence from executable-path discovery so detected managers render immediately while version probing continues
    - onboarding detected-manager rows now show localized `Loading` for version text until per-manager detection tasks reach terminal status
    - onboarding license acceptance is now step 2 (after welcome) and no longer re-enters the onboarding sequence after license acceptance
    - core executable lookup now falls back to direct filesystem probing across known bin locations when `which` lookup fails
    - core runtime now logs per-manager detection timing with structured fields and emits a slow-detection warning threshold at 3000ms
  - post-`0.17.4` onboarding/UX follow-up delivered on `dev`:
    - Control Center manager cards now include a visible drag affordance symbol
    - Control Center Settings card order now follows `General -> Managers -> CLI -> Service Health -> Support & Feedback -> Advanced`
    - `Reset Local Data` now closes Control Center after successful reset so onboarding is re-entered on next interaction
    - CLI now enforces first-run onboarding before normal command execution (except help/version/completion/onboarding), with terminal/menu onboarding flow, `--accept-license`, `--accept-defaults`, machine-mode JSON error semantics when onboarding is required, and explicit `helm onboarding status|run|reset`
  - post-`0.17.4` task/about UX follow-up delivered on `dev`:
    - failed tasks are no longer age-pruned and now persist until replacement, manual dismissal, manager disable cleanup, or local reset
    - task rows now expose explicit failed-task dismissal actions, backed by persisted task-log/task-record deletion
    - About overlay is simplified (OK-only dismissal, metadata removal) and now shows copyright plus Helm-update-detected messaging
    - manager inspector primary actions now use Helm-styled buttons instead of default system button styling
  - post-`0.17.x` upgrade-plan modal follow-up delivered on `dev`:
    - execution-plan sheet state now records the initiating host surface so only that UI (popover or Control Center) presents the modal
    - `Upgrade All` from Control Center/menu now targets Control Center-hosted modal presentation without surfacing the popover
    - execution-plan footer removed the deprecated `Dry Run` action and keeps only cancel/run controls
  - pre-stable `rc.5 -> 0.17.0` hardening follow-up delivered on `dev`:
    - manager display-name localization now resolves through one shared helper across Core + UI surfaces
    - localization file/missing-key diagnostics now emit structured logger events instead of direct `print` output
    - polling cadence now slows further during idle/no-inflight states even while interactive surfaces are visible
    - SQLite connection defaults now enforce `WAL`, `NORMAL` sync, `busy_timeout`, and foreign-key pragmas
    - terminal-task pruning now includes `cancelled` status with terminal-time retention behavior
    - Rust build script now fingerprints Rust/script inputs and skips rebuild work when generated artifacts are unchanged
    - release checklist now includes a dedicated `v0.17.0` stable gate and archival guidance for historical checklist sections
  - website release-readiness follow-up delivered on `dev`:
    - Starlight now runs a local blog plugin that inserts `/blog/` navigation and an RSS social link
    - website now renders a global beta-tester announcement banner with a refined brand-consistent visual treatment
    - blog pages now include social-share actions (X, LinkedIn, Reddit, Email)
    - landing navigation now includes right-aligned `Blog` and `Docs` links for faster access
  - post-`v0.17.1` release-automation guardrails delivered on `dev`:
    - release workflow now treats fallback appcast-publish PR-creation failures as blocking errors (no soft-success path)
    - release workflow now verifies `web/public/updates/appcast.xml` on `main` matches the release tag before marking release success
    - new scheduled/manual `Appcast Drift Guard` workflow now fails when latest stable GitHub release and top appcast version diverge
  - post-`v0.17.5` release-process hardening phase 1 delivered on `dev`:
    - added `scripts/release/preflight.sh` for required git/auth/scope/workflow/secret validation before tagging
    - added `scripts/release/runbook.sh` with `prepare|tag|publish|verify` wrappers
    - release docs/checklist now mandate preflight before release tag creation
  - post-`v0.17.5` release-process hardening phase 2 delivered on `dev`:
    - release workflows now keep hard failures for build/signing/notarization/upload/PR-creation faults while allowing non-red follow-up-required completion when fallback publish PRs are open but not yet merged
    - release workflows now emit publication summary fields: `Artifacts uploaded`, `Publish PR opened`, and `Main metadata synced`
    - release docs now document follow-up merge + rerun expectations when summary indicates metadata sync is pending
  - post-`v0.17.5` release-process hardening phase 3 delivered on `dev`:
    - `main` ruleset bypass policy now uses `pull_request`-only mode (broad `always` bypass removed)
    - preflight now enforces least-privilege bypass policy with required `Policy Gate` check presence and `no always` guardrails
    - docs now capture preferred GitHub Actions integration bypass and repository-role fallback when GitHub rejects integration actors for repository-owned rulesets
  - post-`v0.17.5` release-process hardening phase 4 delivered on `dev`:
    - preflight now enforces pre-tag stable metadata snapshot sanity (`origin/main` appcast + `cli/latest.json` sync and target-order checks for stable tags)
    - new `Release Publish Verify` workflow now runs on publish-metadata pushes to `main` and validates release-object alignment after publish PR merges
    - release checklist/versioning docs now include explicit publish-verifier + drift-guard checkpoint requirements
  - post-`v0.17.5` release-process hardening phase 5 delivered on `dev`:
    - release scripts/workflows now normalize locale environment defaults for operator/CI consistency
    - release logs now use phase prefixes (`[preflight]`, `[build]`, `[publish]`, `[verify]`) for faster triage
    - recurring release friction now has a documented promotion path from `TMP_RELEASE_FRICTION` into permanent decision/runbook/checklist docs
  - pre-1.0 remediation batch delivered on `dev` (`SEC-004`, `BUILD-003`, `BUILD-002`):
    - `scripts/release/build_unsigned_variant.sh` now rejects invalid tag formats and enforces canonical output-root containment for generated zip/pkg paths
    - `Release Contract Checks` CI now runs non-destructive `preflight` + `runbook prepare` contract checks on PRs and validates unsigned-build script safety regressions
    - `Dependency Security` CI now runs Dependency Review on PRs and scheduled/PR `cargo audit` checks for Rust dependencies
  - pre-1.0 remediation batch delivered on `dev` (`COR-002`, `TEST-003`, `DOC-001`):
    - manager executable/timeout preference sync now uses atomic map replacement instead of clear-then-repopulate loops to avoid empty override windows during concurrent reads
    - CLI tests now assert `updates run` mixed-success exit-code behavior and stable machine-output envelope structure
    - common CLI errors now include actionable next-step hints for `helm help`, `helm managers list`, and `helm updates preview`
  - pre-1.0 remediation batch delivered on `dev` (`COR-003`, `REL-006`, `DOC-004`):
    - install/uninstall mutation success now updates cached installed/outdated snapshots without requiring manual full refresh
    - 1.0 crash/error reporting posture is now explicitly local-only with documented policy, payload schema, privacy constraints, and operational owner (`docs/operations/CRASH_REPORTING_POLICY.md`)
    - architecture + PR checklist terminology contract enforcement remains explicit (`manager`/`task`/`service` user-facing, `adapter` internal)
  - post-`v0.17.5` refresh reliability + diagnostics hardening delivered on `dev`:
    - task output persistence now records effective cwd/timing/exit metadata and structured error details used by diagnostics commands
    - diagnostics summary now reports failure-class counters for faster operator triage
    - coordinator health inspection now reports stale-state reasons and request timeout handling now performs one stale-state recovery retry
    - refresh/search request-response now retries once for transient timeout/network-resolution failures, and npm list timeout is now 120s
  - post-`v0.17.6` manager execution/timeout hardening delivered on `dev`:
    - selected executable overrides now prepend executable parent directories to process `PATH`, and npm/pnpm/yarn script selections now add node-runtime `bin` hints so constrained-shell refresh flows resolve `#!/usr/bin/env node` correctly
    - process execution now uses activity-aware timeout semantics (hard timeout + output-idle timeout) so long-running commands that keep emitting output avoid false timeout failures
    - timeout defaults are global by task type (not manager-specific hardcoding) and manager-specific tuning is now configurable via inspector hard/idle timeout controls with SQLite-backed persistence (`manager_preferences` migration v8)
    - diagnostics surfaces now include process context (`program_path`, `PATH` snippet) and explicit timeout error codes (`hard_timeout`, `idle_timeout`) for faster root-cause classification
  - GitHub governance hardening delivered on `dev`:
    - branch rulesets now explicitly enforce `main`/`dev`/`docs`/`web` with branch-specific required checks
    - `Policy Gate` now validates PR base/head/scope policy for all protected branches
    - `Docs Checks` and `Web Build` workflows now gate `docs` and `web` branches respectively
    - blocking ruleset `update` enforcement was removed after protected-ref merge-block diagnostics so normal PR merges can complete
    - CodeQL now runs on `main` push + schedule/manual (non-PR gate) to reduce merge friction while retaining scanning coverage
  - CLI kickoff delivered on `dev`:
    - draft CLI spec published at `docs/architecture/HELM_CLI_SPEC.ms` with command surface, output contract, and shared-coordinator target architecture
    - new Rust CLI crate scaffolded at `core/rust/crates/helm-cli` (binary: `helm`) with read-only commands (`status`, `ls`/`packages`, `updates`, `tasks`, `managers`, `settings`) and `--json` output
    - runtime-backed command slice added for `refresh` and `managers detect` using Helm core orchestration with process-adapter bootstrap + manager executable override sync
    - `--wait` / `--detach` global flags are now parsed; shared CLI coordinator routing now supports true detach for coordinator-backed single-task mutations (`packages install|uninstall|upgrade`, `managers detect <id>`, `managers install|update|uninstall`)
    - read-only contract pass delivered: identifier contracts and coordinator/read-only behavior clarified in spec; `tasks show` read command added; documented JSON contract surface now emits stable envelopes with `schema_version` + `generated_at` for `status`, `managers list`, `packages list`, `updates summary`, and `tasks show`
    - manager-selection command slice delivered: `managers executables list|set` and `managers install-methods list|set` now persist per-manager selection preferences
    - package mutation slice delivered: `packages install|uninstall|upgrade|pin|unpin` now supports explicit manager targeting (`--manager` / `name@manager`) with ambiguity-safe `packages show`
    - manager lifecycle mutation slice delivered: `managers install|update|uninstall` now routes through method-aware targets for supported managers (`mise`, `mas`, `rustup`, `homebrew_formula`)
    - updates orchestration slice delivered: `updates preview` and `updates run --yes` now execute cached-snapshot upgrade planning/runs with support for `--include-pinned` and `--allow-os-updates`
    - task inspection slice delivered: `tasks logs`, `tasks output`, and `tasks follow` are now available in CLI (`follow` currently guarded by timeout to avoid indefinite non-terminal polling)
    - task cancellation surface (`tasks cancel`) now routes through the shared CLI coordinator with process-level cancellation for coordinator-owned tasks
    - settings mutation surface expanded with `settings reset <key>` for implemented keys (`safe_mode`, `homebrew_keg_auto_cleanup`)
    - command-scoped help delivered across top-level and nested namespaces (for example `helm packages install help`, `helm managers executables help`, `helm help managers executables set`)
    - read-only polish delivered: `helm updates` default dispatch now no longer panics without an explicit subcommand; JSON envelope consistency now applies to `search`, `managers show`, `settings list`, and `settings get`; `settings list --json` now includes `auto_check_for_updates`
    - read-only list ergonomics delivered: `--limit` now applies to `packages list`/`ls`, `updates list` (including `helm updates --limit ...`), and `tasks list` (including `helm tasks --limit ...`)
    - global diagnostics verbosity delivered: CLI now supports `-v` / `--verbose` and emits runtime/coordinator diagnostic traces to `stderr` for investigation workflows while preserving `stdout` output contracts
    - settings persistence expanded: `auto_check_for_updates` and `auto_check_frequency_minutes` now support `settings get|set|reset` and are reflected by `settings list` + `self status`
    - `self` namespace baseline delivered for Homebrew-formula installs: `self status|check|update` now provide method-aware status, live snapshot check, and task-backed update execution (wait/detach) with explicit guidance for unsupported install paths
    - `self auto-check` command slice delivered: `self auto-check status|enable|disable|frequency <minutes>` now maps directly to persisted auto-check settings (`auto_check_for_updates`, `auto_check_frequency_minutes`) with nested help/completion coverage
    - manager enablement parity hardening delivered: `managers disable` now performs best-effort cancellation of queued/running tasks for that manager through the CLI coordinator and reports cancellation diagnostics in JSON/human output
    - mutation JSON envelope hardening delivered: manager enable/disable, settings set/reset, manager detection wait output, and shared manager-result payloads now consistently emit the standard envelope (`schema`, `schema_version`, `generated_at`, `data`)
    - diagnostics/ergonomics slice delivered: `diagnostics summary|task|manager|export` now exposes structured CLI diagnostics inspection/export, and `completion bash|zsh|fish` now emits shell completion scripts
    - manager-priority slice delivered: `managers priority list|set|reset` now persists override ordering in SQLite and is applied to manager/update ordering
    - detach coverage expanded: multi-step workflows (`refresh`, `updates run`, `managers detect --all`) now support coordinator-backed detach mode with workflow job IDs; `packages pin|unpin --detach` is accepted
    - parity hardening delivered: GUI+CLI now share coordinator transport authority (FFI bridge + local coordinator host with external-coordinator routing for mutation/cancel flows), self-update policy is now provenance-aware beyond Homebrew-only installs (`direct-script` direct updates + channel-managed guidance), and coordinator hosts now run scheduled due-based auto-check ticks with persisted `auto_check_last_checked_unix`
    - CLI contract hardening delivered: granular task-oriented exit-code mapping (`2` task failure, `3` partial failure, `4` cancellation) and global-flag support for `--json|--ndjson`, `-q|--quiet`, `--no-color`, `--locale <id>`, and `--timeout <seconds>`
    - audit-remediation slice delivered: direct self-update transport failures now emit structured JSON error payloads with actionable guidance in `--json` mode; install provenance marker schema is now centralized at `docs/contracts/install-marker.schema.json` with Rust + installer CI validation; residual CLI recon/dead-code artifacts were removed
    - audit-remediation follow-up delivered: `helm doctor` top-level alias now routes to diagnostics (defaulting to provenance output), self-update force mode is now restricted to `direct-script` installs only, coordinator auto-check ticks now require direct-script marker policy before endpoint fetches, and direct install/update network paths now enforce allowlisted HTTPS hosts with explicit timeout policy (with opt-in `file://` testing override)
    - audit-remediation follow-up delivered: top-level machine-mode parity now covers help/version/completion/error flows for `--json`/`--ndjson`, NDJSON list payloads now emit one envelope per item (with explicit empty-list envelope behavior), string-based exit-code heuristics are removed in favor of explicit marker-based classification with deterministic runtime fallback (`1`) for untyped errors, CLI release metadata publication now separates stable (`latest.json`) vs prerelease (`latest-rc.json`) pointers; policy-gate now locks CLI metadata mutation to publish/emergency lanes; and scheduled/manual CLI metadata drift guard validation is now added
    - audit-remediation follow-up delivered: Rust-side install-marker writes now use symlink-safe atomic replacement; direct self-update binary replacement now rejects symlink/non-file target paths and enforces bounded payload size (`HELM_CLI_SELF_UPDATE_MAX_DOWNLOAD_BYTES`, default 64 MiB); and release workflows now extend immutable action pinning + per-job token scopes with CLI tag/version verification before publication
    - audit-remediation follow-up delivered: stable CLI update metadata now points to published `v0.17.2` CLI release assets with real checksums (no placeholder zeros), and auto-check last-checked timestamps now update only after eligible direct self-managed check attempts instead of policy-gated skips
    - audit-remediation follow-up delivered: distribution profile contract is now centralized in `docs/contracts/distribution-profiles.json` and consumed by shared build orchestration (`scripts/build.sh`, `scripts/release/build_unsigned_variant.sh`, matrix-based `release-all-variants.yml` auxiliary jobs); Swift update-authority mapping now has one source (`AppUpdateConfiguration`), targeted updater policy tests pass on macOS, and GUI checksum-publication symmetry is explicitly documented as deferred while Sparkle remains canonical GUI integrity authority
    - trust-chain future work is now explicitly tracked: detached signatures + signing-key rotation for CLI update artifacts (`docs/roadmap/CLI_DISTRIBUTION_CI_MILESTONES.md`, milestone M5)
- latest stable release on `main`: `v0.17.6`
- validation gates are green through the stable cut (`cargo test`, macOS `xcodebuild` tests, locale integrity/length audits, release workflow smoke across `v0.17.0-rc.1` through `v0.17.0-rc.5`)
- `v0.15.0` released on `main` (tag `v0.15.0`)
- `v0.14.0` released (merged to `main`, tagged, manager rollout + docs/version alignment complete)
- `v0.14.1` released (merged to `main` via `#65`, tagged `v0.14.1`)
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
- `v0.18.x` — Local security groundwork (internal-only)
- `v0.19.x` — Stability & Pre-1.0 hardening

## v0.17.x Delivery Tracker (Stable `0.17.3` Complete)

- [x] `feat/v0.17-log-foundation` — task log event model, SQLite persistence migration, FFI/XPC retrieval surface.
- [x] `feat/v0.17-task-log-viewer` — per-task log viewer UI with filters and pagination.
- [x] `feat/v0.17-structured-error-export` — structured support/error export payloads with redaction.
- [x] `feat/v0.17-service-health-panel` — service/runtime health diagnostics panel.
- [x] `feat/v0.17-manager-detection-diagnostics` — per-manager detection diagnostics and reason visibility.
- [x] `feat/v0.17-diagnostics-hardening` — silent-failure sweep, attribution consistency, integration/doc exit checks.
- [x] `v0.17.0-rc.1` localization follow-through — manager display-name key coverage expanded across all implemented manager IDs with brand-preserving labels; Hungarian (`hu`) locale added with onboarding + service/error translation bootstrap and CI parity checks.
- [x] `v0.17.0-rc.2` updater/install hardening — Sparkle sandbox installer entitlements + installer launcher service metadata added; prerelease appcast short-version labeling now preserves RC identifiers.
- [x] post-`rc.2` updater version-label alignment — non-App-Store prerelease builds now preserve prerelease marketing version so Sparkle "up to date" messaging reflects full RC versions.
- [x] post-`rc.2` running-task execution transparency — running tasks now expose inline expand/collapse details showing command and live-updating output.
- [x] post-`rc.2` cross-manager package presentation consolidation — package list and popover search now collapse same-name entries into one package row and display all contributing managers beneath the package name.
- [x] post-`rc.2` inspector rich-description hardening — inspector now renders HTML package descriptions as attributed text, with safe-link filtering and readable fallback behavior.
- [x] post-`rc.2` inspector layout hardening — inspector detail containers now stay full-width with leading alignment to avoid centered narrow text content.
- [x] post-`rc.2` updater prerelease guardrails — updater eligibility now rejects bundle marketing/build metadata mismatches that would blur prerelease vs stable version semantics.
- [x] post-`rc.2` diagnostics/runtime hardening — task-output store now enforces bounded command/output buffering for long-running tasks, and Hungarian locale coverage includes the new task/inspector strings.
- [x] post-`rc.3` updater release-notes hosting — release workflow now generates a per-tag website release-notes HTML page from `CHANGELOG.md`, publishes it with appcast updates, and links Sparkle release-notes URLs to the hosted page.
- [x] post-`rc.3` onboarding/legal acceptance — Developer ID onboarding now requires explicit license-terms acceptance tracked by version + timestamp, with re-prompting on license-version changes and a persistent About link to review terms.
- [x] post-`rc.3` popover/control-center interaction hardening — status-item popover no longer coexists with Control Center; status-item clicks focus Control Center while open; popover/overview health and metrics now deep-link to the appropriate Control Center section.
- [x] post-`rc.3` manager-priority workflow — manager cards are priority-ordered by authority with installed-first enforcement, drag-reorder support, and advanced-settings restore-default-priority action.
- [x] post-`rc.3` manager inspector install-metadata expansion — inspector now shows all discovered executable paths (active path emphasized), install-method metadata with recommended/preferred tags, and expanded per-manager install-method catalogs.
- [x] post-`rc.3` About diagnostics metadata enhancement — About overlay now surfaces build number, distribution channel, update authority, and last update-check timestamp.
- [x] post-`rc.3` control-center workflow polish — reset-local-data clears license-acceptance state; running-task row taps toggle details; settings metrics deep-link to managers/updates/tasks; inspector selection clears on section changes and selected entities are highlighted.
- [x] post-`rc.3` startup/interaction polish — launch-at-login setting added (macOS 13+), popover cursor handling restored for hover affordance clarity, full-window Control Center drag support enabled, and count-heavy UI lists now use precomputed manager count maps for smoother drag/scroll behavior on lower-spec Macs.
- [x] pre-`rc.4` stabilization — popover outside-click behavior hardened to click-only event handling; floating-panel cursor forcing removed; consolidated package manager preference now authority-aware; executable-path discovery cost reduced via undetected-manager skip + discovery caching; targeted policy/manager-status regression tests added.
- [x] post-`rc.4` issue-remediation — softwareupdate symbol mapping corrected; manager drag-vs-window-drag precedence fixed; inflight-task dedupe now prefers running/newer rows for live command/output panes; Packages gained localized `Pinned` filtering with upgradable exclusion and overflow-safe horizontal chip layout.
- [x] post-`rc.4` UX/task-diagnostics hardening — popover search package rows gained quick icon actions (install/uninstall/update/pin), package inspector actions moved to icon+tooltip buttons, manager inspector executable paths now scroll when long and include error-state `View Diagnostics`, failed tasks now support inline details with single-selection expansion, and task retention timing now starts from terminal transition time.
- [x] post-`rc.4` privileged-auth support — core execution now wraps `requires_elevation` requests with structured `sudo -A` plus askpass helper provisioning, enabling first-class administrator authentication prompts for privileged install/update operations while retaining command/output diagnostics visibility.
- [x] post-`rc.4` responsiveness hardening — section-scoped derived state now backs overview/managers/popover/settings metrics; polling cadence adapts to visible interactive surfaces; package-description rendering now uses a bounded core LRU cache; lazy stacks now back scroll-heavy managers/overview/updates/settings/popover-search surfaces.
- [x] `v0.17.0-rc.5` release-prep consolidation — post-`rc.4` issue-remediation, UX/task diagnostics hardening, privileged-auth execution support, and responsiveness improvements are bundled with changelog/docs alignment for the next RC cut.

RC-3 release gate for `v0.17.x`:
- Logs are accessible in UI.
- No silent failures in task execution/reporting paths.
- Support data export works and is operator-usable.
- Sparkle updater can launch installer successfully for eligible direct-channel installs.
- Appcast `sparkle:shortVersionString` preserves prerelease labels for RC builds.
- Sparkle updater eligibility rejects prerelease/stable bundle-version metadata mismatches.
- Task execution transparency surfaces command + live output while keeping diagnostics storage bounded.
License/compliance follow-through:
- Keep `docs/legal/THIRD_PARTY_LICENSES.md` updated as dependency sets change.
- Treat third-party notice validation as a required release gate (`docs/RELEASE_CHECKLIST.md`).
- Add release-automation support for producing a distribution-ready third-party notices artifact in a future docs/automation slice.

---

## v0.16.x Kickoff Plan (Completed)

### Alpha.1 — Channel-Aware Updater Scaffolding (Completed on `feat/v0.16.0-kickoff`)

Delivered:

- Added runtime channel configuration model for app-update behavior:
  - `HelmDistributionChannel` (`developer_id`, `app_store`, `setapp`, `fleet`)
  - `HelmSparkleEnabled` gating to prevent accidental Sparkle activation in non-direct channels
- Added `AppUpdateCoordinator` with strict channel isolation and manual update-check entry point plumbing
- Added optional Sparkle bridge (`#if canImport(Sparkle)`) while preserving non-Sparkle build compatibility
- Wired Sparkle SPM package linkage into the Helm app target for direct-channel runtime update checks
- Pinned Sparkle SPM dependency to exact `2.8.1` to keep compatibility aligned with macOS 11+ targets.
- Added user entry points:
  - status menu `Check for Updates`
  - popover About overlay `Check for Updates`
- Added localized `app.overlay.about.check_updates` in both locale trees (`locales/` + app resource mirror)
- Added default app metadata keys in `Info.plist`:
  - `HelmDistributionChannel=developer_id`
  - `HelmSparkleEnabled=false`
- Added channel-profile build configs and generation flow:
  - profile templates under `apps/macos-ui/Config/channels/`
  - build output `apps/macos-ui/Generated/HelmChannel.xcconfig`
  - base config now includes generated channel config when present
- Added shared channel xcconfig renderer (`apps/macos-ui/scripts/render_channel_xcconfig.sh`) and refactored build generation to use that single path.
- Helm target now injects channel/feed/signature plist keys from build settings:
  - `HelmDistributionChannel`
  - `HelmSparkleEnabled`
  - `SUAllowsDowngrades`
  - `SUFeedURL`
  - `SUPublicEDKey`
- Helm app Info.plist now includes explicit placeholders for those updater metadata keys so packaged-artifact verification can read deterministic values.
- Release DMG workflow now passes direct-channel Sparkle build metadata and validates required Sparkle secrets before signed release builds.
- Release DMG workflow now verifies packaged channel/Sparkle invariants and Sparkle framework linkage in the signed app bundle.
- Added regression coverage for app update channel config parsing + Sparkle gating behavior (`AppUpdateConfigurationTests`).
- Added fail-fast build-script policy checks for invalid channel/Sparkle combinations.
- Added CI channel-policy matrix validation (`apps/macos-ui/scripts/check_channel_policy.sh`) ahead of Xcode build/test.
- Hardened Sparkle feed policy so Developer ID + Sparkle now requires `https://` at both build-render and runtime configuration gates.
- Added explicit downgrade hardening: `SUAllowsDowngrades` defaults to disabled, release artifacts are verified as non-downgradeable, and runtime Sparkle gating rejects downgrade-enabled metadata.
- Added install-location hardening for self-update: runtime Sparkle gating now rejects mounted-DMG (`/Volumes/...`) and App Translocation execution paths.
- Added package-manager install hardening for self-update: runtime Sparkle gating now rejects package-manager-managed installs via Homebrew Cask receipt detection plus Homebrew/MacPorts path heuristics.
- Added `com.apple.security.network.client` to Helm app sandbox entitlements so Sparkle feed requests are allowed in direct-channel builds.
- Sparkle runtime now clears persisted feed URL overrides from user defaults at startup and logs the resolved feed URL for manual check attempts.
- Added localized operator feedback for blocked update checks in About/menu surfaces so policy-based unavailability is explicit instead of silently hidden.

Validation:

- `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`
- `swiftlint lint --no-cache apps/macos-ui/Helm/Core/HelmCore.swift apps/macos-ui/Helm/AppDelegate.swift apps/macos-ui/Helm/Views/PopoverOverlayViews.swift apps/macos-ui/Helm/Core/L10n+App.swift`

### Alpha.2 — Installer Packaging Hardening (Completed on `feat/v0.16.0-kickoff`)

Delivered:

- Added packaged-DMG verification script (`apps/macos-ui/scripts/verify_release_dmg.sh`) to enforce:
  - app payload presence in mounted DMG
  - `/Applications` symlink correctness
  - expected DMG background asset
  - updater metadata invariants (`HelmDistributionChannel`, `HelmSparkleEnabled`, `SUAllowsDowngrades`, `SUFeedURL`, `SUPublicEDKey`)
  - Sparkle framework linkage and app codesign verification from packaged artifact
- Wired packaged-DMG verification into release workflow before notarization (`.github/workflows/release-macos-dmg.yml`).
- Added Sparkle appcast generation script (`apps/macos-ui/scripts/generate_sparkle_appcast.sh`) for finalized/stapled DMGs.
- Release workflow now generates and uploads `appcast.xml` from the final DMG artifact.
- Added website feed scaffold at `web/public/updates/appcast.xml` for direct-channel Sparkle hosting.
- Appcast generation now uses Sparkle's packaged `sign_update` binary from SPM artifacts instead of invoking `swift run` against Sparkle sources.
- Release workflow now publishes generated `appcast.xml` into `web/public/updates/appcast.xml` on `main` (with automatic PR fallback when direct push is blocked by branch protections).
- Release workflow now enforces Sparkle appcast policy checks (`apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh`) to keep `0.16.x` on full-installer-only updates (no deltas).
- Release workflow now fails fast if `HELM_SPARKLE_FEED_URL` hostname does not resolve in CI DNS.
- Release workflow now pre-renders channel overrides and passes explicit Sparkle/channel build settings into `xcodebuild` so release artifact metadata reflects CI secrets in the same build invocation.
- Release workflow now re-signs Sparkle nested binaries/framework with Developer ID + secure timestamp before notarization.
- Release workflow now lets appcast generation auto-discover Sparkle `sign_update` from available DerivedData artifact paths instead of forcing a single fixed location.
- Sparkle appcast generation now falls back to downloading Sparkle's official SPM artifact bundle and using its `sign_update` binary if local discovery paths are empty.
- Appcast publication now checks `git status --porcelain` for the feed path so newly added files are published instead of being misdetected as unchanged.
- Appcast publication fallback now fails closed when Actions cannot auto-create PRs (`createPullRequest` denied), and emits a manual compare URL so operators can complete publication without silent release success.
- Status-menu `Support Helm` submenu now includes all six support destinations configured in settings (GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo).
- About overlay now includes a `Support Helm` button that opens the same six-option support picker.
- Added interruption/recovery validation runbook for release operators:
  - `docs/validation/v0.16.0-rc.9-installer-recovery.md`
- Build metadata generation now derives monotonic numeric bundle build numbers from semantic versions to keep Sparkle update ordering stable.

---

## Website Workstream (2026-02-21)

Completed:

- Added website redesign planning docs aligned to the Helm brand system:
  - `docs/website/WEBSITE_REDESIGN_PLAN.md`
  - `docs/website/DESIGN_TOKENS.md`
  - `docs/brand/TYPOGRAPHY_COLOR_SYSTEM.md`
  - `docs/brand/WEBSITE_TYPOGRAPHY_COLOR_SPEC.md`
- Implemented a custom Helm visual theme for the Astro/Starlight site:
  - `web/src/styles/helm-theme.css`
  - wired through `web/astro.config.mjs`
- Rebuilt landing page structure and copy hierarchy in `web/src/content/docs/index.mdx` with dual-audience framing:
  - Hero
  - Problem
  - Solution
  - Editions (Helm consumer + Helm Business)
  - Architecture
  - Helm Pro
  - Footer CTA
- Applied explicit Helm website typography/color specification in `web/src/styles/helm-theme.css`:
  - Neue Haas Grotesk heading stack, Inter body text, SF Mono code
  - specified H1/H2/H3/body/small scale and heading color mapping by theme
  - 8pt spacing rhythm and restrained Pro-only gold accents
  - calm, structured visual tone (no neon/startup-style hero effects)
- Added reusable theme-aware screenshot rendering for website content:
  - `web/src/components/ThemeImage.astro`
  - visual tour and landing architecture screenshot now support light/dark asset switching by active site theme
- Completed website content alignment pass across docs pages:
  - updated release-status wording consistency for `v0.16.0` release finalization
  - clarified consumer vs Helm Business positioning in overview + FAQ
  - refreshed installation/usage/visual-tour copy for current UX
- Completed manual accessibility verification pass for key routes:
  - automated Axe CLI scan across key website routes reports zero violations after remediation
  - patched homepage hero secondary CTA contrast to resolve Axe `color-contrast` failure
  - verified heading hierarchy and image alt coverage on docs content
  - verified focus-visible and reduced-motion support in theme CSS

Immediate follow-up:

- Perform manual visual QA in both light and dark theme across mobile/tablet/desktop breakpoints before release publishing.
- Replace visual-tour screenshots after UI styling refresh in `web/src/assets/tour/` and re-run manual QA.
  - use paired filenames so theme switching remains automatic: `name.png` (light) and `name-dark.png` (dark)
- Purchase Neue Haas Grotesk commercial webfont license before production website deployment using that typeface.

---

## v0.15.x Kickoff Plan (Completed)

### Alpha.1 — Plan Model + Inspector Foundations (Completed on `feat/v0.15.x-alpha.1-kickoff`)

Delivered:

- Added explicit execution-plan model with ordered step metadata surfaced through FFI/service/UI
- Mapped each planned step to manager/action/package context and stable identifiers for later task correlation
- Rendered initial ordered plan details in inspector with localized reason/status fields

Validation:

- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

### Alpha.2 — Execution Transparency + Partial Failure Summary (Completed on `feat/v0.15.x-alpha.1-kickoff`)

Delivered:

- Linked runtime task updates to plan-step identifiers for in-flight and completed state projection
- Added partial-failure summaries with grouped causes and affected managers/packages
- Added retry affordances scoped to failed plan steps (without rerunning successful steps)

Validation:

- `cargo test -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

### Alpha.3 — Operator Controls for Large Plans (Completed on `feat/v0.15.x-alpha.1-kickoff`)

Delivered:

- added plan-scoped controls for manager and package filtering in Updates
- added scoped actions for run, cancel remaining, and retry failed-only plan steps
- enforced authority-first step ordering via shared planner helpers used by scope/execution logic
- added planner regression tests for authority ordering and scope filtering
- fixed scoped-run behavior so baseline preview `queued` steps execute while already-projected queued/running/completed steps remain guarded

Validation:

- `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`
- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

### Alpha.4 — Final 0.15.0 Cut Readiness (Completed on `feat/v0.15.x-alpha.1-kickoff`)

Progress so far:

- shared plan-step ID resolution now drives both dashboard projection and scoped-action task correlation paths
- duplicate step-ID handling hardened in retry/projection failure-group mapping paths
- scoped run execution now advances authority phase-by-phase instead of submitting all manager steps concurrently
- cancel remaining now terminates active scoped run sequencing before cancelling matching in-flight tasks
- cancel remaining now also cancels scoped projected in-flight tasks that have not yet landed in listTasks snapshots
- phase sequencing now waits for submission callbacks and protects newly queued projections until task snapshots catch up
- stale callbacks from superseded scoped-run tokens no longer clear active run-in-progress state
- scoped phase waiting now uses a bounded timeout and invalidates stalled run tokens
- planner regression tests expanded for scoped-run gating, ID fallback coverage, and projected cancellation task-ID extraction
- Rust adapter/runtime files normalized with formatting-only cleanup (no behavior changes)
- release notes/checklist prep for first `v0.15.0` pre-release cut is now scaffolded in `CHANGELOG.md` and `docs/RELEASE_CHECKLIST.md`
- Xcode version metadata settings now use a checked-in base xcconfig with optional generated override so clean checkouts build without pre-generated artifacts
- Updates section now scrolls end-to-end so long plan/failure lists remain fully accessible during manual validation
- Updates rows now support full-row inspector hit targets, display-order numbering, and scoped-run in-progress feedback
- Failed-task inspector now provides suggested repro command hints and a single `View Diagnostics` action
- Task inspector now includes a dedicated `Command` field with resolved repro command text (or unavailable fallback)
- Diagnostics modal now includes dedicated `diagnostics`, `stderr`, and `stdout` tabs
- Support diagnostics manager rows now remain stable via authority-first + alphabetical ordering
- Popover failure banner now uses a `Review` action (instead of `Upgrade All`) when failures exist, routing to Control Center Tasks and selecting the first failed task
- Removed redundant Updates `Dry Run` button since equivalent plan context is already visible inline
- Added execution-to-inspector task-output plumbing:
  - per-task runtime context now flows into process requests
  - process output capture is keyed by task ID and exposed via FFI/XPC (`helm_get_task_output` / `getTaskOutput`)
  - inspector fetches task output on demand for diagnostics without adding payload bloat to task polling
- completed brand-system visual refinement pass for app-design slice:
  - tokenized SwiftUI color hierarchy (Helm Blue primary, Rope Gold premium accent) and surface/elevation/radius defaults
  - standardized button and card styling across Control Center, popover overlays, packages/managers rows, and settings
  - refined dark-mode deck contrast and selection/focus hierarchy without layout re-architecture
  - added Pro button style hook and premium CTA treatment for support/upgrade-path surfaces
  - validation passed: `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`

Deliver:
- open PR with final `v0.15.0` prep deltas into `dev` for verified commit lineage
- after merge to `dev`, open `dev` -> `main` PR and complete CI before tagging

### Exit Gate

- users can inspect full ordered execution plans with meaningful context
- partial failures are clearly attributable and actionable
- transparency state remains synchronized between task system and plan UI

---

## v0.14.1 Patch Track (Completed)

### UI/UX Slice (Completed on `dev`)

Delivered:

- Onboarding manager rows compressed to single-line name/version metadata
- Homebrew naming clarified to "Homebrew (formulae)" and "Homebrew (casks)"
- Package list row highlight for inspector-selected package
- Removed redundant package-section search chip; retained top-right global search field
- Added inline clear control to top-right search field
- Inspector package panel now includes description (when present) and context actions (Update, Pin/Unpin, View Manager)

### Follow-Up Stabilization Slice (Completed on `dev`)

Delivered:

- Onboarding manager rows now keep manager name + detected version on a single line in both:
  - "Finding Your Tools"
  - "Pick Your Managers"
- Task list visibility now deduplicates in-flight task rows by `(manager, task_type)` and keeps bounded terminal history
- Task list visibility now fetches a wider recent-task window to avoid hiding long-running in-flight entries under queue churn
- Task pruning timeout policy now removes terminal (`completed`/`failed`/`cancelled`) records using terminal-status timestamps
- Duplicate task submission guard added for manager install/update/uninstall and package upgrade actions when an identical labeled task is already queued/running
- Refresh trigger now skips creating a new refresh sweep while refresh/detection tasks are already in flight
- RubyGems now participates in per-package upgrade eligibility in the SwiftUI workflow
- Added regression tests for task list dedup/terminal-history visibility and prune-policy status filtering

### Cache/Persistence Slice (Completed on `dev`)

Delivered:

- Search cache persistence now keeps one row per `(manager, package)` instead of accumulating duplicates by query/version tuple
- Search cache upserts preserve previously known non-empty version/summary metadata when newer search responses omit those fields
- Added regression coverage for search-cache deduplication and summary preservation semantics
- Control-center available-cache refresh now deduplicates by package ID and preserves non-empty summaries across cache rows
- Package aggregation now enriches installed/outdated package records with cached summaries when available
- Package filtering now matches query text against package summaries and merges remote-search summary/latest metadata into local package rows

### Adapter Behavior Slice (Completed on `dev`)

Delivered:

- Enabled RubyGems for per-package update action eligibility in the SwiftUI control center workflow
- Added Homebrew dependency preflight for Homebrew-backed manager installs (`mise`, `mas`)
- Added explicit localized service error for missing Homebrew dependency:
  - `service.error.homebrew_required`
  - propagated across all supported locales and mirrored locale resources

### Search + Inspector Actions Slice (Completed on `dev`)

Delivered:

- Remote search now queues manager-scoped search tasks across all enabled/detected search-capable managers
- Search task labels now include manager + query context for clearer in-flight task rows
- `Refresh Now` now warms available package cache entries via manager-scoped background search tasks
- Package inspector description behavior now includes cached immediate display, background refresh, loading placeholder, and unavailable fallback states
- Task inspector now displays localized failure feedback for failed tasks, including Homebrew install troubleshooting hints
- Package inspector now exposes context-appropriate package actions (Install/Uninstall/Update/Pin/Unpin/View Manager)
- Package install/uninstall actions are now wired through new FFI + service methods for supported managers

Release closure:

- merged `dev` -> `main` via PR `#65`
- created/pushed annotated tag `v0.14.1`

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

- Security roadmap restructured with staged boundaries:
  - `0.18.x` local groundwork, `0.19.x` hardening
  - `1.3.x` Security Advisory System (Pro)
  - `1.4.x` Shared Brain
- CHANGELOG.md, CURRENT_STATE.md, NEXT_STEPS.md, ROADMAP.md updated for rc.1

---

## v0.13.0-rc.2 — Support & Feedback Entry Points (Completed)

### Support & Feedback Card (Completed)

Delivered:

- New "Support & Feedback" SettingsCard in control-center Settings surface
- 5 action buttons: Support Helm, Send Feedback, Report a Bug, Request a Feature, Copy Diagnostics
- Support Helm destinations include all six configured channels: GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo
- "Include Diagnostics" toggle (default OFF): copies diagnostics to clipboard before opening GitHub issue template
- Transient "Copied!" confirmation with animated opacity transition
- `HelmSupport` updated with template-specific URLs (`reportBug`, `requestFeature` methods)

### Localization (Completed)

Delivered:

- 9 new L10n keys (`app.settings.support_feedback.*`) added to all 6 locales (en, es, de, fr, pt-BR, ja)
- Canonical and mirror locale files synchronized

### GitHub & Documentation (Completed)

Delivered:

- `.github/FUNDING.yml` created for GitHub Sponsors and Patreon support buttons (plus direct support links for Buy Me a Coffee, Ko-fi, PayPal, and Venmo)
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

## v0.14.0 — Stable Release Cut (Completed)

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

### Release Execution (Completed)

- Branch/PR execution:
  - ✅ merge release finalization branch into `dev`
  - ✅ open PR from `dev` to `main` and run CI checks (`#60`)
- Release finalization:
  - ✅ merge `dev` into `main` (via `#60`)
  - ✅ create/push annotated tag `v0.14.0`

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

- All 7 locales (en, es, de, fr, pt-BR, ja, hu) at full key parity ✅
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

### Priority 9 — CLI Companion (New Goal)

Implement:

- Approve and iterate CLI specification in `docs/architecture/HELM_CLI_SPEC.ms`
- Keep GUI + CLI on one shared coordinator/runtime path (single task authority)
- Scaffold `helm` binary with read-only command surface and stable `--json` output contracts
- Add mutating command/task lifecycle coverage to match GUI capabilities incrementally

---

## Non-Goals (Pre-1.0)

- Plugin system
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
- 0.14.x and 0.15.x release execution are complete on `main` (`v0.14.1` and `v0.15.0`).
- 0.17.6 release execution is complete on `main`; 0.17.x diagnostics/logging delivery and post-`0.17.x` follow-up stabilization are now closed with stable lineage `v0.17.0`, `v0.17.1`, `v0.17.2`, `v0.17.3`, `v0.17.4`, `v0.17.5`, and `v0.17.6`.
