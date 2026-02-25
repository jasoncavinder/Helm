# Remediation Log

## 2026-02-26 — Batch `MNT-005B`, `MNT-005C`, `SEC-003B`

### Scope

- `MNT-005B`: moved coordinator stale-lock and launch-on-demand lifecycle/state-machine helpers into `coordinator_transport.rs` while keeping call-site behavior unchanged.
- `MNT-005C`: documented coordinator transport invariants in `docs/architecture/CLI_COORDINATOR_TRANSPORT_INVARIANTS.md` and linked the doc from CLI coordinator tests via a path-presence assertion.
- `SEC-003B`: added centralized FFI diagnostics redaction for task output and task logs, including strict env-style allowlist semantics (non-allowlisted env assignments are redacted by default).

### Verification

Commands run:

- `cargo fmt --manifest-path core/rust/Cargo.toml --all`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli coordinator_`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi redaction_`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi build_ffi_task_output_record_redacts_sensitive_fields_by_default`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi map_task_log_record_redacts_sensitive_message_payloads`

Manual verification:

- Confirmed `main.rs` now delegates stale-lock cleanup and launch policy checks to `coordinator_transport.rs` helpers, preserving existing coordinator recovery behavior.
- Confirmed coordinator invariants documentation exists at the new architecture doc path and is exercised by CLI test coverage.
- Confirmed `helm_get_task_output` and `helm_list_task_logs` now pass diagnostics fields through one redaction layer; sensitive env-like keys and auth/token/password-style pairs are masked in default payloads.

Remaining risks:

- `SEC-003A` (core diagnostics redaction before persistence) remains pending; this batch only hardens FFI exposure defaults.
- Existing test warning noise from unrelated unused imports (`helm-cli` and `helm-ffi`) remains unchanged in this batch.

## 2026-02-26 — Batch `REL-004A`, `REL-004B`, `MNT-005A`

### Scope

- `MNT-005A`: make coordinator file-IPC transport boundary explicit in CLI by introducing `FileIpcCoordinatorTransport` and routing coordinator state-dir/path resolution through `coordinator_transport.rs` entry points.
- `REL-004A`: switch FFI coordinator bridge policy to local/XPC-preferred mode on macOS by default; keep external file-IPC bridge as policy-controlled behavior.
- `REL-004B`: gate legacy file-IPC compatibility with explicit opt-in (`HELM_LEGACY_FILE_COORDINATOR_IPC`) and add bridge-selection regression coverage.

### Verification

Commands run:

- `cargo fmt --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml --all`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli coordinator_transport::tests::`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli coordinator_ipc_paths_use_private_modes_and_consistent_ownership`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-ffi parse_legacy_file_coordinator_ipc_flag_`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-ffi coordinator_bridge_external_file_ipc_selection_requires_opt_in_and_ready`

Manual verification:

- Confirmed CLI coordinator command-path parsing now uses module-level transport helpers (`parse_internal_coordinator_state_dir_arg`, `FileIpcCoordinatorTransport`) rather than inline state-dir parsing/path construction in `main.rs`.
- Confirmed FFI coordinator bridge now defaults to local mode on macOS unless `HELM_LEGACY_FILE_COORDINATOR_IPC` is explicitly set to a truthy value.
- Confirmed legacy file-IPC compatibility path is still available under explicit opt-in, preserving short-term compatibility while making the default safer.
- Confirmed existing coordinator IPC file-permission tests still pass for compatibility paths.

Remaining risks:

- `REL-004A` required a scope split: `REL-004A1` is delivered in this batch (FFI bridge policy), while CLI daemon transport migration remains pending as `REL-004A2`.
- FFI test builds still emit a pre-existing unused-import warning unrelated to this batch.

## 2026-02-26 — Batch Unblocking Split `SEC-003`, `REL-004`, `MNT-005`

### Scope

- `SEC-003`: split large redaction hardening into `SEC-003A` (core redaction pipeline) and `SEC-003B` (FFI default-redacted exposure/allowlist enforcement).
- `REL-004`: split large coordinator transport hardening into `REL-004A` (XPC-first transport path) and `REL-004B` (bounded/removable file-IPC compatibility path).
- `MNT-005`: split coordinator transport refactor into `MNT-005A/B/C` to keep review slices small and sequencing explicit.

### Verification

Commands run:

- `rg -n "SEC-003|SEC-003A|SEC-003B|REL-004|REL-004A|REL-004B|MNT-005|MNT-005A|MNT-005B|MNT-005C|No open decision blockers" /Users/jasoncavinder/Projects/Helm/docs/audits/remediation-backlog.md`
- `git diff -- /Users/jasoncavinder/Projects/Helm/docs/audits/remediation-backlog.md /Users/jasoncavinder/Projects/Helm/docs/audits/remediation-log.md`

