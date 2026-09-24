# uv Global Tools: Read-Only Foundation

Date: 2026-09-23

Status: bounded Rust foundation for the v0.20 global-tool manager requested in
[Issue #526](https://github.com/jasoncavinder/Helm/issues/526). This is not a
registered adapter or a completed certification record.

Follow-up: the [core read integration](uv-global-tools-read-integration.md) now
binds explicit executable/store paths to shared task execution and persistence.
The foundation evidence below remains historical and bounded; that follow-up
does not activate uv in the app/CLI or complete lifecycle certification.

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
Local uv 0.12.9 help confirms the requested switches. The opt-in real-command
test below additionally checks nonempty output on 0.12.9 and 0.12.18, without
establishing an oldest supported release or a complete adapter capability boundary.

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
- GUI builds, user QA, and a real Helm-adapter lifecycle are not claimed for this
  unregistered Rust-only slice. The separate opt-in test below exercises uv
  itself with generated fixtures, not Helm's mutation/orchestration path.

In particular, latest lookup does not supply the installed requirement as a
version constraint. The regression fixture intentionally includes an exact pin
and a newer latest version. It must never directly produce an `OutdatedPackage`
or an executable upgrade step. uv's own upgrade behavior is constraint-aware;
see the [upstream upgrade documentation](https://docs.astral.sh/uv/concepts/tools/#upgrading-tools).

## Opt-In Real Command Test

`uv_tool_real_contract.rs` launches a standard-library-only Python fixture driver
with explicit executable paths and no shell. The driver creates two small local
wheels in a fresh store, captures real command output, and passes those captures
to the production Rust parser. List arguments come from the production builder.

The harness clears inherited environment/configuration, sets its own HOME and
tool/bin/cache/Python/config/data/temp directories, forces offline mode, disables
Python downloads and managed Python, uses an explicitly supplied interpreter,
and installs only generated wheels from a local `--no-index --find-links` source.
No package build backend or fixture entrypoint runs. Each subprocess has a
30-second deadline. Run artifacts and captures remain under
`artifacts/uv-real-contract/`; there is no automatic cleanup or host tool mutation.

```bash
HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
HELM_UV_CONTRACT_PYTHON=/absolute/path/to/python3 \
cargo test -p helm-core --test uv_tool_real_contract \
  --manifest-path core/rust/Cargo.toml -- --ignored --nocapture
```

The test is ignored by default and does not install test tools during the normal
Rust quality gate. Explicit invocation fails rather than skipping if either
executable is missing. On 2026-09-23 it passed on arm64 macOS with Python 3.14.7
and each of uv 0.12.9 (`9f9286029`) and 0.12.18 (`01cb90c1a`). The latter used the
official release archive in the worktree, checked against its published SHA-256
`cf40e0c6a202190ccd9e0406dcfdd5b2d6668a9a5c779b17948963df32aafe5b`, not a host upgrade.

Verified scenarios:

- Empty inventory and no newer versions parse through their distinct contracts.
- One tool exporting two executables remains one observed distribution.
- New local version 2.0 appears as latest for both an exact `==1.0` pin and a
  `<2` constraint; upgrading preserves the pin and advances the range only to
  1.1. Discovery therefore still does not authorize a mutation target.
- A failed install of a nonexistent local package leaves inventory unchanged.
- A malformed test receipt makes uv exit 0 with warnings and partial stdout;
  both installed/latest parsers reject the incomplete result. Restoring the
  receipt restores the original complete observation.
- Uninstalling one test tool removes its executable links without damaging the
  other tool; removing the second restores confirmed empty inventory.

This is bounded real upstream-command and parser evidence, not a certified
Helm manager. Network/index behavior, installed provenance, transport failures,
cancellation, persistence acceptance, Python/uv version ranges, GUI/CLI wiring,
and post-action reconciliation through Helm remain separate gates.

## Remaining Before Activation

1. Complete supported-version policy and selected executable/tool-store discovery
   and ownership, including multiple installations and non-registry tools. The
   core read integration has explicit caller-selected paths and a provisional
   0.12.9-0.12.18 version gate, not automatic discovery or ownership proof.
2. Extend the explicit-scope read integration with local search and network-aware
   discovery. Detection, offline installed inventory, task cancellation, and
   cache-preserving parse/transport failure regressions now have bounded core
   coverage; live GUI/CLI wiring remains gated.
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
