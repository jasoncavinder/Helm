# Helm Pre-1.0 Release Readiness Audit

Date: 2026-02-25  
Branch audited: `chore/pre1-quality-audit` (`7475d5d`)  
Reference branch for published release metadata: `origin/main`

## Scope

This audit covers:
- CI coverage: what runs, what is missing, and what is flaky/unstable
- Unit/integration/e2e coverage depth for release-critical paths
- Release integrity: versioning, artifact generation, signing/notarization, Sparkle/appcast publication
- Reproducibility controls: lockfiles, toolchain pinning, deterministic build constraints
- Crash/error reporting posture with privacy constraints

Decision baseline used by this readiness audit:
- `DEC-001` through `DEC-005` are resolved (see `docs/audits/quality-audit-decisions.md`).
- Release gating reflects resolved policies for coordinator transport, timeout caps, SHA pinning phases, metadata-branch truth, and diagnostics redaction defaults.

## Evidence Collected

- Workflow/config inspection:
  - `.github/workflows/*.yml`
  - `scripts/release/preflight.sh`
  - `scripts/release/runbook.sh`
  - `docs/RELEASE_CHECKLIST.md`
  - `docs/VERSIONING.md`
- Ruleset inspection:
  - `gh api repos/jasoncavinder/Helm/rulesets/13089765`
  - `gh api repos/jasoncavinder/Helm/rulesets/13089773`
  - `gh api repos/jasoncavinder/Helm/rulesets/13089779`
  - `gh api repos/jasoncavinder/Helm/rulesets/13089780`
- CI run history:
  - `gh run list --limit 200 --json ...`
  - targeted run views/logs for failures (`22376523135`, `22376776163`, `22353232199`, `22377301909`)
- Test execution:
  - `cargo test --workspace` (one full run showed a timeout failure; subsequent full run passed)
  - `cargo test --workspace -q` (full pass)
  - repeated rustup detection check (`5/5` pass in isolated reruns)
- Release metadata state:
  - `git show origin/main:web/public/updates/appcast.xml`
  - `git show origin/main:web/public/updates/cli/latest.json`
  - `git show origin/main:web/public/updates/cli/latest-rc.json` (missing)

## CI Coverage Audit

### What Runs Today

| Area | Workflow(s) | Trigger(s) | Coverage Summary |
|---|---|---|---|
| Core Rust + macOS build/test | `.github/workflows/ci-test.yml` | PRs to `main`/`dev`, push to `main` | `cargo test --workspace`, `cargo clippy --workspace -D warnings`, Xcode build+test, channel-policy check |
| Formatting gate | `.github/workflows/ci-test.yml` | PRs to `main`/`dev`, push to `main` | `cargo fmt --all -- --check` (added in this audit slice) |
| i18n/localization integrity | `.github/workflows/i18n-lint.yml` | PRs to `main`/`dev`, push to `main` | locale sync, key/placeholder integrity, string overflow checks, hardcoded UI string block |
| Swift lint | `.github/workflows/swiftlint.yml` | PRs to `main`/`dev`, push to `main` | strict SwiftLint run |
| Static analysis/security | `.github/workflows/semgrep.yml`, `.github/workflows/codeql.yml` | Semgrep on PR/push; CodeQL on `main` push + weekly schedule/manual | Semgrep auto ruleset; CodeQL Swift analysis |
| Branch policy/scope | `.github/workflows/policy-gate.yml` | PRs into `main`/`dev`/`docs`/`web` | branch naming/scope, release publish path constraints |
| Docs/Web branch-specific gates | `.github/workflows/docs-checks.yml`, `.github/workflows/web-build.yml` | PR/push on `docs` and `web` | docs structure/merge marker check; web build |
| Release publication + integrity | `release-macos-dmg.yml`, `release-cli-direct.yml`, `release-publish-verify.yml`, `appcast-drift.yml`, `cli-update-drift.yml` | release events, manual dispatch, scheduled drift checks, publish-path pushes on `main` | artifact generation, signing/notarization, metadata publication via PR, drift detection |
| Installer contract checks | `.github/workflows/cli-installer-checks.yml` | path-filtered PR/push + manual | shellcheck + install.sh local manifest smoke with marker schema validation |

### Required Checks in Rulesets

From active branch rulesets:
- `main` (`13089765`) and `dev` (`13089773`) require:
  - `Policy Gate`
  - `Rust Core Tests`
  - `Xcode Build Check`
  - `hardcoded-ui-strings`
  - `Semgrep scan`
  - `Lint Swift`
- `docs` (`13089779`) require:
  - `Policy Gate`
  - `Docs Checks`
