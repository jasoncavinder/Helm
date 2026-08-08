# Native macOS Incremental Migration Map

Status: approved planning handoff for v0.19-v0.22

## Invariants

- SwiftUI/AppKit own presentation, input, application menus, windows, platform integration, and accessibility semantics.
- Rust/service own business rules, detection, policy, eligibility, provenance, recommendation/planning truth, orchestration, cancellation, verification, receipt/recovery truth, and persistence.
- Existing XPC/FFI contracts evolve additively and remain version/revision aware. UI does not sequence authority phases or infer terminal truth from visual state.
- Each slice can ship behind a narrow route/feature flag or by destination. Do not maintain duplicate business logic to support old and new presentation.
- Locale keys are added to canonical `locales/` and mirrored to app resources with parity checks.
- No slice touches signing, notarization, release metadata, appcasts, or credentials.

## v0.19: Native Foundation and First-Run Value

### Experience Direction Lock

Before production information-architecture or styling work begins in Slices 19.2 and 19.4, review the code-rendered Dashboard and status-item popover proposal in `docs/app-design/proposals/v019-visual-direction/` and record one explicit approved experience direction. Proposal work remains in the non-shipping `tools/design-lab/` package and cannot enter the application target by implication.

The proposal deliberately reopens the approved five-destination navigation model, but it does not change the canonical application contract until the product owner approves that change and the architecture documents are updated. Slice 19.1 command/window plumbing may proceed while this review is open because it establishes native behavior without locking the final destination model, materials, density, or product styling. The approved experience must then be validated at minimum and expanded window sizes, in light/dark and key/inactive states, across healthy, attention, failure, partial-data, offline, and active-work states before broad component migration.

### Slice 19.1: Application command and window foundation

Affected files/components:

- `apps/macos-ui/Helm/HelmApp.swift`
- `AppDelegate.swift`, `AppDelegate+Windows.swift`
- `Views/ControlCenterViews.swift`, `ControlCenterModels.swift`
- New presentation-only command/window files under `apps/macos-ui/Helm/`

Native primitives:

- Standard `NSWindow` or SwiftUI Window scene compatible with accessory activation.
- `NSToolbar` / SwiftUI toolbar, app `Commands`, standard menu groups.
- Frame autosave/restoration, key/main/inactive state.

Dependencies:

- Decide AppDelegate versus Scene ownership without duplicating windows.
- Stable deep-link destination/entity model.
- macOS 11 availability strategy.

Boundary risks:

- Command enablement must query presentation projections/core capability, not duplicate policy.
- Window close must not cancel workflows.

Localization/accessibility:

- App/Edit/View/Window/Help command labels across all locales.
- Menu key equivalents, toolbar labels/help, window title, FKA order.

Validation:

- Open/close/minimize/zoom/activate on multiple Spaces/displays.
- Command-Comma, Command-F/W/Q, View sidebar/inspector commands.
- `860x600` and `1280x800`, light/dark/key/inactive.

Rollback/incremental strategy:

- Keep old content host inside new window frame first.
- Route one shared `openControlCenter` command to the new window; revert host wiring without changing core.

### Slice 19.2: Native sidebar, split view, selection, and focus bridge

Affected files/components:

- `ControlCenterViews.swift`, `ControlCenterModels.swift`
- Destination views and inspector host
- Targeted new AppKit representables for key-view/focus gaps

Native primitives:

- `NavigationSplitView`/sidebar `List(selection:)` where baseline behavior passes; otherwise `NSSplitViewController` + source-list bridge.
- Native trailing inspector split item and thin dividers.

Dependencies:

- Approved five-destination IA.
- Stable selection IDs and restoration payload.

Boundary risks:

- Selection projections must not become business state.
- Avoid a repository-wide state-container rewrite; adapt `ControlCenterContext` incrementally.

Localization/accessibility:

- Health, Updates, Packages, Activity, Sources labels and shortcuts.
- Arrow/type selection, FKA groups, focus restoration, inactive selection.

