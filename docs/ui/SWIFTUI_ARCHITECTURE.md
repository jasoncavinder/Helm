# Helm SwiftUI Architecture

This document describes the current SwiftUI architecture of the Helm macOS app.

It reflects the actual implementation as of v0.14.0.

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

`AppDelegate` manages three UI surfaces:

1. **Status Item** — `NSStatusItem` with custom icon and badge overlays
2. **Floating Panel** — `FloatingPanel: NSPanel` for the menu bar popover
3. **Control Center Window** — `ControlCenterWindow: NSWindow` for the full dashboard

Key types:
- `FloatingPanel` — Borderless `NSPanel` with `Cmd+F` and `Escape` key handling
- `ControlCenterWindow` — 1120×740 unified compact toolbar window with `Cmd+F`, `Escape`, and `Cmd+W`
- `EventMonitor` — Detects clicks outside the panel to dismiss it
- `VisualEffect: NSViewRepresentable` — Window material backing

Status item features:
- Right-click context menu (About, Upgrade All, Settings, Refresh, Control Center, Quit)
- Badge overlays: `.count(Int, NSColor)`, `.symbol(String, NSColor)`, `.dot(NSColor)`

---

## State Management

Helm uses **shared singleton `ObservableObject` instances** — not view models or a store/reducer pattern.

### Core Instances

| Instance | Type | Access Pattern | Role |
|----------|------|---------------|------|
| `HelmCore.shared` | `ObservableObject` | `@ObservedObject` / `@StateObject` | All data, XPC communication, business logic |
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

### ControlCenterContext

**File:** `Views/ControlCenterModels.swift`

Shared UI state for the control center:
- `selectedSection: ControlCenterSection` — Active sidebar tab
- `selectedManagerId`, `selectedPackageId`, `selectedTaskId` — Inspector selection
- `searchQuery` — Global search text
- `popoverOverlayRequest` — Active overlay (search, settings, about, quit confirmation)
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
│   ├── Attention banner (upgrade-all action)
│   ├── Active tasks list
│   ├── Manager snapshot grid
│   └── Footer actions (search, settings, quit)
├── Overlay system (search, quick settings, about, quit confirmation)
└── SpotlightOverlay (walkthrough, 6 popover steps)
```

### Control Center (Full Window)

**File:** `Views/ControlCenterViews.swift`

```
ControlCenterWindowView
├── ControlCenterTopBar (search bar, health display)
├── HSplitView
│   ├── ControlCenterSidebarView
│   │   └── ControlCenterSidebarRow × 6 sections
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
| `HealthBadgeView` | ControlCenterModels.swift | Status badge (healthy/attention/error/running/notInstalled) |
| `PackageRowView` | Components/PackageRowView.swift | Package row with status, version, actions |
| `TaskRowView` | Components/TaskRowView.swift | Task row with spinner, description, cancel |
| `ManagerItemView` | Components/ManagerItemView.swift | Manager avatar tile with status dot |
| `HelmPrimaryButtonStyle` | Components/HelmButtonStyles.swift | Orange/red gradient button |
| `HelmSecondaryButtonStyle` | Components/HelmButtonStyles.swift | Light background bordered button |
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
| `OperationalHealth` | healthy, attention, error, running, notInstalled |
| `PopoverOverlayRoute` | search, quickSettings, about, confirmQuit |

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
