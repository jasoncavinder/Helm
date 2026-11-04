# November Security Closeout

## Confirmed repository state

Verified November 3, 2026 (Hawaii time; November 4 UTC):

- #653 merged to `main` as `b593036d`: rustls-webpki, webpki-roots and Tokio patches.
- #652 merged to `main` as `6480b892`: targeted website dependency patches and
  full-lockfile weekly/PR audit. Its Web Build and Cloudflare Pages check report
  success for that exact merged revision.
- #650 merged to `dev` as `9a78afe5`, preserving the reviewed tracker correction
  and current security dependencies. Its post-merge Web Build passed.
- Superseded dependency PR #649 is closed; the GitHub Dependabot API returns
  zero open repository alerts. No alert was manually dismissed by this work.
- #651 remains open for the disputed upstream cache-policy behavior and its
  non-blocking dependency maintenance signal. Zero advisory findings do not
  prove that every reported cache edge case is repaired.

App/CLI release versions, tags, appcast/feed pointers, signing configuration,
governing licensing and publication metadata remain unchanged. The stable Rust
patches are merged source, not a new binary release for installed users.

## Tracker follow-up

Carry the already-reviewed #650 correction (`969bae40`) onto current `main`,
without merging any v0.20 runtime work. The Python implementation and regression
tests are byte-for-byte identical to merged `dev`.

- Reject foreign, missing-repository and non-main tracking records before
  parsing contributor-editable markers.
- Emit bounded per-PR warnings for missing, malformed or branch-mismatched
  markers while retaining valid rows.
- Keep API/pagination failures fatal; never infer delivery from merge state.

The workflow's existing permissions, schedule, dispatch and write path are
unchanged. This follow-up is required because the scheduled tracker runs on
`main`, not the development branch where the correction was reviewed.

## Verification and remaining work

The disposable VM passed all 28 tracker tests and 20 runner/cache/audit policy
tests. The same tracker suite against the old implementation failed with seven
failures and nine errors, covering the rejected/spoofed/malformed records rather
than a missing test prerequisite. The script and tests compare exactly with
merged `dev`. Docs/release-line checks and `git diff --check` also pass.

Evidence is retained in
`/Users/agent/helm-certification/security-tracker-closeout-20261103/logs/`.
Tests use isolated HOME/temp/development DB paths and fake GitHub APIs; no token
was copied into the VM and no real workflow write or publication was exercised.
Ordinary fresh PR CI and owner review remain required before merging.

After this main-branch carry-back, resume v0.20's authenticated Sparkle grant
integration, real update-execution/recovery verification, outstanding adapter
dispositions and final integrated-candidate gates from `dev`. The security
maintenance detour does not complete or expand those release gates.