- `web` (`13089780`) require:
  - `Policy Gate`
  - `Web Build`

### Missing / Weak CI Coverage

1. No dependency-vulnerability gate for Rust/web lockfiles in PR CI (`cargo-audit`/`cargo-deny`/dependency-review absent).
2. No PR-time execution of release preflight/runbook scripts (`scripts/release/preflight.sh`, `scripts/release/runbook.sh`) to catch operator-flow regressions earlier.
3. No UI automation test workflow (only unit-level Swift tests run via xcodebuild test target).
4. No real-manager e2e matrix for install/update/remove (integration tests are primarily process-fake based in `core/rust/crates/helm-core/tests/end_to_end_*.rs`).
5. Drift workflows are informative but not required checks on release-promotion PRs, so red scheduled/push drift can coexist with otherwise green PR gates.

### Flaky / Unstable Signals

1. Local test flake observed once:
   - `core/rust/crates/helm-core/tests/end_to_end_rustup.rs`
   - `detect_rustup_through_full_orchestration_path` timed out once (`CoreErrorKind::Timeout`), then passed on isolated rerun and 5-loop stress reruns.
2. `Release Publish Verify` transient failures are reproducible timing artifacts, not random flakes:
   - Runs `22376523135` and `22376776163` failed with `appcast=0.17.5 cli_latest=0.17.6` during staged publish-PR merges.
3. `CLI Update Metadata Drift Guard` is currently red by policy mismatch:
   - Runs `22353232199` and `22377301909` failed because prerelease tag exists (`v0.17.0-rc.4`) while `web/public/updates/cli/latest-rc.json` is absent on `origin/main`.

## Test Depth Audit (Unit / Integration / E2E)

### Strong Coverage Areas

1. Rust adapter/parser/unit coverage is broad (`core/rust/crates/helm-core/src/adapters/*.rs`, 300+ tests).
2. Orchestration/runtime behavior has focused integration suites:
   - `orchestration_runtime_queue.rs`
   - `orchestration_adapter_runtime.rs`
   - `orchestration_adapter_execution.rs`
   - `orchestration_sqlite_task_persistence.rs`
3. End-to-end orchestration-path tests exist for key managers using fake executors:
   - Homebrew, mise, rustup, mas, softwareupdate.
4. CLI has meaningful unit-level contract tests for parsing, diagnostics classification, provenance, and self-uninstall policy (`core/rust/crates/helm-cli/src/main.rs`, `provenance.rs`).
5. Swift tests cover update configuration, upgrade-plan logic, localization overflow, and diagnostics redaction (`apps/macos-ui/HelmTests/*.swift`).

### Coverage Gaps on Critical Paths

1. No real-manager e2e for package install/uninstall/upgrade flows across npm/pnpm/yarn/pipx/pip/homebrew in CI.
2. No UI-level end-to-end path tests for update/install/remove interactions.
3. Release workflows are validated operationally but lack isolated contract tests for merge-order edge cases in publish metadata verification.
4. No deterministic stress suite for timeout-sensitive orchestration tests (flake detection / soak).

## Release Integrity Audit

### Controls in Place

1. Versioning and branch model are documented and enforced:
   - `docs/VERSIONING.md`
   - `docs/RELEASE_CHECKLIST.md`
2. Preflight/runbook hardening exists:
   - `scripts/release/preflight.sh`
   - `scripts/release/runbook.sh`
3. DMG release workflow performs:
   - secret preflight
   - Rust + Swift tests
   - signing and nested Sparkle re-signing
   - entitlement/channel verification
   - DMG layout verification
   - notarization + stapling
   - appcast + release-notes generation and publish PR fallback
   - file: `.github/workflows/release-macos-dmg.yml`
4. CLI release workflow performs:
   - universal binary build
   - checksum generation
   - version-to-tag verification
   - release upload
   - CLI metadata publish PR fallback
   - main-branch metadata verification summary
   - file: `.github/workflows/release-cli-direct.yml`
5. Post-publish consistency workflows exist:
   - `.github/workflows/release-publish-verify.yml`
   - `.github/workflows/appcast-drift.yml`
   - `.github/workflows/cli-update-drift.yml`

### Integrity Gaps

1. Prerelease metadata policy is currently inconsistent in production:
   - `origin/main` has stable metadata (`appcast=0.17.6`, `cli/latest.json=0.17.6`) but missing `cli/latest-rc.json` while prerelease tags still exist.
2. Publish-order race causes transient red runs on `Release Publish Verify` when CLI metadata merges before appcast/release notes.
3. `scripts/release/build_unsigned_variant.sh` does not validate `TAG_NAME` format locally (tag/path hardening gap for operator tooling).

