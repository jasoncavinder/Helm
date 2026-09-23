# uv Global Tools: Read-Only Foundation

Date: 2026-09-23

Status: bounded Rust foundation for the v0.20 global-tool manager requested in
[Issue #526](https://github.com/jasoncavinder/Helm/issues/526). This is not a
registered adapter or a completed certification record.

## Implemented Contract

`core/rust/crates/helm-core/src/adapters/uv_tool.rs` provides a structured
`CommandSpec` builder and a pure `ProcessOutput` parser. No process is launched
by this module, and no registry, persistence, UI, or CLI capability is added.

- Installed inventory requests `uv --color never --no-progress tool list
  --show-version-specifiers --offline` with `UV_PYTHON_DOWNLOADS=never`.
- Latest-version discovery substitutes `--outdated` for `--offline`. It may
  require network access and must eventually run through Helm's network policy.
- A distribution remains one tool even if it exports several executables.
  Distinct distributions sharing an executable name remain distinct observations.
- Installed versions, uninterpreted requirements, and discovered latest versions
  remain separate facts. This parser performs no PEP 440 comparison, constraint
  evaluation, source classification, pin inference, or upgrade authorization.
- Nonzero/terminated output, invalid encoding, oversized captures, malformed
  structure, duplicate normalized identities, and missing executable rows fail
  closed. Any unexpected stderr also fails: uv can skip broken tools and exit 0.
- Empty installed inventory requires uv's explicit `No tools installed`
  diagnostic. Silent empty latest output means no latest observations, not an
  empty installed inventory. Callers must retain previous cached state on errors.
- Errors contain a category and, where applicable, a line number, not captured
  diagnostics or potentially credential-bearing requirement URLs. Successful
  observations retain raw requirements internally; future diagnostics and UI
  must redact them before display or export.

The builder deliberately does not select a tool store or suppress user index
configuration. It is not yet a complete `ProcessRequest`: the integration must
bind the selected executable/store, a neutral working directory, task identity,
timeouts, cancellation, bounded capture, and authoritative snapshot acceptance.

## Evidence and Limits

The synthetic fixtures and tests in
`core/rust/crates/helm-core/tests/uv_tool_contract.rs` cover installed/latest/empty
output, pins, executable children, warnings with successful exit, malformed and
partial output, identity normalization, CRLF, opaque Python versions and source
requirements, and diagnostic privacy. They execute no uv mutation commands and
are not the maintainer's installed-tool inventory.

The output shape was checked against the pinned upstream
[uv 0.12.18 list implementation](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/list.rs)
and [tool-list tests](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/tests/tool/tool_list.rs).
Local uv 0.12.9 help confirms the requested switches, but does not establish the
oldest supported release or certify nonempty real output across versions.

Local validation for this slice:

- The Rust quality gate passed: 1,339 workspace tests, formatting, and Clippy
  with warnings denied. The total includes 15 focused uv contract tests.
- A separate workspace Clippy run including all targets passed with warnings
  denied, and the docs-sync/release-line and whitespace checks passed.
- Local uv 0.12.9 returned exit 0 and `No tools installed` for both list modes
  against an isolated empty tool store. That smoke check used worktree-local
  `UV_TOOL_DIR`, `UV_TOOL_BIN_DIR`, and `UV_CACHE_DIR`, forced offline mode and
  disabled Python downloads, and added `--no-config` only in the smoke harness.
  It did not inspect or modify the maintainer's installed tools and does not
  certify nonempty inventory, network discovery, or mutations.
- The host compiler lacked rustfmt/Clippy components, so validation used a
  worktree-local Rust 1.98.1 toolchain without changing the normal installation.
- GUI builds, user QA, and real installed-tool lifecycle tests are not claimed
  for this unregistered Rust-only slice.

In particular, latest lookup does not supply the installed requirement as a
version constraint. The regression fixture intentionally includes an exact pin
and a newer latest version. It must never directly produce an `OutdatedPackage`
or an executable upgrade step. uv's own upgrade behavior is constraint-aware;
see the [upstream upgrade documentation](https://docs.astral.sh/uv/concepts/tools/#upgrading-tools).

## Remaining Before Activation

1. Establish supported uv versions and selected executable/tool-store ownership,
   including multiple installations and non-registry tools.
2. Integrate read-only detection, installed inventory, local search, and
   network-aware discovery through the existing process/orchestration boundary.
   Prove parse/transport failures cannot clear authoritative cached inventory.
3. Resolve PEP 440 ordering, constraints, pins, configured indexes, and source
   eligibility before creating any actionable update candidate. Preserve unknown
   eligibility rather than equating latest discovery with a permitted upgrade.
4. Add reviewed install/upgrade/uninstall, exact target validation, cancellation,
   and installed-state reconciliation. Do not infer success from exit status alone.
5. Register the complete capability set consistently in core/service/GUI/CLI,
   with localized presentation and manager-policy coverage.
6. Run isolated real lifecycle certification, including constrained, broken,
   offline, cancelled, alternate-store, and multiple-executable scenarios under
   [Issue #500](https://github.com/jasoncavinder/Helm/issues/500).

Project environments, arbitrary virtual environments, Python runtime management,
general PyPI search, and uv executable self-updates remain outside this slice.
