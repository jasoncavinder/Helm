# Consolidated Remediation Backlog

Date: 2026-02-25  
Source inputs: `quality-audit-remediation-checklist.md`, `security-review.md`, `correctness-edgecases.md`, `maintainability.md`, `release-readiness.md`, `test-plan.md`, `docs-ux-drift.md`, `quality-audit-decisions.md`.

Notes:
- This backlog is de-duplicated across sources.
- Findings marked already fixed in source audits are excluded.
- `DEC-*` blockers map to `docs/audits/quality-audit-decisions.md`.

## Batch Status Updates (2026-02-25)

- `SEC-002` — Done (`78594cd`)
- `SEC-005` — Done (`PR: TBD`, commit pending)
- `TEST-005` — Done (`PR: TBD`, commit pending)
- `SEC-004` — Done (`PR: TBD`, commit pending)
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

## Prioritized Backlog

| ID | Severity | Category | Affected area (path/module) | Fix type | Effort | Risk | Dependencies / Blockers | Acceptance criteria |
|---|---|---|---|---|---|---|---|---|
| SEC-001 | High | Security | `core/rust/crates/helm-core/src/execution/tokio_process.rs` | Hardening | M | Med | None | `HELM_SUDO_ASKPASS` override is disallowed by default (or strictly validated); elevated flows use only trusted helper paths; regression tests cover accepted/rejected override paths. |
| SEC-002 | High | Security | `core/rust/crates/helm-cli/src/main.rs` (`process_is_alive`, `coordinator_process_looks_owned`) | Hardening | S | Low | None | Coordinator helper probes use absolute `/bin/ps` (or sanitized fixed `PATH`); test/assertion verifies no PATH-resolved `ps` invocation remains. |
| SEC-003 | High | Security | `core/rust/crates/helm-core/src/execution/{tokio_process.rs,task_output_store.rs}`, `core/rust/crates/helm-ffi/src/lib.rs` | Hardening | L | Med | DEC-005 | Sensitive token patterns are redacted before persistence/exposure; diagnostics export/UI/CLI never surface raw secrets; redaction tests include auth headers/tokens/API keys. |
| SEC-004 | High | Security | `scripts/release/build_unsigned_variant.sh` | Bugfix | S | Low | None | `TAG_NAME` is validated against release regex and canonicalized output paths are enforced under expected artifact roots; traversal attempts fail with explicit errors. |
| SEC-005 | High | Security | `core/rust/crates/helm-cli/src/main.rs` (self-update fetch path) | Hardening | M | Med | None | Redirects are followed only with per-hop allowlist validation (or disabled by policy); final URL host must pass allowlist; integration tests cover redirect-to-disallowed-host rejection. |
| COR-001 | High | Correctness | `core/rust/crates/helm-cli/src/main.rs` (`submit_refresh_request_response` / coordinator wait path) | Bugfix | M | Med | DEC-002 | Wait-timeout behavior matches documented policy (effective manager/global timeout with explicit ceiling decision); no hard-coded 60s cap mismatch; regression tests assert policy alignment. |
| COR-002 | High | Correctness | manager override sync path in CLI/FFI runtime map updates | Bugfix | M | Med | None | Selected executable + timeout profile maps update via atomic swap; no clear-then-set empty window under concurrent submissions; concurrency regression test passes. |
| REL-001 | High | Build/Release | `web/public/updates/{appcast.xml,cli/latest.json,cli/latest-rc.json}`, `.github/workflows/cli-update-drift.yml` | Bugfix | S | Low | DEC-004 | Published stable/prerelease metadata policy is explicit and implemented; drift guard is green for chosen policy state; metadata files exist/align as required. |
| REL-002 | High | Build/Release | `.github/workflows/release-publish-verify.yml`, release publish sequencing | Hardening | M | Med | None | Publish verification is deterministic across PR merge order; no transient red runs during normal publish PR sequencing; added contract tests/simulated order scenarios pass. |
| REL-003 | High | Reliability | coordinator IPC tests (`helm-cli` + `helm-ffi`) | Test | M | Low | None | Tests assert coordinator directories are `0700` and request/response/temp files are `0600`; ownership assumptions are validated for both CLI and FFI paths. |
| REL-004 | High | Security | coordinator request/response transport (file IPC auth) | Hardening | L | Med | DEC-001 | Request/response channel includes per-session nonce/token capability checks; stale/forged files are rejected with explicit diagnostics; compatibility path documented and tested. |
| BUILD-001 | High | Build | `.github/workflows/*.yml` | CI | S | Low | DEC-003 | All third-party `uses:` references are pinned to immutable SHAs (or policy-approved exceptions documented); workflow suite remains green post-update. |
| BUILD-002 | High | Security | CI workflows (dependency checks) | CI | S | Low | None | CI runs dependency vulnerability checks (e.g., `cargo-audit`/`cargo-deny` and dependency review) on PR/schedule; failures are visible and actionable. |
| COR-003 | Med | Correctness | `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` | Bugfix | M | Med | None | Successful install/uninstall updates cached state (targeted refresh or equivalent) without requiring manual full refresh; integration tests verify snapshot freshness. |
| COR-004 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/homebrew.rs` (and similar manager paths) | Bugfix | S | Low | None | Repeated uninstall of already-removed package is treated as idempotent success for known benign manager responses; tests cover already-absent cases. |
| COR-005 | Med | Reliability | `core/rust/crates/helm-cli/src/main.rs` coordinator bootstrap | Bugfix | M | Med | None | Concurrent first-start bootstrap is serialized (lock/marker); parallel CLI launches do not race-reset state or spawn conflicting daemons; race test passes. |
| COR-006 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/process_utils.rs` | Bugfix | M | Low | None | Non-UTF8 manager output no longer hard-fails parse path by default; tolerant decoding path preserves operability and diagnostics context; regression tests include invalid UTF8 bytes. |
| COR-007 | Med | Reliability | execution timeout/error classification + diagnostics surfaces | Hardening | M | Med | None | Offline/proxy/captive portal failures are classified distinctly from generic timeouts; user-facing diagnostics include actionable hints; tests cover representative network failure classes. |
| COR-008 | Med | Correctness | `core/rust/crates/helm-core/src/adapters/detect_utils.rs` | Hardening | M | Low | None | Shim/root discovery includes documented manager ecosystems (e.g., `rtx`) and conflict diagnostics when selected/default executable diverge; tests cover path precedence cases. |
| COR-009 | Med | UX/CLI behavior | `core/rust/crates/helm-cli/src/main.rs` (`cmd_tasks_follow`) | Bugfix | M | Low | None | Machine mode (`--json`/`--ndjson`) has defined streaming behavior for follow, or emits explicit documented non-support contract with stable exit code; tests cover contract. |
| COR-010 | Low | UX/CLI behavior | `core/rust/crates/helm-cli/src/main.rs` (`build_json_payload_lines`) | Hardening | S | Low | None | NDJSON contract is explicitly defined per command; nested array payload behavior is deterministic/documented; tests verify expected envelope shape. |
| UX-001 | Med | UX/Docs | task output model across core/FFI/UI (`helm_get_task_output`, Swift `CoreTaskOutputRecord`) | Hardening | M | Med | DEC-005 | Core task output fields (`cwd`, `program`, `PATH` snippet, exit/termination/error context) are consistently propagated and rendered with backward-compatible decoding; privacy-safe redaction rules are applied. |
| PERF-001 | Med | Performance | coordinator poll loop in CLI workflow wait paths | Hardening | M | Low | None | Polling cadence is adaptive or event-driven; idle wakeups are reduced measurably; timeout/failure semantics remain deterministic in tests. |
| PERF-002 | Low | Performance | manager enablement hot paths in orchestration | Refactor | M | Low | None | Manager enablement snapshot is cached with correct invalidation on preference/detection updates; behavior parity tests pass. |
| BUILD-003 | Med | Build/Release | CI workflows + `scripts/release/{preflight.sh,runbook.sh}` | CI | S | Low | None | Release preflight/runbook contract checks are exercised in CI (non-destructive mode) to catch regressions before release tags. |
| BUILD-004 | Med | Build/Reproducibility | CI toolchain provisioning (`rust-toolchain`, SwiftLint provisioning) | CI | M | Low | None | Rust toolchain and SwiftLint versions are pinned/reproducible in CI; version drift is explicit and controlled; docs reflect pin update process. |
| BUILD-005 | Med | Build/Reproducibility | release pipeline provenance | CI | M | Med | None | Build provenance/SBOM generation is implemented, or explicit signed deferral decision is documented with target milestone and owner. |
| REL-005 | Med | Build/Release | branch/ruleset required-check policy | CI | S | Low | None | Release drift/publish verify checks have explicit gating policy (required vs advisory) and match branch intent; ruleset/workflow docs are consistent. |
| REL-006 | Med | Reliability/Policy | crash/error reporting strategy docs + telemetry policy | Docs | S | Low | None | 1.0 crash reporting posture is explicitly decided (none vs opt-in channel) and documented with privacy constraints, data schema, and operational owner. |
| TEST-001 | Med | Test | timeout-sensitive orchestration tests (`end_to_end_rustup`, `end_to_end_mise`) | Test | M | Low | None | Soak/repeat test target exists; flake budget defined; repeated runs show deterministic pass criteria. |
| TEST-002 | High | Test | manager lifecycle integration coverage (install/update/remove/pin) | Test | M | Med | None | Integration tests cover at least one manager in authoritative, standard, and guarded classes for install/update/remove behavior including idempotency assertions. |
| TEST-003 | Med | Test | CLI non-interactive contract tests | Test | S | Low | None | Mixed-success `updates run` cases assert stable exit codes and JSON envelope schema; tests run in CI. |
| TEST-004 | Med | Test | workflow contract tests for metadata convergence | Test | M | Low | REL-001, REL-002 | Automated tests simulate stable/prerelease metadata states and publish PR merge order; expected pass/fail conditions are asserted. |
| TEST-005 | Med | Test/Security | CLI self-update negative-path tests | Test | S | Low | SEC-005 | Tests cover redirect host rejection, oversized payload bounds, and policy-blocked update channels. |
| TEST-006 | Med | Test | real-manager e2e matrix (Homebrew + node/python/ruby) | Test | L | Med | BUILD-004 | CI or scheduled canary matrix runs against real manager binaries and reports pass/fail independently from fast PR suite. |
| TEST-007 | Low | Test/Reliability | guarded OS update validation lanes | Test | L | High | None | Dedicated VM/canary validation exists for destructive guarded update paths with explicit safety controls and rollback guidance. |
| TEST-008 | Low | Test/Reliability | Sparkle updater end-to-end recovery scenarios | Test | L | Med | None | Automated updater interruption/recovery scenarios run on signed artifacts in dedicated environment; failures produce actionable diagnostics. |
| TEST-009 | Low | Test/Release | staged release rehearsal environment | Test | L | Med | REL-001, REL-002 | Full publish rehearsal executes without touching production metadata and validates release flow end-to-end. |
| DOC-001 | Low | UX/Docs | CLI errors in `core/rust/crates/helm-cli/src/main.rs` | Docs | S | Low | None | Common CLI errors include actionable next-step hints (`helm help`, `helm managers list`, `helm updates preview`); contract tests verify messages. |
| DOC-002 | Low | Docs | release-line copy across README/banner/docs | Docs | S | Low | None | One canonical source/value for current release channel copy is used across README/website/release docs; drift checks or lint prevent mismatch. |
| DOC-003 | Low | Docs/Build | Starlight content registry/config for guides | Docs | S | Low | None | Website build no longer emits duplicate id warnings for `guides/faq`, `guides/installation`, `guides/usage`. |
| DOC-004 | Low | Docs/UX | terminology consistency docs (`manager`/`adapter`/`service`) | Docs | S | Low | None | User-facing docs consistently use approved terms; architecture docs define internal terms once; terminology check added to docs review checklist. |
| MNT-001 | Med | Maintainability | `core/rust/crates/helm-cli/src/main.rs` | Refactor | M | Med | None | CLI command handlers are split by domain modules without behavior/contract changes; existing CLI tests remain green. |
| MNT-002 | Med | Maintainability | `core/rust/crates/helm-ffi/src/lib.rs` | Refactor | M | Med | None | FFI high-frequency error keys are centralized (constants/typed enum mapping); repeated boilerplate paths use shared helpers; no behavior regressions. |
| MNT-003 | Med | Maintainability | `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` | Refactor | M | Med | None | Detect/refresh orchestration logic is decomposed into pure helpers with added unit tests for stage selection/reduction behavior. |
| MNT-004 | Low | Maintainability | `apps/macos-ui/Helm/Core/HelmCore+{Fetching,Actions,Settings}.swift` | Refactor | M | Low | None | Shared decode/error handling wrappers reduce duplication in HelmCore extensions; Swift tests pass with unchanged UX behavior. |
| MNT-005 | Low | Maintainability | coordinator transport separation from CLI command file | Refactor | L | Med | DEC-001 | Coordinator protocol/state-machine is isolated behind module boundary; command dispatch no longer owns transport internals; compatibility tests pass. |

## Blocked (Decision Required)

Decision IDs below are blockers for related backlog items.

| Decision ID | Blocks | Question |
|---|---|---|
| DEC-001 | REL-004, MNT-005 | Keep file-based coordinator IPC and harden incrementally, or migrate to stronger local IPC primitive now? |
| DEC-002 | COR-001 | Should request-response wait timeout be derived from effective manager/global policy, or use an independent orchestration cap? |
| DEC-003 | BUILD-001 | Enforce full immutable action SHA pinning immediately, or phase by workflow criticality? |
| DEC-004 | REL-001 | Should `dev` carry publish-ready metadata artifacts, or should publish metadata truth be `main`/release branches only? |
| DEC-005 | SEC-003, UX-001 | Should UI expose full diagnostics context (`PATH`/program) by default, redacted by default, or behind advanced/export-only surfaces? |
