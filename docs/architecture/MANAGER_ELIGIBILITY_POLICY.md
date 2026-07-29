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

Policy checks are applied in these places:

- manager status computation (`enabled` is effective `configured && eligible`)
- manager enable action gate (`enable` rejected when ineligible)
- runtime submission gate (ineligible treated as disabled)
- startup/status self-heal (persist auto-disable for stale invalid states)

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
