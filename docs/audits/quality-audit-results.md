# Helm Pre-1.0 Quality Audit Results

Date: 2026-02-25  
Branch: `chore/pre1-quality-audit`

## Scope

Audit goals:
- code quality
- security
- reliability
- UX/CLI edge cases
- build/release resilience

Non-goals:
- feature expansion
- architecture redesign

## A) Repo Orientation (Key Subsystems)

- Core engine and orchestration:
  - `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs`
  - `core/rust/crates/helm-core/src/orchestration/adapter_execution.rs`
  - `core/rust/crates/helm-core/src/orchestration/runtime_queue.rs`
- Manager adapters and process surfaces:
  - `core/rust/crates/helm-core/src/adapters/mod.rs`
  - `core/rust/crates/helm-core/src/adapters/*_process.rs`
- Privileged operations/process execution:
  - `core/rust/crates/helm-core/src/execution/mod.rs`
  - `core/rust/crates/helm-core/src/execution/tokio_process.rs`
- Persistence and migrations:
  - `core/rust/crates/helm-core/src/sqlite/store.rs`
  - `core/rust/crates/helm-core/src/sqlite/migrations.rs`
- Service/UI boundary and FFI:
  - `apps/macos-ui/HelmService/Sources/HelmServiceDelegate.swift`
  - `apps/macos-ui/HelmService/Sources/HelmService.swift`
  - `core/rust/crates/helm-ffi/src/lib.rs`
- CLI coordinator/network update path:
  - `core/rust/crates/helm-cli/src/main.rs`
  - `core/rust/crates/helm-cli/src/provenance.rs`
- Release/build policy:
  - `.github/workflows/*.yml`
  - `scripts/release/preflight.sh`
  - `scripts/release/runbook.sh`

## B) Validation Evidence Run During Audit

- `cargo test --workspace` (in `core/rust`): passed.
  - `helm-cli`: 55 passed
  - `helm-core`: 311 passed
  - `helm-ffi`: 25 passed
  - integration suites (orchestration, sqlite, search, process executor): passed
- `swiftlint lint --strict --reporter github-actions-logging`: passed (0 violations).
- `apps/macos-ui/scripts/check_locale_integrity.sh`: passed.
- `apps/macos-ui/scripts/check_locale_lengths.sh`: passed (no high-risk overflow candidates).
- `xcodebuild -project Helm.xcodeproj -scheme Helm -destination 'platform=macOS' test -quiet`:
  - failed inside sandbox (CoreSimulator/XPC error)
  - passed when rerun with escalated permissions

## C) Threat Model Lite

Primary trust boundaries and inputs:
- External manager process outputs are untrusted and parser-facing.
- CLI/network update metadata is untrusted until validated.
- Local IPC/coordinator files in temp-dir are local-attack surface.
- XPC callers are untrusted until code-signing checks pass.
- User package/manager input is untrusted.

Current handling (observed):
- package identifiers are validated against empty/whitespace/option-like values (`adapters/mod.rs:211-258`).
- XPC connection uses team-ID signature validation (`HelmServiceDelegate.swift:24-50`).
- self-update validates checksums and blocks symlink target replacement (`helm-cli/src/main.rs:4627-4717`).

## D) Failure Mode Sweep

Checked/inspected cases:
- process spawn failures and timeout classes:
  - captured with structured attribution (`tokio_process.rs:61-78`, `task_output_store.rs:17-31`)
- stale coordinator state:
  - timeout-triggered stale-state recovery exists (`helm-cli/src/main.rs:6257-6310`)
- cancellation and concurrent manager execution behavior:
  - per-manager serialization + cancellation enforcement in runtime queue (`runtime_queue.rs:107-164`, `179-243`)
- release metadata drift:
  - dedicated guards exist, but working-tree artifacts are inconsistent with docs in current snapshot

## E) Edge-Case Sweep

Checked/inspected cases:
- CLI global flag combinations (`--wait` vs `--detach`, machine mode): covered by tests in `helm-cli` test suite.
- non-interactive/TTY mode split: covered by `parse_args_no_args_*` tests.
- manager eligibility edge cases (system RubyGems/pip): covered by tests and policy checks.
- missing dependencies and constrained PATH behavior: execution-layer override logic exists and is tested.

---

## Findings

### Security

