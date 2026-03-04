# Quick-Win PR Suggestions

1. **Align Request-Response Wait Timeout With Manager Profiles**  
Affected files:
- `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs`
- `core/rust/crates/helm-core/tests/orchestration_adapter_runtime.rs`

2. **Add Coordinator IPC Permission Regression Tests (CLI + FFI)**  
Affected files:
- `core/rust/crates/helm-cli/src/main.rs`
- `core/rust/crates/helm-ffi/src/lib.rs`
- new test files in `core/rust/crates/helm-core/tests/` or crate-local test modules

3. **Make Manager Override Sync Atomic**  
Affected files:
- `core/rust/crates/helm-ffi/src/lib.rs`
- `core/rust/crates/helm-core/src/execution/mod.rs`

4. **Expose Structured Task Failure Context Through FFI**  
Affected files:
- `core/rust/crates/helm-ffi/src/lib.rs`
- `core/rust/crates/helm-core/src/execution/task_output_store.rs`
- `apps/macos-ui/Helm/Core/HelmCore.swift`

5. **Show Timeout/Error-Code Context In Task Inspector**  
Affected files:
- `apps/macos-ui/Helm/Core/HelmCore.swift`
- `apps/macos-ui/Helm/Views/InspectorViews.swift`

6. **Pin Remaining GitHub Actions To Immutable SHAs**  
Affected files:
- `.github/workflows/ci-test.yml`
- `.github/workflows/codeql.yml`
- `.github/workflows/semgrep.yml`
- `.github/workflows/web-build.yml`
- `.github/workflows/docs-checks.yml`
- `.github/workflows/swiftlint.yml`
- `.github/workflows/i18n-lint.yml`
- `.github/workflows/appcast-drift.yml`

7. **Add Branch-Aware Metadata Drift Policy Doc + Guardrails**  
Affected files:
- `docs/operations/CLI_RELEASE_AND_CI.md`
- `docs/RELEASE_CHECKLIST.md`
- `.github/workflows/cli-update-drift.yml`
- `.github/workflows/release-publish-verify.yml`

8. **Adaptive Backoff For Coordinator Polling**  
Affected files:
- `core/rust/crates/helm-cli/src/main.rs`
- `core/rust/crates/helm-ffi/src/lib.rs`

9. **Cache Manager Enablement Snapshot Per Refresh/Detect Batch**  
Affected files:
- `core/rust/crates/helm-core/src/orchestration/adapter_runtime.rs`
- `core/rust/crates/helm-core/tests/orchestration_adapter_runtime.rs`

10. **Normalize Locale Defaults For Scripted Build/Test Flows**  
Affected files:
- `scripts/release/preflight.sh`
- `scripts/release/runbook.sh`
- `apps/macos-ui/scripts/check_locale_integrity.sh`
- `apps/macos-ui/scripts/check_locale_lengths.sh`

11. **Stable/Beta Messaging Consistency Sweep**  
Affected files:
- `README.md`
- `web/src/components/starlight/Banner.astro`
- `docs/CURRENT_STATE.md`
- `docs/NEXT_STEPS.md`

12. **Coordinator Request Authenticity Token (Incremental Hardening)**  
Affected files:
- `core/rust/crates/helm-cli/src/main.rs`
- `core/rust/crates/helm-ffi/src/lib.rs`
