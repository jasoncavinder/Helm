# Native macOS Component Inventory

Status: approved planning inventory for incremental v0.19-v0.22 migration
Baseline: `origin/dev` at `278ae026`

## Decision Key

- **Retain**: domain-specific value exceeds the cost of custom behavior.
- **Replace**: a native component provides materially better behavior.
- **Simplify**: keep the purpose but reduce custom visual/interaction semantics.
- **Investigate**: prototype or OS-version testing is required before commitment.

Milestones identify the first delivery target, not permission to move business rules into SwiftUI/AppKit.

## App Shell and Navigation

| Component | Current implementation / source | Purpose | Native equivalent | Decision | Rationale | Accessibility/input implications | Milestone | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Status item icon and badge | `AppDelegate.swift`, `AppDelegate+StatusBadge.swift`, `Assets.xcassets/MenuBarIcon.imageset` | Ambient health and entry point | `NSStatusItem` template image, menu title/accessibility description | Simplify | `NSStatusItem` is correct; manual micro-badges need fewer semantics and stronger testing. | Preserve textual tooltip/label; distinguish state without color; test the status-item popover with VoiceOver. | v0.20 | Medium: status priority changes can hide urgent state. |
| Right-click status menu | Removed; both status-item click types route through `AppDelegate.togglePanel(_:)` | Previously duplicated fast global commands | Shared status-item popover plus standard app commands | Remove | A second command surface duplicated information architecture and captured localized titles only at launch. Refresh remains a compact footer action; update checking lives in About; existing popover routes retain Dashboard, Upgrade All, Settings, Support, and Quit. Either click toggles the popover while Dashboard is closed; primary click focuses an open Dashboard and secondary click keeps the popover available alongside it. | Primary and secondary clicks expose the same focusable, localized SwiftUI surface; validate VoiceOver, Full Keyboard Access, and dismissal parity. | v0.19 | Low: verify click routing in both Dashboard visibility states and confirm no AppKit status menu remains. |
| Floating popover panel | `FloatingPanel` in `AppDelegate+Windows.swift` | Ambient interactive triage | `NSPopover`, `MenuBarExtra`, or constrained `NSPanel` | Investigate | Current panel supports complex focus but duplicates deep work. OS 11 baseline and status-item behavior require prototype comparison. | Must restore status-item focus, dismiss predictably, clamp to screen, and support FKA. | v0.19 prototype, v0.20 delivery | High: activation/Spaces/accessibility regressions. |
| Control Center window | `ControlCenterWindow` and `openControlCenter()` | Main operational workspace | Standard `NSWindow`/SwiftUI Window scene with restoration | Replace foundation | Current fixed sizing contradicts Mac window behavior. | Standard key/main/inactive state; minimize/zoom; autosaved frame; key-view loop. | v0.19 | High: AppDelegate lifecycle and accessory-app activation. |
| Custom content top bar | `ControlCenterTopBar` | Title, search, Refresh, Upgrade All | `NSToolbar` / SwiftUI `.toolbar`, `.searchable` where viable | Replace | Native toolbar supplies frame placement, overflow, inactive state, sizing, and customization policy. | Every item needs label/help/menu equivalent; FKA and VoiceOver order follow system. | v0.19 | Medium: macOS 11 availability and search binding. |
| Custom sidebar scroll/buttons | `ControlCenterSidebarView`, `ControlCenterSidebarButtonStyle` | Top-level navigation | `NavigationSplitView`/`List(selection:)` with `.sidebar`, or AppKit source list bridge | Replace | Native source-list selection, arrows, icon sizing, inactive state, and hide/show behavior are missing. | Arrow/type selection, focus group, selected state, View menu toggle. | v0.19 | High: macOS 11 fallback and state restoration. |
| Raw `HSplitView` content/inspector | `ControlCenterWindowView` | Main and contextual detail columns | `NavigationSplitView` or `NSSplitViewController`; inspector command | Replace foundation | Need adaptive pane hiding, autosaved widths, and standard dividers. | Focus moves by pane, inspector toggle announced, hidden pane preserves selection. | v0.19 | High: existing nested scroll/selection assumptions. |
| Always-visible inspector | `ControlCenterInspectorView` | Selected package/task/source detail | Native trailing inspector/split item | Simplify | Inspector remains valuable, but should be contextual, hideable, and bounded. | FKA enters after content; Escape returns focus; selection relationship announced. | v0.19-v0.20 | Medium: existing inspector holds local sheet state. |
| Full-window background dragging | `window.isMovableByWindowBackground`, context suppression | Compensate for transparent titlebar | Standard window frame drag regions | Replace | Native frame removes gesture conflicts with list reorder and controls. | Eliminates pointer-only ambiguity and accidental drags. | v0.19 | Low after toolbar migration. |