Validation:

- Automated selection/restoration tests where feasible.
- Manual FKA/VoiceOver through all panes and hidden inspector/sidebar states.

Rollback/incremental strategy:

- New shell can host current section views one at a time.
- Preserve old section enum mapping only during migration; remove after all destinations move.

### Slice 19.3: Standard Settings window

Affected files/components:

- `HelmApp.swift`
- `Views/SettingsPopoverView.swift`
- `AppDelegate.configureStatusMenu()`
- Settings-related `HelmCore+Settings.swift` projections/actions

Native primitives:

- SwiftUI `Settings` scene or standard AppKit settings window.
- Grouped Form, standard toggles/pickers, noncustomizable pane navigation.

Dependencies:

- Command/app-menu foundation.
- Placement map from IA.

Boundary risks:

- Do not move live service diagnostics or source operations into preference models.
- Saving errors remain core/service truth.

Localization/accessibility:

- Pane titles, form labels/help, managed-setting reason, text expansion.

Validation:

- Command-Comma and single-instance behavior.
- All locales, minimum pane sizes, keyboard/VoiceOver, save failure fixture.

Rollback/incremental strategy:

- Move one card/pane at a time; old Settings section links to real Settings during transition.
- Remove duplicated CLI card and old section only after parity checklist passes.

### Slice 19.4: Component semantic baseline

Affected files/components:

- `Views/Components/HelmButtonStyles.swift`
- `FilterButton.swift`, `LabeledContentRow.swift`
- `HealthBadgeView`, generic cards, icon buttons, hover modifier

Native primitives:

- Standard button roles/styles, `GroupBox`, `LabeledContent` where available with fallback, semantic colors, native focus.

Dependencies:

- Component inventory decisions.

Boundary risks:

- Visual refactor must not change action capability or policy enablement.

Localization/accessibility:

- Accessible labels/help, 20 pt absolute minimum targets and 28 pt preferred, color-independent states.

Validation:

- Appearance matrix and Accessibility Inspector fixtures.

Rollback/incremental strategy:

- Replace call sites by component category; no global theme removal until destination migrations finish.

### Slice 19.5: Project WOW Environment Brief foundation

Affected files/components:

- Replace `Views/Onboarding/Onboarding*.swift` flow presentation.
- Retire mandatory launch path in `Views/Walkthrough/*`.
- Health destination and first-run session presentation.
- XPC/FFI presentation models affected by the Project WOW contract commit.

Native primitives:

- Real Control Center shell, grouped list/outline, system progress, legal sheet, contextual help.

Dependencies:

- Project WOW Environment Brief/setup-session/redaction schemas and discovery consent classification.
- Revision-aware staged discovery projection.

Boundary risks:

- UI must not infer recommendations or apply defaults outside core contracts.
- Legal acceptance completes before scan where required; no administrator prompt at launch.

Localization/accessibility:

- Trust statement, coverage/freshness, finding labels, batched announcements, cancel/use-now paths.

Validation:

- First useful/current/partial/offline/service-failure fixtures.
- Shell <1 s p95, first personalized result and complete brief budgets.
- No mutation/network assertions through core integration tests.

Rollback/incremental strategy:

- Feature flag new first-run route for synthetic/dev profiles.
- Keep existing onboarding until Environment Brief contracts and state restoration pass; do not duplicate discovery logic.

## v0.20: Core Workflow and IA Migration

### Slice 20.1: Health destination and popover triage

Affected files/components:

- `DashboardView.swift`, `PopoverHelpers.swift`, `PopoverOverlayViews.swift`
- `ControlCenterSectionViews.swift` Overview
- `AppDelegate` panel sizing/activation/status priority

Native primitives:

- Constrained status-item panel/popover decision from v0.19 prototype.
- Native list/status summary and Health finding selection.

Dependencies:

- Five-destination shell and deep-link model.
- Coverage/freshness/finding projections.

Boundary risks:

- Status priority and aggregate health remain core/presentation projection truth, not view recomputation.

