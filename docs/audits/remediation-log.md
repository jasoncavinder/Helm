# Remediation Log

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
