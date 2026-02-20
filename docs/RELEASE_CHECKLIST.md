# Release Checklist

This checklist is required before creating a release tag on `main`.

## v0.16.0 (In Progress)

### Sparkle Feed and Distribution Safety
- [ ] `HELM_SPARKLE_FEED_URL`, `HELM_SPARKLE_PUBLIC_ED_KEY`, and `HELM_SPARKLE_PRIVATE_ED_KEY` secrets are present for release workflow.
- [ ] `HELM_SPARKLE_FEED_URL` is set to the canonical feed URL: `https://helmapp.dev/updates/appcast.xml`.
- [ ] Sparkle feed endpoint is published at `web/public/updates/appcast.xml` (or `HELM_SPARKLE_FEED_URL` points to the hosted equivalent).
- [ ] Release workflow generates and uploads `appcast.xml` alongside DMG artifacts.
- [ ] Release workflow publishes generated `appcast.xml` into `web/public/updates/appcast.xml` on `main` (or auto-opens fallback PR if direct push is blocked).
- [ ] Runtime self-update is blocked for package-manager-managed installs (Homebrew Cask receipt detection + Homebrew/MacPorts path heuristics) and enabled for eligible direct-channel DMG installs.
- [ ] Generated `CURRENT_PROJECT_VERSION` is monotonic for Sparkle version ordering (semver-derived numeric build number).
- [ ] Sparkle package reference remains pinned to `2.8.1` in `apps/macos-ui/Helm.xcodeproj/project.pbxproj` for macOS 11+/12 compatibility.
- [ ] Appcast policy validation passes in release workflow (`apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh`), ensuring full-installer-only feed output (no deltas).
- [ ] Delta update policy (`full installer only` for `0.16.x`) is documented in `docs/DECISIONS.md` and reflected in release automation.

### Installer/Updater Recovery Validation
- [ ] Execute interruption/recovery validation runbook: `docs/validation/v0.16.0-rc.1-installer-recovery.md`.
- [ ] Confirm workflow rerun behavior for same tag remains idempotent (artifact clobber + deterministic appcast publish target).
- [ ] Confirm protected-branch recovery path by validating fallback appcast PR flow if direct `main` push is rejected.

### Sparkle Key Bootstrap (One-Time)
1. Locate Sparkle key tooling from Xcode DerivedData artifacts:
   `find "$HOME/Library/Developer/Xcode/DerivedData" -path '*/SourcePackages/artifacts/sparkle/bin/generate_keys' -print -quit`
2. Generate or reuse keys:
   `.../generate_keys`
3. Export private key from Keychain to base64 (single line) and store as `HELM_SPARKLE_PRIVATE_ED_KEY`.
4. Print public key with `.../generate_keys -p` and store as `HELM_SPARKLE_PUBLIC_ED_KEY`.
5. Add/update repository secrets:
   `gh secret set HELM_SPARKLE_PRIVATE_ED_KEY`
   `gh secret set HELM_SPARKLE_PUBLIC_ED_KEY`
   `gh secret set HELM_SPARKLE_FEED_URL`

## v0.15.0 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` `[Unreleased]` notes track final `v0.15.0` delivery and stabilization changes.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect completed `v0.15.0` alpha.1–alpha.4 scope plus final prep changes.
- [x] README/website status text aligned to `v0.15.0` pre-release testing.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Versioning
- [x] Workspace version bumped to `0.15.0` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.15.0` in `core/rust/Cargo.lock`.
- [ ] Generated app version artifacts aligned to `0.15.0` by build flow (build-generated, not tracked in git).

### Branch and Tag
- [ ] Open PR with final prep deltas into `dev` (for verified commit provenance).
- [ ] Merge prep PR into `dev`.
- [ ] Open PR from `dev` to `main` for `v0.15.0` and complete CI checks.
- [ ] Merge `dev` into `main` for release.
- [ ] Create annotated tag from `main`: `git tag -a v0.15.0 -m "Helm v0.15.0"`.
- [ ] Push tag: `git push origin v0.15.0`.

## v0.14.1 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.14.1` patch notes.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect released `v0.14.1` state on `main`.
- [x] README/website release status updated for `0.14.1`.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Versioning
- [x] Workspace version bumped to `0.14.1` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.14.1` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.14.1`.

