# Helm SwiftUI Architecture

This document describes the current SwiftUI architecture of the Helm macOS app.

It describes the current implementation lineage through the active `v0.19.x` Original Wayfinder migration. Native shell and Settings foundations coexist with legacy destination hosts until their parity checklists pass.

Target migration references:

- Approved IA and window responsibilities: `docs/app-design/NATIVE_MACOS_INFORMATION_ARCHITECTURE.md`
- Current-to-target component decisions: `docs/app-design/NATIVE_MACOS_COMPONENT_INVENTORY.md`
- Incremental implementation slices: `docs/app-design/NATIVE_MACOS_MIGRATION_MAP.md`
- Complete presentation states and quality gates: `docs/app-design/NATIVE_MACOS_STATE_MATRIX.md` and `docs/app-design/NATIVE_MACOS_QUALITY_BUDGETS.md`

Original Wayfinder presents Dashboard, Plan, Library, and Activity as peer workspaces, keeps Environment persistently available, and routes Settings to a standard separate scene/window. Legacy section hosts remain behind stable mappings during incremental migration. Business rules, planning, orchestration, execution, verification, and recovery remain service/core-owned throughout.

---

## App Entry Point

**File:** `Helm/HelmApp.swift`

```
HelmApp: App (@main)
├── @NSApplicationDelegateAdaptor → AppDelegate
├── @StateObject var core = HelmCore.shared
└── Scene: Settings { ... }
```

The app uses `AppDelegate` for all window, popover, and status item management. SwiftUI's `MenuBarExtra` and `WindowGroup` are **not** used.

---

## Window & Panel Management

**File:** `Helm/AppDelegate.swift`

`AppDelegate` manages three app-owned UI surfaces, while SwiftUI owns the standard Settings scene:

1. **Status Item** — `NSStatusItem` with custom icon and badge overlays
2. **Floating Panel** — `FloatingPanel: NSPanel` for the menu bar popover
3. **Control Center Window** — `ControlCenterWindow: NSWindow` for the full dashboard
4. **Settings Window** — Single-instance SwiftUI `Settings` scene shared by app, status-menu, popover, and Dashboard routes

Key types:
- `FloatingPanel` — Borderless `NSPanel` with `Cmd+F` and `Escape` key handling
- `ControlCenterWindow` — 1120×740 unified compact toolbar window with native sidebar, inspector, search, refresh, and Upgrade All controls plus `Cmd+F`, `Escape`, and `Cmd+W`
- `EventMonitor` — Detects clicks outside the panel to dismiss it
- `VisualEffect: NSViewRepresentable` — Window material backing

Status item features:
- Right-click context menu (About, Upgrade All, Settings, Refresh, Dashboard, Quit)
- Badge overlays: `.count(Int, NSColor)`, `.symbol(String, NSColor)`, `.dot(NSColor)`

---

## State Management

Helm uses **shared singleton `ObservableObject` instances** — not view models or a store/reducer pattern.

### Core Instances

| Instance | Type | Access Pattern | Role |
|----------|------|---------------|------|
| `HelmCore.shared` | `ObservableObject` | `@ObservedObject` / `@StateObject` | UI state, XPC communication, intent dispatch, and presentation projections |
| `ControlCenterContext()` | `ObservableObject` | `@EnvironmentObject` | UI navigation, selection, overlay routing |
| `WalkthroughManager.shared` | `ObservableObject` | `@ObservedObject` | Onboarding walkthrough orchestration |
| `LocalizationManager.shared` | `ObservableObject` | `@ObservedObject` | Locale loading and string resolution |

### HelmCore Decomposition

`HelmCore` is split across 5 files:

| File | Responsibility |
|------|---------------|
| `Core/HelmCore.swift` | Connection, polling, published state properties, XPC setup |
| `Core/HelmCore+Dashboard.swift` | Computed properties: `allKnownPackages`, `filteredPackages()`, `aggregateHealth`, `visibleManagers` |
| `Core/HelmCore+Actions.swift` | Mutation methods: `upgradePackage()`, `cancelTask()`, `togglePackagePin()`, manager operations |
| `Core/HelmCore+Fetching.swift` | XPC data fetching: `fetchPackages()`, `fetchTasks()`, `fetchManagerStatus()`, `fetchSearchResults()` |
| `Core/HelmCore+Settings.swift` | Settings: safe mode, keg cleanup, keg policies, manager enable/disable |

