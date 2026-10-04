# Manager Eligibility Policy

This document defines manager `detected` vs `manageable` policy.

Detection and manageability are separate:

- `detected=true` means Helm found an installation.
- `is_eligible=true` means that installation is allowed for Helm-managed actions.

Core source of truth:

- `core/rust/crates/helm-core/src/manager_policy.rs`

## Lessons Learned

1. Separate `detected` from `manageable`; never assume they are equivalent.
2. Enforce policy centrally in core, not only in one UI surface.
3. Block at enable-time and at runtime task submission.
4. Self-heal stale state (auto-disable invalid enabled managers).
5. Return structured reason codes and localized service keys.
6. Distinguish policy-blocked from permission-blocked:
   - policy-blocked: no escalation path
   - permission-blocked: escalation may be valid

## Current Matrix

| Manager | Policy Status | Blocked Executable Criteria | Reason Code | Service Error Key |
|---|---|---|---|---|
| `rubygems` | blocked on macOS base-system executable | `/usr/bin/gem` (exact or canonical) | `rubygems.system_unmanaged` | `service.error.rubygems_system_unmanaged` |
| `bundler` | blocked on macOS base-system executable | `/usr/bin/bundle` (exact or canonical) | `bundler.system_unmanaged` | `service.error.bundler_system_unmanaged` |
| `pip` | blocked on macOS base-system executable | `/usr/bin/python3`, `/usr/bin/pip`, `/usr/bin/pip3` (exact or canonical) | `pip.system_unmanaged` | `service.error.pip_system_unmanaged` |
| `npm` | no hard policy block currently | n/a | n/a | n/a |
| `pnpm` | no hard policy block currently | n/a | n/a | n/a |
| `yarn` | no hard policy block currently | n/a | n/a | n/a |
| `homebrew_formula` | no hard policy block currently | n/a | n/a | n/a |
| `macports` | no hard policy block currently | n/a | n/a | n/a |

## Enforcement Points

