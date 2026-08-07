# Release Checklist

This checklist is required before creating a release tag on `main`.

## Third-Party License Compliance (All Releases)

- [x] Re-audit dependency licenses and update `docs/legal/THIRD_PARTY_LICENSES.md` when versions or dependency sets change.
- [x] Confirm release materials retain required third-party attribution/license texts for shipped runtime dependencies.
- [x] Confirm Sparkle license + external attributions remain preserved for channels that include Sparkle.
- [x] If distributing artifacts that include `sharp/libvips` binaries (outside static-site output), include LGPL notice/corresponding-source obligations for that artifact.

## Website Hosting Integrity (All Releases)

- [x] Confirm `.github/workflows/deploy-web.yml` is absent (Cloudflare Pages is the production website host).
- [x] Confirm website hosting/operations docs still point to Cloudflare Pages and not GitHub Pages.

## GitHub Governance Integrity (All Releases)

- [ ] Confirm branch rulesets are active for `main` and `dev`.
- [ ] Confirm required checks match current policy:

  Required branch checks (unconditional for every PR to `main` or `dev`):
    - `Policy Gate`, `Rust Core Tests`, `Xcode Build Check`, `hardcoded-ui-strings`, `Semgrep scan`, `Lint Swift`

  Path-scoped workflows (run only when relevant paths change):
    - `Docs Checks` — triggers on docs, AGENTS.md, CONTRIBUTING.md, PR template
    - `Web Build` — triggers on website content and build paths
- [ ] Confirm advisory release-monitor workflows are **not** required branch checks:
  - `Release Publish Verify`
  - `Appcast Drift Guard`
  - `CLI Update Metadata Drift Guard`
- [ ] Confirm repo merge settings remain enabled/configured: auto-merge on, update-branch on, and delete-branch-on-merge off.
- [ ] Confirm release metadata publication remains PR-based (no direct-push fallback path in `.github/workflows/release-macos-dmg.yml`).
- [ ] Confirm `main` ruleset bypass actors are least-privilege for release publish PR flow (prefer GitHub Actions app `pull_request` bypass, avoid broad `always` bypass).

## Release Preflight (All Releases, Mandatory Before Tagging)

- [x] Run `scripts/release/preflight.sh --tag <vX.Y.Z|vX.Y.Z-rc.N>` from a clean local clone before tag creation.
- [x] Read and follow `docs/operations/RELEASE_FLOW.md` before any release mutation.
- [x] Run `scripts/release/rehearsal_dry_run.sh --tag <vX.Y.Z|vX.Y.Z-rc.N>` and retain its passing report.
- [x] Confirm `Release macOS Canary` passed on `macos-26` after the latest release-workflow or toolchain change.
- [x] Dispatch and pass `Release Publish Auth Check` with `write_probe=true` after any release credential rotation or permission change.
- [x] Confirm preflight reports token scopes include `repo` and `workflow`.
- [x] Confirm preflight validates required release workflows are present and enabled.
- [x] Confirm preflight validates required DMG/signing/update secrets are present.
- [x] Confirm `RELEASE_PUBLISH_PAT` is configured and authenticated so release-generated publish PRs can receive required checks and auto-merge normally.
- [x] Confirm preflight validates `main` ruleset publish-PR bypass policy (prefer GitHub Actions app `pull_request` bypass when available; otherwise use `Repository admin` `pull_request` fallback; no `always` bypass actors).
- [x] Confirm preflight snapshot sanity passes for stable tags (`origin/main` appcast + `cli/latest.json` in sync and behind target tag).
- [x] Confirm crash/error reporting policy remains local-only for the release line and no automatic remote telemetry path was introduced (`docs/operations/CRASH_REPORTING_POLICY.md`).
- [x] Optional wrapper path: `scripts/release/runbook.sh prepare --tag <tag>`.

## Release Publication Verification (All Releases)

- [x] Review release workflow summary output for both release workflows:
  - `Artifacts uploaded: yes`
  - `Publish PR opened: yes/no`
  - `Main metadata synced: yes/no`
- [x] If workflow summary reports follow-up required (publish PR still open), merge the publish PR and run workflow_dispatch with `verify_only=true` for the corresponding release workflow to verify `Main metadata synced: yes` without rebuilding artifacts.
- [x] Confirm release publication verification status is green after publish PR merge:
  - `Release Publish Verify`
  - `Appcast Drift Guard`
  - `CLI Update Metadata Drift Guard`
