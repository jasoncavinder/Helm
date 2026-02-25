# Helm Pre-1.0 Critical Path Test Plan

Date: 2026-02-25  
Branch context: `chore/pre1-quality-audit`

## Objective

Define the top 10 critical path scenarios for pre-1.0 confidence, with explicit test types and phased automation targets.

## Critical Path Scenario Matrix

| ID | Scenario | Current Coverage (Today) | Suggested Test Types | Automate Now | Automate Later |
|---|---|---|---|---|---|
| CP-01 | Homebrew package lifecycle: install, uninstall, upgrade, pin/unpin (`homebrew_formula`) | Strong adapter unit coverage in `core/rust/crates/helm-core/src/adapters/homebrew.rs`; orchestration-path tests in `tests/end_to_end_homebrew.rs` focus on detect/list/search/outdated, not real install/remove side-effects | unit + orchestration integration + real-manager smoke | Add integration tests for install/uninstall state transitions and idempotent uninstall behavior | Add macOS runner real `brew` smoke with disposable package set |
| CP-02 | Node manager lifecycle under constrained PATH (`npm`, `pnpm`, `yarn`) with selected executable overrides | Adapter unit tests exist (`adapters/npm.rs`, `pnpm.rs`, `yarn.rs`); execution override tests exist in `execution/mod.rs` for PATH/runtime hints | unit + execution-contract integration + CLI integration | Add coordinator/CLI regression tests for selected executable + shebang runtime resolution across install/uninstall/upgrade | Add cross-toolchain matrix e2e (system node vs mise/asdf-managed node) |
| CP-03 | Python/Ruby package manager lifecycle + system-policy guards (`pip`, `pipx`, `rubygems`, `bundler`) | Adapter unit tests exist; manager eligibility policy checks exist in `manager_policy.rs`; no real-manager e2e coverage in CI | unit + policy integration + manager e2e | Add integration tests for policy rejection of system executables and idempotent remove semantics | Add venv/rbenv/mise real-manager smoke flows |
| CP-04 | Tool runtime lifecycle (`rustup`, `mise`) with detect/list/upgrade/uninstall semantics | End-to-end orchestration tests exist: `tests/end_to_end_rustup.rs`, `tests/end_to_end_mise.rs`; timeout-sensitive soak tests and repeat-run driver now exist | integration + soak/stress | Run `scripts/tests/timeout_sensitive_orchestration_soak.sh` (defaults: `5` runs, failure budget `0`) in pre-release/soak validation | Add real-manager upgrade/uninstall e2e on macOS runners |
| CP-05 | Guarded OS/security updates (`softwareupdate`, `xcode_command_line_tools`, `rosetta2`) with explicit confirmation and safe-mode semantics | `tests/end_to_end_softwareupdate.rs` and adapter unit tests validate confirmation token and safe-mode block | unit + guarded integration + manual destructive validation | Add coverage for reject-paths (missing confirmation, safe mode true, disallowed contexts) across all guarded managers | Add scheduled VM/manual canary for destructive system update flows |
| CP-06 | Cross-manager `updates run` / `upgrade_all`: authority order, failure isolation, pin enforcement | `tests/authority_ordering.rs`, `tests/multi_manager_orchestration.rs`, CLI workflow tests in `helm-cli/src/main.rs` | integration + contract tests + CLI non-interactive output tests | Add regression test for mixed manager outcomes with stable exit codes/JSON envelopes | Add long-running concurrency stress suite with injected slow/failing managers |
| CP-07 | Manager install/uninstall lifecycle (`helm managers install|update|uninstall`) and cleanup behavior | FFI/CLI logic present (`helm-ffi/src/lib.rs`, `helm-cli/src/main.rs`), but test depth is thinner than adapter package operations | unit + integration + filesystem side-effect tests | Add tests for install-method selection, persisted preference handling, and uninstall cleanup contracts | Add real-manager install/remove e2e (Homebrew/MacPorts/mise/asdf) |
| CP-08 | Direct CLI self-update and self-uninstall integrity path (`helm self update`, `install.sh`) | URL/checksum/update-policy logic in `helm-cli/src/main.rs`; installer smoke in `.github/workflows/cli-installer-checks.yml`; marker schema contract enforced | unit + integration + script smoke | Add integration tests for redirect handling, oversized payloads, and channel-policy block paths | Add canary update drills against staged release assets |
| CP-09 | Sparkle/direct DMG update integrity and runtime eligibility gating | Release workflow + scripts enforce signing/notarization/appcast policy (`release-macos-dmg.yml`, `verify_release_dmg.sh`, `verify_sparkle_appcast_policy.sh`); Swift tests include update configuration | workflow contract + Swift unit + release artifact validation | Add fixture-based appcast policy contract tests and eligibility-state matrix tests | Add end-to-end updater interruption/recovery automation on dedicated macOS runners |
| CP-10 | Release publication convergence: tag -> artifacts -> publish PRs -> main metadata verify/drift guard | Automated in `release-cli-direct.yml`, `release-macos-dmg.yml`, `release-publish-verify.yml`, `appcast-drift.yml`, `cli-update-drift.yml`; observed transient/policy failures | workflow contract + release rehearsal test | Add deterministic contract tests for merge-order states and prerelease metadata policy behavior | Add full staged release rehearsal per release train (dry-run repo or staging project) |

## Automation Roadmap

### Phase A (Automate Now, low-medium effort)

1. Add integration tests for install/uninstall state transitions and idempotency on top managers (CP-01/02/03).
2. Execute timeout soak regression driver for rustup/mise orchestration paths (`scripts/tests/timeout_sensitive_orchestration_soak.sh`) as part of pre-release validation (CP-04).
3. Add non-interactive CLI contract tests for mixed-success upgrade-all with stable exit codes/JSON schema (CP-06).
4. Add workflow-contract tests for release publish metadata convergence and prerelease metadata expectations (CP-10).
5. Add negative-path self-update integration cases (redirect/host policy/payload bound) for CLI updater (CP-08).

### Phase B (Automate Later, higher effort / infra-heavy)

1. Real-manager e2e matrix across Homebrew + node/python/ruby toolchains (CP-01/02/03/07).
2. Destructive guarded-update validation on dedicated macOS VM/canary lanes (CP-05).
3. Full Sparkle update interruption/recovery automation on signed test artifacts (CP-09).
4. Release rehearsal environment that executes end-to-end publish flow without touching production metadata (CP-10).

## Immediate Acceptance Criteria (Pre-1.0)

1. All Phase A tests are green and required in CI for `main`/`dev` PRs.
2. No known nondeterministic failures in critical orchestration tests after soak runs.
3. Release publish metadata workflows are green for both stable and prerelease policy states.
4. Install/update/remove paths for at least one manager in each authority class are covered by integration tests:
   - Authoritative: `mise` or `rustup`
   - Standard: `npm`/`pipx`/`cargo`
   - Guarded: `softwareupdate` (safe-mode and confirmation gating)

## Notes

- Existing test suite is strong at parser/adapter and orchestration-contract level.
- Highest residual risk is real-environment behavior during mutable manager operations and release metadata convergence edge cases.