### Branch and Tag
- [x] Open PR from `dev` to `main` for `v0.14.1` and complete CI checks (`#65`).
- [x] Merge `dev` into `main` for release.
- [x] Create annotated tag: `git tag -a v0.14.1 -m "Helm v0.14.1"`
- [x] Push tag: `git push origin v0.14.1`

## v0.14.0 (Completed)

### Scope and Documentation
- [x] 0.14 manager delivery slices completed through `v0.14.0-alpha.5` (container/VM, detection-only, security/firmware, optional managers, Homebrew cask status).
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect alpha.1–alpha.5 delivery and post-alpha.5 readiness items.
- [x] 0.14 manager capability sweep artifact committed:
  - `docs/validation/v0.14.0-alpha.5-manager-capability-sweep.md`
- [x] `CHANGELOG.md` includes `0.14.0` release notes.
- [x] README/website release status updated for `0.14.0`.
- [x] Distribution/licensing architecture planning docs aligned for `0.14.0`:
  - build matrix and channel authority mapping documented
  - consumer vs fleet lifecycle separation documented
  - roadmap phases added for Sparkle, MAS, Setapp, Fleet, PKG/MDM, offline licensing

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO -only-testing:HelmTests test`).

### Versioning
- [x] Workspace version bumped to `0.14.0` in `core/rust/Cargo.toml`.
- [x] Generated app version artifacts aligned to `0.14.0`.

### Branch and Tag
- [x] Release finalization branch merged to `dev`.
- [x] PR opened from `dev` to `main` for `v0.14.0` and CI checks completed (`#60`).
- [x] `dev` merged into `main` for release (via `#60`).
- [x] Create annotated tag: `git tag -a v0.14.0 -m "Helm v0.14.0"`
- [x] Push tag: `git push origin v0.14.0`

## v0.13.0-rc.1 (In Progress)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-rc.1` notes for inspector sidebar, upgrade reliability, status menu, and task labels.
- [x] `README.md` reflects `v0.13.0-rc.1` status.
- [x] Website docs updated for `v0.13.0-rc.1`.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect rc.1 checkpoint.
- [x] Security Advisory System milestone added to ROADMAP.md (1.3.x).