- [x] Review `TMP_RELEASE_FRICTION`; promote recurring friction items into durable docs (`docs/DECISIONS.md`, `docs/operations/CLI_RELEASE_AND_CI.md`) and keep temporary notes uncommitted.

## v0.19.0-rc.1 (Manager and Updater Modernization RC)

### Scope and Versioning

- [x] Workspace and local Rust package versions identify `0.19.0-rc.1`.
- [x] `CHANGELOG.md`, current-state, next-steps, roadmap, and platform-policy documentation describe the candidate without changing public stable pointers from `v0.18.2`.
- [x] The macOS 13 Ventura baseline is explicit and the obsolete macOS 11/12 login-helper fallback is removed.
- [x] Dependency modernization retains the `v0.18.2` SQLite online-backup and migration-ledger safety contract.
- [x] npm/related install-instance normalization, MAS refresh handling, Sparkle inventory/update visibility, external-bundle GUI update actions, and scheduled eligible-channel Helm checks are included.
- [x] Direct Developer ID builds expose a default-off beta/RC opt-in; Helm remains explicitly excluded from the separate Sparkle Apps manager.
- [x] GUI appcast publication preserves one stable default-channel item and replaces only the matching signed `beta` prerelease item.

### Required Validation

- [x] Full repository quality gate passes on the v0.18.2-based integration branch.
- [x] SQLite migration compatibility gate passes with `rusqlite 0.40.1`, including backup and tamper/recovery coverage.
- [x] Sparkle appcast/recovery checklist and arm64 Xcode tests pass.
- [x] Appcast channel contracts verify stable/RC coexistence, replacement, malformed-channel rejection, and channel-specific release verification.
- [x] Third-party dependency licenses are re-audited for the candidate dependency graph.
- [x] Non-mutating `v0.19.0-rc.1` release rehearsal and preflight complete without unresolved errors.

### Protected Integration and Publication

- [ ] Merge the candidate through the protected branch path and rerun required checks from current `main`.
- [ ] Confirm a fresh successful `Release macOS Canary` after the final workflow/toolchain changes.
- [ ] Dispatch and pass `Release Publish Auth Check` with `write_probe=true` under explicit maintainer authorization.
- [ ] Obtain explicit maintainer approval before creating or pushing `v0.19.0-rc.1` or publishing artifacts.
- [ ] Confirm the published appcast retains `v0.18.2` as the default-channel item and adds `v0.19.0-rc.1` only as the `beta` item.
- [ ] Complete signed/notarized GUI and direct CLI prerelease publication and post-publication verification.

## v0.18.2 (Final v0.18 Containment Release Gate, Completed)

### Scope

- [x] Append-only SQLite migration identity manifest records released migration version, name, up/down SQL, and SHA-256 definition checksum.
- [x] Migration `20` persists and backfills migration-definition checksums without rewriting prior released migrations.
- [x] SQLite initialization rejects ledger gaps, unknown versions, changed names, changed checksums, and missing checksums on checksum-aware databases.
- [x] Verified private pre-migration backups, integrity checks, bounded retention, and Reset Local Data cleanup provide a recoverable migration boundary.
- [x] Debug databases are separated from release data by default while explicit `HELM_DB_PATH` overrides remain supported.
- [x] Initialization failures propagate through CLI, FFI, XPC, and Service Health paths instead of being silently discarded.
- [x] Migration compatibility checks run in normal CI and GUI/CLI release workflows.
- [x] Project WOW and Native Mac Experience v0.18 design contracts are complete and included as planning-only release content; no target first-run or redesigned UI behavior is claimed.
- [x] The owner-run moderated-study checkpoint remains open and required before v0.20 workflow sign-off and v0.22 UI lock.

### Required Validation

- [x] Stateful v0.17.12 upgrade, affected-v0.18.0 recovery, rollback, tampering, backup, CLI, and FFI migration suites pass on the merged implementation.
- [x] Run the full repository quality gate, documentation sync, Sparkle checklist, and non-mutating `v0.18.2` release rehearsal from the release-preparation branch.
- [x] Merge release preparation through `dev` and the protected `dev` to `main` path.
- [x] Run rehearsal and preflight from a clean, current `main` checkout.
- [x] Confirm a fresh successful `Release macOS Canary` on `macos-26` after the final release changes.
- [x] Dispatch and pass `Release Publish Auth Check` with `write_probe=true` under the maintainer's release authorization.
- [x] Create and push annotated tag `v0.18.2` from the verified `main` revision.
- [x] Complete signed/notarized GUI and direct CLI artifact publication.
- [x] Merge generated metadata PRs if required and complete post-publication verification and drift guards.