## Content, Lists, and Selection

| Component | Current implementation / source | Purpose | Native equivalent | Decision | Rationale | Accessibility/input implications | Milestone | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Overview metric cards | `MetricCardView`, `SettingsMetricPill`, `MetricChipView` | Summarize counts and deep-link | `LabeledContent`, status summary, toolbar/sidebar badges | Simplify | Three implementations repeat the same data and create dashboard weight. | Use links/buttons with explicit action labels, not a combined unlabeled card. | v0.20 | Low. |
| Manager health card grid | `ManagerHealthCardView` | Compare source health | `List`/`Table`, disclosure summary | Replace | Grid scales poorly and custom tap selection lacks keyboard semantics. | Native row selection, arrows, inactive selection, context menu. | v0.20 | Medium: preserve manager selection IDs. |
| Package rows | `PackageRowView`, `PackagesSectionView` | Compare package/version/status and act | `Table` with columns, list fallback at narrow width | Replace | Package data is tabular and needs sorting, selection, context actions, and density. | Arrow/type navigation, accessible column headers, row actions via commands/context menu. | v0.20 | High: consolidated package identity and member selection. |
| Task rows and inline output | `TaskRowView` | Live status, cancel, output disclosure | `List`/`OutlineGroup` plus disclosure and detail | Simplify/replace container | Inline output is domain-specific; row selection and disclosure should be native. | Disclosure action, status value, live-region restraint, selectable output text. | v0.20-v0.21 | Medium: preserve polling and single expansion. |
| Manager cards | `ManagerSectionRow` | Source state, enablement, actions, reorder | `Table`/source list with columns and inspector | Replace | Card combines selection, drag, toggle, and icon commands in one hit region. | Keyboard reorder commands; toggle label; row context actions; no pointer-only drag. | v0.20 | High: dependency and eligibility confirmations. |
| Custom selection overlays | package/manager/task/update views and `HelmTheme.selection*` | Mark selected entity | System list/table selection | Replace | Inactive state, focus, contrast, and platform consistency come for free. | Must preserve one logical selection and selected accessibility trait. | v0.19-v0.20 | Medium: deep links and cross-domain retained selection. |
| Status/health pill | `HealthBadgeView` | Compact health semantic | `Label`, system status text, badge where appropriate | Simplify/retain exception | Domain state needs icon + word, but a pill is not always necessary. | Never color-only; localized state value; increased-contrast border if retained. | v0.19 semantic layer | Low. |
| Package filter chips | `FilterButton`, horizontal `ScrollView` | Status filters | Segmented picker for small exclusive set; token/menu/sidebar filters for combinable set | Replace | Current filters appear mutually exclusive but include a separate pinned toggle and can scroll out of view. | Arrow navigation within segmented control; selected value announced; menu parity. | v0.20 | Medium: filter combination semantics must be explicit. |
| Manager filter custom menu label | `PackagesSectionView` | Scope package list | Standard `Picker`/`Menu` in toolbar | Replace | Native menu needs no custom rounded background. | Label and current value announced; keyboard menu access. | v0.20 | Low. |
| Updates plan step buttons | `RedesignUpdatesSectionView` | Ordered plan and task projection | `Table`/`OutlineGroup` selection | Replace | Authority stages and status are structured rows, not buttons. | Arrow selection, columns, hierarchy disclosure, context inspection. | v0.20 | High: preserve backend-owned order and IDs. |
| Risk flag rows | `riskRow` | Privilege/restart consequences | Summary group / `LabeledContent`, warning callout | Simplify | Binary circles underspecify consequence and recovery. | Full text, heading, status; no color-only active signal. | v0.20 | Low. |

## Controls, Menus, and Actions

