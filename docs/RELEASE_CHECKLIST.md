# Release Checklist

This checklist is required before creating a release tag on `main`.

## v0.9.2 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.9.2` release notes for Spanish/German locale expansion.
- [x] `README.md` reflects `v0.9.2`.

### Versioning
- [x] Workspace version bumped to `0.9.2` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.9.2` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts updated:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi`).
- [x] macOS app build succeeds in CI/dev environment constraints.

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.9.2 -m "Helm v0.9.2"`
- [ ] Push tag:
  - `git push origin v0.9.2`

## v0.9.1 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.9.1` hotfix notes for localization resource resolution.
- [x] `README.md` reflects `v0.9.1`.

### Versioning
- [x] Workspace version bumped to `0.9.1` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.9.1` in `core/rust/Cargo.lock`.
- [x] Generated app version artifact updated:
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi`).
- [x] macOS app build succeeds in CI/dev environment constraints.

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.9.1 -m "Helm v0.9.1"`
- [ ] Push tag:
  - `git push origin v0.9.1`

## v0.9.0 (Completed)

### Scope and Documentation
- [x] `docs/ROADMAP.md` marks `0.9.x` as completed and aligned to delivered scope.
- [x] `CHANGELOG.md` includes `0.9.0` release notes.
- [x] `README.md` reflects `v0.9.0` and milestone status.
- [x] `PROJECT_BRIEF.md` implementation phases reflect `0.9.x` completion.

### Versioning
- [x] Workspace version bumped to `0.9.0` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.9.0` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts updated from workspace version:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi`).
- [x] macOS app build succeeds in CI/dev environment constraints.

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.9.0 -m "Helm v0.9.0"`
- [x] Push tag:
  - `git push origin v0.9.0`
- [x] Final release commit SHA recorded: `c54f302`.

## v0.8.0 (Completed)

### Scope and Documentation
- [x] `docs/ROADMAP.md` marks `0.8.x` as completed and aligned to delivered scope.
- [x] `CHANGELOG.md` includes `0.8.0` release notes.
- [x] `README.md` reflects `v0.8.0` and milestone status.
- [x] `PROJECT_BRIEF.md` implementation phases reflect `0.8.x` completion.

### Versioning
- [x] Workspace version bumped to `0.8.0` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.8.0` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts updated from workspace version:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test -p helm-ffi`).
- [x] macOS app build succeeds in CI/dev environment constraints.

### Branch and Tag
- [x] Merge `dev` into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.8.0 -m "Helm v0.8.0"`
- [x] Push tag:
  - `git push origin v0.8.0`

## v0.8.0-rc.2 (Completed)

### Scope and Documentation
- [x] `docs/ROADMAP.md` status remains aligned with 0.8 policy/pinning scope.
- [x] `CHANGELOG.md` includes `0.8.0-rc.2` release notes.
- [x] `README.md` reflects current pre-1.0 release candidate status.

### Versioning
- [x] Workspace version bumped to `0.8.0-rc.2` in `core/rust/Cargo.toml`.
- [x] Generated app version artifacts updated from workspace version:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust format check passes (`cargo fmt --all`).
- [x] Targeted Rust tests for Homebrew/mise/rustup upgrade paths pass.
- [x] macOS app build succeeds in CI/dev environment constraints.

### Tag and Publish
- [x] Create annotated tag:
  - `git tag -a v0.8.0-rc.2 -m "Helm v0.8.0-rc.2"`

## v0.7.0 (Completed)

### Scope and Documentation
- [x] `docs/ROADMAP.md` reflects `0.7.0` milestone delivery.
- [x] `CHANGELOG.md` includes `0.7.0` release notes.
- [x] `README.md` links to roadmap, versioning, changelog, and release checklist.

### Versioning
- [x] Workspace version bumped to `0.7.0` in `core/rust/Cargo.toml`.
- [x] Generated app version artifacts updated from workspace version:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust format check passes (`cargo fmt --all --check`).
- [x] Rust tests pass (`cargo test -p helm-core` and `cargo test -p helm-ffi`).
- [x] macOS app build succeeds in CI/dev environment constraints.

### Branch and Merge
- [x] Feature branch merged into `dev`.
- [x] `dev` merged into `main`.
- [x] Final release commit SHA recorded: `2204a05` (`merge: dev into main for v0.7.0 release`).

### Tag and Publish
- [x] Create annotated tag from `main`:
  - `git tag -a v0.7.0 -m "Helm v0.7.0"`
- [x] Push tag:
  - `git push origin v0.7.0`
- [x] Publish GitHub release notes from `CHANGELOG.md`.