## v0.18.1 (Corrective Release Gate, Completed)

### Required Remediation

- [x] Resolve the critical migration `17` collision identified after the `v0.18.0` withdrawal.
- [x] Detect the known development migration identity before applying dependent released migrations.
- [x] Verify the fix against a temporary backup of the affected production database without modifying the live store.
- [x] Verify fresh database creation and migration from the `v0.17.12` schema.
- [x] Verify safe recovery for databases already touched by `v0.18.0`.
- [x] Verify migration atomicity, idempotency, failure rollback, and data preservation.
- [x] Verify affected-database startup through a CLI subprocess and Refresh acceptance through an isolated FFI process.
- [x] Verify Refresh and CLI installation through the final signed app/XPC service boundary against recovered database copies.
- [x] Run targeted SQLite, doctor/repair, Rust workspace, macOS UI/service, documentation, web, and release-contract checks after rebasing onto withdrawal-complete `main`.
- [x] Run the non-mutating `v0.18.1` release rehearsal.
- [x] Merge the hotfix through the protected branch path and run rehearsal/preflight from a clean, current `main` checkout.
- [x] Run a fresh successful `Release macOS Canary` on `macos-26`.
- [x] Obtain explicit maintainer approval before tag creation and publication.
- [x] Complete the full rehearsal, preflight, canary, signing, notarization, publication, and post-publication verification gates.

## v0.18.0 (Withdrawn)

`v0.18.0` was published on 2026-08-03 and withdrawn after discovery of a critical SQLite migration defect. The immutable tag is retained for auditability; `v0.18.1` is the corrective public stable successor.

### Scope

- [x] SQLite is canonical for doctor scan generations, finding lifecycle, repair knowledge, local overrides, provenance, and repair history.
- [x] Bundled and imported knowledge uses canonical integrity validation and maps only to allowlisted typed repair actions.
- [x] CLI and FFI/XPC doctor and repair paths persist scans, resolve effective knowledge, record execution history, and verify repair outcomes.
- [x] Internal security advisory records and cache persistence provide deterministic normalization, validation, ordering, freshness, and pruning without a public feature surface.
- [x] Provider fetchers, scheduler wiring, doctor advisory findings, and public advisory CLI/GUI surfaces remain explicitly deferred beyond `0.18.x`.

### Required Validation

- [x] Doctor/repair and advisory persistence suites cover deterministic lifecycle, hostile imports, stale completion, normalization, and transactional rejection behavior.
- [x] Run the full repository quality gate, documentation sync, Sparkle checklist, and non-mutating release rehearsal from the release-preparation branch.
- [x] Merge release preparation through `dev` and the protected `dev` to `main` path.
- [x] Run rehearsal and preflight from a clean, current `main` checkout.
- [x] Run a fresh successful `Release macOS Canary` on `macos-26` after the final release changes.
- [x] Dispatch and pass `Release Publish Auth Check` with `write_probe=true` after explicit maintainer approval.
- [x] Obtain explicit maintainer approval before tag creation and publication.
- [x] Complete release publication and post-publication metadata verification.

## v0.17.12 (Stable Patch Release Gate, Completed)

### Scope

- [x] Bulk and scoped upgrade authority sequencing is backend-owned through Rust/FFI/XPC rather than SwiftUI polling.
- [x] Individual upgrade task records, `plan_step_id` labels, terminal output, and per-task cancellation remain visible through existing diagnostics surfaces.
- [x] Scoped-workflow cancellation blocks future authority-phase submission.
- [x] Website, appcast, and CLI update metadata identify the published `v0.17.12` stable release.

### Required Validation

- [x] Verify authoritative tasks reach terminal state before standard or guarded bulk-upgrade tasks are submitted.
- [x] Verify scoped-workflow cancellation prevents later-phase scheduling and individual in-flight tasks remain cancellable.
- [x] Run the required preflight, rehearsal, canary, publish-auth, publication, and post-publication verification gates from `docs/operations/RELEASE_FLOW.md`.

## v0.17.10 (Stable Patch Release Gate, Completed)

- Published on `main` on 2026-03-11 with tag, release metadata, and post-publication verification complete.
- Historical execution details remain preserved in git history, published release metadata, and the `v0.17.10` changelog entry.