| Component | Current implementation / source | Purpose | Native equivalent | Decision | Rationale | Accessibility/input implications | Milestone | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Primary/secondary/tertiary button styles | `HelmButtonStyles.swift` | Brand action hierarchy | `.borderedProminent`, `.bordered`, `.plain`, control roles | Replace default; retain rare brand exception | Native buttons adapt focus, inactive, contrast, tint, size, and OS appearance. | Default/cancel/destructive roles and focus rings become consistent. | v0.19 | Medium: visual regression, semantics improve. |
| Gold Pro button | `HelmProButtonStyle` | Future premium/support emphasis | Standard button with restrained badge/accent | Simplify | Gradient, double stroke, and shadows are excessive in app chrome; support donation is not a Pro action. | Ensure readable label and no gold-only entitlement meaning. | v0.19-v0.22 | Low. |
| Circular icon button | `HelmIconButtonStyle` | Compact row/toolbar action | Borderless toolbar button or standard icon button | Replace | Custom circles visually mimic mobile controls and duplicate hover/focus behavior. | Minimum 20x20, target 28x28; label/help required; menu equivalent. | v0.19-v0.20 | Low. |
| Update-all pill | `UpdateAllPillButtonStyle` | Popover primary action | Standard prominent button | Replace | One popover primary action remains, but custom capsule is unnecessary. | Default action only in bounded confirmation, not ambient panel. | v0.20 | Low. |
| Hover pointer modifier | `.helmPointer()` | Signal clickability | System cursor/hover behavior | Remove | Mac buttons/rows do not require web-style pointing-hand forcing. | Prevent hover-only discovery; visible/menu commands remain. | v0.19-v0.20 | Low. |
| Enablement switches | manager cards/onboarding/Settings | Toggle persistent source state/preferences | Standard `Toggle` | Retain native control, remove `scaleEffect` | Scaling creates undersized hit targets and nonstandard appearance. | Full-size control, associated label, policy-blocked help. | v0.19-v0.20 | Low. |
| Radio-group install selection | package/manager sheets | Choose manager/version/method | Standard `Picker(.radioGroup)` in form/sheet | Retain | Native and appropriate for bounded choices. | Arrow navigation, group label, default action, validation message. | v0.20 | Medium: long localized option labels. |
| Context menus | largely absent from rows | Selection-scoped commands | `.contextMenu` / `NSMenu` shared commands | Add | Expert efficiency without making actions undiscoverable. | Must duplicate visible/menu commands and respect disabled reason. | v0.20 | Medium: command routing. |

## Cards, Overlays, Sheets, and Progress

| Component | Current implementation / source | Purpose | Native equivalent | Decision | Rationale | Accessibility/input implications | Milestone | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Generic card surface | `.helmCardSurface`, `SettingsCard`, popover card backgrounds | Group content | `GroupBox`, grouped `Form`, list sections, separators | Replace by structure; retain only bounded summaries | Cards are overused and carry custom radius/shadow/contrast burden. | System groups provide headings and contrast adaptation. | v0.19-v0.22 | Medium: layout will change broadly. |
| Attention banner | `PopoverAttentionBanner` | Highest-priority state and action | Compact status callout / native button | Simplify/retain exception | Popover needs one domain-specific triage summary. | Heading + consequence + one action; announce only on meaningful change. | v0.20 | Low. |
| Popover overlay card system | `PopoverOverlayCard`, overlay routes | Search, quick Settings, About, quit | Separate native popover/menu/Settings/About/alert | Replace | It creates modal-within-panel behavior, blur, and focus ambiguity. | Standard dismissal, Escape, focus return, VoiceOver modal semantics. | v0.19-v0.20 | Medium. |
| Spotlight overlay and tooltip | `Walkthrough/SpotlightOverlay.swift` | Mandatory product tour | Help menu tour or contextual Tip-style callout | Replace/remove mandatory flow | Obscures real interface after setup and adds custom focus/motion. | Optional, dismissible, no focus trap, Reduce Motion, no replay by default. | v0.19 | Low if removed after WOW replacement. |
| Upgrade preview sheet | `RedesignUpgradeSheetView` | Confirm bulk execution | Native sheet after full plan review | Replace content, retain sheet pattern | Sheet is appropriate, but current content is less complete than Updates page. | Default Run and cancel; risk/rollback summary; focus returns to trigger. | v0.20 | Medium. |
| SwiftUI alerts | Managers, package, reset, inspector | Bounded confirmation/block | `Alert` / `confirmationDialog` / `NSAlert` where required | Retain and standardize | Native role semantics are correct; messages need shared state terms. | Destructive/default/cancel roles; no multi-step content in alert. | v0.20-v0.21 | Medium. |
| Large task diagnostics popover | `InspectorTaskDetailView` -> `TaskDiagnosticsSheetView` | Logs/output/support detail | Activity detail pane or dedicated diagnostics window | Replace | 700x420 tabbed popover from a 280 pt inspector is structurally unstable. | Standard window focus, selectable text, tabs/segmented control, menu copy/export. | v0.20 | Medium. |
| Progress spinners | `ProgressView` across views | Indeterminate local work | Native `ProgressView`, determinate when contract provides progress | Retain/simplify | Native progress is correct; placement/state copy is inconsistent. | Label every long-running indicator; avoid announcement storms; cancellation nearby. | v0.19-v0.21 | Medium: progress contracts may be coarse. |
| Inline task output expansion | `TaskRowLiveOutputView` | Observe long work | Disclosure row plus log detail | Retain exception | Live output is valuable domain data. | Selectable monospaced text, pause auto-scroll, Copy command, capped announcements. | v0.20-v0.21 | Medium. |