#### SEC-01: Coordinator IPC state files lacked explicit private permissions before this audit patch
- Severity: High
- Impact: Local users/processes on shared machines could potentially read or tamper with coordinator request/response payloads if directory/file permissions are too broad.
- Reproduction / reasoning:
  - coordinator state roots are under `TMPDIR` (`helm-cli/src/main.rs:5965-5976`, `helm-ffi/src/lib.rs:438-448`).
  - pre-patch code created directories/files without explicit permission hardening (`helm-cli/src/main.rs:6007-6032`, `6180-6202`; `helm-ffi/src/lib.rs:479-505`, `2236-2258`).
- Code pointers:
  - `core/rust/crates/helm-cli/src/main.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Recommended fix:
  - enforce `0700` on state directories and `0600` on request/response files.
  - status in this branch: implemented.

#### SEC-02: Coordinator request IDs are predictable and transport is unauthenticated
- Severity: Medium
- Impact: Predictable request IDs (`pid + timestamp + counter`) plus file-based IPC can make spoofed request/response races easier for local adversaries.
- Reproduction / reasoning:
  - request IDs are generated deterministically (`helm-cli/src/main.rs:5998-6005`, `helm-ffi/src/lib.rs:470-477`).
  - no signature/MAC/owner verification for payloads before handling (`helm-ffi/src/lib.rs:2213-2230`, `helm-cli/src/main.rs:6350-6363`).
- Code pointers:
  - `core/rust/crates/helm-cli/src/main.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Recommended fix:
  - add per-session random nonce capability (or migrate to a local authenticated IPC primitive) and verify request origin.

#### SEC-03: Several CI workflows still use floating action tags
- Severity: Medium
- Impact: Mutable action tags increase supply-chain risk compared with commit-SHA pinning.
- Reproduction / reasoning:
  - multiple workflows still use `@v*` refs (for example: `ci-test.yml`, `codeql.yml`, `semgrep.yml`, `web-build.yml`, `docs-checks.yml`, `swiftlint.yml`, `i18n-lint.yml`, `appcast-drift.yml`).
- Code pointers:
  - `.github/workflows/ci-test.yml:21,35,55`
  - `.github/workflows/codeql.yml:21,24,45`
  - `.github/workflows/semgrep.yml:18,21`
  - `.github/workflows/web-build.yml:21,24`
  - `.github/workflows/docs-checks.yml:18`
  - `.github/workflows/swiftlint.yml:19`
  - `.github/workflows/i18n-lint.yml:17`
  - `.github/workflows/appcast-drift.yml:22`
- Recommended fix:
  - pin all third-party actions to immutable SHAs and keep a rotation/update policy.

### Correctness

#### COR-01: `submit_refresh_request_response` hard-caps wait at 60s
- Severity: High
- Impact: Refresh/search/detect requests can be marked timed out even when manager timeouts are intentionally longer, causing false failures and unnecessary retries.
- Reproduction / reasoning:
  - orchestration waits with fixed `Duration::from_secs(60)` (`adapter_runtime.rs:387-390`).
  - execution defaults include `Refresh` idle timeout 120s (`execution/mod.rs:421-426`).
  - manager profiles can further tune hard/idle values (`execution/mod.rs:435-452`).
- Code pointers:
  - `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs:371-435`
  - `core/rust/crates/helm-core/src/execution/mod.rs:421-452`
- Recommended fix:
  - derive orchestration wait timeout from effective manager/task timeout profile or remove fixed cap for this path.

#### COR-02: Manager executable/timeout override sync is non-atomic
- Severity: Medium
- Impact: Concurrent task submissions can observe temporarily cleared global override maps and run with wrong executable/timeout profile.
- Reproduction / reasoning:
  - sync path clears global maps then repopulates (`helm-ffi/src/lib.rs:859-885`).
  - map-clearing and repopulation are separate operations over global mutable state (`execution/mod.rs:465-486`).
- Code pointers:
  - `core/rust/crates/helm-ffi/src/lib.rs:859-885,3576-3600`
  - `core/rust/crates/helm-core/src/execution/mod.rs:465-486`
- Recommended fix:
  - apply overrides via atomic swap of prebuilt maps (single lock section) rather than clear-then-set loops.

#### COR-03: Locale default assumptions (`C.UTF-8`) are not portable across all macOS hosts
- Severity: Low
- Impact: noisy warnings and possible subtle locale-dependent behavior in scripts/tooling.
- Reproduction / reasoning:
  - observed repeatedly during local `xcodebuild`/scripts: `setlocale: LC_ALL: cannot change locale (C.UTF-8)`.
