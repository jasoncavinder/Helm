# Changelog

All notable changes to this project are documented in this file.

The format is based on Keep a Changelog and follows SemVer-compatible Helm versioning.

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
