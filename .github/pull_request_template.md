## Summary

- What changed:
- Why:

## Branch Policy

- [ ] Base branch is correct for this scope (`dev` or `main` hotfix/promotion flow).
- [ ] Head branch naming follows policy.

Accepted prefixes for PRs into `dev`:
  - `feat/`, `fix/`, `chore/`, `test/`, `refactor/`, `agent/`, `hotfix/`, `release/`, `docs/`, `web/`
  - Dependabot is an accepted automated exception.

Accepted source categories for PRs into `main`:
  - `dev` (primary branch promotion)
  - `hotfix/*`, `release/*`
  - `chore/publish-updates-*`, `chore/publish-cli-updates-*`
  - `hotfix/cli-updates-emergency-*`
  - Dependabot where supported

- [ ] If targeting `main`, source branch is valid per the categories above.

## Scope Declaration

- [ ] App/core/runtime changes included
- [ ] Docs-only changes included
- [ ] Website-only changes included

## Validation

- [ ] Relevant local validation was run (tests/lint/build as applicable).
- [ ] Required CI checks for the target branch are expected to pass.
- [ ] No unrelated changes were bundled.
- [ ] If docs were changed, terminology matches contract (`manager`/`task`/`service` for user-facing docs; `adapter` reserved for architecture/developer docs).

## Release Impact

- [ ] No release impact.
- [ ] Release impact exists and checklist/docs were updated (`docs/RELEASE_CHECKLIST.md`, `docs/CURRENT_STATE.md`, `docs/NEXT_STEPS.md`).

## SQLite Migration Safety

- [ ] This PR does not change SQLite migrations or migration behavior.
- [ ] If it does, the change only appends a new migration and manifest entry; frozen origin fixtures and preservation/rollback/reset tests were updated.
- [ ] If it does, `scripts/ci/check_sqlite_migration_compatibility.sh` passed and the migration safety changes received independent review.