## v0.17.9 (Stable Patch Release Gate, Completed)

- Completed and published on `main` on 2026-03-11.
- Historical execution details remain preserved in git history, published release metadata, and the `v0.17.9` changelog entry.

## Historical RC and Prior-Release Checklists (Archive)

The sections below are retained for traceability. Unchecked items in archived sections are historical records and are non-blocking for the current active release gate.

## v0.17.0-rc.5 (Post-rc.4 Remediation + Auth/Responsiveness Hardening RC)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.17.0-rc.5` release-candidate notes for post-`rc.4` remediation/hardening work.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect `rc.5` release execution status.
- [x] Website changelog includes both `0.17.0-rc.4` and `0.17.0-rc.5` entries in release order.
- [x] Third-party dependency baseline was re-audited and release context updated in `docs/legal/THIRD_PARTY_LICENSES.md` (audit date `2026-02-22`).

### Versioning
- [x] Workspace version bumped to `0.17.0-rc.5` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.17.0-rc.5` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.17.0-rc.5` (`apps/macos-ui/Generated/HelmVersion.swift`, `apps/macos-ui/Generated/HelmVersion.xcconfig`).

### Stabilization Scope
- [x] Packages now include localized `Pinned` filtering with upgradable exclusion behavior.
- [x] Popover package search rows expose quick icon actions (install/uninstall/update/pin).
- [x] Failed-task inline command/output expansion and manager inspector error diagnostics (`View Diagnostics`) are available.
- [x] Privileged operations marked `requires_elevation` execute via structured `sudo -A` with askpass support.
- [x] Control-center/popover responsiveness hardening delivered (section-scoped derived-state snapshots, adaptive polling cadence, and lazy-stack usage for scroll-heavy views).

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).
- [x] Third-party license audit commands complete without runtime-license scope regressions (`cargo metadata`, `cargo tree`, website lockfile license scan).

### Branch and Tag
- [x] Commit release-prep deltas on `dev`.
- [x] Create annotated RC tag from `dev` lineage: `git tag -a v0.17.0-rc.5 -m "Helm v0.17.0-rc.5"`.
- [x] Push commit + RC tag (`git push origin dev` and `git push origin v0.17.0-rc.5`).
- [x] Publish GitHub pre-release for `v0.17.0-rc.5`.

## v0.17.0-rc.1 (Diagnostics & Logging RC)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.17.0-rc.1` release-candidate notes for all diagnostics/logging slices.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect merged `0.17.x` delivery state and RC prep status.
- [x] `docs/RELEASE_CHECKLIST.md` includes `v0.17.0-rc.1` release tasks.

### Versioning
- [x] Workspace version bumped to `0.17.0-rc.1` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.17.0-rc.1` in `core/rust/Cargo.lock`.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Branch and Tag
- [x] Open release-prep PR into `dev` and complete CI checks.
- [x] Merge release-prep PR into `dev`.
- [x] Create annotated RC tag from `dev` lineage: `git tag -a v0.17.0-rc.1 -m "Helm v0.17.0-rc.1"`.
- [x] Push RC tag: `git push origin v0.17.0-rc.1`.

## v0.17.0-rc.2 (Updater Install/Version Label Hotfix RC)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.17.0-rc.2` release-candidate notes for updater install + appcast version-label fixes.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect `rc.2` updater hardening status.
- [x] Website changelog includes `0.17.0-rc.2` notes.

### Versioning
- [x] Workspace version bumped to `0.17.0-rc.2` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.17.0-rc.2` in `core/rust/Cargo.lock`.

### Updater/Sparkle Hardening
- [x] App entitlements include Sparkle installer/status mach-lookup exceptions and shared-preference exception in both debug/release profiles.
- [x] App metadata enables Sparkle installer launcher service (`SUEnableInstallerLauncherService=true`).
- [x] Appcast generation supports explicit prerelease display version and writes that value into `sparkle:shortVersionString`.
- [x] Release workflow passes tag-derived display version into appcast generation.
- [x] Release DMG verification enforces Sparkle installer launcher + entitlement requirements.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Branch and Tag
- [x] Open release-prep PR into `dev` and complete CI checks.
- [x] Merge release-prep PR into `dev`.
- [x] Create annotated RC tag from `dev` lineage: `git tag -a v0.17.0-rc.2 -m "Helm v0.17.0-rc.2"`.
- [x] Push RC tag: `git push origin v0.17.0-rc.2`.

## v0.17.0-rc.4 (Post-rc.3 Interaction/Priority Stabilization RC)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.17.0-rc.4` release-candidate notes for post-`rc.3` interaction/prioritization stabilization.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect `rc.4` release execution status.
- [x] Website changelog includes `0.17.0-rc.4` notes.

