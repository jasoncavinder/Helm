# Ubuntu CI Baseline Mitigation

Checkpoint: 2026-09-30 (HST). Scope: #600's deadline mitigation, not Ubuntu 26.04
certification or a Helm app release. The later migration is tracked in #609.

## Source and Change

- Main-based hotfix: `75e79777`, PR #610, based on `303dbc93`.
- Dev forward-port: `e27a1348`, based on `a7fbb5fb` (merged #605).
- Both replace all 18 Ubuntu selections across 15 workflows with `ubuntu-24.04`,
  including the three Linux CodeQL entries and scheduled/manual release controls.
- Job/check names, triggers, permissions and macOS selectors are unchanged.
  The dev port preserves its PR concurrency, native-observer tests, certification
  harnesses and Ventura/arm64 checks; these were not backported to main.
- Compiled Cargo cache keys and restore prefixes now include the explicit OS
  baseline, runner architecture and installed exact Rust version. Keys retain
  the lockfile hash. There is no old or cross-baseline restore fallback.
- A pinned CI-only YAML parser and 17 policy regressions enforce these contracts
  in Release Contract Checks. No Helm dependency or runtime code changed.

## Evidence

Both workflow trees pass in the disposable macOS 27 arm64 VM, Python 3.9.6:

- All 22 workflows pass the runner/cache policy.
- All 17 regressions pass, including negative controls for every direct Ubuntu
  job, all Linux CodeQL entries, quoted/YAML-extension inputs, matrix/list
  selectors, broad restore fallbacks, cache key dimensions, Rust mismatch,
  split cache restore/save actions, dynamic selectors/paths and duplicate keys.
- Existing toolchain-pin contract and negative regression pass.
- Actionlint 1.7.8 passes both trees. Its upstream arm64 archive checksum matched
  the release manifest. Dev's eight PR-concurrency regressions also pass.
- Host source-only docs-sync/release-line and whitespace checks pass. No Helm
  runtime test, GUI launch or production data access occurred on the host.

VM evidence is retained under
`/Users/agent/helm-certification/ubuntu-baseline-20260930/logs/`, with copies in
the task worktrees' ignored `artifacts/vm-evidence/` directories.

Hosted evidence for PR #610 source `75e79777`:

- [Rust Core Tests](https://github.com/jasoncavinder/Helm/actions/runs/36820316734/job/110234279022)
  passed on Ubuntu 24.04.5, image `20260927.320.1`. The new
  `cargo-v2-ubuntu-24.04-X64-rust-1.97.1-<lock hash>` namespace had a cold miss,
  completed tests/fmt/Clippy and saved its cache successfully.
- [Release Contract Checks](https://github.com/jasoncavinder/Helm/actions/runs/36820316742/job/110234279135)
  passed, including the 17 new tests under Python 3.12, actionlint, existing
  release fixtures and rehearsal contract. No live publication was dispatched.
- Policy, docs, dependency review, Cargo Audit, Web Build, i18n, SwiftLint,
  Semgrep and Xcode checks also passed at the checkpoint; full head-check and
  independent-review status must still be verified before merge.

## Landing and Limits

Review/merge both protected-branch PRs and verify the remote branches retain no
floating Ubuntu selections before closing the deadline portion of #600. Keep
the deliberate 26.04 actionlint/canary/cold-and-warm-cache migration gates in
#609. Neither static VM checks nor the 24.04 hosted pass certifies 26.04, native
Intel/Ventura behavior, all signed-GUI workflows, or the final v0.20 candidate.
The OS label is version-pinned, not an immutable image. No cache deletion,
signing-credential change, appcast edit or release publication occurred.
