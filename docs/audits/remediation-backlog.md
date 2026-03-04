# Consolidated Remediation Backlog

Date: 2026-02-25  
Source inputs: `quality-audit-remediation-checklist.md`, `security-review.md`, `correctness-edgecases.md`, `maintainability.md`, `release-readiness.md`, `test-plan.md`, `docs-ux-drift.md`, `quality-audit-decisions.md`.

Notes:
- This backlog is de-duplicated across sources.
- Findings marked already fixed in source audits are excluded.
- `DEC-*` blockers map to `docs/audits/quality-audit-decisions.md`.

## Batch Status Updates (2026-02-25)

- `SEC-002` — Done (`78594cd`)
- `SEC-005` — Done (`d830148`)
- `TEST-005` — Done (`d830148`)
- `TEST-006A` — Done (`d3745d8`)
- `TEST-006B` — Done (`e3a8287`)
- `TEST-006C` — Done (`PR: TBD`)
- `TEST-006` — Done (`PR: TBD`; split parent closed via `TEST-006A` + `TEST-006B` + `TEST-006C`)
- `TEST-008A` — Done (`1b73dd9`)
- `TEST-008B` — Done (`78cd1b6`)
- `TEST-008C` — Done (`PR: TBD`)
- `TEST-008` — Done (`PR: TBD`; split parent closed via `TEST-008A` + `TEST-008B` + `TEST-008C`)
- `TEST-009A` — Done (`bc24d72`)
- `TEST-009B` — Done (`1067564`)
- `TEST-009C` — Done (`PR: TBD`)
- `TEST-009` — Done (`PR: TBD`; split parent closed via `TEST-009A` + `TEST-009B` + `TEST-009C`)
- `TEST-007` — Split (`6be6a20`; follow-up `TEST-007A` + `TEST-007B` + `TEST-007C` pending)
- `TEST-007A` — Done (`02cf80c`)
- `TEST-007B` — Done (`c86596c`)
- `TEST-007C` — Done (`PR: TBD`)
- `TEST-007` — Done (`PR: TBD`; split parent closed via `TEST-007A` + `TEST-007B` + `TEST-007C`)
- `SEC-004` — Done (`e67d20e`)
- `BUILD-003` — Done (`961c430`)
- `BUILD-002` — Done (`c2580c8`)
- `BUILD-004` — Done (`c131a75`)
- `COR-002` — Done (`39b6b17`)
- `TEST-003` — Done (`ff95ae2`)
- `DOC-001` — Done (`ff95ae2`)
- `SEC-001` — Done (`09b7e71`)
- `COR-004` — Done (`4fc8a86`)
- `COR-010` — Done (`6f39e3a`)
- `COR-005` — Done (`0b0b9dc`)
- `REL-002` — Done (`b385593`)
- `REL-005` — Done (`b8a60a2`)
- `DOC-002` — Done (`f28ec00`)
- `COR-009` — Done (`a8c9e13`)
- `DOC-003` — Done (`17bb923`)
- `DOC-004` — Done (`1e7b655`)
- `COR-003` — Done (`5fdb22d`)
- `REL-006` — Done (`7b914c6`)
- `COR-006` — Done (`aa10cb4`)
- `COR-008` — Split (`80bba06`; follow-up `COR-008B` pending)
- `COR-008A` — Done (`80bba06`)
- `TEST-002` — Done (`PR: TBD`)
- `COR-008B` — Done (`7414d6e`)
- `COR-008` — Done (`7414d6e`; split parent closed via `COR-008A` + `COR-008B`)
- `REL-003` — Done (`7414d6e`)
- `MNT-002` — Done (`7414d6e`)
- `COR-001` — Done (`efdb904`)
- `REL-001` — Done (`67a8ce9`)
- `BUILD-001` — Done (`PR: TBD`)
- `COR-007` — Done (`64802ee`)
- `PERF-002` — Done (`ecaaeba`)
- `MNT-003` — Done (`ecaaeba`)
- `PERF-001` — Done (`1ad330d`)
- `TEST-004` — Split (`f55ba6a`; follow-up `TEST-004A` + `TEST-004B` done)
- `TEST-004A` — Done (`f55ba6a`)
- `TEST-004B` — Done (`f55ba6a`)
- `TEST-004` — Done (`f55ba6a`; split parent closed via `TEST-004A` + `TEST-004B`)
- `BUILD-005` — Done (`8865639`)
- `TEST-001A` — Done (`8887d9a`)
- `TEST-001B` — Done (`f5404c9`)
- `TEST-001` — Done (`f5404c9`; split parent closed via `TEST-001A` + `TEST-001B`)
- `MNT-004A` — Done (`PR: TBD`)
- `MNT-004B` — Done (`e24d3a2`)
- `MNT-004` — Done (`e24d3a2`; split parent closed via `MNT-004A` + `MNT-004B`)
- `MNT-001` — Split (`PR: TBD`; follow-up `MNT-001A` + `MNT-001B` + `MNT-001C` done)
- `MNT-001A` — Done (`263aa37`)
- `MNT-001B` — Done (`77c1657`)
- `MNT-001C` — Done (`PR: TBD`)
- `MNT-001` — Done (`PR: TBD`; split parent closed via `MNT-001A` + `MNT-001B` + `MNT-001C`)
- `SEC-003` — Done (`PR: TBD`; split parent closed via `SEC-003A` + `SEC-003B`)
- `SEC-003A` — Done (`2854558`)
- `SEC-003B` — Done (`3f505be`)
- `REL-004A` — Done (`PR: TBD`; split parent closed via `REL-004A1` + `REL-004A2`)
- `REL-004A1` — Done (`552a6b9`)
- `REL-004A2` — Done (`PR: TBD`)
- `REL-004B` — Done (`552a6b9`)
- `REL-004` — Done (`PR: TBD`; split parent closed via `REL-004A` + `REL-004B`)
- `UX-001` — Done (`4b3f54b`)
- `MNT-005A` — Done (`f4c3d4a`)
- `MNT-005B` — Done (`330f8f4`)
- `MNT-005C` — Done (`330f8f4`)
- `MNT-005` — Done (`330f8f4`; split parent closed via `MNT-005A` + `MNT-005B` + `MNT-005C`)

