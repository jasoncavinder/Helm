# Bootstrap Feasibility and Typed-Action Matrix

Status: v0.18 planning audit against current core/service routes

This matrix establishes what can enter `0.19.x` first-run planning without inventing arbitrary execution. It distinguishes an existing typed core/service route from a first-run setup-session action, which is not implemented in v0.18.

Rule: a current lifecycle or repair route is necessary but not sufficient. Before first-run apply, v0.19 must wrap it in the setup-session plan contract, revalidate current state, preserve policy/consent gates, execute through orchestration, verify, and record an Action Receipt.

## Candidate Improvement Matrix

| Candidate | Current executable route | Current verification | Recovery/rollback truth | v0.19 first-run status |
|---|---|---|---|---|
| Clear stale selected executable | Compiled repair action `manager.clear_selected_executable_override` in `helm-core::repair` | Follow-up manager detection resolves or retains finding | Prior preference can be restored only while the snapshot remains valid | **Eligible after setup-session wrapper** |
| Complete mise/rustup/asdf post-install setup | Compiled repair action `manager.apply_post_install_setup_defaults` | Follow-up setup check and manager detection | Remove only the exact Helm-owned shell block; otherwise forward recovery | **Eligible after setup-session wrapper** |
| Repair metadata-only Homebrew manager install | Compiled repair actions `homebrew.reinstall_formula` and `homebrew.uninstall_formula` | Follow-up doctor scan | Forward recovery; uninstall has explicit confirmation and impact | **Eligible after setup-session wrapper** |
| Enable an eligible detected manager | Existing core/FFI/service preference route with eligibility/dependency enforcement | Manager status reflects effective enabled state | Restore prior preference if still policy-valid | **Eligible after setup-session wrapper** |
| Install Helm CLI shim | Existing GUI/service integration with Helm ownership marker | Shim path and ownership marker rechecked | Remove only when Helm-created ownership still matches | **Eligible after setup-session wrapper** |
| Review an update plan | Existing backend-owned upgrade-plan path | Revalidate plan and post-upgrade manager/package state | Review is no-change; package rollback is not promised | **Eligible as review-only foundation** |
| Install mise | `plan_manager_install`: script installer, Homebrew, MacPorts, or Cargo route | Manager-scoped detection/refresh plus setup check | Provenance-specific uninstall only when current policy permits | **Conditional after first-run action wrapper and source-integrity gates** |
| Install rustup | `plan_manager_install`: rustup-init or Homebrew route | Manager-scoped detection/refresh plus setup check | Provenance-specific uninstall; no generic rollback promise | **Conditional after first-run action wrapper and source-integrity gates** |
| Install asdf | `plan_manager_install`: script installer or Homebrew route | Manager-scoped detection/refresh plus setup check | Provenance-specific uninstall where supported | **Conditional after first-run action wrapper and source-integrity gates** |
| Install mas | `plan_manager_install`: Homebrew route | Manager-scoped detection/refresh | Homebrew uninstall only when provenance/dependency policy permits | **Conditional after first-run action wrapper** |
| Install npm/pnpm/yarn/pip/pipx/poetry/rubygems/bundler/cargo/cargo-binstall/podman/colima | Current manager lifecycle planners route supported methods, primarily through Homebrew | Manager-scoped detection/refresh | Provenance/dependency-specific; some parent-formula installs have broad impact | **Defer from smallest v0.19 first-run set pending per-manager plan fixtures** |
| Install Homebrew itself | No `plan_manager_install(ManagerId::HomebrewFormula, ...)` route | Detection exists; package/update operations require an existing installation | No verified installer rollback | **Not eligible; dedicated typed installer workflow required** |
| Install Xcode Command Line Tools | No manager-install planner route; current scope is guarded detection/status/update handling | `xcode-select`/adapter detection | No rollback promise | **Not eligible; dedicated typed guarded workflow required** |
| Install MacPorts itself | No manager-install planner route for `ManagerId::MacPorts` | Detection/version checks exist | Installer rollback not verified | **Not eligible** |

## Current Core Evidence

- Manager install planners: `core/rust/crates/helm-core/src/manager_lifecycle.rs` (`plan_manager_install`).
- Planner-exposed methods: `core/rust/crates/helm-core/src/registry.rs` plus `manager_supported_install_methods` filtering.
- Compiled repair registry: `core/rust/crates/helm-core/src/repair.rs`.
- Post-install setup verification and apply boundary: `core/rust/crates/helm-ffi/src/lib.rs` and doctor/repair modules.
- CLI shim ownership/status route: `apps/macos-ui/Helm/Core/HelmCore+Settings.swift` through the service boundary.

## v0.19 Admission Gate

A candidate becomes executable from first run only when all are true:

1. The action ID resolves to a finite compiled planner/action implementation; the schema never carries a command, argument vector, script, URL-selected executable, or arbitrary substitution.
2. Plan input records manager/source, network, privilege, files/settings, dependency branch, verification, and recovery limits.
3. Current environment, policy, consent, provenance, eligibility, dependency, disk/power where applicable, and plan revision are revalidated immediately before submission.
4. Execution uses existing coordinator/orchestration tasks and real cancellation semantics.
5. Post-verification observes the declared result before UI/CLI says Verified.
6. Receipt records applied, verified, unchanged, failed/unverified, and rollback-limited state.

The smallest v0.19 exceptional set should begin with review-only plans and one or more already-compiled repair/preference actions. Broad bootstrap installation remains conditional or deferred until its dedicated fixtures and integrity policy pass.
