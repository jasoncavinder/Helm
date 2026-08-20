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

Original Wayfinder is the approved Dashboard and status-item popover direction. The decision, comparison evidence, Course Indicator contract, and implementation handoff are recorded in `docs/app-design/proposals/v019-visual-direction/`. Proposal code remains in the non-shipping `tools/design-lab/` package and cannot enter the application target by implication.

The approved shell uses Dashboard, Plan, Library, and Activity as peer workspaces with Environment at the sidebar foot as persistent contextual infrastructure. Implement the Course Indicator from one shared revisioned Dashboard/popover state projection; never synthesize health or progress in SwiftUI. Validate minimum and expanded window sizes, light/dark and key/inactive states, plus healthy, attention, approval, failure, partial-data, offline, determinate-work, and indeterminate-work fixtures before broad component migration.

### Slice 19.1: Application command and window foundation

Implementation checkpoint on `dev`:

- The existing content host now runs in a renamed, restored, resizable Dashboard window with an `860x600` minimum, unconstrained expansion, Command-1 open, and Command-Comma Settings routing.
- Stable `Dashboard`, `Plan`, `Library`, `Activity`, and `Environment` deep links bridge into legacy sections without moving business state into SwiftUI.
- One revisioned Wayfinder projection now owns status priority and semantic Course Indicator mode across Dashboard, popover, and status item. It validates backend-owned determinate progress and otherwise remains indeterminate.
- The Dashboard now uses a native toolbar for sidebar, contextual inspector, search, refresh, and Upgrade All controls. Localized app commands cover Dashboard open, search focus, refresh, workspace routing, and sidebar/inspector visibility without moving policy into SwiftUI.
- Remaining Slice 19.1 work is manual multi-Space, multi-display, key/inactive, minimize/zoom, menu-placement, and focus validation. Slice 19.2 still owns destination-specific selection and contextual-detail migration.

### Slice 19.2 implementation checkpoint

- The production shell presents Dashboard, Plan, Library, and Activity as peer rows in a native sidebar `List(selection:)`, with Environment persistent at the sidebar foot and Settings outside the peer workspace list.
- Existing destination hosts and stable section/deep-link mappings remain in place as the rollback boundary; this checkpoint changes presentation composition rather than business state or routing authority.
- Dashboard renders Original Wayfinder's Course Indicator from the shared revisioned projection. Determinate progress appears only when that projection contains validated backend-owned progress; all other modes use semantic non-percentage arcs.
- The indicator carries text, symbol, VoiceOver label/value/hint, increased-contrast treatment, Reduce Transparency treatment, and static Reduce Motion behavior. Color is never the only state signal.
- Dashboard intentionally suppresses the legacy inspector. Other migrated destinations retain the existing inspector until their contextual-detail contracts are redesigned.
- Remaining Slice 19.2 work includes selection restoration/focus tests and manual minimum/expanded/key/inactive validation. Owner-assisted VoiceOver and Full Keyboard Access QA exposed a missing Headings-rotor entry and asymmetric noninteractive-header traversal in the debug-gated Environment Brief; Issue #388 tracks the unresolved accessibility gate. Destination-specific visual and workflow redesign remains v0.20 work.

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

- Approved Original Wayfinder IA.
- Stable selection IDs and restoration payload.

Boundary risks:

- Selection projections must not become business state.
- Avoid a repository-wide state-container rewrite; adapt `ControlCenterContext` incrementally.

Localization/accessibility:

- Dashboard, Plan, Library, Activity, Environment labels and shortcuts.
- Arrow/type selection, FKA groups, focus restoration, inactive selection.

Validation:

- Automated selection/restoration tests where feasible.
- Manual FKA/VoiceOver through all panes and hidden inspector/sidebar states.

Rollback/incremental strategy:

- New shell can host current section views one at a time.
- Preserve old section enum mapping only during migration; remove after all destinations move.

### Slice 19.3: Standard Settings window

Implementation checkpoint on `dev`:

- The SwiftUI `Settings` scene now owns a real single-instance Settings window. Command-Comma, the standard app menu, the popover footer, and the Dashboard sidebar all use the same platform window route.
- The quick-Settings popover overlay and the Basic/Advanced status-menu split are removed. Refresh remains available contextually from the Dashboard toolbar, Command-R, and the popover footer.
- The Settings window now presents General, Updates, Sources, CLI, and Support as native sidebar panes. General and update preferences are separated, the accidental duplicate CLI card is removed, and the existing cards/actions remain the parity boundary while destination migration continues.
- Operational-card relocation and removal of the legacy in-window Settings destination remain open until the parity checklist passes. They are explicitly deferred until after `v0.19.0-rc.2` so the candidate can validate the native pane architecture without rushing broader Dashboard/diagnostics placement; this pane slice does not hide or duplicate those actions.

Superseding lifecycle clarification from the v0.20 native-sidebar slice:

- SwiftUI still owns Settings content, pane navigation, commands, and presentation state, but `AppDelegate` now owns the one concrete window through a single cached `SettingsPanel`. This bounded AppKit lifecycle exception keeps Settings available beside external full-screen Spaces and allows explicit panel geometry without moving preference behavior out of SwiftUI.
- `HelmApp` retains its `Settings` scene declaration as the SwiftUI app-command host. The standard `.appSettings` command group is replaced, and Command-Comma plus every in-app Settings route goes through `HelmSettingsOpenRouter` to the same cached panel, preventing a second platform-created Settings window.

Affected files/components:

- `HelmApp.swift`
- `Views/SettingsPopoverView.swift`
- Settings-related `HelmCore+Settings.swift` projections/actions

Native primitives:

- SwiftUI Settings content and commands hosted in one AppKit-owned `SettingsPanel` lifecycle exception.
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
- Dashboard destination and first-run session presentation.
- XPC/FFI presentation models affected by the Project WOW contract commit.

Native primitives:

- Real Dashboard shell, grouped list/outline, Course Indicator/system progress, legal sheet, contextual help.

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

Implementation checkpoint:

- The read-only Environment Brief model and revision-aware projection are wired to existing authoritative manager-status and detection-task snapshots.
- Contract coverage includes current, partial, cached/offline, and service-failure states plus JSON field/enum alignment.
- Versioned debug-only first-use/current/partial/offline/service-failure fixtures inject through the shared presentation boundary, and persisted legal/discovering/brief state restores against current brief identity/revision while discarding stale manager selection.
- A debug-only `HELM_ENVIRONMENT_BRIEF_FIRST_RUN=enabled|preview` route now hosts legal, discovering, current, cached, partial, and service-failure presentation in the real Dashboard window. `enabled` applies only to unfinished onboarding; `preview` supports non-destructive review on a completed development profile, and `HELM_ENVIRONMENT_BRIEF_APPEARANCE=light|dark` makes fixture screenshots appearance-deterministic. Fixture previews remain service-independent, the surface performs no mutation or network work, and the existing onboarding remains the production route.
- The 2026-08-08 presentation checkpoint rendered all five fixtures at minimum/expanded sizes in light/dark, plus German/Hungarian/Japanese and +40% text expansion at minimum size. It corrected the Course Indicator to represent mapped coverage and allowed system facts to wrap without truncation; see `docs/validation/v0.19-environment-brief-presentation-validation.md`.
- Owner-assisted Full Keyboard Access and VoiceOver execution found the unresolved header rotor/traversal defect tracked in Issue #388. Setup-session/receipt mapping, contextual manager inspection, remediation and retest of #388, and the separately reviewed default-route replacement remain open.

Rollback/incremental strategy:

- Feature flag new first-run route for synthetic/dev profiles.
- Keep existing onboarding until Environment Brief contracts and state restoration pass; do not duplicate discovery logic.

## v0.20: Core Workflow and IA Migration

### Slice 20.1: Dashboard state and popover triage