Localization/accessibility:

- One-condition copy; status icon + word; VoiceOver status menu; Reduce Transparency.

Validation:

- Warm/cold opening budgets, screen-edge/multi-display, dismissal/focus return, all state fixtures.

Rollback/incremental strategy:

- Replace popover regions from bottom up; retain Open Helm throughout.
- Feature switch can route status action directly to Control Center if panel activation regresses.

### Slice 20.2: Updates reviewed plan

Affected files/components:

- `ControlCenterSectionViews.swift` Updates and `RedesignUpgradeSheetView`
- Upgrade-plan inspector portions of `InspectorViews.swift`
- `ControlCenterContext` plan selection

Native primitives:

- Table/outline grouped by authority, standard toolbar filters, bounded final sheet.

Dependencies:

- Existing backend-owned upgrade workflow and plan-step IDs.
- Shared Project WOW recommendation/plan semantics.

Boundary risks:

- UI never reorders phases, schedules managers, or infers verification.

Localization/accessibility:

- Column headers, risk/privilege/restart/rollback copy, default/cancel roles.

Validation:

- 0/1/100+ step fixtures, pins, OS exclusions, stale plan, partial failure, cancellation.

Rollback/incremental strategy:

- New Updates destination consumes same plan contract; old sheet removed only after parity.

### Slice 20.3: Packages and global search

Affected files/components:

- `PackageListView.swift`, `PackageRowView.swift`
- Package portions of `InspectorViews.swift`
- Popover/global search presentation

Native primitives:

- SwiftUI `Table` where baseline permits or `NSTableView` bridge; toolbar search; context menu.

Dependencies:

- Consolidated package/member IDs, progressive search revisions/cancellation.

Boundary risks:

- Preferred manager and install capability stay in `HelmCore`/core projections.
- Search transport cannot block main actor.

Localization/accessibility:

- Column/row semantics, local/cached/remote/deferred labels, type selection, icon action labels.

Validation:

- 20,000-row stress, local <=100 ms p95, remote progress/cancel, offline and partial-source fixtures.

Rollback/incremental strategy:

- New table behind destination route; inspector reuses current action methods until contract migration.

### Slice 20.4: Activity, diagnostics, and workflow continuity

Affected files/components:

- `TaskListView.swift`, `TaskRowView.swift`
- Task/log portions of `InspectorViews.swift`
- Notifications/deep links in `AppDelegate.swift`

Native primitives:

- List/outline grouped by Needs Attention/Active/History; detail or dedicated diagnostics window; standard notification deep links.

Dependencies:

- Workflow/session relationships and revisioned task/log projections.

Boundary risks:

- Terminal and cancellation truth remains orchestration-owned.
- Output buffers remain bounded; diagnostics redaction core/support-owned.

Localization/accessibility:

- Status values, batched progress announcements, selectable output, Copy/Export commands.

Validation:

- Queued/running/cancelling/cancelled/interrupted/applied/verifying/verified/failure fixtures; notification context.

Rollback/incremental strategy:

- Current task IDs map to Activity records; preserve old Tasks route alias until deep links migrate.

### Slice 20.5: Sources and manager lifecycle

Affected files/components:

- `ManagersView.swift`, manager portions of `InspectorViews.swift`
- `ManagerItemView.swift`, manager status projections

Native primitives:

- Source table/list, context menus, inspector, standard install/destructive sheets.

Dependencies:

- Existing provenance, instance, eligibility, dependency, lifecycle planner, doctor/repair contracts.

Boundary risks:

- No lifecycle strategy, dependency, recommendation ordering, or policy logic in SwiftUI.

Localization/accessibility:

- Policy versus permission terms, provenance explanation, keyboard reorder, managed state.

Validation:

- Not installed/disabled/ineligible/multi-instance/setup required/install/update/uninstall/repair fixtures.

Rollback/incremental strategy:

- Migrate table selection first, inspector subsections next, sheets last; retain existing core actions.

### Slice 20.6: First reviewed and verified improvement