## Prioritized Backlog

| ID | Severity | Category | Affected area (path/module) | Fix type | Effort | Risk | Dependencies / Blockers | Acceptance criteria |
|---|---|---|---|---|---|---|---|---|
| SEC-001 | High | Security | `core/rust/crates/helm-core/src/execution/tokio_process.rs` | Hardening | M | Med | None | `HELM_SUDO_ASKPASS` override is disallowed by default (or strictly validated); elevated flows use only trusted helper paths; regression tests cover accepted/rejected override paths. |
| SEC-002 | High | Security | `core/rust/crates/helm-cli/src/main.rs` (`process_is_alive`, `coordinator_process_looks_owned`) | Hardening | S | Low | None | Coordinator helper probes use absolute `/bin/ps` (or sanitized fixed `PATH`); test/assertion verifies no PATH-resolved `ps` invocation remains. |
| SEC-003 | High | Security | `core/rust/crates/helm-core/src/execution/{tokio_process.rs,task_output_store.rs}`, `core/rust/crates/helm-ffi/src/lib.rs` | Hardening | L | Med | None | Split into `SEC-003A` + `SEC-003B`; close parent when both child acceptance criteria pass with redaction-by-default behavior preserved. |
| SEC-003A | High | Security | `core/rust/crates/helm-core/src/execution/{tokio_process.rs,task_output_store.rs}` | Hardening | M | Med | None | Centralized output redaction runs before diagnostics persistence/exposure; token/auth-header/API-key patterns are redacted with regression tests for positive and negative cases. |
| SEC-003B | High | Security | `core/rust/crates/helm-ffi/src/lib.rs` diagnostics/output paths | Hardening | S | Med | None | FFI diagnostics responses apply centralized redaction by default, enforce strict env allowlist semantics, and include tests proving sensitive env-like keys never surface in default payloads. |
| SEC-004 | High | Security | `scripts/release/build_unsigned_variant.sh` | Bugfix | S | Low | None | `TAG_NAME` is validated against release regex and canonicalized output paths are enforced under expected artifact roots; traversal attempts fail with explicit errors. |
| SEC-005 | High | Security | `core/rust/crates/helm-cli/src/main.rs` (self-update fetch path) | Hardening | M | Med | None | Redirects are followed only with per-hop allowlist validation (or disabled by policy); final URL host must pass allowlist; integration tests cover redirect-to-disallowed-host rejection. |
| COR-001 | High | Correctness | `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` (`submit_refresh_request_response` wait path) | Bugfix | M | Med | DEC-002 | Wait-timeout behavior matches documented policy (effective manager/global timeout with explicit ceiling decision); no hard-coded 60s cap mismatch; regression tests assert policy alignment. |
| COR-002 | High | Correctness | manager override sync path in CLI/FFI runtime map updates | Bugfix | M | Med | None | Selected executable + timeout profile maps update via atomic swap; no clear-then-set empty window under concurrent submissions; concurrency regression test passes. |
| REL-001 | High | Build/Release | `web/public/updates/{appcast.xml,cli/latest.json,cli/latest-rc.json}`, `.github/workflows/cli-update-drift.yml` | Bugfix | S | Low | DEC-004 | Published stable/prerelease metadata policy is explicit and implemented; drift guard is green for chosen policy state; metadata files exist/align as required. |
| REL-002 | High | Build/Release | `.github/workflows/release-publish-verify.yml`, release publish sequencing | Hardening | M | Med | None | Publish verification is deterministic across PR merge order; no transient red runs during normal publish PR sequencing; added contract tests/simulated order scenarios pass. |
| REL-003 | High | Reliability | coordinator IPC tests (`helm-cli` + `helm-ffi`) | Test | M | Low | None | Tests assert coordinator directories are `0700` and request/response/temp files are `0600`; ownership assumptions are validated for both CLI and FFI paths. |
| REL-004 | High | Security | coordinator request/response transport (file IPC auth) | Hardening | L | Med | None | Split into `REL-004A` + `REL-004B`; close parent when XPC-first transport is default and any file-IPC compatibility path is explicitly bounded/tested. |
| REL-004A | High | Security | coordinator transport (`core/rust/crates/helm-cli/src/coordinator_transport.rs`, FFI coordinator bridge) | Hardening | M | Med | None | Split into `REL-004A1` + `REL-004A2`; close parent when local/XPC-first policy is default on macOS and CLI transport migration scope is completed. |
| REL-004A1 | High | Security | `core/rust/crates/helm-ffi/src/lib.rs` coordinator bridge policy | Hardening | S | Low | None | macOS defaults to local/XPC-backed coordinator bridge; external file-IPC bridge requires explicit opt-in flag; regression tests cover opt-in parsing and external-bridge selection policy. |
| REL-004A2 | High | Security | CLI coordinator daemon transport path (`core/rust/crates/helm-cli/src/main.rs`) | Hardening | M | Med | None | CLI coordinator request/response path is migrated away from default file-IPC polling transport with parity tests for submit/cancel/workflow paths and stale-state recovery behavior. |
| REL-004B | High | Security | legacy file-IPC compatibility path + docs/tests | Hardening | S | Low | None | Legacy file-IPC compatibility path is feature-flagged (`HELM_LEGACY_FILE_COORDINATOR_IPC`) and default-off on macOS; tests assert default local-mode bridge selection and flag-gated legacy behavior. |
| BUILD-001 | High | Build | `.github/workflows/*.yml` | CI | S | Low | DEC-003 | All third-party `uses:` references are pinned to immutable SHAs (or policy-approved exceptions documented); workflow suite remains green post-update. |
| BUILD-002 | High | Security | CI workflows (dependency checks) | CI | S | Low | None | CI runs dependency vulnerability checks (e.g., `cargo-audit`/`cargo-deny` and dependency review) on PR/schedule; failures are visible and actionable. |
| COR-003 | Med | Correctness | `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` | Bugfix | M | Med | None | Successful install/uninstall updates cached state (targeted refresh or equivalent) without requiring manual full refresh; integration tests verify snapshot freshness. |
| COR-004 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/homebrew.rs` (and similar manager paths) | Bugfix | S | Low | None | Repeated uninstall of already-removed package is treated as idempotent success for known benign manager responses; tests cover already-absent cases. |
| COR-005 | Med | Reliability | `core/rust/crates/helm-cli/src/main.rs` coordinator bootstrap | Bugfix | M | Med | None | Concurrent first-start bootstrap is serialized (lock/marker); parallel CLI launches do not race-reset state or spawn conflicting daemons; race test passes. |
| COR-006 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/process_utils.rs` | Bugfix | M | Low | None | Non-UTF8 manager output no longer hard-fails parse path by default; tolerant decoding path preserves operability and diagnostics context; regression tests include invalid UTF8 bytes. |
| COR-007 | Med | Reliability | execution timeout/error classification + diagnostics surfaces | Hardening | M | Med | None | Offline/proxy/captive portal failures are classified distinctly from generic timeouts; user-facing diagnostics include actionable hints; tests cover representative network failure classes. |
| COR-008 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/detect_utils.rs` + manager status surfaces | Hardening | M | Low | None | Split into `COR-008A` + `COR-008B`; close parent when both child acceptance criteria pass. |
| COR-008A | Med | Correctness | `core/rust/crates/helm-core/src/adapters/detect_utils.rs` | Hardening | S | Low | None | Shim/root discovery includes documented `rtx` paths (shim + versioned installs) and path-precedence coverage validates extra-path-first behavior. |
| COR-008B | Med | Correctness | manager executable status diagnostics (CLI/FFI surfaces) | Hardening | M | Med | None | Divergence between selected and detected default executable paths is surfaced as explicit diagnostics in manager status outputs; tests cover aligned vs diverged path reporting. |
| COR-009 | Med | UX/CLI behavior | `core/rust/crates/helm-cli/src/main.rs` (`cmd_tasks_follow`) | Bugfix | M | Low | None | Machine mode (`--json`/`--ndjson`) has defined streaming behavior for follow, or emits explicit documented non-support contract with stable exit code; tests cover contract. |
| COR-010 | Low | UX/CLI behavior | `core/rust/crates/helm-cli/src/main.rs` (`build_json_payload_lines`) | Hardening | S | Low | None | NDJSON contract is explicitly defined per command; nested array payload behavior is deterministic/documented; tests verify expected envelope shape. |
| UX-001 | Med | UX/Docs | task output model across core/FFI/UI (`helm_get_task_output`, Swift `CoreTaskOutputRecord`) | Hardening | M | Med | None | Core task output fields (`cwd`, `program`, `PATH` snippet, exit/termination/error context) are consistently propagated and rendered with backward-compatible decoding; privacy-safe redaction rules are applied. |
| PERF-001 | Med | Performance | coordinator poll loop in CLI workflow wait paths | Hardening | M | Low | None | Coordinator startup/bootstrap wait paths use bounded adaptive poll intervals instead of fixed 50ms sleeps; readiness timeout remains deterministic (`COORDINATOR_DAEMON_READY_TIMEOUT_MS`); poll-interval unit tests verify bounded backoff behavior. |
| PERF-002 | Low | Performance | manager enablement hot paths in orchestration | Refactor | M | Low | None | Manager enablement snapshot is cached with correct invalidation on preference/detection updates; behavior parity tests pass. |
| BUILD-003 | Med | Build/Release | CI workflows + `scripts/release/{preflight.sh,runbook.sh}` | CI | S | Low | None | Release preflight/runbook contract checks are exercised in CI (non-destructive mode) to catch regressions before release tags. |
| BUILD-004 | Med | Build/Reproducibility | CI toolchain provisioning (`rust-toolchain`, SwiftLint provisioning) | CI | M | Low | None | Rust toolchain and SwiftLint versions are pinned/reproducible in CI; version drift is explicit and controlled; docs reflect pin update process. |
| BUILD-005 | Med | Build/Reproducibility | release workflows + provenance contracts | CI | M | Med | None | Release workflows generate deterministic provenance manifests for published artifacts (CLI + DMG/appcast/release-notes), include them in workflow artifacts/release uploads, and CI contract checks validate manifest schema/subject integrity. |
| REL-005 | Med | Build/Release | branch/ruleset required-check policy | CI | S | Low | None | Release drift/publish verify checks have explicit gating policy (required vs advisory) and match branch intent; ruleset/workflow docs are consistent. |
| REL-006 | Med | Reliability/Policy | crash/error reporting strategy docs + telemetry policy | Docs | S | Low | None | 1.0 crash reporting posture is explicitly decided (none vs opt-in channel) and documented with privacy constraints, data schema, and operational owner. |
| TEST-001 | Med | Test | timeout-sensitive orchestration tests (`end_to_end_rustup`, `end_to_end_mise`) | Test | M | Low | None | Split into `TEST-001A` + `TEST-001B`; close parent when both child acceptance criteria pass. |
| TEST-001A | Med | Test | `core/rust/crates/helm-core/tests/end_to_end_{mise,rustup}.rs` | Test | S | Low | None | `mise` and `rustup` orchestration suites include repeat/soak tests with explicit zero-failure budget constants and deterministic assertions. |
| TEST-001B | Med | Test | timeout-sensitive orchestration execution lane/docs | Test | M | Low | None | A repeat-run execution target (CI lane or documented script) runs timeout-sensitive suites multiple times with explicit pass/fail budget reporting. |
| TEST-002 | High | Test | manager lifecycle integration coverage (install/update/remove/pin) | Test | M | Med | None | Integration tests cover at least one manager in authoritative, standard, and guarded classes for install/update/remove behavior including idempotency assertions. |
| TEST-003 | Med | Test | CLI non-interactive contract tests | Test | S | Low | None | Mixed-success `updates run` cases assert stable exit codes and JSON envelope schema; tests run in CI. |
| TEST-004 | Med | Test | workflow contract tests for metadata convergence | Test | M | Low | REL-001, REL-002 | Split into `TEST-004A` + `TEST-004B`; close parent after both child acceptance criteria pass in CI. |
| TEST-004A | Med | Test | `scripts/release/tests/publish_verify_state_contract.sh` | Test | S | Low | REL-002 | Stable publish-verify contract covers deterministic merge-order outcomes, including both publish heads open; `MATCHING_HEADS` assertions are stable across permutations. |
| TEST-004B | Med | Test | `scripts/release/publish_verify_prerelease_state.sh`, `.github/workflows/release-contract-checks.yml` | Test | S | Low | REL-001, REL-002 | Prerelease publish-verify contract script validates synced/pending/mismatch/invalid RC states and is executed by `Release Contract Checks` workflow. |
| TEST-005 | Med | Test/Security | CLI self-update negative-path tests | Test | S | Low | SEC-005 | Tests cover redirect host rejection, oversized payload bounds, and policy-blocked update channels. |
| TEST-006 | Med | Test | real-manager e2e matrix (Homebrew + node/python/ruby) | Test | L | Med | None | Split into `TEST-006A` + `TEST-006B` + `TEST-006C`; close parent when real-manager canary coverage, CI scheduling, and operator runbook criteria are all met. |
| TEST-006A | Med | Test | `scripts/tests/` real-manager smoke contract script | Test | S | Low | None | Add a non-destructive real-manager smoke script that verifies binary availability/version checks for Homebrew, Node, Python, and Ruby managers and emits per-manager pass/fail summary with deterministic exit behavior. |
| TEST-006B | Med | Test | `.github/workflows/` real-manager canary lane | CI | M | Med | TEST-006A | Add a scheduled/manual canary workflow that runs the real-manager smoke contract, stores logs/artifacts, and reports failure independently from fast PR checks. |
| TEST-006C | Low | Docs/Test | `docs/operations/` canary runbook | Docs | S | Low | TEST-006A, TEST-006B | Add runbook guidance for running/interpreting the real-manager canary locally and in CI, including expected failure triage paths and known environment preconditions. |
| TEST-007 | Low | Test/Reliability | guarded OS update validation lanes | Test | L | High | None | Split into `TEST-007A` + `TEST-007B` + `TEST-007C`; close parent when guarded-update VM scenario docs, non-destructive contract checks, and advisory lane/runbook safety controls are all in place. |
| TEST-007A | Low | Test/Docs | guarded OS update scenario/safety matrix docs | Docs | S | Low | None | Define guarded OS update validation scenarios and required safety controls (snapshot/rollback, isolation, mutation guardrails, abort conditions) with explicit operator-visible pass/fail signals. |
| TEST-007B | Low | Test/Reliability | guarded update decision/contract harness (`scripts/tests/` or `scripts/release/tests/`) | Test | M | Med | TEST-007A | Add deterministic non-destructive contract tests for guarded OS update orchestration decisions (allow/deny/confirm/rollback-required states) with machine-readable report output and stable exit semantics. |
| TEST-007C | Low | CI/Test | advisory guarded-update validation lane + runbook wiring | CI | S | Med | TEST-007A, TEST-007B | Add manual/scheduled advisory lane that executes guarded-update contract checks, retains artifacts/logs, and link rollback/triage guidance in ops docs for safe execution outside production hosts. |
| TEST-008 | Low | Test/Reliability | Sparkle updater end-to-end recovery scenarios | Test | L | Med | None | Split into `TEST-008A` + `TEST-008B` + `TEST-008C`; close parent when updater-recovery scenarios are defined, contract-tested, and wired into an advisory lane. |
| TEST-008A | Low | Test/Docs | Sparkle recovery scenario matrix docs | Docs | S | Low | None | Define updater interruption/recovery scenarios (interrupted download, interrupted apply, stale appcast, invalid metadata) with explicit expected outcomes and operator-observable signals. |
| TEST-008B | Low | Test | `scripts/release/tests/` updater recovery contract scripts | Test | M | Med | TEST-008A | Add deterministic contract tests using fixtures/mocks for updater recovery decision logic and appcast/state transitions, with stable assertions suitable for CI. |
| TEST-008C | Low | CI/Test | release/advisory contract workflow wiring | CI | S | Low | TEST-008B | Wire updater recovery contract tests into an advisory workflow path (manual/scheduled) with artifact retention for failure analysis. |
| TEST-009 | Low | Test/Release | staged release rehearsal environment | Test | L | Med | None | Split into `TEST-009A` + `TEST-009B` + `TEST-009C`; close parent when dry-run release rehearsal contract, script execution, and CI invocation are in place. |
| TEST-009A | Low | Release/Docs | rehearsal environment contract docs | Docs | S | Low | None | Define required rehearsal environment inputs (branch/tag conventions, sandbox artifacts, metadata targets) and explicit “no production mutation” guarantees. |
| TEST-009B | Low | Release/Test | `scripts/release/` rehearsal dry-run script | Test | M | Med | TEST-009A | Add a release rehearsal script that executes preflight/prepare/verify in dry-run mode and writes a machine-readable report without mutating production metadata or releases. |
| TEST-009C | Low | CI/Release | release contract-check workflow integration | CI | S | Low | TEST-009B | Add optional/manual workflow invocation for rehearsal dry-run with captured report artifacts and explicit pass/fail contract checks. |
| DOC-001 | Low | UX/Docs | CLI errors in `core/rust/crates/helm-cli/src/main.rs` | Docs | S | Low | None | Common CLI errors include actionable next-step hints (`helm help`, `helm managers list`, `helm updates preview`); contract tests verify messages. |
| DOC-002 | Low | Docs | release-line copy across README/banner/docs | Docs | S | Low | None | One canonical source/value for current release channel copy is used across README/website/release docs; drift checks or lint prevent mismatch. |
| DOC-003 | Low | Docs/Build | Starlight content registry/config for guides | Docs | S | Low | None | Website build no longer emits duplicate id warnings for `guides/faq`, `guides/installation`, `guides/usage`. |
| DOC-004 | Low | Docs/UX | terminology consistency docs (`manager`/`adapter`/`service`) | Docs | S | Low | None | User-facing docs consistently use approved terms; architecture docs define internal terms once; terminology check added to docs review checklist. |
| MNT-001 | Med | Maintainability | `core/rust/crates/helm-cli/src/main.rs` | Refactor | M | Med | None | Split into `MNT-001A` + `MNT-001B` + `MNT-001C`; close parent when all child acceptance criteria pass with `helm-cli` test suite green. |
| MNT-001A | Med | Maintainability | `core/rust/crates/helm-cli/src/{main.rs,coordinator_transport.rs}` | Refactor | S | Low | None | Coordinator transport/path/polling/ownership helper implementations are extracted into `coordinator_transport.rs` with no behavior or contract changes; coordinator transport tests remain green. |
| MNT-001B | Med | Maintainability | `core/rust/crates/helm-cli/src/{main.rs,json_output.rs}` | Refactor | S | Low | None | JSON/NDJSON envelope construction is extracted into `json_output.rs` while preserving existing schema shape and NDJSON split semantics; payload-shape tests remain green. |
| MNT-001C | Med | Maintainability | `core/rust/crates/helm-cli/src/{main.rs,cli_errors.rs}` | Refactor | S | Low | None | Exit-marker parsing and failure-classification helpers are extracted into `cli_errors.rs` with unchanged marker semantics and classification hints; existing `helm-cli` tests remain green. |
| MNT-002 | Med | Maintainability | `core/rust/crates/helm-ffi/src/lib.rs` | Refactor | M | Med | None | FFI high-frequency error keys are centralized (constants/typed enum mapping); repeated boilerplate paths use shared helpers; no behavior regressions. |
| MNT-003 | Med | Maintainability | `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` | Refactor | M | Med | None | Detect/refresh orchestration logic is decomposed into pure helpers with added unit tests for stage selection/reduction behavior. |
| MNT-004 | Low | Maintainability | `apps/macos-ui/Helm/Core/HelmCore+{Fetching,Actions,Settings}.swift` | Refactor | M | Low | None | Split into `MNT-004A` + `MNT-004B`; close parent when both child acceptance criteria pass. |
| MNT-004A | Low | Maintainability | `apps/macos-ui/Helm/Core/HelmCore+Settings.swift` | Refactor | S | Low | None | Settings decode/error handling for JSON payloads is centralized through a shared helper for at least two call sites with no behavior change; app build remains green. |
| MNT-004B | Low | Maintainability | `apps/macos-ui/Helm/Core/HelmCore+{Fetching,Actions}.swift` | Refactor | M | Low | None | Shared decode/error helper coverage is extended to additional HelmCore extensions without altering user-visible messages or error attribution behavior. |
| MNT-005 | Low | Maintainability | coordinator transport separation from CLI command file | Refactor | L | Med | None | Split into `MNT-005A` + `MNT-005B` + `MNT-005C`; close parent when coordinator protocol/transport boundaries are isolated and tested. |
| MNT-005A | Low | Maintainability | `core/rust/crates/helm-cli/src/{main.rs,coordinator_transport.rs}` | Refactor | S | Low | None | Coordinator transport trait/boundary is explicit and command handlers call module entry points only (no inline transport details in `main.rs`). |
| MNT-005B | Low | Maintainability | `core/rust/crates/helm-cli/src/coordinator_transport.rs` | Refactor | M | Low | None | Coordinator state-machine and lifecycle helpers are consolidated in transport module with no behavior changes; existing transport tests remain green. |
| MNT-005C | Low | Maintainability | coordinator docs/tests | Refactor | S | Low | None | Coordinator transport invariants (boundary, fallback policy, ownership expectations) are documented and linked from CLI coordinator tests. |

## Blocked (Decision Required)

No open decision blockers. `DEC-001..DEC-005` are resolved in `docs/audits/quality-audit-decisions.md`.