Owner-review and implementation record: `docs/app-design/proposals/v020-wayfinder-popover/README.md`. Its design-lab renderer remains non-shipping historical input; the review and production-replacement gates are resolved.

Implementation checkpoint on `dev`:

- The legacy mini-Dashboard popover is replaced by the approved fixed-footprint unified Wayfinder composition in the production `WayfinderPopoverView`.
- The shared projection drives one Course truth, one optional next action, affected System/Tools/Apps/Packages route tones, one context fact, and completed-check freshness.
- Deep links preserve originating condition, destination, focus, route domain, and affected manager. Owner QA verified the corrected visible Environment filter for all four route stages before PR #490 merged.
- Library, Plan, Activity, Environment, and Dashboard own detail, confirmation, recovery, and search rather than hosting those workflows inside the transient panel.
- A debug-only six-state `HELM_WAYFINDER_POPOVER_FIXTURE` seam uses the production projectors/view and suppresses refresh plus Helm update-check submission while active. It is a state-validation seam, not a whole-workflow synthetic dataset.
- The canonical `v0.20-whole-workflow-v1` corpus and typed semantic validator cover all seven moderated tasks without host scanning or mutation. The Debug-only absolute-path selector now projects Task 2 through the shipping Plan path and Task 3 through the shipping Library/global-search path; the other five tasks remain unprojected, and each remaining workflow slice owns that work.

Affected files/components:

- `DashboardView.swift`, `PopoverHelpers.swift`, `PopoverOverlayViews.swift`
- `ControlCenterSectionViews.swift` Overview
- `AppDelegate` panel sizing/activation/status priority

Native primitives:

- Constrained status-item panel/popover decision from v0.19 prototype.
- Native Dashboard status summary, Course Indicator, and finding selection.

Dependencies:

- Original Wayfinder shell and deep-link model.
- Coverage/freshness/finding projections.

Boundary risks:

- Status priority and aggregate health remain core/presentation projection truth, not view recomputation.

Localization/accessibility:

- One-condition copy; status icon + word; VoiceOver status menu; Reduce Transparency.

Validation:

- Warm/cold opening budgets, screen-edge/multi-display, dismissal/focus return, all state fixtures.
- Preserve the completed production-view fixture/accessibility matrix and dataset-seeding record in `docs/validation/v0.20-wayfinder-popover-research-readiness.md`; do not claim the owner-moderated checkpoint until every protocol destination projects the corpus and participant records exist.

Rollback/incremental strategy:

- Replace popover regions from bottom up; retain Open Helm throughout.
- Feature switch can route status action directly to Dashboard if panel activation regresses.

### Slice 20.2: Plan reviewed workflow

Affected files/components:

- `ControlCenterSectionViews.swift` Plan and `ReviewedUpgradeConfirmationSheet`
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

- New Plan destination consumes the same plan contract; old Updates route/sheet is removed only after parity.

Implementation checkpoint:

- Task 2's 12 synthetic update records project through the shipping Plan and inspector with backend authority order preserved, ten selected non-OS rows, two fixed visible exclusions, scoped selection/risk behavior, and a bounded read-only confirmation. The selector blocks service/database/network/mutation startup and fails closed for an unreadable corpus.
- The Plan now uses a Ventura-compatible native `NSOutlineView` grouped by backend authority, with native row selection, keyboard traversal, inclusion controls, contextual inspector routing, standard toolbar manager/search filters, and a virtualized scrolling boundary for large plans. Presentation grouping is deterministic and does not alter execution order.
- The legacy generic Upgrade All preview is removed. Dashboard routes to Plan, notification Upgrade All requests the same Plan-owned reviewed flow, and the final sheet snapshots exact selected rows and execution order, automatic/interactive eligibility, vendor-app handoffs, and selected privilege/reboot risk. Run Plan disables if any reviewed step, eligibility, or risk value drifts before submission.
- Automated coverage now exercises empty, single-step, and 125-step plans; fixed pin and OS exclusions; stale-selection and stale-confirmation rejection; partial-failure status; scoped cancellation; selected-row risk; and hidden-selection preservation. The current-host owner visual, Full Keyboard Access, and VoiceOver gate is complete; targeted third-party Sparkle handoff evidence and the protected Ventura lane remain open.