Manual verification:

- Confirmed each split child item has explicit severity/category/fix-type/effort/risk/dependency/acceptance criteria fields.
- Confirmed parent items (`SEC-003`, `REL-004`, `MNT-005`) now describe split-closure conditions rather than unresolved single large deliverables.
- Confirmed decision-blocker section now reflects resolved decision state (`DEC-001..DEC-005` resolved).

Remaining risks:

- This is backlog decomposition only; no runtime hardening has been implemented yet.
- Follow-up implementation batches must execute child items in order to retire the parent risks.

## 2026-02-26 — Batch `TEST-001B`, `MNT-004B`

### Scope

- `TEST-001B`: add a dedicated timeout-sensitive orchestration repeat-run script with explicit pass/fail budget reporting and document it in the critical-path test plan.
- `MNT-004B`: extend shared JSON decode/error handling from settings into additional `HelmCore` fetch decode paths while preserving existing error-attribution keys.

### Verification

Commands run:

- `HELM_TIMEOUT_SENSITIVE_SOAK_RUNS=1 HELM_TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET=2 /Users/jasoncavinder/Projects/Helm/scripts/tests/timeout_sensitive_orchestration_soak.sh`
- `xcodebuild -project /Users/jasoncavinder/Projects/Helm/apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -configuration Debug CODE_SIGN_IDENTITY=- CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO build`

Manual verification:

- Confirmed the new soak driver prints explicit run/target totals and a final `passes/failures/budget` summary and exits based on the configured failure budget.
- Confirmed `HelmCore+Fetching.swift` decode call sites now route through `decodeCorePayload(...)` while retaining existing `source`/`action`/`taskType` attribution (`core.fetching`, per-call decode action keys, matching task classes).
- Confirmed `HelmCore+Settings.swift` remains the shared helper location and still supports settings-specific wrapper behavior.

Remaining risks:

- `TEST-001B` currently provides a documented/manual execution target and is not yet wired as a dedicated CI lane.
- macOS build logs continue to emit local locale warnings (`LC_ALL=C.UTF-8`) but the build succeeds.

## 2026-02-25 — Batch `MNT-001A`, `MNT-001B`, `MNT-001C`

### Scope

- `MNT-001A`: extract coordinator transport/path/polling/process-ownership helpers from `main.rs` into `coordinator_transport.rs` without changing behavior.
- `MNT-001B`: extract JSON/NDJSON envelope construction helpers from `main.rs` into `json_output.rs` while preserving schema contract behavior.
- `MNT-001C`: extract CLI exit-marker parsing and failure-classification/hint helpers from `main.rs` into `cli_errors.rs` with no user-visible behavior changes.

### Verification

Commands run:

- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli coordinator_`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli build_json_payload_lines`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli`

Manual verification:

- Confirmed coordinator helper implementations now live in `core/rust/crates/helm-cli/src/coordinator_transport.rs` with the same polling intervals, lock/ready paths, and `ps` ownership checks used previously.
- Confirmed JSON envelope helper logic now lives in `core/rust/crates/helm-cli/src/json_output.rs` and preserves existing `schema`/`schema_version`/`generated_at`/`data` payload shape and NDJSON array-splitting rules.
- Confirmed exit marker parsing (`__HELM_EXIT_CODE__`) and JSON-error marker handling (`__HELM_JSON_ERROR_EMITTED__`) plus failure classification/hint mapping now live in `core/rust/crates/helm-cli/src/cli_errors.rs`, with existing CLI tests passing unchanged.

Remaining risks:

- This is a pure maintainability extraction, so behavior risk is low; the primary risk is future drift between thin wrappers in `main.rs` and extracted helper modules if new helper paths are added without tests.
- Existing unrelated `helm-cli` test-module unused-import warnings remain and were not changed in this batch.

## 2026-02-25 — Batch `BUILD-005`, `TEST-001A`, `MNT-004A`

### Scope

- `BUILD-005`: add release-provenance manifest generation for CLI and DMG release workflows, upload manifests with release artifacts, and add provenance contract validation to `Release Contract Checks`.
- `TEST-001A`: add repeat/soak coverage with explicit zero-failure budgets for timeout-sensitive orchestration suites (`end_to_end_mise`, `end_to_end_rustup`).
- `MNT-004A`: centralize JSON decode/error handling in `HelmCore+Settings.swift` for shared settings payload decode paths (`listPackageKegPolicies`, `previewUpgradePlan`).

### Verification

Commands run:

- `/Users/jasoncavinder/Projects/Helm/scripts/release/tests/provenance_manifest_contract.sh`
- `ruby -e 'require "yaml"; %w[/Users/jasoncavinder/Projects/Helm/.github/workflows/release-cli-direct.yml /Users/jasoncavinder/Projects/Helm/.github/workflows/release-macos-dmg.yml /Users/jasoncavinder/Projects/Helm/.github/workflows/release-contract-checks.yml].each { |f| YAML.load_file(f); puts "#{f}: ok" }'`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-core --test end_to_end_mise mise_timeout_sensitive_orchestration_soak_budget`
- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-core --test end_to_end_rustup rustup_timeout_sensitive_orchestration_soak_budget`
- `xcodebuild -project /Users/jasoncavinder/Projects/Helm/apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -configuration Debug CODE_SIGN_IDENTITY=- CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO build`

Manual verification:

- Confirmed release workflows now generate deterministic provenance manifests via `scripts/release/generate_provenance_manifest.sh` and include `provenance-<tag>.json` in workflow artifacts and GitHub release uploads.
- Confirmed provenance contract coverage is now part of `release-contract-checks.yml`.
- Confirmed new soak tests encode explicit flake budget constants (`TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET = 0`) and pass for both `mise` and `rustup`.
- Confirmed `HelmCore+Settings.swift` now reuses one decode/error helper while preserving source/action/task attribution keys and existing user-visible behavior.

Remaining risks:

- `BUILD-005` currently provides deterministic manifest provenance; signed attestations/SBOM publication remain future hardening work.
- `TEST-001` parent remains open via follow-up `TEST-001B` (dedicated repeat-run execution lane/reporting target not yet implemented).
- `MNT-004` parent remains open via follow-up `MNT-004B` (helper extraction not yet extended beyond settings extension).

## 2026-02-25 — Batch `PERF-001`, `TEST-004A`, `TEST-004B`

### Scope

- `PERF-001`: reduce fixed coordinator polling churn by switching bootstrap/startup wait loops to bounded adaptive backoff while keeping deterministic timeout semantics.
- `TEST-004A`: strengthen stable publish-verify merge-order contracts to assert deterministic `MATCHING_HEADS` outcomes, including the dual-open-publish-head case.
- `TEST-004B`: add prerelease publish-verify state contract coverage and wire it into CI release contract checks.

### Verification

Commands run:

- `cargo test --manifest-path /Users/jasoncavinder/Projects/Helm/core/rust/Cargo.toml -p helm-cli poll_interval_backoff_is_bounded`
- `/Users/jasoncavinder/Projects/Helm/scripts/release/tests/publish_verify_state_contract.sh`
- `/Users/jasoncavinder/Projects/Helm/scripts/release/tests/publish_verify_prerelease_state_contract.sh`

Manual verification:

- Confirmed coordinator bootstrap/startup waits now use elapsed-time-based interval helpers with bounded ceilings, and readiness timeout remains explicitly bounded.
- Confirmed stable publish-verify contract now asserts `MATCHING_HEADS` for single-head and dual-head pending states.
- Confirmed prerelease publish-verify state script covers synced/pending/mismatch/invalid flows and the new prerelease contract test is invoked in `release-contract-checks.yml`.

Remaining risks:

- Adaptive polling thresholds are conservative defaults; extremely slow/contended environments may still require future tuning.
- Prerelease publish-verify contract currently depends on the existing branch naming convention (`chore/publish-cli-updates-v<rc-tag>-rc`); any naming-policy change must update both script and contract test.

## 2026-02-26 — Batch `COR-007`, `PERF-002`, `MNT-003`

### Scope

- `COR-007`: classify explicit "check your internet connection" failures as `network_offline` and lock behavior with a regression test.
- `PERF-002`: avoid repeated manager preference/detection scans in ordered detect/refresh flows by building and reusing per-phase manager enablement snapshots.
- `MNT-003`: decompose dense detect/refresh orchestration flow into pure helper functions (capability planning, detect-result reduction, detection gate behavior, missing-adapter error shaping) with targeted unit coverage.

### Verification

Commands run:

- `cargo fmt --manifest-path core/rust/Cargo.toml --all`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli classify_failure_class_detects_check_internet_connection_pattern`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core build_refresh_capability_plan_reflects_support_flags`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core build_manager_enablement_map_`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test orchestration_adapter_runtime refresh_all_ordered_recomputes_enablement_after_preference_update`

Manual verification:

- Confirmed offline diagnostic classification now captures the "check your internet connection" phrase and emits `network_offline`.
- Confirmed detect/refresh ordered flows now take a per-phase enablement snapshot and pass it through submit paths, reducing repeated preference/detection lookups during a phase.
- Confirmed helper extraction keeps behavior intact: detect responses still reduce to success/error uniformly, detect-installed=false still short-circuits follow-up list actions, and missing-adapter phase failures still return structured `InvalidInput`.
- Confirmed manager enablement changes between operations are respected (`refresh_all_ordered_recomputes_enablement_after_preference_update`).

Remaining risks:

- Enablement snapshots are phase-scoped, so preference flips mid-phase are not observed until the next phase/operation.
- `PERF-002` improvements reduce lookup churn in hot ordered flows, but do not change lookup behavior for standalone direct submissions.

## 2026-02-26 — Batch `COR-001`, `REL-001`, `BUILD-001`

### Scope

- `COR-001`: replace fixed request-response wait timeout in refresh/search/detect flows with policy-derived timeout bounded by operation-class orchestration caps (`min(policy_timeout, orchestration_cap)`), and add regression coverage.
- `REL-001`: make `CLI Update Metadata Drift Guard` branch-aware so publish-truth checks run only on `main`/`release/*`, and treat prerelease metadata pointer validation as optional unless `latest-rc.json` is present.
- `BUILD-001`: complete phase-1 immutable SHA pinning for release/security workflows and document phased pin policy.

### Verification

Commands run:

- `cargo fmt --manifest-path core/rust/Cargo.toml --all`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core refresh_wait_timeout_`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test orchestration_adapter_runtime submit_refresh_request_response_retries_once_on_timeout`
- `for f in .github/workflows/release-contract-checks.yml .github/workflows/appcast-drift.yml .github/workflows/dependency-security.yml .github/workflows/codeql.yml .github/workflows/semgrep.yml; do rg -n "^\\s*uses:" "$f"; done`
- `rg -n "IS_PUBLISH_TRUTH_BRANCH|main\\|release/\\*|Prerelease metadata file not present|non-publish branch" .github/workflows/cli-update-drift.yml`

Manual verification:

- Confirmed refresh request-response waits now use task-class defaults with manager override support and enforce operation caps (Detection `120s`, Search `180s`, Refresh `300s`) with unit tests for default, below-cap, and above-cap behaviors.
- Confirmed CLI metadata drift guard exits early on non-publish refs and no longer fails when prerelease tags exist but `latest-rc.json` is intentionally absent.
- Confirmed release/security workflows now reference immutable SHAs for third-party actions in phase 1 scope (`release-contract-checks`, `appcast-drift`, `dependency-security`, `codeql`, `semgrep`).
- Confirmed release docs now state branch truth policy and phased SHA pinning expectations.

Remaining risks:

- `COR-001` currently consumes manager hard-timeout overrides when present; if per-operation policy storage is expanded later, the timeout source should be refactored to avoid conflating operation classes with a single manager hard-timeout value.
- `REL-001` keeps prerelease metadata optional by policy; if preview channel publication becomes mandatory later, drift guard behavior and docs will need to tighten accordingly.
- `BUILD-001` phase 2/3 pinning is still pending for non-release/non-security workflows.

## 2026-02-26 — Batch `COR-008B`, `REL-003`, `MNT-002`

### Scope

- `COR-008B`: surface explicit selected-vs-default executable diagnostics in CLI/FFI manager status outputs and add aligned/diverged coverage.
- `REL-003`: add coordinator IPC permission/ownership tests for both CLI and FFI transport paths.
- `MNT-002`: centralize high-frequency FFI service error keys into constants and replace stringly-typed call sites.

### Verification

Commands run:

- `cargo fmt --manifest-path core/rust/Cargo.toml --all`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli selected_executable_differs_from_default_reports_alignment_and_divergence`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli manager_executable_path_diagnostic_reports_expected_states`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli coordinator_ipc_paths_use_private_modes_and_consistent_ownership`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi manager_status_reports_executable_path_divergence_diagnostics`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi manager_status_reports_executable_path_alignment_diagnostics`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-ffi coordinator_ipc_paths_use_private_modes_and_consistent_ownership`

Manual verification:

- Confirmed CLI manager status payloads and human output now expose `selected_executable_differs_from_default` and `executable_path_diagnostic`.
- Confirmed FFI manager status rows include additive executable-path diagnostics and divergence signal for aligned/diverged selected-path scenarios.
- Confirmed coordinator state/request/response/ready files are asserted as private (`0700` dirs, `0600` files) with consistent owner UID in both CLI and FFI tests.
- Confirmed repeated FFI service error keys now route through centralized constants for invalid-input/storage/unsupported/process/internal paths.

Remaining risks:

- Manager status output shape is additive; consumers with strict schema validation must tolerate new fields.
- Coordinator permission tests are `#[cfg(unix)]`; non-Unix environments rely on existing behavior without these assertions.
- Existing unrelated test-only unused-import warnings remain in CLI/FFI test builds.

## 2026-02-26 — Batch `COR-006`, `COR-008`, `TEST-002`

### Scope

- `COR-006`: make adapter process stdout handling tolerant of non-UTF8 bytes on successful process exits so refresh/list flows fail less often under mixed-encoding manager output.
- `COR-008`: extend executable discovery roots for `rtx` ecosystems and lock precedence behavior with targeted tests; split divergence-diagnostics surfacing into follow-up child item `COR-008B`.
- `TEST-002`: add integration lifecycle coverage across authoritative (`asdf`), standard (`npm`), and guarded (`homebrew`) managers, including guarded idempotency assertions.

### Verification

Commands run:

- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core run_and_collect_stdout_uses_lossy_decode_for_non_utf8_stdout`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core run_and_collect_stdout_preserves_process_failure_shape`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core discover_executable_path_prefers_extra_paths_over_path`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core discover_executable_path_finds_rtx_versioned_installs`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core additional_and_versioned_roots_include_rtx_locations`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test manager_lifecycle_matrix`

Manual verification:

- Confirmed `run_and_collect_stdout` now returns lossy-decoded stdout for `ExitCode(0)` responses instead of failing with UTF8 parse errors, while preserving existing manager/task/action attribution for non-zero exits.
- Confirmed detection roots now include `~/.local/share/rtx/shims` and `~/.local/share/rtx/installs`, and precedence tests verify extra search roots win ahead of ambient `PATH`.
- Confirmed new lifecycle integration test matrix covers install/update/remove behavior across authority classes and asserts guarded idempotency behavior for already-installed/already-absent Homebrew formulas.
- Recorded `COR-008` as split:
  - `COR-008A` complete in `80bba06` (discovery roots + precedence tests)
  - `COR-008B` pending for manager-status divergence diagnostics surfacing.

Remaining risks:

- Lossy decode trades strict parse failure for operability; adapters that require strict machine-parse output may still fail later in parser-specific stages if replacement characters break schema/format assumptions.
- `COR-008B` remains open; selected/default executable divergence is not yet surfaced in CLI/FFI manager-status diagnostics outputs.

## 2026-02-25 — Batch `SEC-005`, `TEST-005`, `SEC-004`

### Scope

- `SEC-005`: harden CLI self-update fetch path to disable implicit HTTP redirects and enforce per-hop allowlist URL policy validation before following redirects.
- `TEST-005`: add regression coverage for redirect-host policy rejection, oversized update payload bounds, and channel-managed self-update policy blocking.
- `SEC-004`: confirm existing release unsigned-variant script hardening remains enforced (`e67d20e`) and record finalized backlog reference.

### Verification

Commands run:

- `scripts/release/tests/build_unsigned_variant_contract.sh`
- `bash -n scripts/release/build_unsigned_variant.sh`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli resolve_update_redirect_target_rejects_disallowed_hosts`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli read_update_bytes_with_limit_rejects_oversized_payload`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli self_update_policy_blocks_channel_managed_paths`

Manual verification:

- Confirmed staged self-update transport changes are limited to redirect policy hardening and associated test imports/tests in `core/rust/crates/helm-cli/src/main.rs`.
- Confirmed backlog commit mapping now references concrete hashes:
  - `SEC-004` -> `e67d20e`
  - `SEC-005` -> `d830148`
  - `TEST-005` -> `d830148`

Remaining risks:

- Redirect resolution remains intentionally strict and fail-closed; uncommon redirect patterns from third-party endpoints may now hard-fail with URL-policy/HTTP errors until explicitly supported.
- Redirect hop limit is fixed at `5`; longer redirect chains fail deterministically.

## 2026-02-26 — Batch `COR-003`, `REL-006`, `DOC-004`

### Scope

- `COR-003`: ensure successful install/uninstall mutation responses update cached installed/outdated snapshots without requiring manual refresh.
- `REL-006`: define and document explicit 1.0 crash/error reporting posture with privacy constraints, payload schema expectations, and operational ownership.
- `DOC-004`: finalize backlog status with implemented terminology-contract commit (`1e7b655`).

### Verification

Commands run:

- `cd core/rust && cargo test -p helm-core --test sqlite_store_skeleton --test orchestration_adapter_runtime`
- `cd core/rust && cargo fmt --all`
- `cd core/rust && cargo test -p helm-core --test sqlite_store_skeleton --test orchestration_adapter_runtime` (post-format re-run)

Manual verification:

- Confirmed `persist_adapter_response` now applies install/uninstall mutation outcomes directly to package snapshots (`apply_install_result`, `apply_uninstall_result`), while preserving existing pin/unpin/upgrade behavior.
- Confirmed regression coverage exists at both persistence and orchestration layers:
  - `sqlite_store_skeleton`: install/uninstall snapshot cache behavior
  - `orchestration_adapter_runtime`: mutation success updates cache state without manual refresh
- Confirmed crash/error reporting posture is now explicitly documented as local-only for 1.0 in:
  - `docs/DECISIONS.md` (Decision 032)
  - `docs/ARCHITECTURE.md` (system-boundary policy note)
  - `docs/operations/CRASH_REPORTING_POLICY.md` (policy/schema/privacy/owner)
  - `docs/RELEASE_CHECKLIST.md` (release gate check)
- Confirmed `DOC-004` backlog status now references implemented commit `1e7b655`.

Remaining risks:

- Install mutation versions remain best-effort (`after_version` from adapter response); managers that cannot provide an explicit post-install version may still persist `null` until a later refresh.
- Local-only crash reporting intentionally favors privacy over centralized observability; operational triage still depends on user-initiated diagnostics export.

## 2026-02-25 — Batch `BUILD-004`, `BUILD-002`, `BUILD-003`

### Scope

- `BUILD-004`: pin Rust toolchain versions in CI/release workflows and replace mutable Homebrew SwiftLint install with pinned portable artifact + SHA-256 verification.
- `BUILD-002`: finalize dependency-security remediation status using implemented workflow baseline (`c2580c8`) and re-verify coverage remains active.
- `BUILD-003`: extend release-contract checks with a CI toolchain pin contract script to prevent version-drift regressions.

### Verification

Commands run:

- `scripts/release/tests/ci_toolchain_contract.sh`
- `scripts/release/tests/build_unsigned_variant_contract.sh`
- `scripts/release/tests/publish_verify_state_contract.sh`
- `scripts/release/check_release_line_copy.sh`
- `scripts/release/preflight.sh --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy`
- `scripts/release/runbook.sh prepare --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy`
- `ruby -e 'require "yaml"; %w[.github/workflows/ci-test.yml .github/workflows/dependency-security.yml .github/workflows/swiftlint.yml .github/workflows/release-contract-checks.yml .github/workflows/release-cli-direct.yml .github/workflows/release-macos-dmg.yml .github/workflows/release-all-variants.yml].each { |f| YAML.load_file(f); puts "#{f}: ok" }'`

Manual verification:

- Confirmed all `dtolnay/rust-toolchain` workflow invocations now pin `toolchain: 1.93.1` (no `toolchain: stable` remains).
- Confirmed SwiftLint CI now installs `portable_swiftlint.zip` for pinned `0.59.1` and verifies SHA-256 before execution.
- Confirmed `release-contract-checks.yml` now enforces CI toolchain pin invariants through `scripts/release/tests/ci_toolchain_contract.sh`.

Remaining risks:

- SwiftLint binary distribution URL/asset naming is an external dependency; if upstream release assets change, SwiftLint CI will fail closed until the pinned version/hash is rotated.
- Rust toolchain pin upgrades now require explicit maintenance updates across workflows and the contract script constants.

## 2026-02-25 — Batch `SEC-002`, `COR-005`, `COR-010`

### Scope

- `SEC-002`: remove PATH-resolved `ps` usage from coordinator process-health/ownership probes.
- `COR-005`: serialize coordinator bootstrap/reset startup with a lock file so parallel CLI launches do not race-reset state or spawn duplicate daemons.
- `COR-010`: already completed on this branch in `6f39e3a` (no additional code changes required in this batch).

### Verification

Commands run:

- `cargo fmt --manifest-path core/rust/Cargo.toml --all`
- `cargo test --manifest-path core/rust/Cargo.toml -p helm-cli`

Manual verification:

- Confirmed `process_is_alive` and `coordinator_process_looks_owned` now invoke `PS_COMMAND_PATH` (`/bin/ps`) and no longer rely on PATH lookup.
- Confirmed coordinator startup path now gates `spawn_coordinator_daemon` behind `acquire_coordinator_bootstrap_lock` and re-checks ping while the lock is held.
- Confirmed new regression test `coordinator_bootstrap_lock_serializes_parallel_acquisition` proves second acquisition blocks until the first lock is released.

Remaining risks:

- Lock stale detection is time+PID based; if platform process inspection fails unexpectedly, stale-lock clearing may wait until timeout and return a deterministic bootstrap error.
- Locking prevents startup/reset races, but does not change existing coordinator request timeout policy semantics.

## 2026-02-25 — Batch `SEC-002`, `SEC-005`, `TEST-005`

### Scope

- `SEC-002`: eliminate PATH-resolved `ps` for coordinator helper probes.
- `SEC-005`: enforce redirect-hop URL allowlist validation for CLI self-update HTTP fetch paths.
- `TEST-005`: add regression tests for redirect-policy rejection, payload limits, and channel policy behavior.

### Verification

Commands run:

- `cd core/rust && cargo test -p helm-cli`
- `cd core/rust && cargo fmt`
- `cd core/rust && cargo test -p helm-cli` (post-format re-run)

Manual verification:

- Confirmed coordinator probe functions now invoke `PS_COMMAND_PATH` (`/bin/ps`) instead of PATH-resolved `ps`.
- Confirmed self-update HTTP agent uses redirects disabled and fetch/download paths perform explicit redirect-hop handling.
- Confirmed redirect targets are resolved then validated against HTTPS + allowlist policy before next hop.

Remaining risks:

- Relative redirect resolution is intentionally conservative string-based URL composition; if an endpoint emits uncommon redirect forms, it will fail closed with URL-policy/HTTP error rather than proceed.
- Redirect hop limit is fixed (`5`); environments requiring longer chains will now fail explicitly.

## 2026-02-25 — Batch `SEC-004`, `BUILD-003`, `BUILD-002`

### Scope

- `SEC-004`: harden release unsigned-build helper tag/path handling.
- `BUILD-003`: add CI release preflight/runbook non-destructive contract checks.
- `BUILD-002`: add CI dependency vulnerability checks for PR + scheduled coverage.

### Verification

Commands run:

- `scripts/release/tests/build_unsigned_variant_contract.sh`
- `scripts/release/preflight.sh --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy`
- `scripts/release/runbook.sh prepare --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy`
- `bash -n scripts/release/build_unsigned_variant.sh scripts/release/tests/build_unsigned_variant_contract.sh`
- `ruby -e 'require "yaml"; %w[.github/workflows/release-contract-checks.yml .github/workflows/dependency-security.yml].each { |f| YAML.load_file(f); puts "#{f}: ok" }'`

Manual verification:

- Confirmed invalid release tags are rejected before any build/tool invocation with explicit `TAG_NAME` format errors.
- Confirmed symlinked `OUTPUT_ROOT/<variant>` paths that resolve outside the output root are rejected with explicit containment errors.
- Confirmed non-destructive preflight/runbook contract paths execute successfully from a non-main, dirty branch context.

Remaining risks:

- New GitHub workflows were validated for YAML syntax locally but were not executed in GitHub Actions in this run.
- `cargo audit` output depends on the current RustSec advisory database state and may fail in CI when newly published advisories appear.

## 2026-02-25 — Batch `COR-002`, `TEST-003`, `DOC-001`

### Scope

- `COR-002`: replace clear-then-set manager execution preference sync with atomic map swaps.
- `TEST-003`: add regression tests for `updates run` mixed-success exit-code stability and machine-output envelope contract.
- `DOC-001`: make common CLI errors actionable with explicit next-step hints (`helm help`, `helm managers list`, `helm updates preview`) and add contract tests.

### Verification

Commands run:

- `cd core/rust && cargo test -p helm-core replace_manager_execution_preferences_avoids_empty_read_window`
- `cd core/rust && cargo test -p helm-cli updates_run_`
- `cd core/rust && cargo test -p helm-cli parse_args_unknown_command_includes_help_hint`
- `cd core/rust && cargo test -p helm-cli parse_manager_id_unknown_includes_managers_list_hint`
- `cd core/rust && cargo test -p helm-cli updates_run_requires_yes_message_includes_preview_hint`
- `cd core/rust && cargo fmt --all`

Manual verification:

- Confirmed CLI and FFI preference-sync paths now build full executable/timeout maps and apply them through one runtime swap call instead of clear-then-repopulate loops.
- Confirmed unknown command and unknown manager-id error messages now include actionable guidance.
- Confirmed `updates run` missing-`--yes` error now points operators to `helm updates preview` before rerun.

Remaining risks:

- Atomic swap is process-local and protects in-process readers from empty windows; it does not introduce cross-process coordination semantics (not required for this item).
- Existing unrelated local workspace changes remain outside this batch and were intentionally not modified.

## 2026-02-25 — Batch `SEC-001`, `COR-004`, `COR-010`

### Scope

- `SEC-001`: harden elevated askpass helper trust model and disable env override by default.
- `COR-004`: treat known Homebrew "already absent" uninstall responses as idempotent success.
- `COR-010`: make NDJSON top-level array splitting behavior explicit for nested-array payloads and lock it with tests/docs.

### Verification

Commands run:

- `cd core/rust && cargo fmt --all`
- `cd core/rust && cargo test -p helm-core`
- `cd core/rust && cargo test -p helm-cli build_json_payload_lines_`

Manual verification:

- Confirmed elevated command preparation rejects relative/symlink/untrusted askpass paths and blocks `HELM_SUDO_ASKPASS` unless explicit opt-in (`HELM_SUDO_ASKPASS_ALLOW_OVERRIDE=1`) is present.
- Confirmed Homebrew uninstall now returns mutation success for "No such keg"/already-absent cases while preserving hard failures for unrelated uninstall errors.
- Confirmed NDJSON contract behavior remains top-level-array-only splitting, with nested arrays preserved per item and object-contained arrays kept in a single envelope.

Remaining risks:

- Operators relying on legacy implicit `HELM_SUDO_ASKPASS` override behavior must now explicitly opt in with `HELM_SUDO_ASKPASS_ALLOW_OVERRIDE=1`.
- Homebrew error matching is string-based for known benign "already absent" signatures and may need extension if Homebrew changes phrasing.

## 2026-02-25 — Batch `REL-002`, `REL-005`, `DOC-002`

### Scope

- `REL-002`: make release publish verification deterministic when publish PRs merge in different order.
- `REL-005`: codify required-vs-advisory release check policy and enforce it in preflight ruleset validation.
- `DOC-002`: define a canonical release-line source and enforce cross-surface copy drift checks.

### Verification

Commands run:

- `scripts/release/tests/publish_verify_state_contract.sh`
- `scripts/release/check_release_line_copy.sh`
- `bash -n scripts/release/publish_verify_state.sh scripts/release/tests/publish_verify_state_contract.sh scripts/release/check_release_line_copy.sh scripts/release/preflight.sh`
- `scripts/release/preflight.sh --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy`
- `ruby -e 'require "yaml"; %w[.github/workflows/release-publish-verify.yml .github/workflows/release-contract-checks.yml].each { |f| YAML.load_file(f); puts "#{f}: ok" }'`

Manual verification:

- Confirmed `Release Publish Verify` now classifies stable metadata mismatch as follow-up-required (non-red) when matching open publish branch(es) exist for the target stable version.
- Confirmed the workflow still fails hard on invalid/missing metadata and mismatch states that have no open publish PR counterpart.
- Confirmed release preflight ruleset policy check now fails if advisory release monitors are configured as required branch checks.
- Confirmed canonical release-line contract (`docs/contracts/release-line.json`) is validated against README/banner/docs via `scripts/release/check_release_line_copy.sh`.

Remaining risks:

- Pending-state detection keys off standard publish branch naming (`chore/publish-updates-*`, `chore/publish-cli-updates-*-stable`); manual nonstandard branch names will not be recognized as in-progress publication.
- Release-line drift check currently validates the primary release-copy surfaces and should be expanded if additional canonical version callouts are added later.

## 2026-02-25 — Batch `COR-009`, `DOC-003`, `DOC-004`

### Scope

- `COR-009`: define explicit non-support contract for `helm tasks follow` machine mode (`--json`/`--ndjson`) with stable exit-code marker and regression tests.
- `DOC-003`: add a website content-id guard for guide routes and enforce it in Web Build CI.
- `DOC-004`: formalize terminology contract (`manager`/`adapter`/`task`/`service`) in architecture docs and add checklist enforcement in PR review template.

### Verification

Commands run:

- `cd core/rust && cargo test -p helm-cli tasks_follow_`
- `cd core/rust && cargo fmt --all`
- `cd web && npm run check:content-ids`
- `cd web && npm run build`

Manual verification:

- Confirmed `tasks follow` now returns a marked exit-code contract (`1`) in machine mode with a stable non-support error message.
- Confirmed CLI help text for `tasks`/`tasks follow` now explicitly documents machine-mode non-support and exit-code behavior.
- Confirmed guide id validation passes and Web Build succeeds without duplicate-id warnings for `guides/faq`, `guides/installation`, and `guides/usage`.
- Confirmed docs terminology guidance now explicitly distinguishes user-facing `manager`/`task`/`service` terms from internal `adapter` usage and is reflected in PR checklist review criteria.

Remaining risks:

- The guide-id guard validates source-id uniqueness at content-file level; runtime heading-id collisions inside rendered page content remain a separate class and are still best caught by full website builds.
