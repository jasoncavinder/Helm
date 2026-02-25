# Remediation Log

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