### Versioning
- [x] Workspace version bumped to `0.17.0-rc.4` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.17.0-rc.4` in `core/rust/Cargo.lock`.
- [x] Generated app version artifacts aligned to `0.17.0-rc.4` (`apps/macos-ui/Generated/HelmVersion.swift`, `apps/macos-ui/Generated/HelmVersion.xcconfig`).

### Stabilization Scope
- [x] Launch-at-login setting is available with platform-aware behavior messaging.
- [x] Popover/control-center interaction exclusivity is enforced, with control-center deep links from health/summary cards.
- [x] Manager inspector shows full executable-path discovery and install-method metadata tags.
- [x] Manager priority ordering is authority-aware with drag reorder + restore defaults.
- [x] Popover outside-click dismissal ignores pointer movement and responds to click events only.
- [x] Cursor handling preserves hover affordances for interactive controls.
- [x] Manager status discovery avoids undetected-manager deep scans and caches detected-manager path discovery.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Branch and Tag
- [x] Open release-prep PR into `dev` and complete CI checks.
- [x] Merge release-prep PR into `dev`.
- [x] Create annotated RC tag from `dev` lineage: `git tag -a v0.17.0-rc.4 -m "Helm v0.17.0-rc.4"`.
- [x] Push RC tag: `git push origin v0.17.0-rc.4`.

## v0.17.0-rc.3 (Post-rc.2 Stabilization RC)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.17.0-rc.3` release-candidate notes for post-`rc.2` updater/task/package stabilization.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect `rc.3` release-prep status.
- [x] Website changelog includes `0.17.0-rc.3` notes.

### Versioning
- [x] Workspace version bumped to `0.17.0-rc.3` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.17.0-rc.3` in `core/rust/Cargo.lock`.

### Stabilization Scope
- [x] Sparkle "up to date" messaging preserves prerelease versions in non-App-Store channels.
- [x] Running tasks support single-row expand/collapse details with resolved command + live output.
- [x] Package/search UI consolidates same-name rows across managers while retaining manager-scoped actions.
- [x] Inspector package descriptions render HTML-formatted content with safe-link filtering and readable fallback.
- [x] Inspector side-panel detail text containers maintain full-width leading alignment (no centered narrow content when values are short).
- [x] Updater prerelease eligibility rejects bundle short-version/build metadata mismatches.
- [x] Task-output storage enforces bounded command/output buffering and Hungarian locale coverage includes new task/inspector strings.

### Validation
- [x] Rust tests pass (`cargo test -p helm-core -p helm-ffi --manifest-path core/rust/Cargo.toml`).
- [x] `HelmTests` pass (`xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test`).
- [x] Locale checks pass (`apps/macos-ui/scripts/check_locale_integrity.sh` and `apps/macos-ui/scripts/check_locale_lengths.sh`).

### Branch and Tag
- [x] Open release-prep PR into `dev` and complete CI checks.
- [x] Merge release-prep PR into `dev`.
- [x] Create annotated RC tag from `dev` lineage: `git tag -a v0.17.0-rc.3 -m "Helm v0.17.0-rc.3"`.
- [x] Push RC tag: `git push origin v0.17.0-rc.3`.

## v0.16.2 (Sparkle Connectivity + macOS 11 Alignment)

### Scope and Documentation
- [x] `CHANGELOG.md` includes `0.16.2` patch notes (Sparkle connectivity + deployment target alignment).
- [x] README release status references `v0.16.2`.
- [x] Website changelog includes `0.16.2` release notes.

### Versioning
- [x] Workspace version bumped to `0.16.2` in `core/rust/Cargo.toml`.
- [x] Rust lockfile local package versions aligned to `0.16.2` in `core/rust/Cargo.lock`.

### Sparkle / Updater Validation
- [x] Helm app debug/release entitlements include `com.apple.security.network.client`.
- [x] Sparkle runtime clears persisted feed overrides and logs resolved feed URL for manual checks.
- [x] Release workflow validates `HELM_SPARKLE_FEED_URL` is `https://` and DNS-resolvable before build/signing.
- [x] Xcode build settings report `MACOSX_DEPLOYMENT_TARGET = 11.0` for Helm target.