### Slice 20.3: Library and global search

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

- New Library table behind the destination route; contextual detail reuses current action methods until contract migration.

Implementation checkpoint:

- Task 3's exact Homebrew cached result, delayed Cargo remote result, existing-authority recommendation, source-specific inspector selection, and install proposal now project through the shipping Library and global-search path. Accepting a global result deliberately routes to that exact Library entity instead of changing destinations while the user types.
- The Debug-only offline variant keeps the cached result usable, marks the remote result and network-backed proposal Deferred, and never starts service, database, network, updater, or mutation work. Its bounded install confirmation is explicitly read-only.
- Focused projection and routing tests cover exact record scope, progressive visibility, offline deferral, bounded confirmation requirements, and fail-closed scenario drift. The current-host owner minimum/expanded Light/Dark, Full Keyboard Access, and VoiceOver gate is complete.
- The production result-provenance contract is now explicit and versioned across Rust, FFI/XPC JSON, and Swift. It separates logical result origin from manager-search discovery, preserves canonical source-manager/query facts without claiming every manager search used network access, keeps older payloads decodable, loss-isolates malformed nested data, and fails closed for unknown, endpoint-invalid, or internally inconsistent provenance without discarding the package result. Exact Library source selection remains authoritative in the inspector before persisted preference fallback.
- Production and Task 3 fixture rows now share a Ventura-compatible native `NSTableView`. It provides column headers, native selection and keyboard/type navigation, horizontal fallback at constrained widths, context commands, localized icon actions, and exact consolidated-member routing to the inspector and current action policy. Status, pin, restart, current/latest version, manager, provenance, and recommendation facts remain visible or accessible. Focused automated coverage protects exact-member selection and action identity; the current-host minimum/expanded Light/Dark, Full Keyboard Access, and VoiceOver regression passed on merged `dev` revision `27e8cc21`.
- Production remote-search submissions are now bound to normalized query generations and carry explicit interactive or description request purpose across XPC/FFI into Rust in-flight deduplication, preventing cross-purpose task-ID reuse. Superseding input cancels only task IDs owned by the prior interactive session, including late stale callbacks, through a dedicated local-runtime path whose 500 ms graceful near-completion window treats verified already-terminal Search races as benign while surfacing real failures. Snapshot reconciliation retires explicitly owned task IDs on terminal status or after two consecutive authoritative snapshot absences without adopting unrelated work. Focused macOS and Rust tests cover request-purpose isolation, replacement, callback, cancellation failure, terminal and authoritative-absence retirement, and reset races. The 20,000-row performance budget remains later Slice 20.3 work.

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

### Slice 20.5: Environment and manager lifecycle

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

- Dashboard/Plan/Activity shared workflow presentation.
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
| NME-19-08 | Extend the deterministic Environment Brief fixture seam into rendered first-run screenshots | Preserve debug-only versioned payload injection with no production execution impact | Appearance/localization screenshot repeatability across all five first-run fixtures |
| NME-19-09 | Extend the Environment Brief foundation into setup-session/receipt Dashboard and Activity models | No duplicate view-owned workflow states; preserve the existing read-only brief projection | Contract fixtures for first-run restoration, interrupted sessions, and receipts |
| NME-19-10 | Define local timing signposts | FUR/CRS/search/progress without telemetry transport | Repeatable local measurements and no sensitive payloads |

## Migration Completion Rule

A destination is migrated only when:

- Its old route no longer contains unique functionality.
- Shared command, selection, state, and deep-link contracts pass.
- Native/custom component inventory decisions are reflected.
- All applicable state fixtures, accessibility/input budgets, localization checks, and targeted performance budgets pass.
- Presentation contains no new business/policy/orchestration logic.
- Rollback means reverting presentation routing, not maintaining two competing behavior implementations.
