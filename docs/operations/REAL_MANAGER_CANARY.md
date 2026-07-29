# Real Manager Canary Runbook

Status: Active operational runbook  
Owner: Helm Release Engineering  
Last Updated: 2026-02-26

---

## 1. Purpose

The real-manager canary is a non-destructive environment validation lane for core manager toolchains.

It validates binary availability and version probe execution for:

- Homebrew manager (`brew`)
- Node managers (`node`, `npm`)
- Python managers (`python3`, `pip3`)
- Ruby managers (`ruby`, `gem`)

It is advisory and intentionally separate from merge-gating PR checks.

---

## 2. Local Invocation

Run the smoke script locally:

```bash
scripts/tests/real_manager_smoke.sh \
  --report-path artifacts/real-manager-canary/report-local.json
```

Exit behavior:

- `0`: all manager groups passed
- `1`: one or more manager groups failed

The report contains machine-readable manager and probe results for triage.

---

## 3. CI Invocation

Workflow:

- `.github/workflows/real-manager-canary.yml`

Triggers:

- `workflow_dispatch`
- weekly schedule (`cron`)

Artifacts:

- `real-manager-smoke.log`
- `real-manager-smoke-report.json`

---

## 4. Triage Guidance

If canary fails:

1. Inspect `failed_managers` in report JSON.
2. Review probe-level `detail` and `exit_code` for the failing manager group.
3. Classify failure:
   - environment drift (`binary not found`, missing default runtime)
   - probe regression (unexpected command failure)
4. Apply targeted action:
   - environment drift: update runner image assumptions or document preconditions
   - probe regression: fix script contract or manager detection assumptions
5. Re-run canary manually (`workflow_dispatch`) after remediation.

---

## 5. Safety Constraints

The canary must remain non-destructive:

- only version/introspection commands are allowed
- no install/upgrade/uninstall actions
- no release/tag/metadata mutation actions
