# Release Checklist

This checklist is required before creating a release tag on `main`.

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
