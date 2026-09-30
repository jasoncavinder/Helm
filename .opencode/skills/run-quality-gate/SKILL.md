---
name: run-quality-gate
description: Deterministic local validation sweep for Rust, UI, i18n, and release contracts.
license: MIT
compatibility: opencode
metadata:
  audience: maintainers
  workflow: quality-gate
---

# Run Quality Gate

## Purpose

Run merge-readiness checks in a stable order and report exactly what failed.

## When this Skill should trigger

- PR-readiness checks
- CI-like local sweeps
- refactors touching multiple surfaces (`core/rust`, `apps/macos-ui`, release scripts)

## Inputs

- `scope`: `rust`, `i18n`, `ui`, `release-contracts`, or `all`
- optional env: `HELM_SKIP_XCODE=1` to skip `xcodebuild test`
- optional env: `HELM_XCODE_ARCH=arm64|x86_64` to override the detected macOS Xcode test destination arch

## Outputs

- pass/fail status for requested scope
- first failing command (if any)
- short next-step recommendation

## Safety rules

- never hide or skip failing commands silently
- `release-contracts` remains non-destructive
- do not publish/tag/release from this skill
- Read the repository's current runtime-placement policy before selecting a
  command. The helper is a local runner, not a VM dispatcher: `rust` runs tests,
  `ui` normally runs `xcodebuild test`, and `all` runs both. Do not invoke those
  scopes on the host when agent runtime testing is restricted to the VM/CI.
- A migration compatibility script can also run Cargo tests. Inspect transitive
  commands before categorizing any helper as static or host-safe.

## VM-Only Agent Runs

When the owner/repository requires VM runtime testing, split the gate by actual
execution rather than passing `HELM_SKIP_XCODE=1` and claiming everything passed:

1. Confirm the designated guest is reachable and use a new private evidence
   directory with isolated `HOME`, cache/temp paths and explicit `HELM_DB_PATH`.
   Do not reuse the owner's stable database or alter production Helm.
2. Use the host only for permitted source/static work and unsigned compilation.
   `cargo test --no-run` and `xcodebuild build-for-testing` prepare artifacts;
   they do not supply runtime passes. Signing and GUI launch remain separate
   approval boundaries.
3. Run Rust tests, migrations, process/CLI/service checks and app-target test
   bundles in the VM, or isolated CI. If the VM lacks Xcode, use the established
   compatible XCTest runtime and copied test bundle, not a host test fallback.
4. Transfer the required fixture tree, schema files, locale mirrors and contract
   documents with the artifacts. Preserve source-relative working directories
   and any `#filePath` fixture mapping. An absent fixture is a harness failure to
   correct and rerun, not an excuse to omit the test or mark the product broken.
5. Execute release-contract fixtures/stubs in the VM without credentials or
   publication. Real release rehearsal/preflight remains a separately authorized
   source/branch-specific process; do not copy host signing or GitHub credentials
   into the guest merely to make a generic `all` invocation work.
6. Record exact source/subtree and artifact hashes, test totals/ignored cases,
   environment, logs and failed attempts. If source changes after testing, rerun
   affected gates or demonstrate exact unchanged-subtree attribution. Separate
   unsigned compilation, automated runtime, signed GUI and owner acceptance.
7. If an existing Rosetta installation is used, record x86_64 Mach-O and
   translation evidence separately from native Intel hardware/older-OS coverage.
   Native child tools may differ in architecture support. Reproduce failures
   directly before changing product architecture selection or calling a mixed-
   architecture toolchain limitation a Helm regression.

The successful supplied-VM procedure and its evidence limits are illustrated in
`docs/validation/v0.20-unattended-review-index.md`. Guest paths and SSH setup are
task inputs, not permanent credentials or hosts embedded in this skill.

## Steps executed

1. Select the permitted execution environment, then run
   `.opencode/skills/run-quality-gate/scripts/run-quality-gate.sh <scope>` there
   when its prerequisites are present, or split scopes as described above.
2. Execute checks in deterministic order for the selected scope.
3. Stop on first failure and report failure details.
4. Report success only when all selected checks pass.