`HelmCore` is a presentation coordinator, not an execution orchestrator. In particular, bulk and scoped upgrade intents are sent to the XPC/Rust workflow path; SwiftUI renders task and workflow state but does not sequence authority phases or poll for phase completion to schedule subsequent managers.

### ControlCenterContext

**File:** `Views/ControlCenterModels.swift`

Shared UI state for the control center:
- `selectedSection: ControlCenterSection` — Active sidebar tab
- `selectedManagerId`, `selectedPackageId`, `selectedTaskId` — Inspector selection
- `searchQuery` — Global search text
- `popoverOverlayRequest` — Active overlay (search, about, quit confirmation)
- `isSidebarVisible`, `isInspectorVisible` — Dashboard split visibility controlled by toolbar and app commands
- Focus and dismiss tokens for keyboard shortcut coordination

---

## View Hierarchy

### Popover (Menu Bar Panel)

**File:** `Views/DashboardView.swift`

```
RedesignPopoverView
├── OnboardingContainerView (if !hasCompletedOnboarding)
│   ├── OnboardingWelcomeView
│   ├── OnboardingDetectionView
│   ├── OnboardingConfigureView
│   └── OnboardingSettingsView
├── Main popover content
│   ├── Health status header
│   ├── Status banner (highest-priority state and action)
│   ├── Active tasks list
│   ├── Manager snapshot grid
│   └── Footer actions (search, settings, quit)
├── Overlay system (search, about, quit confirmation)
└── SpotlightOverlay (walkthrough, 6 popover steps)
```

### Control Center (Full Window)

**File:** `Views/ControlCenterViews.swift`

```
ControlCenterWindowView
├── Native toolbar (sidebar, title, search, inspector, refresh, Upgrade All)
├── HStack / contextual HSplitView
│   ├── ControlCenterSidebarView
│   │   ├── Dashboard/Plan/Library/Activity workspace list
│   │   ├── Environment route
│   │   └── Settings window route
│   ├── ControlCenterSectionHostView (routes to active section)
│   │   ├── RedesignOverviewSectionView (metrics, manager health cards, recent tasks)
│   │   ├── RedesignUpdatesSectionView (staged upgrade preview)
│   │   ├── PackagesSectionView (Views/PackageListView.swift)
│   │   ├── TasksSectionView (Views/TaskListView.swift)
│   │   ├── ManagersSectionView (Views/ManagersView.swift)
│   │   └── SettingsSectionView (Views/SettingsPopoverView.swift)
│   └── ControlCenterInspectorView (Views/InspectorViews.swift)
│       ├── InspectorTaskDetailView
│       ├── InspectorPackageDetailView
│       ├── InspectorManagerDetailView
│       └── Empty state
└── SpotlightOverlay (walkthrough, 7 control center steps)
```

### Settings (Standard Window)

**Files:** `HelmApp.swift`, `Views/SettingsPopoverView.swift`

The SwiftUI `Settings` scene is the sole direct Settings destination and provides standard Command-Comma/single-instance behavior. It currently hosts the existing Settings card body without its Dashboard navigation summary as an explicit transition. Dedicated preference panes and relocation of service diagnostics, support operations, reset, and walkthrough actions remain subsequent parity-gated work.

### Sidebar Sections

| Section | Enum Value | View | File |
|---------|-----------|------|------|
| Overview | `overview` | `RedesignOverviewSectionView` | ControlCenterViews.swift |
| Updates | `updates` | `RedesignUpdatesSectionView` | ControlCenterViews.swift |
| Packages | `packages` | `PackagesSectionView` | PackageListView.swift |
| Tasks | `tasks` | `TasksSectionView` | TaskListView.swift |
| Managers | `managers` | `ManagersSectionView` | ManagersView.swift |
| Settings | `settings` | `SettingsSectionView` | SettingsPopoverView.swift |

