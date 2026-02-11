# Helm Rust Core Workspace

This workspace holds the UI-agnostic core contracts for Helm.

Current scope:
- Shared domain models
- Manager adapter trait and request/response contracts
- Orchestration engine and task queue
- SQLite persistence
- Homebrew adapter implementation
- Process execution strategy (`TokioProcessExecutor`)

Out of scope in this stage:
- UI integration
- XPC service boundary wiring

