# Helm Repository Scaffold

This repository is scaffolded to follow the Helm layered architecture:

- `apps/macos-ui/` (SwiftUI layer placeholder)
- `service/macos-service/` (background service boundary placeholder)
- `core/rust/` (Rust core workspace)

Only the Rust core workspace contracts are initialized in this stage.
No UI or service implementation is included yet.

Helm is currently pre-1.0. See docs/DEFINITION_OF_DONE.md for release criteria.
