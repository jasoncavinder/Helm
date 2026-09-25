# uv Global-Tool Runtime and Lifecycle

Date: 2026-09-24. Status: implementation integrated for v0.20; release
certification remains open under #526 and #500. This follows the historical
[eligibility slice](uv-tool-eligibility.md), rather than retroactively changing
its read-only evidence.

## Implemented Scope

- GUI/FFI and CLI register the same `UvToolAdapter`. Environment exposes the
  manager; Library lists installed global tools; installed-tool search is local;
  eligible upgrades enter Plan; structured operations and results use Activity.
- Detect, refresh, list installed/outdated, local search, and individual tool
  install/upgrade/uninstall are supported. A tool distribution is one package,
  even when it exposes multiple executables. Known-name installation is available
  through the existing package-install API/CLI; there is no new PyPI catalog.
- Automatic discovery skips version-manager shims and enumerates bounded concrete
  mise/asdf versions without executing dispatchers. Direct aliases collapse;
  genuinely distinct installations require selection. An explicit choice never
  silently falls back. Retained Homebrew kegs remain excluded from enumeration.
- Commands bind the executable and tool-store filesystem identity before and
  after execution. Missing storage is not authoritative empty inventory; only an
  explicit install may initialize a user-owned store. No elevation is used.
- Store/tool/bin directories must be user-owned and not group/world-writable.
  Receipt reads reject symlinks/nonregular files and are bounded; entrypoints
  must resolve into the selected tool environment. Receipt and interpreter
  identity are rechecked before resolution/execution.

## Resolution and Verification

`uv tool list --outdated` reports latest availability, not necessarily a version
allowed by the installed requirement. Helm instead asks `uv pip compile` to
resolve the complete supported receipt requirements, extras, and constraints
against the tool's existing Python interpreter, using `--upgrade --no-build`.
Discovery cannot execute source build backends or download a Python runtime.
PEP 440 comparisons use the existing exact-pinned uv matcher.

Supported user/system `uv.toml`, explicit `UV_CONFIG_FILE`, receipt options,
and shared uv source environment settings remain authoritative. Tool commands
ignore project and pip-only configuration, so Helm does too. Scalar and array
precedence follows uv; configuration and receipt changes invalidate results.
Credential restoration is restricted to a matching stored/configured index URL.
Scratch inputs are private, task output is bounded/private, and failures never
include raw receipt, index credentials, or subprocess diagnostics.

Upgrade requests bind the cached candidate version and opaque tool-store identity.
Before mutation Helm resolves again and rejects stale scope/version targets.
`uv tool upgrade name==candidate` adds a temporary constraint without replacing
the original installed requirement. Helm pins require explicit unpinning before
an upgrade. Every operation verifies the actual installed version or absence,
entrypoint placement/removal, saved requirement/effective source policy, and
unaffected tools. A zero exit status alone is not success. Failed/incomplete
reads retain committed package state; an authoritative empty listing can clear it.
Shared task cancellation, timeouts, network deferral, and response persistence
remain in use. Offline local inventory/search and removal do not require network;
network discovery/install/upgrade follow Helm's connectivity gate.

## Explicit Boundaries

- The current runtime accepts stable uv 0.12.9 through 0.12.18. Older/newer or
  prerelease versions fail explicitly; widening this tested boundary requires
  command/receipt contract review and lifecycle evidence, not a guessed parser.
- Supported receipts are modern registry requirements with standard version
  specifiers, extras, and constraints. Plain index URLs, extra index URLs,
  absolute find-links sources, no-index, and supported resolver options are
  preserved. Unknown options/schema, relative sources, named per-requirement
  indexes, Git/URL/path/editable requirements, markers, overrides, exclusions,
  build constraints, and source-build-only graphs fail closed for manual review.
  They are not replaced with public-index candidates or reported as current.
- Helm does not manage project dependencies, arbitrary virtual environments,
  Python runtime installation, or dependency-only updates inside an unchanged
  top-level tool. Existing tools with unsupported receipts remain inventory
  observations, not authorized mutations.
- Managing global tools does not authorize installation/update/removal of the
  uv executable. Its Homebrew/mise/asdf owner and existing read-only install
  strategy remain authoritative. The Add manager chooser does not invent a uv
  bootstrap method.
- External concurrent changes and partial subprocess failures are not rolled
  back automatically. Verification failure is reported honestly; refresh and
  review are required before retrying.

## Evidence and Next Gate

Local verification on 2026-09-24 passed: 1,421 Rust tests (five opt-in tests
remain ignored by the default suite), workspace formatting and all-target Clippy,
359 arm64 macOS tests, all seven locale/mirror checks, channel/docs checks, and
SQLite compatibility checks. The separate real adapter lifecycle test passed on
uv 0.12.9 and 0.12.18 with Python 3.14.7 using isolated offline wheel stores.
These are automated development-build results, not signed GUI certification.

The new deterministic tests cover receipt identity and bounds, complete resolver
output, configuration precedence/credential isolation, stale targets, unsafe
storage, redirected entrypoints, cancellation/failure redaction, zero-exit
verification failure, runtime candidate/store binding, pins, offline gating,
local search, FFI Plan routing, and GUI executable-lifecycle restrictions.

An opt-in adapter test uses generated local wheels and a child process with a
clean environment, disposable home/config/cache/tool/bin directories, and no
network. It exercises constrained candidate discovery, incompatible Python and
unsatisfiable dependency rejection, exact pins, unchanged discovery receipts,
reviewed upgrade, verified removal, known-name install, failed install, and
preservation of another tool. It never touches host tools or the stable database.

```sh
HELM_UV_CONTRACT_EXECUTABLE=/absolute/path/to/uv \
HELM_UV_CONTRACT_PYTHON=/absolute/path/to/python3 \
cargo test --manifest-path core/rust/Cargo.toml -p helm-core \
  --test end_to_end_uv_lifecycle -- --ignored --nocapture
```

The owner has offered a disposable Parallels macOS VM with SSH for the all-adapter
certification pass. Before RC sign-off, record actual GUI/CLI lifecycle evidence,
real task cancellation/timeouts, private authenticated sources, conflicting
installations, empty/new stores, failure recovery, supported OS/version coverage,
and every declared adapter capability there. Local fixtures and a Ventura
deployment target do not substitute for those runtime results. Keep #500/#526
open until their acceptance evidence is complete; no release is authorized by
this implementation record.

Upstream basis: uv 0.12.18's
[tool upgrade](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/upgrade.rs),
[configuration combination](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-settings/src/combine.rs),
[configuration discovery](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv-dirs/src/lib.rs),
and [configuration guide](https://docs.astral.sh/uv/concepts/configuration-files/).