## v0.16.1 (Documentation-Only)

### Scope and Documentation
- [x] `README.md` updated for `macOS 11+ (Big Sur)` minimum and staged security rollout summary.
- [x] `docs/ROADMAP.md` and website roadmap updated for milestone restructuring (`0.18.x` groundwork, `0.19.x` hardening, `1.4.x` Shared Brain insertion, `1.4.x+` forward shift).
- [x] `docs/ARCHITECTURE.md` includes staged security model and explicit Security Advisory vs Shared Brain separation.
- [x] `docs/DECISIONS.md` includes ADR for platform baseline + milestone restructure + system separation.
- [x] `docs/CURRENT_STATE.md` and `docs/NEXT_STEPS.md` reflect `v0.16.1` docs/planning state and revised milestone sequence.
- [x] `CHANGELOG.md` and website changelog include `0.16.1` documentation-only release notes.

### Validation
- [x] Documentation consistency sweep confirms no pre-`1.4.x` placement of Shared Brain infrastructure.
- [x] Documentation consistency sweep confirms no centralized-backend dependency claims for `1.3.x` Security Advisory System.

## v0.16.0 (In Progress)

### Sparkle Feed and Distribution Safety
- [x] `HELM_SPARKLE_FEED_URL`, `HELM_SPARKLE_PUBLIC_ED_KEY`, and `HELM_SPARKLE_PRIVATE_ED_KEY` secrets are present for release workflow.
- [x] Helm app entitlements include `com.apple.security.network.client` for Sparkle feed access.
- [ ] `HELM_SPARKLE_FEED_URL` points to a resolvable, stable HTTPS feed endpoint.
  - Recommended fallback while custom-domain DNS is unstable: `https://github.com/jasoncavinder/Helm/releases/latest/download/appcast.xml`.
- [ ] If using `https://helmapp.dev/updates/appcast.xml`, confirm public DNS resolution for `helmapp.dev` before release cut.
- [x] Sparkle feed endpoint is published at `web/public/updates/appcast.xml` (or `HELM_SPARKLE_FEED_URL` points to the hosted equivalent).
- [x] Release workflow generates and uploads `appcast.xml` alongside DMG artifacts.
- [x] Release workflow publishes generated `appcast.xml` into `web/public/updates/appcast.xml` on `main` (or auto-opens fallback PR if direct push is blocked).
- [x] Release workflow generates and uploads per-tag website release notes HTML from `CHANGELOG.md` at `build/release-assets/release-notes/<tag>.html`.
- [x] Release workflow publishes per-tag website release notes to `web/public/updates/release-notes/<tag>.html` and appcast `sparkle:releaseNotesLink` points to the hosted URL.
- [x] Runtime self-update is blocked for package-manager-managed installs (Homebrew Cask receipt detection + Homebrew/MacPorts path heuristics) and enabled for eligible direct-channel DMG installs.
- [x] Generated `CURRENT_PROJECT_VERSION` is monotonic for Sparkle version ordering (semver-derived numeric build number).
- [x] Sparkle package reference remains pinned to `2.8.1` in `apps/macos-ui/Helm.xcodeproj/project.pbxproj` for macOS 11+ compatibility.
- [x] Appcast policy validation passes in release workflow (`apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh`), ensuring full-installer-only feed output (no deltas).
- [x] Delta update policy (`full installer only` for `0.16.x`) is documented in `docs/DECISIONS.md` and reflected in release automation.

### Installer/Updater Recovery Validation
- [x] Execute interruption/recovery validation runbook: `docs/validation/v0.16.0-rc.9-installer-recovery.md`.
- [x] Confirm workflow rerun behavior for same tag remains idempotent (artifact clobber + deterministic appcast publish target).
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

## v0.15.0 (Completed)

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
- [x] Generated app version artifacts aligned to `0.15.0` by build flow (build-generated, not tracked in git).

### Branch and Tag
- [x] Open PR with final prep deltas into `dev` (for verified commit provenance).
- [x] Merge prep PR into `dev`.
- [x] Open PR from `dev` to `main` for `v0.15.0` and complete CI checks.
- [x] Merge `dev` into `main` for release.
- [x] Create annotated tag from `main`: `git tag -a v0.15.0 -m "Helm v0.15.0"`.
- [x] Push tag: `git push origin v0.15.0`.

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
