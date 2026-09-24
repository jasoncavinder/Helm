# uv Tool Candidate Eligibility

Date: 2026-09-24. Status: pure core policy, not runtime activation or a complete
update resolver. This continues [installation policy](uv-installation-policy.md)
for #526; neither #526 nor certification issue #500 is closed by this slice.

## Implemented Contract

`adapters::uv_tool_eligibility` consumes caller-supplied installed observations,
modern `uv-receipt.toml` bytes, registry distribution metadata, and the observed
tool environment's Python version. It performs no filesystem access, subprocess
execution, network access, or persistence. The caller must still obtain and bind
all inputs to the same selected executable, tool store, tool, and owning source.

- The primary receipt requirement and entrypoint names must match the accepted
  installed observation. Distribution names use the inventory parser's Python
  normalization. Display-only `required` and `latest` annotations are not policy.
- PEP 440 parsing, ordering, and membership use `pep440_rs` 0.7.3, not SemVer or
  lexical comparison. All same-tool requirements and constraints intersect.
  Exact pins, ranges, exclusions, wildcards, compatible releases, epochs, and
  pre/dev/post/local versions have deterministic coverage. A newer in-range
  version can pass filtering even when the displayed latest version cannot.
- Other registry requirements, extras, and dependency constraints are parsed,
  not resolved. An accepted candidate always remains `NeedsResolution`; the
  future resolver must receive the original complete receipt and configuration.
  This policy does not prove a satisfiable dependency graph or wheel compatibility.
- Requires-Python is checked against supplied, complete interpreter version
  evidence, never the receipt's Python selection request. Unknown Python or
  distribution metadata is an explicit unresolved state. A known absent Python
  restriction (`Some("")`) differs from metadata that has not been read (`None`).
  Unknown yank status cannot be assumed false; yanked candidates are rejected.
- Explicit allow/disallow prerelease settings are recognized. Default, explicit,
  and fallback preference modes leave matching prereleases unresolved by uv's
  resolver. They are not inferred from installed prereleases, exclusions, or a
  latest-only listing. Stable preference/backtracking semantics are not duplicated.
- Git, URL, path, editable, directory, and virtual sources cannot be replaced
  with registry candidates. Private/index-specific and find-links configuration
  requires source-aware integration. Legacy string receipts, markers/groups,
  overrides, build constraints, exclusions, unknown options, and unfamiliar fields
  are explicit limitations rather than silently discarded policy.
- Input is capped at 1 MiB per receipt, 256 combined requirements/constraints,
  256 entries per entrypoint/extras list, and 4 KiB per interpreted string. Errors contain no raw
  parser diagnostics, URLs, credentials, or paths. The policy retains an SHA-256
  receipt fingerprint but no raw receipt/path data in debug output. A fingerprint
  is evidence identity, not filesystem freshness or source authentication.

Rejection of every **observed** candidate does not mean the tool is current.
Incomplete/latest-only discovery may have omitted a valid intermediate release.
No result converts to `OutdatedPackage`, a Plan step, or update authorization.
GUI/CLI registration, executable lifecycle restrictions, and the provisional
0.12.9-0.12.18 read gate are unchanged. No signed QA rebuild is needed here.

## Upstream Basis

The schema was checked against uv 0.12.18's
[receipt](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-tool/src/receipt.rs),
[tool](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-tool/src/tool.rs), and
[requirement](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-distribution-types/src/requirement.rs)
implementations. [Tool upgrade documentation](https://docs.astral.sh/uv/concepts/tools/#upgrading-tools)
requires retaining installation constraints/settings. The pinned
[prerelease resolver](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-resolver/src/prerelease.rs)
demonstrates why PEP 440 membership alone does not decide prerelease preference.
The dependency API is documented by [pep440_rs](https://docs.rs/pep440_rs/0.7.3/pep440_rs/).

## Verification

The focused eligibility contract has 20 deterministic tests. The opt-in real uv
harness also passes its actual generated-wheel receipts into this parser: tool
identity, requirements, and entrypoints must parse, then the isolated find-links
source must remain source-unresolved rather than become public-index eligibility.
This is bounded receipt evidence, not live candidate discovery or registered
adapter lifecycle certification.

Validation passed on arm64 macOS using the existing isolated Rust 1.98.1 toolchain:
the Rust quality gate reported 1,401 passed, 0 failed, and 4 intentional opt-in
skips; formatting, workspace Clippy, separate all-target Clippy with warnings
denied, docs-sync/release-line, and whitespace checks passed. The real-command
harness passed independently on uv 0.12.9 and 0.12.18 with Python 3.14.7. It used
disposable, offline local-wheel stores, not host tools or the stable Helm database.
No Ventura runtime or GUI/CLI activation claim is made.

```sh
cargo test -p helm-core --manifest-path core/rust/Cargo.toml \
  --test uv_tool_eligibility_contract

HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
HELM_UV_CONTRACT_PYTHON=/absolute/path/to/python3 \
cargo test -p helm-core --manifest-path core/rust/Cargo.toml \
  --test uv_tool_real_contract -- --include-ignored
```

## Next Gate

Bind receipt/interpreter/distribution acquisition to the guarded read context and
preserve private-index/source settings. Add bounded, network-aware constrained
resolution so accepted top-level candidates become verified outcomes without
rewriting receipts or changing sources. Then complete permission/ownership checks,
reviewed per-tool operations, and post-action reconciliation before activation.
Local search, GUI/CLI registration, final supported-version policy, and the real
adapter lifecycle matrix remain explicit requirements for #526/#500.
