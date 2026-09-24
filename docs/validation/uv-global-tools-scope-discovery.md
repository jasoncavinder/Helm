# uv Global Tools: Read-Scope Discovery

Date: 2026-09-23

Status: staged core-only follow-up to [read integration](uv-global-tools-read-integration.md).
This advances [Issue #526](https://github.com/jasoncavinder/Helm/issues/526), but
does not close it or certify the adapter under
[Issue #500](https://github.com/jasoncavinder/Helm/issues/500).

## Contract

`adapters::uv_tool_scope::UvToolDiscovery` resolves direct executable locations
and their tool storage before constructing a guarded `UvToolContext`.

- Explicit selection never falls back to PATH or another installation if it fails.
  Automatic search checks direct `uv` children in a bounded list of absolute
  directories. The environment helper uses absolute PATH entries, user-local and
  Cargo bin directories, and standard Homebrew/MacPorts bin directories. Relative
  entries are ignored by that helper; explicit relative inputs are rejected.
- Symlink aliases of the same canonical executable collapse into one candidate.
  Distinct canonical executables return `SelectionRequired` without running any
  candidate. An empty search returns `NotFound`; an unavailable explicit selection
  is an error. Hard-link equivalence and inactive versioned installation enumeration
  are not claimed by this slice.
- Known `shims` components in either the supplied or canonical executable path,
  and aliases resolving to mise/asdf/rtx dispatchers, fail before execution and
  require selection of a concrete uv executable. No login shell,
  shim dispatcher resolution, recursive Cellar scan, or project activation runs.
  Unrecognized wrappers are not authenticated by this resolver.
- The selected canonical executable must be an executable regular file and pass
  the existing stable 0.12.9-0.12.18 version gate. Identity is checked around the
  version/directory probes; replacement requires rediscovery.
- `uv tool dir` supplies storage instead of a guessed home-directory convention.
  An explicit override, or inherited `UV_TOOL_DIR`, must be absolute. A supplied
  override must agree with the result after alias resolution. Defaults remain uv's
  responsibility when no override is configured.
- Probes use structured arguments, offline mode, no project configuration, no
  persistent cache, disabled Python downloads, and `/` as the working directory.
  Shared execution preserves task identity/cancellation, a 10-second default hard
  limit, and private combined output capture capped at 16 KiB. Shared timeout
  policy clamps the requested 10-second idle limit to 9 seconds below that hard
  limit; normal task timeout overrides remain applicable.
- A successful directory response must contain exactly one absolute UTF-8 path
  with no diagnostics or control characters. Failed, terminated, oversized,
  malformed, relative, and mismatched output is rejected without including raw
  output in errors. Spaces in legitimate directory names are preserved.
- Missing storage returns `ToolStoreMissing` without creating it or constructing
  an inventory source. It is not authoritative empty inventory and cannot clear
  cached packages. Existing non-directory, inaccessible, or dangling-symlink
  stores fail. Only an existing directory produces `Ready`.
- A ready context carries canonical executable/store paths, their selected/configured
  and reported aliases,
  and filesystem identities (device/inode on supported Unix platforms). The source
  validates this binding before and after each process call. Alias retargeting,
  removal, or replacement invalidates it; failure yields no accepted snapshot.
  The legacy explicit `UvToolContext::new` constructor remains unguarded and is not
  a substitute for resolved discovery when the production runtime is activated.

These checks detect ordinary scope changes; they are not an atomic filesystem
transaction, executable authentication, or a defense against adversarial concurrent
replacement. No Homebrew/mise/standalone ownership confidence, uninstall authority,
source eligibility, or mutation permission is inferred from a path. Changing a
selected store requires fresh discovery and a separate runtime handoff; this
module does not merge multiple stores into one snapshot.

Upstream references: [uv tool dir](https://docs.astral.sh/uv/reference/cli/#uv-tool-dir)
and [tool storage](https://docs.astral.sh/uv/reference/storage/#tools).

## Verification

The Rust quality gate passed with 1,367 tests passed, 0 failed, and 3 deliberately
ignored opt-in tests. Formatting, warnings-as-errors Clippy, separate all-target
Clippy, docs-sync/release-line copy, and whitespace checks passed. Validation used
the existing isolated Rust 1.98.1 binaries without changing the host toolchain.

`uv_tool_scope_contract.rs` covers direct selection, alias collapse, multiple
executables, invalid/missing paths, known shims, version rejection, replacement
during probing, strict directory output, absent storage, task identity, alias
retargeting, and store replacement during an accepted-inventory refresh. The
SQLite regression waits for persistence completion and proves that rejected
replacement reads leave committed inventory intact.

Independent-review regressions additionally cover ordinary-looking aliases to
`asdf/shims/uv` through explicit selection and directory search. Both reproduced
two probe executions on the original PR head; checking the canonical path's
components rejects both cases before any process starts.

After remediation, the full Rust quality gate passed with 1,369 tests passed,
0 failed, and 3 deliberately ignored opt-in tests. Formatting and both workspace
and all-target warnings-as-errors Clippy passed. The scope and read-integration
suites, including their real smoke tests, passed on uv 0.12.9 with 16 and 10 tests
respectively.

The opt-in real smoke exercises discovery followed by the existing adapter's
offline `ListInstalled` request in disposable stores. It also confirms that
querying a missing store reports it without creating it. Run with:

```bash
HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
cargo test -p helm-core --test uv_tool_scope_contract \
  --manifest-path core/rust/Cargo.toml -- --include-ignored --nocapture
```

Local real smoke passed on uv 0.12.9 and 0.12.18, each with 14 tests passed. The
existing read-integration suite including its real smoke also passed on 0.12.9.
Tests use disposable paths and databases; host-managed tool inventory and the stable
Helm database are not inspected or modified. The signed QA application is unchanged.

## Remaining Before Activation

Ownership/eligibility, version-manager concrete selection, source and Python
constraints, candidate authorization, reviewed mutations, post-action verification,
local search, network discovery, and GUI/CLI runtime registration remain separate
gates. The uv registry still advertises only its staged read capabilities and FFI
still reports it unimplemented. This slice changes no release or appcast metadata.