---

## Shared Components

| Component | File | Purpose |
|-----------|------|---------|
| `HealthBadgeView` | ControlCenterModels.swift | Status badge (healthy/updatesReady/needsReview/error/running/unavailable/notInstalled) |
| `PackageRowView` | Components/PackageRowView.swift | Package row with status, version, actions |
| `TaskRowView` | Components/TaskRowView.swift | Task row with spinner, description, cancel |
| `ManagerItemView` | Components/ManagerItemView.swift | Manager avatar tile with status dot |
| `HelmPrimaryButtonStyle` | Components/HelmButtonStyles.swift | Helm Blue primary action button |
| `HelmSecondaryButtonStyle` | Components/HelmButtonStyles.swift | Blue-tinted bordered secondary action |
| `HelmTertiaryButtonStyle` | Components/HelmButtonStyles.swift | Text-only lower-emphasis action |
| `HelmProButtonStyle` | Components/HelmButtonStyles.swift | Rope Gold Pro-context action |
| `HelmIconButtonStyle` | Components/HelmButtonStyles.swift | Compact icon action treatment |
| `FilterButton` | Components/FilterButton.swift | Toggle-style filter |
| `LabeledContentRow` | Components/LabeledContentRow.swift | Label + value row |
| `MetricCardView` | ControlCenterViews.swift | Large metric card (overview) |
| `ManagerHealthCardView` | ControlCenterViews.swift | Manager status card (overview) |

---

## Onboarding & Walkthrough

### Onboarding Wizard (First Launch)

**Files:** `Views/Onboarding/Onboarding*.swift`

4-step flow: Welcome → Detection → Configure → Settings

### Guided Walkthrough

**Files:** `Views/Walkthrough/WalkthroughState.swift`, `Views/Walkthrough/SpotlightOverlay.swift`

- **Popover walkthrough** — 6 steps: health badge, attention banner, active tasks, manager snapshot, footer actions, search field
- **Control center walkthrough** — 7 steps: sidebar, overview, packages, tasks, managers, settings, updates

Uses `SpotlightAnchorKey` preference system with even-odd fill cutout and animated transitions.

`WalkthroughManager` persists completion state via `UserDefaults` (separate from onboarding).

---

## Models

| Model | File | Key Properties |
|-------|------|---------------|
| `PackageItem` | Models/Package.swift | id, name, version, latestVersion, managerId, pinned, restartRequired |
| `TaskItem` | Models/Task.swift | id, description, status, managerId, taskType, labelKey, labelArgs |
| `ManagerInfo` | Models/ManagerInfo.swift | id, displayName, category, authority, capabilities, isImplemented |

`ManagerInfo.all` defines metadata for the full 0.14 manager inventory (28 managers) with optional and detection-only flags.

---

## Enums

**File:** `Views/ControlCenterModels.swift`

| Enum | Values |
|------|--------|
| `ControlCenterSection` | overview, updates, packages, tasks, managers, settings |
| `ManagerAuthority` | authoritative, standard, guarded |
| `OperationalHealth` | healthy, updatesReady, needsReview, error, running, unavailable, notInstalled |
| `PopoverOverlayRoute` | search, about, confirmQuit |

---

## Inspector

**File:** `Views/InspectorViews.swift`

Selection priority: `selectedTaskId` → `selectedPackageId` → `selectedManagerId` → empty state.

Selection is coordinated across views — selecting a task clears package/manager selection, selecting a manager clears task/package selection, etc.

---

## Localization

All visible strings use `L10n` key-based lookup via `LocalizationManager`. No hardcoded user-facing text in Swift files.

---

## Constraints

- UI performs no business logic (all logic in `HelmCore` or `ManagerInfo` computed properties)
- No direct process execution in views
- Keyboard Tab traversal does not work (macOS SwiftUI `.focusable()` limitation — requires NSViewRepresentable bridging)
- `Cmd+F`, `Cmd+W`, and `Escape` keyboard shortcuts are functional
