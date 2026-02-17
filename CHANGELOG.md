# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and follows SemVer-compatible Helm versioning.

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
