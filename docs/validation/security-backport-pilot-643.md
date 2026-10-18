# Security Backport Pilot: PR #643

Date: 2026-10-17 Pacific/Honolulu (2026-10-18 UTC).

## Scope And Provenance

- Source: [PR #643](https://github.com/jasoncavinder/Helm/pull/643), merged into
  `dev` at `8e7eb1ac8e1e33bc4200ec5c491c082493ccf52d`.
- Selected source commit: `3734546a55a3f11238f6f6034c09fd4a5c07be5d`.
- Stable base: `ea86f279df4ba9f3d1158add27603c451d73ae93`.
- Candidate: [PR #646](https://github.com/jasoncavinder/Helm/pull/646), commit
  `f9f8d1e80cec89c54c5d9f7f0cedabd5754f6b2a`.
- Only `core/rust/Cargo.lock` changes: rustls `0.23.45` to `0.23.46`, with the
  corresponding registry checksum. Two insertions and two deletions.
- Advisories: RUSTSEC-2026-0346, RUSTSEC-2026-0347, RUSTSEC-2026-0348,
  RUSTSEC-2026-0349.
- The source commit's `docs/validation/v0.20-sparkle-ownership-binding.md` and
  the source PR's Sparkle implementation were not copied to stable.

## Verification

The pilot exercised `scripts/security/backport.py prepare` in dry-run and apply
modes, using the existing GitHub CLI authorization. A second identical apply
returned the existing PR without creating a duplicate. The read-only tracking
command reported the open backport with delivery unverified.

The stable Rust workspace compiled with `cargo test --locked --workspace
--no-run`. All 43 resulting test executables ran in the disposable macOS VM with
isolated home, temporary files, caches and Helm database configuration:
**1,311 tests passed, zero failed**. The host was used for compilation only;
the production Helm installation was not launched, replaced or stopped.

An initial VM harness run omitted a documentation fixture required by a CLI
test. After transferring the unchanged stable documentation, the entire set of
43 executables passed. This was a test-harness correction, not a product change.
Doctests and signed GUI upgrade smoke were not part of this local test run.

The fresh [GitHub Cargo Audit run](https://github.com/jasoncavinder/Helm/actions/runs/39098725286/job/117590420333)
passed on the candidate without new advisory ignores. The [Rust Core Tests](https://github.com/jasoncavinder/Helm/actions/runs/39098725210/job/117590419761)
and [Xcode Build Check](https://github.com/jasoncavinder/Helm/actions/runs/39098725210/job/117590419642)
also passed. Always check the PR's current required checks before merging;
these links are evidence for the recorded candidate, not a permanent merge
authorization.

A supplementary VM audit fetched the current advisory database and found no
unsuppressed advisory, but warned that its isolated environment could not update
the crates.io index. The successful GitHub audit, not that incomplete registry
check, is the audit gate evidence.

Pipeline-specific VM validation includes 23 regression tests, actionlint on both
changed workflows, Linux runner policy validation and its 17 regression tests,
plus docs/release-line consistency checks. Regression coverage includes mixed-PR
isolation, authorization, exact source membership, path/mode restrictions,
conflicts, already-applied changes, dry-run non-mutation, duplicate/orphan
handling, repository/tree mismatches and separation of merge from delivery.

## Activation And Delivery Boundaries

- The live pilot exercised the CLI path. The new Actions workflow cannot run
  from default `main` until the pipeline PR merges; issue-comment dispatch and
  scheduled tracking have not yet been exercised live.
- Bot-created PR workflow approval, branch protection and independent review
  remain required. No publication approval is implied by a passing backport.
- No main/dev merge, version bump, release tag, appcast, signing change or
  production deployment was performed for this pilot.
- Installed stable users receive the Rust fix only after a separately approved
  signed app/CLI patch release and artifact/metadata verification.

Review the Rust pilot first. Once it merges, update other main-targeted security
PRs against the repaired stable base and re-run their audits. The pipeline and
website dependency PRs must not bypass Cargo Audit while waiting for this fix.

## Subsequent Activation

PRs #646 and #647 subsequently merged. The owner-authorized
[2026-10-18 activation and development sync](security-backport-activation-20261018.md)
records the successful manual-dispatch dry-run against the existing merged pilot.
It updates the earlier activation status, not the delivery or fresh-write-path
limitations above. No stable app/CLI patch release is certified by that test.