## Settings, Onboarding, and Domain Exceptions

Implementation checkpoint: the empty scene and quick overlay have been replaced by one native Settings scene shared by Command-Comma, app/status menus, popover, and Dashboard routes. The existing card body remains temporarily hosted for behavior parity; dedicated pane migration and operational-action relocation are still required before removing the legacy in-window destination.

| Component | Current implementation / source | Purpose | Native equivalent | Decision | Rationale | Accessibility/input implications | Milestone | Migration risk |
|---|---|---|---|---|---|---|---|---|
| Empty Settings scene | `HelmApp.swift` | Declares platform Settings | Real SwiftUI `Settings` scene/window | Replace | Current Settings is not reachable through standard app behavior. | Command-Comma, App menu, pane focus and labels. | v0.19 | High: accessory app command/menu integration. |
| In-window Settings section | `SettingsSectionView` | Preferences plus operations | Dedicated Settings panes; operational views elsewhere | Replace | Violates responsibility and includes duplicated CLI card. | Form/group navigation; localization expansion; no inspector. | v0.19 | High: actions must be relocated without loss. |
| Quick Settings popover overlay | `PopoverSettingsOverlayContent` | Refresh and open advanced Settings | Standard Settings command plus contextual Refresh | Remove | Splits Settings and duplicates commands. | Reduces modal depth and focus routes. | v0.19 | Low. |
| Onboarding progress dots/pages | `OnboardingContainerView` | Five-step setup wizard | Real shell plus legal sheet and Environment Brief states | Replace | Configuration-first sequence conflicts with Project WOW. | Headings, logical focus, cancel/resume, streaming announcement budget. | v0.19 | High: shared setup-session contracts. |
| Environment Brief | Not implemented | Personalized first useful result | Native grouped list/outline in Health content | Add domain-specific surface | No generic native component captures coverage/findings/consent together. | Heading hierarchy, streamed row batching, keyboard inspection. | v0.19 | High: schemas and staged discovery. |
| Reviewed first-run plan | Not implemented as WOW contract | Explain proposed typed actions | Updates plan outline + bounded confirmation sheet | Add/reuse | Must share plan semantics with normal Updates. | Selection inspector, consequence text, default/cancel, no hidden action. | v0.19-v0.20 | High: contract parity and revalidation. |
| Verified-improvement progress | Not implemented as a continuous workflow | Execute and verify one approved action | Activity workflow outline/progress | Add domain-specific surface | Must distinguish apply from verification and recovery. | Stage announcements, cancel consequences, focus continuity. | v0.20-v0.21 | High: setup session persistence. |
| Action Receipt | Not implemented | Before/after, actions, verification, recovery limits | Detail view/inspector with standard Share/Copy/Export commands | Add domain-specific surface | Receipt is central Helm trust value and has no native equivalent. | Semantic headings/table, redacted copy default, keyboard/VoiceOver complete. | v0.21 | High: redaction and receipt schema. |

## Custom Component Exception Rule

A custom production component is allowed only when its implementation PR records:

1. The Helm-specific information or safety need a native primitive cannot express.
2. Pointer, keyboard, Full Keyboard Access, VoiceOver, focus, selection, and menu-command behavior.
3. Light, dark, inactive, Increased Contrast, Reduce Motion, and Reduce Transparency behavior.
4. Localization and text-expansion behavior at minimum and expanded sizes.
5. Loading, empty, partial, failure, blocked, offline, cancellation, verification, and recovery states that apply.
6. Screenshot/state fixture ownership and rollback path.

Brand preference alone is not sufficient justification.