Manager-wide eligibility does not guarantee every operation is supported by the
selected executable version. The pnpm adapter checks the selected version before
package install, upgrade (targeted or bulk), and removal. Stable numeric versions
1 through 10 retain the legacy mutation path; bounded live certification is for
10.16.1 only. Version 11+, prereleases and unrecognized versions fail with
`unsupported_capability` and the `pnpm_global_mutation_unsupported` diagnostic
marker before mutation. Their native install groups can affect packages beyond
the individually reviewed item. Detection, inventory, search, update discovery
and Helm virtual pins remain available; this does not auto-disable the manager
or hide updates. Task diagnostics translate the guidance using
`service.error.pnpm_global_mutation_unsupported`. See the
[capability matrix](../validation/v0.20-adapter-cli-capability-matrix.md) and
[#559](https://github.com/jasoncavinder/Helm/issues/559). Shared-core version policy
now drives the cached CLI/FFI package capability fields and a localized
`packageMutationServiceErrorKey` on both CLI and FFI manager-status payloads.
Unsupported/unknown pnpm snapshots retain visible updates but mark Plan rows
`blocked`; they cannot be selected, automatically run or retried by the GUI.
CLI previews add `runnable` and `blockedServiceErrorKey`; `updates run` reports
the blocked result without creating a package task, including detached workflow
execution. Manager enablement, reads, virtual pins and executable lifecycle
authority are unchanged. A fresh manager detection restores capabilities after
an intentional executable/version change. Preview uses cached evidence, never
authorization: the adapter still checks the live version before every mutation.
Group-aware planning/consent remains follow-up work; see the
[preview verification record](../validation/v0.20-pnpm-mutation-availability.md).

Uv global-tool policy: the registered GUI/CLI adapter does not grant uv executable
lifecycle authority. Its install-instance classifier retains conservative
Homebrew/mise/asdf layout hints and read-only executable strategies, even with
unknown-provenance override. Per-tool operations require guarded concrete scope,
user-owned non-writable-by-others storage, matching receipts/entrypoints, and
post-action verification. Updates additionally require a current unpinned cached
candidate/store target and fresh source-aware dependency/Python resolution;
PEP 440 filtering alone is not authorization. Unsupported versions or source
policies fail closed without replacing cached state or falling back to PyPI.
See [lifecycle scope and certification](../validation/uv-global-tools-lifecycle.md),
[installation policy](../validation/uv-installation-policy.md), and
[candidate eligibility](../validation/uv-tool-eligibility.md).

An absent uv tool store does not itself authorize empty inventory. Read-only
search/inventory/refresh may reconcile an empty result only after bounded native
uv confirmation with stable executable, absent-path and surviving-ancestor
evidence. Failed or contradictory probes preserve cached state. Reads never
initialize storage or gain mutation authority from this proof.

Package uninstall confirmation must use per-package capabilities and effective
manager enablement, not install-instance `automation_level` or manager-uninstall
strategy. Those fields describe removal of the manager executable itself. In
particular, read-only uv executable lifecycle metadata does not prohibit supported
uv tool removal. GUI/TUI confirmation and execution retain their distinct checks;
the adapter still validates live scope, receipts and supported versions. See the
[confirmation correction](../validation/v0.20-package-uninstall-confirmation.md).

Cargo upgrades additionally require a supported native installation receipt at
execution time. Crates.io upgrades preserve the existing binary/features/profile/
target choices and explicit root; Git/path/private registries, source overrides
and incomplete or ambiguous metadata fail closed with `cargo_receipt_unsupported`.
This does not disable Cargo discovery. Cached upgrade workflows additionally
retain the [discovered source/scope binding](../validation/v0.20-cargo-reviewed-scope.md).
Fresh installs and existing-package reinstalls use the
[published-lock install policy](../validation/v0.20-cargo-install-policy.md), with
exact task-start candidates, unrelated-state verification and native option
preservation. Unsupported or missing published locks require manual handling;
Helm never silently resolves unlocked dependencies. Explicit install and unbound
low-level calls are not a new durable cross-command approval transaction.

Policy checks are applied in these places:

- manager status computation (`enabled` is effective `configured && eligible`)
- manager enable action gate (`enable` rejected when ineligible)
- runtime submission gate (ineligible treated as disabled)
- startup/status self-heal (persist auto-disable for stale invalid states)

## External App Adoption

External Sparkle adoption is separate from manager enablement and executable
lifecycle authority. The owner-approved per-app policy permits an explicit
"Let Helm manage updates" review when fresh native checks succeed but historical
standalone provenance cannot be proved. It never overrides a known competing
manager claim or unreadable evidence. Native installer receipt claims on the
observed app/metadata/executable and known Setapp bundle markers are denial
signals too; their absence is not complete exclusion coverage. The
[bounded observer](../validation/v0.20-sparkle-receipt-exclusions.md) remains
read-only and cannot grant adoption. Migration 24 records revocable consent;
core reports `UserAdopted`, not `Standalone`, and still requires a separate exact
candidate review and durable install handoff. No shipping capability is enabled
by this core foundation. See the [adoption contract](../validation/v0.20-sparkle-adoption.md)
and [external boundary](EXTERNAL_SPARKLE_BOUNDARY.md).

Permission removal is deliberately not installation eligibility. The
[authenticated revocation review](../validation/v0.20-sparkle-consent-revocation.md)
can remove saved permission for an absent or newly ineligible allowed-root app;
it cannot grant permission, override manager ownership or abort an already
handed-off installer. Exact ledger revision checks prevent an old review from
revoking a newer grant. Shipping adoption/revocation controls remain gated.

## Adding A New Rule

1. Add rule and constants in `manager_policy.rs`.
2. Add localized service key in both locale trees:
   - `locales/*/service.json`
   - `apps/macos-ui/Helm/Resources/locales/*/service.json`
3. Ensure FFI/CLI/TUI/GUI surfaces show eligibility + reason.
4. Add tests:
   - policy unit test
   - runtime submission block test
   - status payload eligibility test
   - self-heal behavior test