- Code pointers:
  - environment defaults in release/check scripts and local shell invocations.
- Recommended fix:
  - standardize on `en_US.UTF-8` fallback handling and avoid hard dependency on `C.UTF-8`.

### Reliability

#### REL-01: Current repo snapshot has stale/missing publish metadata artifacts
- Severity: High
- Impact: updater metadata checks and publish-verify expectations can fail; stable release signals can become inconsistent.
- Reproduction / reasoning:
  - appcast top item currently `0.17.2` (`web/public/updates/appcast.xml:9-16`).
  - `web/public/updates/cli/latest.json` and `latest-rc.json` are missing in this checkout.
  - workflows expect those files (`cli-update-drift.yml:25-31`, `release-publish-verify.yml:15-17,40-48`).
- Code pointers:
  - `web/public/updates/appcast.xml`
  - `.github/workflows/cli-update-drift.yml`
  - `.github/workflows/release-publish-verify.yml`
- Recommended fix:
  - reconcile branch metadata artifacts with current stable (`v0.17.6`) and enforce drift checks on integration branches.

#### REL-02: Coordinator transport still relies on polling and timeout loops
- Severity: Medium
- Impact: transient filesystem delays can cause avoidable timeout failures; behavior is less deterministic under load.
- Reproduction / reasoning:
  - request loop polls every 25ms until timeout (`helm-cli/src/main.rs:6356-6365`, `helm-ffi/src/lib.rs:541-550`).
- Code pointers:
  - `core/rust/crates/helm-cli/src/main.rs:6338-6371`
  - `core/rust/crates/helm-ffi/src/lib.rs:524-557`
- Recommended fix:
  - introduce event-driven signaling or adaptive backoff with jitter and richer timeout diagnostics.

#### REL-03: Task diagnostics captured in core are not fully exposed to GUI/FFI
- Severity: Medium
- Impact: failures are harder to triage from app inspector even though context is already recorded.
- Reproduction / reasoning:
  - core record includes cwd/program/path/error/termination fields (`task_output_store.rs:17-31`).
  - FFI record exports only `command/stdout/stderr` (`helm-ffi/src/lib.rs:3113-3152`).
  - Swift model mirrors the reduced shape (`HelmCore.swift:229-234`).
- Code pointers:
  - `core/rust/crates/helm-core/src/execution/task_output_store.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs`
  - `apps/macos-ui/Helm/Core/HelmCore.swift`
- Recommended fix:
  - expand FFI and UI model to include structured process context + timeout/error codes.

### Performance

#### PERF-01: Coordinator request loop uses fixed short-interval polling
- Severity: Medium
- Impact: unnecessary wakeups/CPU churn during high task throughput or long waits.
- Reproduction / reasoning:
  - 25ms poll interval loops (`helm-cli/src/main.rs:6364`, `helm-ffi/src/lib.rs:549`).
