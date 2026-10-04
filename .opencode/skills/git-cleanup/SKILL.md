---
name: git-cleanup
description: Audit and safely clean completed Helm branches, linked worktrees, and host or disposable-VM build caches while preserving unfinished work and QA evidence. Use for repository maintenance and development disk-space cleanup, not package removal or release publication.
license: MIT
metadata:
  audience: maintainers
  compatibility: opencode
  workflow: git-maintenance
  safety: conservative
---

# Git Cleanup

Use a bounded inspect, classify, approve, mutate, verify workflow. Do not change
product source or run another cleanup merely because this skill was updated.

## Mode And Authorization

- **Audit:** inspection, classification and recommendations only. Use this mode
  for ambiguous requests or requests to inspect/propose, not remove.
- **Cleanup:** an explicit cleanup request authorizes only its stated targets
  and action classes. Confirm missing boundaries before any destructive step.
- Record the authorized host/VM, cache roots, worktree ownership, completion
  references and evidence destination. A prior run's approvals are not standing
  permission to delete more data. Do not embed credentials or fixed guest hosts.
- Another agent's checkout/branch remains protected unless the user explicitly
  authorizes cross-agent cleanup; authorization does not waive preservation.
- Default completion proof is ancestry into the repository's remote default
  branch. Using `origin/dev` as an additional completion reference requires
  explicit approval; keep both `main` and `dev` themselves protected.
- Preserve all Time Machine snapshots per the owner's instruction. Do not thin
  or delete snapshots or alter backups as a disk-space workaround.

## Safety Boundaries

- Read `AGENTS.md`; use an isolated task worktree via
  [agent-worktree-isolation](../agent-worktree-isolation/SKILL.md).
- Never remove the primary/current/locked checkout or one with tracked changes,
  staged changes, conflicts, untracked files, or an active Git operation. Never
  clean caches inside dirty worktrees. Recheck candidates immediately before
  mutation; a clean initial inventory is not a lease on another agent's work.
- Preserve open PR branches, active review/build/test lanes, detached checkouts,
  protected release/hotfix/archive branches and ambiguous completion evidence.
- Never use hard reset, `git clean`, stash, source restoration, forced branch or
  worktree removal, manual worktree-directory deletion, or remote-branch deletion.
  Plain `git branch -d`/`git worktree remove` refusals are retained, not overridden.
- Missing upstreams, age, names and squash-equivalent patches do not prove safe
  deletion. Require the exact branch tip to be an ancestor of an approved ref.
- Ignored files can be irreplaceable QA evidence. Inspect and preserve them
  before removing a clean worktree; Git status alone is insufficient.
- Preserve the host's production Helm/service, credentials and signing assets.
  Development runtime testing belongs in the VM/CI, not on the host.

## Procedure

1. Record scope/approval and baseline inventories in a private, ignored evidence
   directory within the maintenance worktree. Use structured arguments and quoted
   paths. Do not commit machine inventories, logs, archives or credentials.
2. Read [host and Git maintenance](references/host-and-git.md) for worktrees,
   branches, recovery archives or host compiler caches. Read
   [VM cache maintenance](references/vm-caches.md) only for guest cleanup.
3. Capture worktree paths/HEADs/branches/status, protected PRs/active processes,
   local refs, free space, and intended retention/exclusion reasons. Refresh
   remote knowledge only in authorized cleanup mode. If remote/PR state cannot
   be established, retain affected candidates rather than guessing.
4. Produce an exact candidate manifest. Distinguish worktree removal, local
   branch deletion, regenerable-cache removal and metadata pruning. Keep the
   protected set visible. Resolve approval gaps before applying this plan.
5. Apply one bounded pass, rechecking each target and disk reserve. Log successes,
   refusals, errors and any changes in candidate state. Never restart a full
   destructive sweep automatically after partial completion.
6. Verify once at the end: all retained paths still exist, removed paths/refs are
   gone, archives match their manifests, remaining worktrees have valid refs,
   production Helm is undisturbed, and VM evidence is intact. Record incomplete
   checks and failures honestly; unchanged failures are not a reason to force.

## Handoff

Report mode, approved completion refs, before/after free space, actual counts of
removed worktrees/local branches/caches, and meaningful retained/blocking items.
Separate estimated `du` sizes from observed free-space gains; APFS snapshots,
clones and hardlinks can make these differ substantially.

State current branch/upstream and worktree status, remaining dirty worktrees,
verification results, and the private recovery location. Do not claim the whole
repository is clean when only the maintenance checkout is clean. Do not claim
release-readiness from maintenance checks.

Lock an evidence-holding maintenance worktree with an explanatory reason and
report it as **retained**, not ready for cleanup. Retire or move its verified
recovery data only with separate authorization. Do not schedule future cleanup.
