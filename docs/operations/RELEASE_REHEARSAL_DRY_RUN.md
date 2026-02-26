# Release Rehearsal Dry Run

Status: Active operational contract  
Owner: Helm Release Engineering  
Last Updated: 2026-02-26

---

## 1. Purpose

Release rehearsal dry run validates release operator contracts without mutating production release state.

The rehearsal executes three non-destructive phases:

1. preflight (`scripts/release/preflight.sh`)
2. prepare (`scripts/release/runbook.sh prepare`)
3. verify contracts (`scripts/release/tests/publish_verify_state_contract.sh` and `scripts/release/tests/publish_verify_prerelease_state_contract.sh`)

---

## 2. Required Inputs

Required environment:

- a git worktree with Helm release scripts and workflow files
- `bash`, `python3`, `git`, and `gh` available on `PATH`
- GitHub auth available to `gh` with `repo` + `workflow` scopes for preflight checks
- a rehearsal tag in `vX.Y.Z` or `vX.Y.Z-rc.N` format

Recommended rehearsal tag policy:

- use a high non-production semver value not used for real release publication
- CI should derive a unique rehearsal tag per run

---

## 3. Non-Mutation Guarantees

`scripts/release/rehearsal_dry_run.sh` must not:

- create or push git tags
- create/edit/delete GitHub releases
- modify publish metadata on `main` (`appcast.xml`, `latest.json`, `latest-rc.json`)
- open, merge, or close publication PRs

The script is contract-safe by design:

- preflight runs with non-destructive flags
- runbook uses `prepare` only (not `tag`, `publish`, or `verify`)
- verify phase runs deterministic contract scripts against fixture inputs

---

## 4. Operator Command

Example:

```bash
scripts/release/rehearsal_dry_run.sh \
  --tag v99.99.99 \
  --report-path artifacts/release-rehearsal/report-local.json
```

Expected result:

- exit `0` when all dry-run phases pass
- exit non-zero when any phase fails
- machine-readable report written to `--report-path`

---

## 5. Report Contract

Report format is JSON and includes:

- `schema`: `helm.release.rehearsal_report`
- `schema_version`: `1`
- `generated_at` (UTC timestamp)
- `tag`
- `dry_run` (`true`)
- `overall_status` (`passed` or `failed`)
- `steps[]` with per-step:
  - `name`
  - `status`
  - `exit_code`
  - `command`
  - `log_path`
  - `started_at`
  - `finished_at`

---

## 6. CI Integration

`release-contract-checks.yml` runs the rehearsal script in contract mode and uploads the generated report as a workflow artifact for auditability.
