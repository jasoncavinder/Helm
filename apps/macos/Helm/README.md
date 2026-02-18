# Helm Redesign Scaffold

This directory contains a standalone SwiftUI scaffold for the Helm UI redesign initiative.

It is intentionally isolated from `apps/macos-ui` so architecture and interaction choices can be iterated safely before integration.

Structure:
- `App/`: app entry and scene composition
- `Models/`: UI-domain model types
- `State/`: presentation state store
- `Views/`: menu bar, window, sections, components
- `PreviewSupport/`: deterministic mock fixtures
- `Resources/`: localization resources
