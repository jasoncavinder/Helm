# Release Flow

This is the mandatory release sequence for maintainers and agents. It supplements
`docs/RELEASE_CHECKLIST.md`; it does not authorize a release mutation.

## Authority And Safety

- Only a maintainer may rotate secrets, change signing/notarization credentials, or approve a public release.
- Agents must keep release work dry-run/checklist-first and request explicit confirmation before creating a tag, publishing a release, dispatching a publishing workflow, merging a publish PR, or deploying website metadata.
- Release artifact source is the immutable tag. A workflow dispatch runs the workflow definition from `main`, so record both the workflow revision and source-tag revision in the run summary/provenance.
- Do not retag or replace a published release. Recover through a PR and a `verify_only` dispatch.

## Required Gates

Run these from a clean, current `main` checkout before creating a tag:

1. Confirm release changes and release-workflow changes have merged to `main`.
2. Run the non-mutating rehearsal:

   ```bash
   scripts/release/rehearsal_dry_run.sh --tag vX.Y.Z
   ```

3. Run preflight:

   ```bash
   scripts/release/runbook.sh prepare --tag vX.Y.Z
   ```

4. Confirm the scheduled or manually dispatched `Release macOS Canary` is green on `macos-26` after the latest workflow/toolchain change.
5. With explicit maintainer approval, dispatch `Release Publish Auth Check` with `write_probe=true`. Require a passing probe that creates and cleans up an empty branch/PR to validate effective Git contents-write and pull-request permissions.
6. Resolve every error before tagging. `RELEASE_PUBLISH_PAT` is required by stable release preflight; its authentication check must pass before publication.

## Mutating Sequence

After explicit maintainer approval:

1. Create and push the annotated tag:

   ```bash
   scripts/release/runbook.sh tag --tag vX.Y.Z
   ```

2. Create or confirm the GitHub release:

   ```bash
   scripts/release/runbook.sh publish --tag vX.Y.Z
   ```

3. Watch `Release macOS DMG` and `Release CLI Direct Installer` to completion. Do not run auxiliary variants until direct GUI and CLI artifacts have published.
4. Read both publication summaries. Artifact upload and metadata synchronization are separate states.
5. If either workflow opens a publish PR, wait for required checks, merge it through the protected-branch flow, then dispatch that workflow with `verify_only=true`.
6. Run the final release verification:

   ```bash
   scripts/release/runbook.sh verify --tag vX.Y.Z
   ```

7. Confirm public `https://helmapp.dev/updates/appcast.xml` and `https://helmapp.dev/updates/cli/latest.json` reference the released version.

## Recovery Rules

- A failed build, signing, notarization, DMG verification, or asset upload is a hard release failure. Fix the confirmed defect and rerun the affected workflow for the existing tag.
- If `RELEASE_PUBLISH_PAT` is invalid, workflows retry metadata branch/PR creation with `github.token`. That fallback requires a maintainer to merge the resulting publish PR and run `verify_only`.
- If neither credential can create the publish branch/PR, retrieve the generated metadata from the workflow artifact, open the documented `chore/publish-*` PR manually, merge it, then run `verify_only`. Do not regenerate or replace release artifacts.
- If a workflow/runtime upgrade fails the canary, update the runner label, expected Xcode major, and immutable action pin together in a dedicated workflow-maintenance PR before the next tag.
- Record recurring friction in `TMP_RELEASE_FRICTION` and promote it to durable docs or contracts before the next release.

## Credential Ownership

`RELEASE_PUBLISH_PAT` is maintainer-owned. It must be authorized for the repository and have the minimum repository permissions needed to read/write contents and create pull requests. Prefer a dedicated GitHub App installation token when repository ruleset support is available. Run `Release Publish Auth Check` with `write_probe=true` after any rotation or permission change.