Affected files/components:

- Health/Updates/Activity shared workflow presentation.
- First-run plan/progress route replacing later onboarding pages.
- XPC/FFI contract models from Project WOW package.

Native primitives:

- Reviewed plan outline, final sheet, Activity progress/detail.

Dependencies:

- Setup-session/recommendation/plan contract, typed-action boundary, verification state machine.

Boundary risks:

- SwiftUI submits approved plan/session IDs only; no arbitrary command or client-side plan construction.

Localization/accessibility:

- Consequence/recovery text, stage announcements, default/cancel, no forced timing.

Validation:

- No-op, stale plan, policy/permission block, apply success, apply failure, failed verification, cancellation.

Rollback/incremental strategy:

- Start with one allowlisted improvement class; unsupported recommendations remain review-only.

## v0.21: Accessibility, System Integration, and Recovery

### Slice 21.1: Keyboard, VoiceOver, and appearance closure

Affected files/components:

- All migrated destinations/components.
- AppKit focus bridges, command validation, appearance modifiers.

Native primitives:

- AppKit key-view loop/focus APIs where SwiftUI remains insufficient; system accessibility environment values.

Dependencies:

- Stable destination structures from v0.19-v0.20.

Boundary risks:

- Accessibility actions invoke the same command/core path as pointer actions.

Localization/accessibility:

- This slice owns the full budget in `NATIVE_MACOS_QUALITY_BUDGETS.md`.

Validation:

- Full Keyboard Access, VoiceOver rotor/tasks, Increased Contrast, Differentiate Without Color, Reduce Motion/Transparency, all locales.

Rollback/incremental strategy:

- Fix by component/destination; do not add hidden alternate UI that diverges behavior.

### Slice 21.2: Interruption, recovery, Action Receipt, and notifications

Affected files/components:

- Activity and receipt detail.
- Project WOW session/recovery presentation.
- Notification handlers/deep links in `AppDelegate.swift`.

Native primitives:

- Activity detail, inspector, standard Share/Copy/Export commands, notifications with exact deep links.

Dependencies:

- Persisted setup session, receipt/redaction schemas, recovery state machine, honest rollback capability.

Boundary risks:

- UI cannot offer Restore/Rollback unless current core contract marks it valid.
- Notification actions use coordinator authority and revalidate stale targets.

Localization/accessibility:

- Before/after/unchanged/recovery headings; redacted-copy explanation; notification labels/actions.

Validation:

- Crash/close/reopen between each apply/verify/record phase; offline resume; stale policy; notification activation.

Rollback/incremental strategy:

- Receipt read-only view can ship before recovery actions. Add each recovery action only with tested core inverse.

### Slice 21.3: Localization and minimum-size hardening

Affected files/components:

- All new localization keys and layouts.
- Locale mirrors and i18n validation scripts/tests.

Native primitives:

- Wrapping forms/tables/inspectors; system formatters.

Dependencies:

- Near-final content contracts.

Boundary risks:

- Service/core returns localization keys/structured values; no raw user-facing policy strings from views.

Localization/accessibility:

- Seven locales, +40% pseudolocalization, German/Japanese/Hungarian critical paths.

Validation:

- Locale integrity/length scripts, UI tests/screenshots at minimum/expanded sizes.

Rollback/incremental strategy:

- Fix layout/content per shared semantic component rather than per-language special cases.

## v0.22: Fit, Finish, and Validation

### Slice 22.1: Performance and rendering budgets

Affected files/components:

- Snapshot projections/polling in `HelmCore.swift` and fetching/presentation coordinators.
- Tables/lists, rich descriptions, output/log views, search, progress.

Native primitives:

- Lazy/native collection virtualization, revision-aware updates, system progress.

Dependencies:

- Stable UI and representative stress fixtures.

Boundary risks:

- Transport optimization cannot weaken replay, ordering, reconnect, cancellation, or core authority.

Localization/accessibility:

- Optimizations preserve labels, focus, selection, announcements, and complete text.

