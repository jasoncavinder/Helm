# v0.17.5 Release Process Notes (Non-Code-Quality Friction)

## Scope
Track failures or friction in release execution that are not product code defects.

## Log
- 2026-02-24: Initial pre-release quality run on PR #179 failed CI (`Rust Core Tests`) due clippy lint (`collapsible_if`) introduced in branch. Classified as code-quality gate issue, not release-process friction.
- 2026-02-24: Local parallel `cargo clippy` + `cargo test --workspace` caused lock contention and one timeout flake (`detect_rustup_through_full_orchestration_path`) during concurrent heavy load. Serial rerun passed. Classified as local validation artifact; CI unaffected if run serially per workflow.

