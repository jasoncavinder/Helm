# Disposable VM Cache Maintenance

Read with [the skill's authorization and safety rules](../SKILL.md).
Disposable does not mean the current evidence, installed fixtures or accepted QA
profiles can be discarded. Obtain cleanup scope for the actual guest and roots.

## Establish The Boundary

1. Confirm the intended guest identity and reachable SSH session using the
   existing approved connection. Do not disable host-key checking, embed passwords,
   copy host signing credentials, or assume a historical address is still correct.
2. Record guest free space, evidence-root sizes and active compiler/test/Helm/
   package-manager processes. Coordinate with users/agents before cleanup. Leave
   live targets alone; permission to clean is not permission to interrupt a test.
3. Inventory current evidence and recovery requirements: signed app/helper bundles,
   XCTest runtime, package fixture stores, native receipts, QA HOME/profile trees,
   logs, databases and helper ledgers. Use hashes only for files that are quiescent;
   a live WAL database needs a consistent preservation strategy, not a main-file
   hash claimed as a complete integrity check.
4. Preserve existing VM backup policy. Do not create, delete or revert snapshots,
   clone/shutdown the VM, or treat a previous disposable-VM waiver as new approval
   for a different guest. Cache cleanup need not change the VM's backup setup.

## Classify Precisely

List completed build roots and their sizes, then propose exact compiler-cache
subdirectories. For Rust, consider only proven target profile/target-triple
`build`, `deps`, `incremental`, `.fingerprint` directories. A directory named
`target` inside downloaded source is not proof of a build-output root.

Keep sources, executables, signed products, receipts, `.xcresult` bundles, logs,
database sidecars, fixtures, target metadata/temp directories, installed managers
and accepted profiles. Never use a blanket deletion of `target`, the certification
root, `~/Library`, package stores or Application Support.

Resolve every candidate under its approved canonical root without following
symlink ancestors; reject symlink candidates, unexpected mount boundaries,
ownership changes, unreadable areas and overlapping paths. Check retained
launchers/runtime/artifacts for symlinks or other dependencies into candidates.
Keep referenced caches or separately materialize and validate their dependents
before considering them removable. Do not assume a signed bundle has no external
runtime dependencies merely because its own signature is valid.

Symlink loops, root-owned fixtures and ambiguous paths are reasons to retain and
report, not to broaden permissions, use recursive `sudo`, or follow the links.

## Apply And Verify

Approve the exact manifest before applying it. Recheck candidate identities and
process inactivity immediately before each deletion, using structured arguments
and no wildcard expansion. Apply only that manifest, logging removed paths,
failures and free space. Do not add newly discovered paths opportunistically.

Measure final `df` free space and report the actual delta separately from summed
`du` sizes; hardlinks/shared extents make nominal cache sizes misleading. Verify
retained current evidence paths, signed bundles, XCTest runtime, package receipts
and quiescent ledger/database evidence against the baseline. Do not launch Helm,
run package lifecycles, reseed profiles, or reset a database just to demonstrate
cleanup success. A missing retention check is a blocker to a success claim.

Preserve logs/manifests on the host in the private maintenance evidence directory
when authorized. Never transfer credentials. Record the guest's remaining space
and any unresolved inaccessible/retained items. Do not automatically purge caches
on future builds or enable recurring cleanup; each cleanup retains its approval
and evidence boundaries.
