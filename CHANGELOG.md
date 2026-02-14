# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and follows SemVer-compatible Helm versioning.

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
