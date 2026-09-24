# uv Global Tools: Core Read Integration

Date: 2026-09-23

Status: bounded follow-up to the [read-only foundation](uv-global-tools-foundation.md)
for [Issue #526](https://github.com/jasoncavinder/Helm/issues/526). This is not
shipped manager activation or certification under
[Issue #500](https://github.com/jasoncavinder/Helm/issues/500).

Follow-up: [scope discovery](uv-global-tools-scope-discovery.md) now resolves
direct executables and tool directories into guarded read contexts. The explicit
constructor and evidence recorded below remain the original integration baseline.

## Scope and Safety Contract

`adapters::uv_tool_process` provides `UvToolContext`, `ProcessUvToolSource`, and
`UvToolReadAdapter`. Core registry metadata includes `ManagerId::Uv` with only
Detect, Refresh, and ListInstalled. The normal app/CLI runtime does not register
the adapter; FFI still reports uv as unimplemented. There are no manager-install
methods, search/catalog participation, update candidates, or mutation capabilities.

- The caller supplies one immutable absolute executable and tool-store path.
  Relative, parent-traversing, control-containing, and non-UTF-8 paths are rejected;
  equivalent lexical paths are normalized. No PATH or default-store fallback is
  performed. Filesystem canonicalization, selected-install discovery, ownership,
  eligibility, and multi-store aggregation are separate activation work.
- Both commands use the selected executable, an explicit `UV_TOOL_DIR`, a neutral
  working directory, `--directory /`, `--no-config`, `--no-cache`, offline mode,
  and disabled Python downloads. Installed inventory uses `tool list
  --show-version-specifiers --offline`. These configuration restrictions are for
  local inventory only; future network discovery must respect applicable user
  indexes and source policy rather than blindly reusing this scope.
- Every inventory request first rechecks `uv --version`. The provisional gate
  accepts stable 0.12.9 through 0.12.18 only, based on the endpoint command/parser
  evidence. Intermediate patches are allowed but not individually certified.
  Older, newer, prerelease, malformed, and diagnostic-bearing version output fails
  without running inventory. This is a conservative implementation gate, not the
  final supported-version promise.
- Shared process execution supplies task identity and cancellation. Default hard
  deadlines are 10 seconds for version checks and 60 seconds for inventory, with
  a 10-second idle deadline; normal core timeout policy can override defaults.
  Offline `ListInstalled` succeeds through the runtime's network gate. Generic
  `Refresh` retains the existing network policy; no global exemption was added.
- An opt-in shared-process capture mode bounds combined stdout/stderr to 4 MiB
  for uv, suppresses raw output in task logs, and fails on overflow, read error,
  or reader join failure rather than returning a partial capture as success.
  Streams continue draining after overflow until exit or the normal deadline.
  Command, task, timing, and terminal metadata remain available. Other adapters
  keep their existing capture/logging behavior by default.
- The strict foundation parser remains authoritative. One distribution becomes
  one installed package even with multiple executable children. Identifiers use
  the normalized store-path digest and normalized distribution name; the raw path
  is not exported in the identifier. This is namespace separation, not ownership
  proof or a guarantee of anonymous paths.
- Snapshot acceptance changes installed inventory only (`outdated: None`). Parse,
  transport, cancellation, and version errors return no accepted snapshot.
  Confirmed `No tools installed` may clear that selected adapter's inventory;
  silent or partial output may not. The adapter represents one selected store,
  not simultaneous inventory across several stores.
- Raw requirements remain internal observations. They are not promoted to Helm
  pins, source eligibility, actionable versions, or upgrade authorization.

Upstream references: [tool list](https://docs.astral.sh/uv/reference/cli/#uv-tool-list)
and [UV_TOOL_DIR](https://docs.astral.sh/uv/reference/environment/#uv_tool_dir).

## Verification

- Rust workspace quality gate: 1,354 tests passed, 0 failed, 2 opt-in tests ignored;
  formatting and Clippy with warnings denied passed. Separate all-target Clippy
  also passed. Validation used the existing isolated Rust 1.98.1 binaries without
  changing the host toolchain.
- Docs-sync, release-line copy, whitespace, and SQLite migration compatibility
  gates passed. No existing migration definition or schema was changed.
- Nine normal integration tests cover explicit paths, normalized store identity,
  offline inventory with scoped arguments/task identity, version fail-closed
  behavior, distinct store identities, mutation rejection, persistence, and
  cancellation. Disposable SQLite tests wait for the actual persistence completion
  before checking that failed reads preserve inventory and confirmed empty clears it.
- The cancellation regression uses a real blocking subprocess as a uv stand-in
  and verifies that cancellation reaches the shared process boundary and reaps it.
  This is not proof of cancellation during a real uv mutation.
- Shared process regressions cover private output not entering task logs, the
  combined stdout/stderr boundary, oversized output failing instead of returning a
  prefix, and rejection of a zero capture limit. Existing process tests still pass.
- The opt-in empty-store read ran through the real source, shared executor, and
  adapter runtime on arm64 macOS with uv 0.12.9 and 0.12.18. Each uses a fresh
  disposable store with no host-tool inventory or package changes. Nonempty real
  command/parser evidence remains in the foundation record; it does not yet
  exercise a full real Helm adapter lifecycle.

Repeat the isolated empty-store smoke with an explicit executable:

```bash
HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
cargo test -p helm-core --test end_to_end_uv_read \
  --manifest-path core/rust/Cargo.toml -- --ignored --nocapture
```

No GUI QA, installed-build behavior, new release, or user database migration is
claimed. This core-only slice leaves the current signed QA application unchanged.

## Remaining Gates

1. Complete installation ownership/eligibility and concrete version-manager
   selection. Direct discovery and canonical read-scope binding are now covered by
   the follow-up, but are not mutation authority or a final compatibility policy.
2. Add local search and network-aware discovery with Python/PEP 440 constraints,
   pins, source/index eligibility, and unknown-state preservation.
3. Add reviewed mutations and installed-state reconciliation without treating an
   exit code or unconstrained latest observation as verified success.
4. Register the supported capability set consistently across service/GUI/CLI,
   localize its presentation, and run isolated real lifecycle certification.

Project environments, Python runtime management, arbitrary virtual environments,
general PyPI search, and uv executable self-updates remain out of scope.
