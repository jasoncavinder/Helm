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
