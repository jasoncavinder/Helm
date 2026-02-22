## Summary

- What changed:
- Why:

## Branch Policy

- [ ] Base branch is correct for this scope (`dev`, `docs`, `web`, or `main` hotfix/promotion flow).
- [ ] Head branch naming follows policy (`feat/`, `fix/`, `chore/`, `test/`, `refactor/`, `docs/`, `web/`, `hotfix/`, `release/`, or `chore/publish-updates-*`).
- [ ] If targeting `main`, source branch is valid (`dev`, `docs`, `web`, `hotfix/*`, `release/*`, or `chore/publish-updates-*`).

## Scope Declaration

- [ ] App/core/runtime changes included
- [ ] Docs-only changes included
- [ ] Website-only changes included

## Validation

- [ ] Relevant local validation was run (tests/lint/build as applicable).
- [ ] Required CI checks for the target branch are expected to pass.
- [ ] No unrelated changes were bundled.

## Release Impact

- [ ] No release impact.
- [ ] Release impact exists and checklist/docs were updated (`docs/RELEASE_CHECKLIST.md`, `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`).
