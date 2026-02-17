# Release Checklist

This checklist is required before creating a release tag on `main`.

## v0.12.0-beta.4 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.12.0-beta.4` notes for dry-run support in upgrade preview flow.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect dry-run support completion for Priority 4.
- [x] Locale files updated for new dry-run strings across shipped locales and resource mirrors.

### Validation
- [x] Locale integrity checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.12.0-beta.4 -m "Helm v0.12.0-beta.4"`
- [ ] Push tag:
  - `git push origin v0.12.0-beta.4`

## v0.12.0-beta.3 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.12.0-beta.3` notes for dedicated upgrade-preview UI delivery.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect Priority 4 preview-surface completion status.

### Validation
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.12.0-beta.3 -m "Helm v0.12.0-beta.3"`
- [x] Push tag:
  - `git push origin v0.12.0-beta.3`

## v0.12.0-beta.2 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.12.0-beta.2` notes for expanded visual-overflow validation coverage.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect Priority 3 overflow-validation expansion status.
- [x] Validation report committed at `docs/validation/v0.12.0-beta.2-visual-overflow-expansion.md`.

### Validation
- [x] Run full on-device visual overflow validation across `es`, `fr`, `de`, `pt-BR`, and `ja` (`HelmTests/LocalizationOverflowValidationTests`) with expanded surface checks.
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.12.0-beta.2 -m "Helm v0.12.0-beta.2"`
- [x] Push tag:
  - `git push origin v0.12.0-beta.2`

## v0.12.0-beta.1 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.12.0-beta.1` notes for localization validation hardening.
- [x] `README.md` reflects `v0.12.0-beta.1` status and manager coverage.
- [x] Website docs status/overview/roadmap pages are updated for `v0.12.0-beta.1`.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect post-`v0.11.0-beta.2` state and `v0.12.0-beta.1` target kickoff.

### Validation
- [x] Locale key/placeholder integrity script added: `apps/macos-ui/scripts/check_locale_integrity.sh`.
- [x] i18n CI runs locale mirror parity for all shipped locales plus locale integrity checks.
- [x] Run full on-device visual overflow validation across `es`, `fr`, `de`, `pt-BR`, and `ja` (`HelmTests/LocalizationOverflowValidationTests`).
- [x] Validation report committed at `docs/validation/v0.12.0-beta.1-visual-overflow.md`.

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.12.0-beta.1 -m "Helm v0.12.0-beta.1"`
- [x] Push tag:
  - `git push origin v0.12.0-beta.1`

## v0.11.0-beta.2 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.11.0-beta.2` notes for stabilization and validation results.
- [x] `README.md` reflects `v0.11.0-beta.2` status.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect the beta2 checkpoint.
- [x] Website docs status/overview/roadmap pages are updated for `v0.11.0-beta.2`.

### Validation
- [x] Stabilization script passes: `apps/macos-ui/scripts/run_v0110b2_stabilization_checks.sh`.
- [x] Priority 2 manager smoke matrix captured: `apps/macos-ui/scripts/smoke_priority2_managers.sh`.
- [x] Validation notes committed at `docs/validation/v0.11.0-beta.2-smoke-matrix.md`.
- [x] Localization overflow heuristic validation notes committed at `docs/validation/v0.11.0-beta.2-l10n-overflow.md`.
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.11.0-beta.2 -m "Helm v0.11.0-beta.2"`
- [x] Push tag:
  - `git push origin v0.11.0-beta.2`

## v0.11.0-beta.1 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.11.0-beta.1` notes for Priority 2 extended language-manager delivery.
- [x] `README.md` reflects `v0.11.0-beta.1` status and milestone progression.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect the `v0.11.0-beta.1` checkpoint.
- [x] Website-facing docs/state pages are updated for `v0.11.0-beta.1`.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi`).
- [x] macOS unit tests pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' test`).
- [x] i18n lint parity is satisfied (`diff -ru locales/en apps/macos-ui/Helm/Resources/locales/en`).

### Branch and Tag
- [x] Extended manager scope merged to `dev` via PR.
- [x] Create annotated beta tag from `dev` lineage:
  - `git tag -a v0.11.0-beta.1 -m "Helm v0.11.0-beta.1"`
- [x] Push tag:
  - `git push origin v0.11.0-beta.1`

## v0.10.0 (Planned)

### Scope and Documentation
- [x] `CHANGELOG.md` includes consolidated `0.10.0` stable notes (beta scope + hardening + residual fixes).
- [x] `README.md` reflects stable `v0.10.0` status and milestone progression.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect `0.10.x` completion.
- [x] Website docs status/overview/roadmap pages reflect `v0.10.0` and current planned sequence.

### Versioning
- [x] Workspace version bumped to `0.10.0` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.10.0` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.10.0`:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test` in `core/rust`).
- [x] macOS unit tests pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).
- [x] Website build succeeds (`ASTRO_TELEMETRY_DISABLED=1 npm --prefix web run build`).
- [x] Local smoke matrix captured for Priority 1 language managers (`npm`, `pipx`, `pip`, `cargo`, `cargo-binstall`) with detected/not-detected notes.

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.10.0 -m "Helm v0.10.0"`
- [ ] Push tag:
  - `git push origin v0.10.0`

## v0.10.0-beta.2 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.10.0-beta.2` notes for hardening/review and roadmap milestone updates.
- [x] `README.md` reflects `v0.10.0-beta.2` status and resequenced milestone table.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect the beta2 checkpoint and inserted UI/UX milestone.
- [x] Website docs status/overview/roadmap pages are updated for `0.10.0-beta.2`.

### Versioning
- [x] Workspace version bumped to `0.10.0-beta.2` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.10.0-beta.2` in `core/rust/Cargo.lock`.
- [ ] Generated app version artifacts updated (if release flow requires regeneration):
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test` in `core/rust`).
- [x] macOS unit tests pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).
- [x] Website build succeeds (`ASTRO_TELEMETRY_DISABLED=1 npm --prefix web run build`).

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.10.0-beta.2 -m "Helm v0.10.0-beta.2"`
- [ ] Push tag:
  - `git push origin v0.10.0-beta.2`

## v0.10.0-beta.1 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.10.0-beta.1` notes for core language-manager milestone delivery.
- [x] `README.md` reflects `v0.10.0-beta.1` status.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect completed Priority 1 milestone scope.
- [x] Website docs status/roadmap pages are updated for `0.10.0-beta.1`.

### Versioning
- [x] Workspace version bumped to `0.10.0-beta.1` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.10.0-beta.1` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts updated:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test` in `core/rust`).
- [x] macOS unit tests pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).
- [x] Website build succeeds (`ASTRO_TELEMETRY_DISABLED=1 npm --prefix web run build`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.10.0-beta.1 -m "Helm v0.10.0-beta.1"`
- [x] Push tag:
  - `git push origin v0.10.0-beta.1`

## v0.9.3 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.9.3` release notes for localization hardening and upgrade-preview test coverage.
- [x] `README.md` reflects `v0.9.3`.
- [x] Website docs status/roadmap pages reflect current pre-1.0 status and milestone progression.

### Versioning
- [x] Workspace version bumped to `0.9.3` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.9.3` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts updated:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Validation
- [x] Rust tests pass (`cargo test` in `core/rust`).
- [x] macOS unit tests pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.9.3 -m "Helm v0.9.3"`
- [x] Push tag:
  - `git push origin v0.9.3`

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
