# Quality Audit Remediation Checklist

Priority order: highest risk first.

1. Fix orchestration wait-timeout semantics for refresh/search/detect request-response path.  
Effort: M  
Acceptance criteria:
- coordinator wait timeout uses `min(policy_timeout, orchestration_cap)`.
- cap values are documented by operation class.
- regression tests cover policy below/above cap and default behavior.

2. Finalize coordinator IPC hardening follow-up tests (permissions + ownership assumptions).  
Effort: M  
Acceptance criteria:
- unix tests assert state dirs are `0700` and temp/request/response files are `0600`.
- test coverage for both CLI and FFI coordinator paths.

3. Make manager override sync atomic (no clear-then-set race window).  
Effort: M  
Acceptance criteria:
- executable override + timeout profile map update is one atomic swap per map.
- no observable empty-window behavior under concurrent task submission.
- concurrency regression test added.

4. Expand task diagnostics contract from core to FFI/UI with redaction-by-default policy.  
Effort: M  
Acceptance criteria:
- `helm_get_task_output` includes cwd/program/path/exit/termination/error fields.
- UI/CLI default diagnostics remain redacted; full details require explicit reveal/copy/export action.
- backward-compatible decode behavior for older payloads is preserved.

5. Enforce publish metadata truth policy on `main`/release branches only.  
Effort: S  
Acceptance criteria:
- stable metadata truth checks run against `main`/release branches.
- `dev` is not required to carry stable publish artifacts.
- preview metadata, when used, is written to separate preview paths/channels.

6. Pin workflow actions by criticality phases; complete phase 1 pre-release.  
Effort: S  
Acceptance criteria:
- release and security workflows are pinned to immutable SHAs before release.
- CI/test/lint and non-critical automation are tracked in subsequent phases.
- new workflows are SHA-pinned on creation.

7. Migrate coordinator transport to XPC and retire file IPC (or sunset fallback).  
Effort: L  
Acceptance criteria:
- coordinator request/response path is XPC-backed in normal operation.
- file IPC is removed, or explicitly feature-flagged off by default with a one-release sunset plan.
- coordinator transport docs identify XPC as canonical path.

8. Reduce coordinator polling overhead with adaptive backoff or event signaling.  
Effort: M  
Acceptance criteria:
- polling interval is adaptive (or replaced by event-driven mechanism).
- timeout/failure semantics remain deterministic.
- benchmark/trace shows reduced wakeups under idle wait.

9. Cache manager enablement snapshot for orchestration batch operations.  
Effort: M  
Acceptance criteria:
- repeated store scans in hot paths are removed/reduced.
- cache invalidates on manager preference/detection mutations.
- no behavior regressions in enablement/eligibility enforcement.

10. Normalize locale defaults and warning handling in scripts/build tooling.  
Effort: S  
Acceptance criteria:
- no repeated `C.UTF-8` warnings on supported macOS CI/dev hosts.
- scripts still run with strict locale settings and deterministic output.

11. Add cross-layer contract tests for task output schema parity and redaction policy.  
Effort: S  
Acceptance criteria:
- failing test if core captures a field but FFI/UI contract drops it unintentionally.
- failing test if diagnostics redaction defaults regress.
- explicit schema evolution note in docs.

12. Resolve stable-vs-beta wording drift across README/site banners/release docs.  
Effort: S  
Acceptance criteria:
- one canonical message for current channel status.
- consistent wording in README, website banner, and release docs.
