# uv Installation Attribution and Concrete Selection

Date: 2026-09-24 (UTC)

Status: core-only continuation of [read-scope discovery](uv-global-tools-scope-discovery.md).
This advances #526; it does not activate uv in GUI/CLI, certify #500, or enable mutations.

## Contract

- `UvInstallationRoots` captures absolute configured roots without a login shell,
  project configuration, manager command, or network request. Defaults cover the
  standard Homebrew uv Cellars, `MISE_DATA_DIR` (then `XDG_DATA_HOME/mise`, then
  `$HOME/.local/share/mise`), and `ASDF_DATA_DIR` (then `$HOME/.asdf`). Custom roots
  can be supplied explicitly. Legacy/custom backend layouts outside these roots
  still require explicit configuration or concrete-executable selection.
- `versioned_candidates()` enumerates only immediate uv version directories under
  configured mise/asdf roots, checking `bin/uv`, `uv`, and the two macOS release
  archive subdirectories. There is no recursive search or latest-version choice.
  Directory aliases such as `latest` collapse onto the same canonical candidate;
  separate concrete versions remain separate choices. The caller must explicitly
  select a candidate and pass it through the existing version/store resolver.
- Candidate discovery executes nothing. The limit is 16 roots and 256 total
  directory entries; overflow, inaccessible/dangling paths, unrecognized version
  layouts, non-executable files, known shims, and aliases escaping their version
  directory fail rather than return a truncated candidate set. Absent roots return
  no candidates, which is not an installed-tool inventory or permission to clear it.
- Homebrew roots are used for attribution only, never version enumeration. Retained
  kegs must not become duplicate installations. Direct Homebrew selection continues
  through its linked executable and the existing read-scope resolver.
- Classification uses exact components of the canonical executable relative to a
  canonical configured root, not substring matching or the display alias. Matching
  Homebrew/mise/asdf layouts are hints, not receipts proving package ownership.
  Conflicting roots and unknown layouts retain unknown provenance. In particular,
  `$HOME/.local/bin/uv`, `$HOME/.cargo/bin/uv`, and `/usr/local/bin/uv` alone do not
  prove standalone, Cargo, or Homebrew ownership.
- The shared install-instance classifier records known layout hints with bounded
  confidence (0.60) and keeps uv's automation/update/uninstall/remediation strategies
  read-only. Unknown/conflicting evidence has confidence 0.0. Neither classification
  nor the existing unknown-provenance override grants executable lifecycle actions.
  The uv tool adapter never updates or removes its own executable; owning managers
  retain that responsibility.
- This is not an atomic filesystem scan or executable authentication. Selection is
  revalidated by the existing resolver, and accepted read contexts keep their
  before/after executable and tool-store identity guards. Source constraints,
  tool-store write permission/ownership, Python compatibility, reviewed mutations,
  and runtime activation remain separate gates.

The environment conventions follow [mise directory documentation](https://mise.jdx.dev/directories.html)
and [asdf configuration](https://asdf-vm.com/manage/configuration.html).
[uv installation guidance](https://docs.astral.sh/uv/getting-started/installation/#upgrading-uv)
assigns executable updates to the installing package manager when uv is not a
standalone installation. Helm does not infer standalone authority from a bin path.

## Verification

The Rust quality gate passed with 1,381 tests passed, 0 failed, and 4 intentionally
ignored opt-in tests. Formatting, workspace Clippy, separate all-target Clippy
with warnings denied, docs-sync/release-line, and whitespace checks passed.
Validation used the existing isolated Rust 1.98.1 toolchain without changing host
toolchain configuration. The focused scope and read suites passed all 25 and 10
tests respectively (including opt-in smoke tests) on each of uv 0.12.9 and 0.12.18
on arm64 macOS. This is neither Ventura runtime coverage nor adapter certification.

Deterministic regressions cover configured-root precedence, relative-root rejection,
mise/asdf layouts, canonical alias collapse, explicit selection, unchanged Homebrew
keg treatment, exact-component matching, escaped aliases, conflicting attribution,
unsupported layouts, enumeration limits, and denied executable lifecycle actions.
The existing shim, scope-replacement, cancellation, and persistence tests remain in
the focused suite.

The opt-in versioned-selection test copies the explicitly supplied uv executable
into a disposable mise-shaped directory and follows enumeration, explicit selection,
guarded discovery, and offline inventory through Helm's process/runtime boundary.
It is not a live mise/asdf installation lifecycle test. No host tool inventory or
stable Helm database is read or changed.

```sh
HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
cargo test -p helm-core --manifest-path core/rust/Cargo.toml \
  --test uv_tool_scope_contract --test end_to_end_uv_read -- --include-ignored
```

## Next Gate

Continue per-tool source/constraint/Python eligibility and safe candidate selection;
then reviewed tool mutations with permission checks and post-action reconciliation.
Keep local search, network discovery, GUI/CLI registration, version support policy,
and disposable-environment certification explicit. No owner visual retest is needed
for this core-only slice; the signed QA application is unchanged.
