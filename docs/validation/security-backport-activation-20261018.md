# Security Backport Activation And Development Sync

Date: 2026-10-18 Pacific/Honolulu.

## Approved Scope

The owner authorized a non-publishing activation test and a development sync PR.
No release, tag, appcast, production deployment, credential change or protected
branch merge is authorized by this test. The release flow/checklist were reviewed;
tag-version rehearsal, signing and publication gates do not apply to this
non-publishing workflow and remain required for any later app/CLI release.

The source fixes are already reviewed and merged:

- Rust pilot #646: `85fa664af30eca5729ccad71a1880b2809802c76` on `main`.
- Pipeline #647: `c8a2e0352dd5d587cf29bb800103b576854250ee` on `main`.
- Website security #645: `93f150dfb8d58ef6a522facb29cccb1779d9cadd` on `main`.

## Live Activation Result

[Security Backport run 39151773778](https://github.com/jasoncavinder/Helm/actions/runs/39151773778)
completed successfully on trusted `main` revision
`93f150dfb8d58ef6a522facb29cccb1779d9cadd`. The workflow was active, the maintainer
permission check passed using its short-lived GitHub token, and the job confirmed
`BACKPORT_APPLY: false`.

The request selected source PR #643, commit
`3734546a55a3f11238f6f6034c09fd4a5c07be5d`, `core/rust/Cargo.lock`, delivery
`app`, and RUSTSEC-2026-0346/0347/0348/0349. The job returned:

```json
{
  "status": "existing_pr",
  "url": "https://github.com/jasoncavinder/Helm/pull/646",
  "state": "closed",
  "delivery": "not_verified"
}
```

No duplicate branch/PR was created, and the closed pilot was not reopened. The
read-only CLI tracking report independently returned
`merged_to_main_delivery_unverified` for #646, rather than claiming publication.
It used GitHub metadata only, not the owner's installed Helm or its database.

This verifies the manual-dispatch entry point, trusted checkout, maintainer
authorization, merged-source validation and existing-request lookup. It does
**not** certify the issue-comment entry point, scheduled trigger, bot-created PR
write path or subsequent workflow approval on a fresh backport. Tracking was
skipped in this dispatch by design; its read-only CLI result is not evidence of
a scheduled Actions run. The earlier CLI-created pilot and isolated regression
tests remain separate evidence. No new runtime or release capability was enabled.

## Development Sync

The task branch starts at `dev` revision
`926369069caac49f6aa11238f6e44206ecd94e56` and merges the recorded `main` tip above.
The conflict resolution retains every development-state/next-step paragraph and
the existing PR-concurrency tests, adds the security-backport checks, and avoids
duplicating the Linux-runner documentation already carried into `dev`.

The workflow, script and backport regression tests are identical to the reviewed
`main` versions. The `apps`, `core`, `service` and `locales` trees remain unchanged
from the development base, including its already-fixed rustls lockfile. The only
website change is #645's source-map-js `1.2.1` to `1.2.2` version, URL and checksum;
no website source, public copy, version metadata or licensing changes are added.

Candidate CI must validate the integrated workflow contracts, backport tests,
website build and ordinary protected-branch checks. A passing activation is not
a substitute for those checks or independent PR review. No fresh VM app or GUI
test is claimed for this workflow/documentation/dependency-only sync.

## Remaining Delivery

The Rust fix is present in both development and stable source, but installed
stable users still need a separately approved signed app/CLI patch release and
artifact/metadata verification. This record does not certify #645's production
website deployment. The v0.20 product and release-readiness gates are unchanged.