## Reproducible Build Audit

### Determinism Controls Present

1. Lockfiles are committed:
   - `core/rust/Cargo.lock`
   - `web/package-lock.json`
   - `apps/macos-ui/Helm.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved`
2. Sparkle dependency pinning is explicit in `Package.resolved` (`2.8.1`).
3. Release workflows pin many critical third-party actions by commit SHA in high-risk paths.

### Remaining Reproducibility Risks

1. Rust toolchain is floating (`stable`) in CI/release workflows, not pinned to exact version.
2. SwiftLint is installed from Homebrew at runtime in CI (`brew install swiftlint`), which is mutable over time.
3. No signed build provenance or SBOM attestation pipeline.
4. Release metadata intentionally embeds wall-clock fields (`published_at`, appcast `pubDate`), so byte-identical artifacts/metadata are not expected today.

## Crash / Error Reporting and Privacy

### Current Failure Detection

1. Local diagnostics surfaces exist in app and CLI:
   - structured diagnostics export and support payload generation
   - files: `apps/macos-ui/Helm/Core/HelmCore+Settings.swift`, `core/rust/crates/helm-cli/src/main.rs`
2. Task/process failure context persistence exists:
   - timeout class, cwd, exit context, program/path snippets
   - files: `core/rust/crates/helm-core/src/execution/task_output_store.rs`, `tokio_process.rs`
3. Privacy posture is local-first with no shared-backend telemetry in current releases:
   - `docs/ARCHITECTURE.md` section 6.3
   - `README.md` Shared Brain direction note
4. Support export redaction exists for home paths, user paths, email, and GitHub token patterns:
   - `apps/macos-ui/Helm/Core/SupportRedactor.swift`

### Remaining Observability Gaps

1. No automated crash reporting/aggregation pipeline (operator relies on local diagnostics export and CI/release signals).
2. Raw stdout/stderr persistence redaction is incomplete at core task-output storage level (privacy hardening still needed for secret-like output).

## 1.0 Exit Criteria Checklist

### Governance and CI

- [x] Branch rulesets active for `main`/`dev`/`docs`/`web` with expected required checks.
- [x] Main release publish bypass uses pull-request-only mode on `main` ruleset (`RepositoryRole` fallback).
- [x] Core PR gates include test/lint/static-analysis + policy checks.
- [ ] Complete `BUILD-001` phase 1 immutable SHA pinning for release + security workflows (and enforce SHA pinning for new workflows).
- [ ] Drift workflows (`CLI Update Metadata Drift Guard`, `Release Publish Verify`) are consistently green under normal release operations.
- [ ] Add dependency vulnerability gate(s) to PR/scheduled CI.

### Test Depth and Reliability

- [x] Rust workspace tests pass on current branch snapshot.
- [ ] Stabilize/soak-test timeout-sensitive orchestration tests (rustup detection timeout flake observed once).
- [ ] Add real-manager install/update/remove e2e smoke coverage for top managers.
- [ ] Add UI automation smoke for update/install/remove critical user paths.

### Release Integrity

- [x] Preflight + runbook operator flows exist and are documented.
- [x] Release workflows enforce signing/notarization/Sparkle policy and publish fallback PR paths.
- [ ] Branch-aware metadata truth policy is enforced (`main`/release publish truth; `dev` non-publish integration branch).
- [ ] Resolve prerelease metadata policy mismatch (`latest-rc.json` absent while prerelease tags exist).
- [ ] Eliminate transient red state in publish verification when metadata PRs merge in different order.

### Reproducibility and Supply Chain

- [x] Rust/web/Swift package lockfiles are present and versioned.
- [ ] Pin toolchain/tooling versions more tightly in CI/release (Rust toolchain, SwiftLint provisioning).
- [ ] Add provenance/SBOM generation or explicit 1.0 deferral decision.

### Field Diagnostics and Privacy

- [x] Structured diagnostics export exists and includes redaction.
- [x] Local-first privacy posture is documented.
- [ ] Define 1.0 crash reporting strategy (explicitly none vs opt-in local-to-remote channel).
- [ ] Redact sensitive tokens/secrets from persisted task stdout/stderr before long-term storage/display.

## Readiness Verdict

Status: **Not yet release-ready for 1.0**.

Primary blockers are now policy/coverage hardening rather than foundational release mechanics: prerelease metadata drift policy, dependency/supply-chain CI gates, stronger real-manager e2e depth, and explicit privacy-safe crash/failure aggregation strategy for post-release operations.