### Validation
- [x] Rust tests pass (198+ tests: `cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Versioning
- [ ] Workspace version bumped to `0.13.0-rc.1` in `core/rust/Cargo.toml`.
- [ ] Generated app version artifacts aligned to `0.13.0-rc.1`.

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag: `git tag -a v0.13.0-rc.1 -m "Helm v0.13.0-rc.1"`
- [ ] Push tag: `git push origin v0.13.0-rc.1`

## v0.13.0-beta.6 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-beta.6` notes for validation, hardening, and documentation.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect beta.6 checkpoint.
- [x] Validation report template created at `docs/validation/v0.13.0-beta.6-redesign-validation.md`.
- [x] Usability test plan created at `docs/validation/v0.13.0-beta.6-usability-test-plan.md`.

### Validation
- [x] Rust tests pass.
- [x] `HelmTests` pass.

### Branch and Tag
- [x] Create annotated tag from `dev` lineage: `git tag -a v0.13.0-beta.6 -m "Helm v0.13.0-beta.6"`
- [x] Push tag: `git push origin v0.13.0-beta.6`

## v0.13.0-beta.5 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-beta.5` notes for UI purity fixes, legacy removal, and XPC robustness.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect beta.5 checkpoint.

### Validation
- [x] Rust tests pass.
- [x] `HelmTests` pass.

### Branch and Tag
- [x] Create annotated tag: `git tag -a v0.13.0-beta.5 -m "Helm v0.13.0-beta.5"`
- [x] Push tag: `git push origin v0.13.0-beta.5`

## v0.13.0-beta.4 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-beta.4` notes for onboarding walkthrough and localization parity.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect beta.4 checkpoint.

### Validation
- [x] Locale integrity checks pass.
- [x] Rust tests pass.
- [x] `HelmTests` pass.

### Branch and Tag
- [x] Create annotated tag: `git tag -a v0.13.0-beta.4 -m "Helm v0.13.0-beta.4"`
- [x] Push tag: `git push origin v0.13.0-beta.4`

## v0.13.0-beta.3 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-beta.3` notes for accessibility, task cancellation, CI enforcement.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect beta.3 checkpoint.

### Validation
- [x] Rust tests pass.
- [x] `HelmTests` pass.
- [x] `ci-test.yml` workflow added for PR/push CI gates.

### Branch and Tag
- [x] Create annotated tag: `git tag -a v0.13.0-beta.3 -m "Helm v0.13.0-beta.3"`
- [x] Push tag: `git push origin v0.13.0-beta.3`

## v0.13.0-beta.2 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.13.0-beta.2` notes for universal-build/signing/distribution follow-through.
- [x] `README.md` reflects `v0.13.0-beta.2` status and includes beta-testing callout with GitHub Issues link.
- [x] Website docs landing/overview/roadmap pages are updated for `v0.13.0-beta.2`, including beta-test invitation.
- [x] Website installation guide documents beta DMG distribution and drag-to-`Applications` installation flow.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect the redesign beta checkpoint.
- [x] Release workflow exists for signed universal DMG artifacts (`.github/workflows/release-macos-dmg.yml`).

### Validation
- [x] Rust tests pass.
- [x] `HelmTests` pass.
- [x] Website build succeeds.

### Versioning
- [x] Workspace version bumped to `0.13.0-beta.2` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.13.0-beta.2` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.13.0-beta.2`.

### Branch and Tag
- [x] Create annotated tag: `git tag -a v0.13.0-beta.2 -m "Helm v0.13.0-beta.2"`
- [x] Push tag: `git push origin v0.13.0-beta.2`

## v0.12.0 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes consolidated `0.12.0` stable notes (localization hardening + upgrade transparency delivery).
- [x] `README.md` reflects `v0.12.0` status and milestone progression.
- [x] Website docs status/overview/roadmap pages are updated for `v0.12.0`.
- [x] `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, and `docs/ROADMAP.md` reflect post-beta completion and `0.12.0` stabilization.

### Validation
- [x] Locale integrity checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh`).
- [x] Locale mirror parity remains aligned for shipped locales.
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).
- [x] Website build succeeds (`ASTRO_TELEMETRY_DISABLED=1 npm --prefix web run build`).

### Versioning
- [x] Workspace version bumped to `0.12.0` in `core/rust/Cargo.toml`.
- [x] Rust lockfile package versions aligned to `0.12.0` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.12.0`:
  - `apps/macos-ui/Generated/HelmVersion.swift`
  - `apps/macos-ui/Generated/HelmVersion.xcconfig`

### Branch and Tag
- [ ] `dev` merged into `main` for release.
- [ ] Create annotated tag from `main`:
  - `git tag -a v0.12.0 -m "Helm v0.12.0"`
- [ ] Push tag:
  - `git push origin v0.12.0`

## v0.12.0-beta.4 (Completed)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.12.0-beta.4` notes for dry-run support in upgrade preview flow.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect dry-run support completion for Priority 4.
- [x] Locale files updated for new dry-run strings across shipped locales and resource mirrors.

### Validation
- [x] Locale integrity checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme HelmTests -destination 'platform=macOS' -derivedDataPath /tmp/helmtests-deriveddata CODE_SIGNING_ALLOWED=NO CODE_SIGNING_REQUIRED=NO test`).

### Branch and Tag
- [x] `dev` merged into `main` for release.
- [x] Create annotated tag from `main`:
  - `git tag -a v0.12.0-beta.4 -m "Helm v0.12.0-beta.4"`
- [x] Push tag:
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

## v0.10.0 (Completed)

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

## v0.10.0-beta.2 (Completed)

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

## v0.9.2 (Completed)

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

## v0.9.1 (Completed)

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