Validation:

- All numeric budgets, Instruments traces, idle resource record.

Rollback/incremental strategy:

- Optimize measured bottlenecks only; retain polling fallback until event path proves ordering/replay.

### Slice 22.2: State fixtures, appearance, screenshot, and multi-display QA

Affected files/components:

- UI test fixture host and deterministic screenshot assets.
- Every release-critical destination and first-run state.

Native primitives:

- Test-only state injection at presentation boundary; no production fake state.

Dependencies:

- State matrix and UI lock candidate.

Boundary risks:

- Fixtures cannot bypass production rendering semantics or become execution inputs.

Localization/accessibility:

- Light/dark/inactive/contrast/motion/transparency/locales/minimum/expanded matrix.

Validation:

- Link/asset checks, screenshot review, multi-display restoration, failure injection.

Rollback/incremental strategy:

- Baseline updates require reviewed intentional change; preserve previous baselines for PR comparison.

### Slice 22.3: Moderated validation and UI lock

Affected files/components:

- No production scope by default; fixes from validation are bounded PRs.
- Validation reports under `docs/validation/`.

Dependencies:

- Owner-run study in `NATIVE_MACOS_RESEARCH_VALIDATION.md`.

Boundary risks:

- Do not respond to usability findings by moving policy into views or weakening confirmation.

Localization/accessibility:

- Participant and expert accessibility protocols complete.

Validation:

- Thresholds pass; zero S0/S1; S2 owners/retests; performance and state matrices pass.

Rollback/incremental strategy:

- Reopen only the affected destination/component; no late whole-app rewrite.

## Scoped v0.19 Handoff Backlog

These are bounded implementation questions, not blockers to planning closure:

| ID | Backlog item | Decision needed | Validation |
|---|---|---|---|
| NME-19-01 | Prototype `NSPopover` versus retained constrained `NSPanel` on macOS 11+ | Choose the lowest custom activation path that supports interactive triage and FKA | Spaces, screen edges, VoiceOver, dismissal/focus return |
| NME-19-02 | Prototype SwiftUI `NavigationSplitView`/`List` versus AppKit split/source-list bridge | Choose per minimum-OS behavior, not API novelty | Selection, arrows, inactive state, restoration, resize |
| NME-19-03 | Define accessory-app application menu/command ownership | AppDelegate/App commands without duplicate windows | Command-Comma/F/W/Q, Window menu, status menu parity |
| NME-19-04 | Define window restoration payload and stale-ID fallback | Frame/display/destination/panes/selection/filter/scroll schema | Reopen after display removal and entity deletion |
| NME-19-05 | Define presentation revision/deep-link model | Stable destination + entity/session IDs and focus target | Notification/popover/menu activation under stale data |
| NME-19-06 | Build bounded AppKit focus bridge | Resolve current Tab key-loop gap without custom focus fork | FKA forward/reverse traversal and restoration |
| NME-19-07 | Decide Settings pane primitive on macOS 11+ | Native scene/pane behavior and accessory activation | Single instance, pane size, localization, VoiceOver |
| NME-19-08 | Add deterministic UI state-fixture seam | Presentation-only fixtures from versioned payloads | No production execution impact; screenshot repeatability |
| NME-19-09 | Map Project WOW schemas to Health/Activity presentation models | No duplicate view-owned workflow states | Contract fixtures for partial/offline/interrupted/receipt |
| NME-19-10 | Define local timing signposts | FUR/CRS/search/progress without telemetry transport | Repeatable local measurements and no sensitive payloads |

## Migration Completion Rule

A destination is migrated only when:

- Its old route no longer contains unique functionality.
- Shared command, selection, state, and deep-link contracts pass.
- Native/custom component inventory decisions are reflected.
- All applicable state fixtures, accessibility/input budgets, localization checks, and targeted performance budgets pass.
- Presentation contains no new business/policy/orchestration logic.
- Rollback means reverting presentation routing, not maintaining two competing behavior implementations.