- Code pointers:
  - `core/rust/crates/helm-cli/src/main.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Recommended fix:
  - use adaptive polling or event notification.

#### PERF-02: `is_manager_enabled` repeatedly scans persisted preferences/detections
- Severity: Low
- Impact: repeated full-list scans add avoidable overhead in hot orchestration paths.
- Reproduction / reasoning:
  - each check reads all manager preferences and detections (`adapter_runtime.rs:145-175`).
- Code pointers:
  - `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs:140-179`
- Recommended fix:
  - cache enablement snapshot for request batch scope and invalidate on manager-setting mutations.

### UX/CLI Behavior

#### UX-01: Failure output in app lacks key context for support/debugging
- Severity: Medium
- Impact: users see “failed task” without program path/PATH/timeout class that would make remediation obvious.
- Reproduction / reasoning:
  - context is captured but dropped before UI boundary (`task_output_store.rs:17-31` vs FFI/UI structs).
- Code pointers:
  - `core/rust/crates/helm-core/src/execution/task_output_store.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs:3113-3152`
  - `apps/macos-ui/Helm/Core/HelmCore.swift:229-234`
- Recommended fix:
  - include structured fields in `helm_get_task_output` payload and display in inspector diagnostics.

#### UX-02: Build/test output currently includes repeated locale warnings
- Severity: Low
- Impact: warning noise obscures actionable errors in CI/operator logs.
- Reproduction / reasoning:
  - observed repeatedly in escalated `xcodebuild` test run.
- Code pointers:
  - shell env defaults in scripts and local tool invocation context.
- Recommended fix:
  - normalize locale defaults and suppress known benign warnings where possible.

### Build/Release

#### BR-01: Action pinning is only partial across workflows
- Severity: Medium
- Impact: inconsistent security posture between release and non-release pipelines.
- Reproduction / reasoning:
  - release workflows largely pinned; several other workflows are not.
- Code pointers:
  - pinned examples: `release-cli-direct.yml`, `release-macos-dmg.yml`, `release-publish-verify.yml`
  - unpinned examples listed in `SEC-03`
- Recommended fix:
  - standardize immutable pinning baseline across all workflows.

#### BR-02: Version/publication artifacts are inconsistent with current release docs in this branch snapshot
- Severity: High
- Impact: release confidence and automation checks degrade when docs and published metadata diverge.
- Reproduction / reasoning:
  - docs mark `v0.17.6` stable; appcast and CLI metadata files in working tree do not reflect that state.
- Code pointers:
  - `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`, `README.md`
  - `web/public/updates/appcast.xml`
  - missing `web/public/updates/cli/latest.json`
- Recommended fix:
  - enforce metadata artifact parity in pre-merge checks for branches expected to represent release-ready state.

### Test Gaps

#### TG-01: No explicit regression test for orchestration wait-timeout semantics vs manager timeout profiles
- Severity: Medium
- Impact: fixed-cap timeout regressions can reappear unnoticed.
- Reproduction / reasoning:
  - tests cover retry behavior but not profile-aligned wait bounds (`orchestration_adapter_runtime.rs:571-628`).
- Code pointers:
  - `core/rust/crates/helm-core/tests/orchestration_adapter_runtime.rs`
- Recommended fix:
  - add test that reproduces long-running refresh with profile >60s and validates no premature orchestration timeout.

#### TG-02: No direct tests for coordinator IPC permission hardening
- Severity: Medium
- Impact: future refactors can silently weaken file/directory permissions.
- Reproduction / reasoning:
  - new permission hardening is behavior-critical but currently untested.
- Code pointers:
  - `core/rust/crates/helm-cli/src/main.rs`
  - `core/rust/crates/helm-ffi/src/lib.rs`
- Recommended fix:
  - add unit/integration tests that assert created dirs are `0700` and files are `0600` on unix.

#### TG-03: No end-to-end contract test for task output diagnostics field parity across core->FFI->UI
- Severity: Low
- Impact: diagnostics fields can drift between layers without failing tests.
- Reproduction / reasoning:
  - core records richer schema than FFI/UI currently consume.
- Code pointers:
  - `task_output_store.rs`, `helm-ffi/src/lib.rs`, `HelmCore.swift`
- Recommended fix:
  - add contract test for expected JSON shape and UI decode behavior.

### Documentation Drift

#### DOC-01: Stable version messaging is not aligned with publish metadata in this branch snapshot
- Severity: High
- Impact: operator confusion and invalid audit/release assumptions.
- Reproduction / reasoning:
  - docs/readme indicate `v0.17.6` stable while appcast top item is `0.17.2` and CLI metadata files are absent.
- Code pointers:
  - `README.md:12,26,28`
  - `docs/CURRENT_STATE.md:11-24`
  - `docs/NEXT_STEPS.md:14-25`
  - `web/public/updates/appcast.xml:9-16`
- Recommended fix:
  - align branch metadata artifacts with documented stable baseline or clearly mark branch as intentionally non-publish snapshot.

#### DOC-02: Website global banner still frames `v0.17.6` as “latest beta” while release docs frame it as stable
- Severity: Low
- Impact: mixed messaging for users evaluating release channel maturity.
- Reproduction / reasoning:
  - banner text says “Install the latest beta” for `v0.17.6`.
- Code pointers:
  - `web/src/components/starlight/Banner.astro:2`
  - `README.md:12,26`
- Recommended fix:
  - define one canonical wording for pre-1.0 stable vs beta messaging and apply consistently.

## Implemented Low-Risk Fix During This Audit

Implemented in this branch:
- coordinator IPC permission hardening (CLI + FFI)
  - private directory enforcement (`0700`)
  - private temp JSON file mode (`0600`)

Files changed:
- `core/rust/crates/helm-cli/src/main.rs`
- `core/rust/crates/helm-ffi/src/lib.rs`
