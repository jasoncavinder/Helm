# Helm Correctness & Edge-Case Audit (Pre-1.0)

Date: 2026-02-25  
Scope: state transitions, concurrency, non-interactive contracts, filesystem/network realities, manager conflict behavior.

## Summary

This pass identified two high-impact correctness bugs and one medium filesystem edge-case bug that were fixed with regression tests:

1. `updates run` workflow in coordinator mode could silently succeed even when upgrade steps failed.
2. Coordinator request timeouts could trigger launch-on-demand reset, interrupting valid in-flight work.
3. `mise` process PATH bootstrapping could incorrectly include `/.local/bin` when `HOME` is missing.

Additional medium/low issues remain and are listed below with concrete proposed fixes.

## Findings

### State Transitions

### 1) `updates run` coordinator workflow swallowed step failures (Fixed)
- Severity: High
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`run_coordinator_workflow`, `CoordinatorWorkflowRequest::UpdatesRun`)
- Reproduction notes:
  - Run a workflow where at least one upgrade step fails (for example, manager command returns non-zero).
  - Prior behavior: workflow returned `Ok(())` and CLI path could report success even with failed upgrade operations.
- Fix implemented:
  - Count per-step failures via `count_upgrade_step_failures` and return marked error via `manager_operation_failure_error("upgrade", failures)`.
  - Added regression tests for failure-marking and failure counting.

### 2) Install/uninstall snapshot freshness lag after mutations (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs` (`persist_adapter_response`)
- Reproduction notes:
  - Run `helm packages install ...` or `helm packages uninstall ...`.
  - Immediately query cached installed/outdated lists without refresh.
  - Store may still reflect pre-mutation snapshot until refresh.
- Proposed fix:
  - On install/uninstall terminal success, trigger targeted cache update (manager/package scope) or enqueue post-mutation refresh task.

### 3) Uninstall idempotency gaps for already-removed packages (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-core/src/adapters/homebrew.rs` (`execute` uninstall path)
- Reproduction notes:
  - Uninstall package once successfully.
  - Re-run uninstall for same package.
  - Manager error (`not installed`) bubbles as hard failure.
- Proposed fix:
  - Normalize known “already absent” manager messages to successful idempotent outcome.

### Concurrency

### 4) Timeout-driven coordinator requests could reset state and disrupt active work (Fixed)
- Severity: High
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`coordinator_send_request`, `spawn_coordinator_daemon`, `reset_coordinator_state_dir`)
- Reproduction notes:
  - Start long-running coordinator-backed operation.
  - Trigger CLI request timeout on follow-up request (or use short timeout).
  - Prior behavior could fall through to launch-on-demand path, resetting coordinator state directory.
- Fix implemented:
  - Added `should_launch_coordinator_on_demand(...)` guard.
  - Launch-on-demand is now blocked for timeout errors unless stale-state recovery path explicitly handled it.
  - Added regression test for timeout guard behavior.

### 5) Concurrent first-start race for coordinator daemon bootstrap (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`coordinator_send_request`, `spawn_coordinator_daemon`)
- Reproduction notes:
  - Two CLI commands start simultaneously when coordinator is not yet running.
  - Both can attempt startup/reset path.
- Proposed fix:
  - Add lockfile or atomic startup marker to serialize daemon bootstrap.

### Non-Interactive Usage (CI/Scripts)

### 6) `tasks follow` has no machine-readable streaming mode (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`cmd_tasks_follow`)
- Reproduction notes:
  - Run `helm tasks follow --json ...`.
  - Command errors because JSON streaming is unsupported.
- Proposed fix:
  - Add JSON/NDJSON streaming envelope for follow mode or explicit machine-mode follow command.

### 7) NDJSON splitting only occurs when top-level `data` is an array (Not fixed in this slice)
- Severity: Low
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`build_json_payload_lines`)
- Reproduction notes:
  - Run commands that emit object payloads containing nested arrays under `data`.
  - NDJSON mode emits single object envelope, not per-item records.
- Proposed fix:
  - Define per-command NDJSON contracts for nested arrays where streaming semantics are required.

### Version/Reporting

### 8) `--version` and verbose metadata behavior appears stable
- Severity: Informational
- Code pointer: `core/rust/crates/helm-cli/src/main.rs` (`parse_args`, `emit_version_metadata_if_verbose`)
- Reproduction notes:
  - `helm --version` produces deterministic output.
  - Verbose metadata logs to stderr via `verbose_log`.
- Proposed action:
  - Keep existing tests; no immediate change required.

### Filesystem Realities

### 9) Missing `HOME` produced invalid `mise` search/path prefix (`/.local/bin`) (Fixed)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-core/src/adapters/mise_process.rs` (`configure_request`, `detect`)
- Reproduction notes:
  - Execute adapter in environment where `HOME` is unset/blank.
  - Prior behavior prefixed/search-rooted using `/.local/bin`.
- Fix implemented:
  - Added `home_local_bin_root` and `path_with_home_local_bin` helpers.
  - PATH/search roots only include `~/.local/bin` when `HOME` is non-empty.
  - Added regression tests for missing/blank/populated home handling.

### 10) Non-UTF8 stdout parse failure can fail manager operations (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-core/src/adapters/process_utils.rs` (`run_and_collect_stdout`)
- Reproduction notes:
  - Manager outputs bytes not valid UTF-8.
  - Operation returns parse failure instead of tolerant decoding.
- Proposed fix:
  - Add loss-tolerant decode path for display/logging while preserving raw-bytes diagnostics where possible.

### Network Realities

### 11) Proxy/captive/offline ergonomics need stronger handling (Not fixed in this slice)
- Severity: Medium
- Code pointers:
  - `core/rust/crates/helm-core/src/execution/mod.rs` (timeout defaults)
  - `core/rust/crates/helm-ffi/src/lib.rs` (auto-check HTTP agent)
- Reproduction notes:
  - Constrained DNS/proxy/captive environments can cause repeated timeout-style failures without clearer classification.
- Proposed fix:
  - Improve timeout/error classification and expose proxy/offline hints in diagnostics.

### Manager Conflicts

### 12) Shim/toolchain discovery coverage gaps (e.g., `rtx`) can cause system tool fallback (Not fixed in this slice)
- Severity: Medium
- Code pointer: `core/rust/crates/helm-core/src/adapters/detect_utils.rs` (`common_search_roots`)
- Reproduction notes:
  - Use shim roots not present in current discovery list.
  - Helm may prefer system executable over active shim-managed tool.
- Proposed fix:
  - Extend root detection coverage and add conflict-aware diagnostics when selected/default executables diverge.

## Regression Tests Added (Top Risks)

### `core/rust/crates/helm-cli/src/main.rs`
- `manager_operation_failure_error_returns_none_when_no_failures`
- `manager_operation_failure_error_marks_single_and_multiple_failures`
- `count_upgrade_step_failures_counts_errors_without_short_circuiting`
- `upgrade_request_name_encodes_homebrew_cleanup_targets`
- `coordinator_launch_on_demand_is_disabled_for_timeout_errors`

### `core/rust/crates/helm-core/src/adapters/mise_process.rs`
- `home_local_bin_root_requires_non_empty_home`
- `path_with_home_local_bin_skips_prefix_when_home_missing`
- `path_with_home_local_bin_prepends_home_local_bin`

## Validation

Executed:
- `cargo test -p helm-cli`
- `cargo test -p helm-core mise_process`

Result: passing.
