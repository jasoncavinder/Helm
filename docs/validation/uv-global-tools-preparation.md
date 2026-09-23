# uv Global Tools Implementation Preparation

Date: 2026-09-23. Status: investigation and proposed test contract only; **no uv adapter is implemented or certified by this record**.

Scope comes from the [v0.20 roadmap](../ROADMAP.md#planned-uv-global-tool-manager), [feature #526](https://github.com/jasoncavinder/Helm/issues/526), and [all-manager certification #500](https://github.com/jasoncavinder/Helm/issues/500). This preparation does not change the approved scope or the first-RC certification gate.

## Verified Inputs

- Local read-only probes: `uv --version`, `uv tool list --help`, and `uv tool upgrade --help` on uv `0.12.9` (`9f9286029`, aarch64 macOS). No host inventory, tool installation, upgrade, removal, or runtime download was performed.
- The local list help exposes `--outdated`, `--show-paths`, and `--show-version-specifiers`, but no JSON output flag. Do not assume pipx's JSON contract applies to uv or infer a minimum supported version from this single probe.
- Upstream [list implementation at 0.12.18](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/list.rs) emits tool headers followed by executable rows. It can skip malformed or missing environments with warnings and still return success. Its latest-version lookup does not pass the installed requirement's version constraints to the lookup.
- Upstream [upgrade implementation at 0.12.18](https://github.com/astral-sh/uv/blob/0.12.18/crates/uv/src/commands/tool/upgrade.rs) resolves from existing requirements and constraints. The [tools documentation](https://docs.astral.sh/uv/concepts/tools/#upgrading-tools) also states that upgrades retain installation constraints and settings. Source inspection is not a local lifecycle test of 0.12.18.

## Implementation Decisions To Resolve

These are proposed safeguards to implement and test, not claims about current Helm behavior.

1. Define the oldest supported uv version and verify its command/output contract alongside a current version. Probe required capabilities or gate by a tested version boundary; older or unknown output must not masquerade as an empty healthy inventory.
2. Identify one installed tool by its normalized distribution name and selected uv tool store. Executable names are children of that tool, not separate installations. Keep package identity, executable provenance, environment path, and source constraints distinct.
3. Use structured arguments and bounded process execution for inventory, installed-tool search, and per-tool operations. Preserve existing index/receipt ownership; do not copy pipx commands or invent a `--json`/`--dry-run` switch. Establish a neutral working directory so an unrelated project cannot silently change tool behavior.
4. Treat `tool list --outdated` as discovery, not proof that the displayed latest version is an eligible execution target. Exact pins, ranges, exclusions, pre-release policy, Python compatibility, and non-registry sources need explicit treatment. Use Python-package version semantics rather than lexical or SemVer ordering.
5. Resolve the constrained-candidate gap before advertising automatic updates for affected tools. Hiding every constrained tool or repeatedly upgrading it to an unchanged version does not satisfy the roadmap. If a safe target cannot be established, report a truthful limitation and do not silently replace its constraints or source.
6. Reconcile after each mutation. An unchanged version is not proof that the advertised candidate was applied. Retain the selected tool identity, original constraints, and expected result; separately report command failure, verification failure, and constraint-limited/no-change outcomes.
7. Do not update the uv executable through the tool adapter. mise, Homebrew, or another owning manager retains that authority. No project dependency, arbitrary virtual-environment, Python-runtime, or general PyPI-search expansion is included.

## Regression And Certification Matrix

Every row below is **not yet run**. Implement deterministic parser/request/policy regressions first, then record real-environment evidence in disposable profiles or VMs before #500 can close. Help/source inspection above does not count as passing a row.

| Scenario | Required assertion |
|---|---|
| Absent uv, old uv, unsupported flag | Accurate detection/capability limitation; no false empty-success refresh |
| Empty tool store | Empty inventory only from the supported success contract |
| One tool with several entrypoints | One package, exact executable children, no duplicate-install warning |
| Malformed receipt or missing environment | Partial/failed source truth; warnings cannot silently erase cached known tools |
| Multiple uv executables or custom stores | Correct selected executable/store; no cross-install mutation |
| Unconstrained tool | Correct discovery, reviewed per-tool upgrade, observed version verification |
| Exact pin, bounded range, excluded release | No out-of-range automatic target or repeated false Updates Ready state; constraints unchanged |
| Pre-release, local version, extras, extra dependencies | Correct package-version semantics and retained installation settings |
| Git, URL, editable, or local source | Preserve provenance; unsupported mutation stays explicit, without registry substitution |
| Custom/private index | Preserve owning configuration; redact credentials from diagnostics |
| Offline, timeout, cancellation | Cached truth retained; deferred network work and bounded cancellation, without false completion |
| Nonzero exit, success with warnings, failed verification | Distinct truthful results and recovery context |
| Reviewed install/uninstall | Post-action presence/absence verified; no unmanaged executable overwrite |
| Helm pin, deselection, guarded plan | GUI/CLI/service/core agree; no accidental all-tool bypass |

## Delivery Order

1. Version/capability boundary, real output fixtures, deterministic parsing, and executable/store provenance.
2. Registration and read-only inventory/search integration, including localized unsupported/partial states.
3. Constraint-aware candidate determination and reviewed lifecycle operations with reconciliation.
4. Disposable-environment certification of every declared capability and integrated GUI/CLI Plan/Activity behavior.

Do not label an intermediate read-only slice as the completed uv manager, and do not close #526 or #500 until their respective acceptance criteria are met.
