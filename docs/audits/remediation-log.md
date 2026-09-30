# Remediation Log

Record completed remediation items with verification evidence.

## Completed

<!-- Add completed items below -->

### PR #587 - Replace yanked yoke-derive

- **Date:** 2026-09-30
- **Changes:** Update only `yoke-derive` from 0.8.3 to non-yanked 0.8.4 in
  `core/rust/Cargo.lock`. This transitive URL/ICU dependency caused Cargo Audit's
  denied-yanked warning; the failed job did not report a vulnerability advisory.
- **Verification:** The original lockfile reproduces the warning and the updated
  lockfile passes the same `cargo audit --deny warnings` policy, retaining the
  existing `RUSTSEC-2024-0436` and `RUSTSEC-2026-0002` exclusions unchanged.
  Locked workspace tests pass in the disposable macOS VM (860 core, 238 CLI,
  110 FFI unit tests plus integration suites). Workspace/all-target Clippy with
  warnings denied and formatting checks pass.
- **Notes:** No audit suppression, dependency requirement, application logic or
  Swift observer change. Fresh PR CI remains required before merge.

## Template

When completing a remediation item, add an entry like:

```markdown
### ID-XXX — Brief description

- **Date:** YYYY-MM-DD
- **Changes:** List of files changed
- **Verification:** Commands or tests that confirm the fix
- **Notes:** Any follow-up or residual risk
```
