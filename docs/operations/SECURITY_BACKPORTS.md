# Security Backports

Security fixes must reach affected stable users without promoting unfinished
`dev` features. A fix merged into `dev`, a backport merged into `main`, and a
verified public delivery are three different states.

## Scope And Authority

The first automated lane supports existing regular-file changes to
`core/rust/Cargo.lock` or `web/package-lock.json`, separately. It does not infer
security scope from a PR title or copy a whole mixed PR. Source-code fixes,
manifest/API changes, GitHub Actions updates, conflicts, squashed/rebased source
commits that are no longer reachable, and dependent multi-commit changes require
a manually adapted `hotfix/*` PR with the same review and delivery requirements.

The source PR must already be merged into `dev`. A maintainer with current write,
maintain or admin access explicitly selects one full non-merge commit SHA, the
lockfile, advisory IDs and delivery type. The selected commit must be both listed
on that PR and reachable from current `dev`; its PR merge must also be reachable.
The request author is responsible for confirming stable applicability and that
the source changes received review. Main-targeted CI and independent review are
still required, even when the source PR passed all checks.

Urgent stable defects may instead be fixed on `main` first and carried to `dev`
through a separately reviewed PR. Do not delay an urgent stable fix behind the
development milestone. No broad `dev` merge is a security backport.

## Request A Backport

After the pipeline itself has merged into default branch `main`, post exactly
one command as a new comment on the merged source PR:

```text
/security-backport FULL_40_CHARACTER_COMMIT_SHA app core/rust/Cargo.lock RUSTSEC-YYYY-NNNN
```

For a website fix, use `website web/package-lock.json` instead. Multiple advisory
IDs are comma-separated without spaces. Commands on issues, unmerged PRs,
non-`dev` PRs or from users without write access are rejected. A comment command
authorizes branch/PR creation, not a release. Edited comments do not trigger it.

The **Security Backport** workflow also offers a manual dispatch on `main` with
the same inputs. Its `apply` checkbox defaults to false: review the dry-run
summary before enabling PR creation. Repository agents must obtain the dispatch
approval required by AGENTS.md; the CLI alternative below needs no workflow
dispatch and defaults to dry-run as well.

```bash
python3 scripts/security/backport.py prepare \
  --source-pr 643 \
  --commit 3734546a55a3f11238f6f6034c09fd4a5c07be5d \
  --paths core/rust/Cargo.lock \
  --advisories RUSTSEC-2026-0346,RUSTSEC-2026-0347,RUSTSEC-2026-0348,RUSTSEC-2026-0349 \
  --delivery app
```

Run from a dedicated task worktree with this repository as `origin`. Add
`--apply` only after permission to create the backport PR. The script does not
change the checked-out branch, index or working files. It constructs the selected
patch in an isolated index, creates an identical GitHub tree on current `main`,
and opens a deterministic `hotfix/security-pr-<number>-<request-hash>` PR. The
summary and PR record source SHA, base SHA, advisories and omitted source files.

PR #643 is the pilot: select only its rustls `0.23.45` -> `0.23.46` lockfile
change. Its accompanying v0.20 validation document and the rest of the PR's
Sparkle implementation are intentionally excluded. Do not cherry-pick the whole
PR or copy the development lockfile over the stable one.

The pilot backport is [PR #646](https://github.com/jasoncavinder/Helm/pull/646).
See its [validation record](../validation/security-backport-pilot-643.md) for
the exact stable diff, test evidence, and deliberately untested workflow edges.
The [activation record](../validation/security-backport-activation-20261018.md)
documents a passing manual dry-run against the merged pilot, separately from
future issue-comment, scheduled and fresh bot-write-path evidence.

## Safety And Recovery

- Only trusted default-branch workflow code runs with write permissions. The
  `issue_comment` workflow does not check out PR/source code or use
  `pull_request_target`. It passes external values as data, not shell code.
- No candidate build, dependency installation or tests run with the preparer's
  write token. Ordinary PR CI validates the candidate separately.
- The workflow uses the repository's short-lived `GITHUB_TOKEN`, not release
  credentials. It requires Actions to be permitted to create PRs. GitHub may
  hold bot-created PR CI in an approval-required state: choose **Approve
  workflows to run**. Missing checks are not a pass; never bypass protections.
- The source/target repository, full SHA, parent count, source membership,
  reachability, exact path set, regular-file modes and patch size are checked.
- A moved `main` tip before remote writes requires a rerun. A later concurrent
  change remains subject to protected-branch integration checks and review.
- Repeating an identical request returns its existing PR, including a closed
  one. It never reopens a deliberately closed PR, force-pushes, overwrites a
  branch, enables auto-merge or modifies existing PR content.
- If branch creation succeeded but PR creation failed, inspect the retained
  branch and open the PR manually after verifying its exact diff. An orphaned
  branch is reported, never silently reused or deleted.
- Already-applied patches and conflicts stop explicitly. Do not broaden the
  selected paths or import unrelated prerequisites merely to make a patch apply.
- No tags, release versions, appcasts, deployment metadata, audit ignores or
  signing credentials are changed by this workflow.

## Verification And Delivery

Before merging a generated PR, require an independent review of the exact stable
diff, a fresh security audit, relevant regression/build checks, and ordinary
branch protections. Run agent-driven app/test workloads only in the disposable
VM with isolated state. Confirm any excluded source documentation or prerequisites
do not conceal a dependency of the selected fix. When the source used a newer
dependency graph, validate the stable graph rather than trusting the dev result.

The daily tracking job (07:43 Pacific/Honolulu) writes a run summary listing each
generated PR as open, closed without merge, or merged-to-main with delivery
unverified. It is a reconciliation report, not a vulnerability scanner or a
claim that a merged patch is still present after subsequent changes. It never
marks a fix released based solely on merge ancestry or a GitHub release name.
The same read-only report is available locally:

```bash
python3 scripts/security/backport.py status
```

Maintain a delivery record in the backport PR with these distinct checkpoints:

1. **Fixed on dev:** source PR/SHA and advisory evidence.
2. **Fixed on stable branch:** backport PR/merge SHA and passing stable validation.
3. **Delivered:** verified publication/deployment evidence, recorded by a maintainer.

For app/CLI changes, step 3 requires a signed stable patch release, artifact and
stable appcast/CLI metadata verification, plus applicable upgrade smoke evidence.
Use [Release Flow](RELEASE_FLOW.md) and [Release Checklist](../RELEASE_CHECKLIST.md)
with explicit publication approval. Merging to `main` does not update users'
installed binaries. Preserve simultaneous RC/stable channels.

For website-only changes, record the production deployment revision, successful
build and a smoke check; no Helm app version bump is needed. CI-only fixes need
their own reviewed workflow PR and successful subsequent workflow evidence.
Security alerts must not be dismissed merely because a backport PR exists.

After workflow/policy changes land on `main`, carry them into `dev` through the
normal reviewed synchronization path so development CI retains the same safety
contracts. Do not merge unfinished `dev` work into `main` to achieve that sync.

The tracking job intentionally leaves delivery unverified; the maintainer's
linked evidence record is authoritative. Future automated delivery reconciliation
must verify actual artifact/metadata/deployment provenance before changing that.

Scheduled vulnerability scanning and routine version-update batching are separate
from this backport lane. The existing dependency-security workflow remains in
place; this change does not claim full npm audit coverage or clear unrelated
advisories.

## References

- [Git cherry-pick](https://git-scm.com/docs/git-cherry-pick)
- [GitHub token workflow approval behavior](https://docs.github.com/en/actions/concepts/security/github_token)
- [GitHub issue_comment event](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#issue_comment)
