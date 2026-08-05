# /release-check

Use this template for release-readiness checks in dry-run mode.

## Inputs

- tag candidate (`vX.Y.Z` or `vX.Y.Z-rc.N`)
- scope (`contracts`, `appcast`, `full-dry-run`)
- channel context (`stable`, `rc`)

## Prompt Template

Run a release readiness dry-run checklist. Do not publish.

Tag: <tag>
Scope: <scope>
Channel: <channel>

Required output format:
1. Checklist commands to run (non-destructive)
2. Pass/fail results by check
3. Blocking issues
4. Explicit manual steps that still require user confirmation
5. Risk notes before any publish/tag/appcast mutation

Safety constraints:
- no release/appcast publication actions
- dry-run/checklist mode by default
- require explicit confirmation before any mutating release step
