# Helm SwiftUI Architecture Proposal

## Goals

- Keep UI layer presentation-only.
- Isolate business logic in service/core boundaries.
- Support menu bar triage and deeper window workflows.
- Keep state deterministic, testable, and preview-friendly.

## Proposed View Hierarchy

App root:
- `HelmRedesignApp`
  - `MenuBarExtra` -> `StatusPopoverView`
  - `WindowGroup("Control Center")` -> `ControlCenterWindowView`
  - optional `Settings` scene -> `SettingsView`

Window structure:
- `ControlCenterWindowView`
  - `SidebarNavigationView`
  - `SectionHostView`
    - `OverviewSectionView`
    - `UpdatesSectionView`
    - `PackagesSectionView`
    - `TasksSectionView`
    - `ManagersSectionView`
    - `SettingsSectionView`
  - `InspectorPaneView`

Shared components:
- `HealthBadgeView`
- `ManagerHealthCardView`
- `TaskStripView`
- `PackageRowView`
- `EmptyStateView`

## State Management Approach

UI state containers:
- `AppStateStore` (`@MainActor`, `ObservableObject`) for scene-level state.
- `SectionViewModel`s for presentation-only derivations.

Data flow:
1. UI intent dispatches typed actions to `AppStateStore`.
2. Store calls service boundary client (`HelmServiceClientProtocol`).
3. Responses update published state.
4. Views reactively render state.

State shape includes:
- current health snapshot
- manager/package/task collections
- active selection/context
- transient UI flags (sheet, alerts, in-flight actions)

No direct process execution in SwiftUI views.

## Menu Bar Integration

- Menu bar icon reflects aggregate status (healthy/attention/error/running).
- Popover is optimized for:
- status glance
- refresh
- upgrade all
- open control center
- Live task strip mirrors current queue state.

## Window and Panel Usage

- Main Control Center window for sustained workflows.
- Confirmation sheets for high-risk mutating actions.
- Inspector panel for per-item context without route changes.

## Localization Strategy in UI Layer

- All visible strings referenced by localization keys.
- No hardcoded user-facing text in Swift files.
- Preview provider defaults to English locale and can inject alternates.

## Testing Strategy

- Snapshot-style SwiftUI tests for primary sections and states.
- Unit tests for store reducers and section view-model mapping.
- Accessibility smoke tests for keyboard traversal and VoiceOver labels.
