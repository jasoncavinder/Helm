# Host And Git Maintenance

Read with [the skill's authorization and safety rules](../SKILL.md).
The primary checkout is coordination-only. Keep maintenance output private in
the assigned task worktree; do not import old credentials or hardcoded paths.

## Inventory And Classification

Capture the baseline once, then recheck individual targets just before mutation:

```sh
git status --porcelain=v2 --branch --untracked-files=all
git remote -v
git worktree list --porcelain
git for-each-ref --format='%(refname) %(objectname) %(upstream)' refs/heads/ refs/tags/
```

Parse worktree records, not whitespace-separated paths. For each existing path,
record HEAD, branch, lock/detached status, active Git operation and full status
including untracked files. Enumerate ignored material separately without following
symlinks. Unreadable paths, ownership uncertainty and live work mean retain.

Record open PR head branches/SHAs, active reviews and compiler/test/app processes
whose working directories or executables depend on a candidate. A closed PR is
not sufficient: prove the current tip is merged. Stop coordinated writers before
removal; if their inactivity cannot be established, retain the target.

In authorized cleanup mode, inspect remote fetch refspecs/pruning configuration
before `git fetch --all --prune`. Prune only ordinary remote-tracking refs, never
local tags or branches through custom refspecs. Do not use `--prune-tags`.
Resolve the default from the remote's symbolic HEAD, remote metadata or an
unambiguous documented convention. If unresolved, finish with an audit.

For each potential removal, require exact ancestry into a recorded approved ref:

```sh
git merge-base --is-ancestor "$branch" "$completion_ref"
```

Record the actual branch tip and completion-ref SHA, not just names. For multiple
approved refs, record which one proves each candidate complete. Recheck tip and
ancestry immediately before deletion. Squash/rebase equivalence, missing upstream
or a merged PR attached to an older tip is insufficient.

Protect current/default/long-lived branches, every still-checked-out branch,
open PRs and active lanes, and repository-protected names. Conventional protected
names include `main`, `master`, `trunk`, `dev`, `develop`, `development`, `release`,
`production`, `prod`, `staging`, `stable`; prefixes include `release/`, `hotfix/`,
`support/`, `archive/`. Preserve locks, detached checkouts and dirty/untracked work.
Classify remaining candidates as removable, cache-only, or retain, with reasons.

## Recovery Before Removal

Before deleting any checkout or branch, save a verified source-history bundle
outside every candidate checkout:

```sh
git bundle create "$evidence/local-refs-before.bundle" --branches --tags
git bundle verify "$evidence/local-refs-before.bundle"
git bundle list-heads "$evidence/local-refs-before.bundle"
```

Ensure every candidate tip is represented. A bundle does not preserve ignored
files, working-tree changes, LFS content or initialized submodule repositories.
Retain submodule/embedded-repository/LFS candidates unless their separate recovery
requirements have been explicitly resolved; never assume the outer bundle covers
them. Dirty/untracked checkouts remain in place, even if a backup seems possible.

For each clean candidate with ignored files:

1. Enumerate an explicit evidence allowlist and narrowly proven compiler-cache
   exclusions. Keep logs, profiles, receipts, databases (including WAL/SHM),
   binaries, signed apps/helpers, XCTest runtimes, fixtures and result bundles.
   Do not copy credentials into archives; if safe preservation would touch them,
   retain the checkout and report the boundary instead.
2. Ensure no writer is changing that evidence. Recheck path identity, HEAD and
   full status before and after archival. For evidence itself, compare the
   archived inventory/content to the stable source; a readable tar alone does
   not prove completeness. A changed target requires reclassification, not removal.
3. Archive into a temporary name outside the checkout. Preserve symlinks without
   dereferencing, hardlink targets and relevant macOS metadata. Do not extract
   archives over source trees while verifying. Reject unsafe/missing link targets.
4. Read the entire archive, verify its content/metadata and recorded file list,
   then atomically finalize it and store its SHA-256, source HEAD/path/branch,
   sizes and explicit exclusions in a per-worktree manifest. A candidate with
   no ignored material needs a manifest but not an empty archive.
5. Recheck safe-removal conditions and execute plain `git worktree remove`.
   Record refusal and retain it; never work around it with manual deletion.

If preserving ignored data is too expensive, retain the checkout and offer only
the separately authorized, proven cache subset. No archive is better than a
false claim that incomplete evidence can safely replace the original.

## Disk Reserve And Archives

Measure `df` before starting and after each unit. Establish a conservative free-
space floor and budget for a full temporary archive alongside its original plus
ongoing system needs. Stop before breaching the reserve. Recheck during large
archive operations; a between-archive check alone cannot bound disk use.

On exhaustion/interruption, stop the writer and keep the original. Remove only
the known incomplete output created by this run, after confirming the original
still exists; never delete an unrelated file to make room. Resume only from
verified ledger entries and fresh target checks, not the initial plan blindly.

Keep every Time Machine snapshot intact. APFS snapshots can retain deleted cache
blocks while new archives use additional space, so cleanup may temporarily
reduce free space. Report this and retain more source trees rather than thinning
snapshots. `du` sums are estimates, not guaranteed reclaimable capacity.

Optional compaction may discard proven static compiler intermediates such as
`.a`, `.rlib`, `.rmeta`, `.o` only beneath audited Rust `target` or generated-output
roots. Protect every `.app`, `.framework`, `.xctest`, `.xcresult`, `.bundle` subtree
even if it contains one of those extensions. Never use extension-only deletion.

Before replacing an archive, verify its old manifest hash. Compare every retained
member's bytes, type, path, link target, mode, ownership, timestamps and relevant
extended metadata against the original; refuse if a kept hardlink references a
discarded member. Ignore only demonstrably equivalent serialization differences,
never unexplained mismatches. Use a temporary output and atomic replacement after
verification. Update the authoritative manifest hash/size and retain old hash and
exclusion history. Old cleanup-log hashes are then historical, not current.

Large exact exclusion lists can exceed macOS argument limits. Use the archiver's
pattern-file support, escaping literal glob characters and rejecting unrepresentable
names, rather than broadening exclusions. Keep partial outputs identifiable so
an interrupted run cannot mistake one for a verified archive. Do not publish the
one-off maintenance scripts as a generic destructive tool without dedicated tests.

## Narrow Cache Cleanup

Cache-only cleanup still needs approval and clean, inactive, unprotected targets.
Validate canonical roots and every path component; refuse symlink roots, escapes,
unexpected mounts, ownership changes or external references. Do not follow links.

- Rust: after proving a genuine build-output root, candidate subdirectories are
  profile/target-triple `build`, `deps`, `incremental`, `.fingerprint`. Never treat
  arbitrary directories named `target` (for example registry source) as caches.
  Retain top-level executables, reports, fixture temp directories and metadata;
  retain any candidate needed by a saved test runner or symlinked artifact.
- Xcode: read each DerivedData `info.plist` WorkspacePath and map it to an
  explicitly completed archived checkout. Only proven intermediates, index and
  module caches qualify. Keep Products, logs and signed bundles; do not purge all
  DerivedData or caches belonging to active/retained evidence lanes by default.
- Keep shared sccache and package download caches by default. They accelerate
  future work and need separate scope approval to purge.

## Git Mutations And Final Checks

Use `git worktree prune --dry-run --verbose --expire now` before metadata pruning.
Prune only records proved obsolete, not temporarily unavailable mounted paths;
retain locked/unavailable records and report them. Do not confuse metadata pruning
with permission to delete directories.

After successful worktree removal, refresh the checked-out-branch set. Delete an
approved unprotected merged branch only with `git branch -d`. Git may refuse it
because its upstream/current-HEAD test differs from the approved completion ref;
retain it anyway. Do not switch branches, rewrite upstreams or force deletion.

Finish with status/worktree/ref inventories and
`git fsck --connectivity-only --no-dangling`. Verify protected paths/refs and current
archive hashes, enumerate partial/orphan outputs, and record all refusals. Never
silently erase an orphan that might be someone's only recovery copy. Verify a
Git bundle again if there is any uncertainty about its integrity.

Lock the recovery-holding worktree and give its location and restore outline:
inspect/fetch source history from the bundle, then restore retained artifacts
into a fresh isolated checkout after hash validation. Do not overwrite active
work or discard these backups automatically once a PR merges.
