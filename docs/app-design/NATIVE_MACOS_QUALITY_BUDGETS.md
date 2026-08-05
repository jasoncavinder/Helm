# Native macOS Accessibility, Input, and Perceived-Performance Budgets

Status: measurable pre-implementation requirements

These are acceptance budgets, not aspirations. A milestone cannot claim closure by showing an ideal screenshot while missing keyboard, accessibility, state, or timing evidence.

## Test Conditions

Performance results record:

- Mac model, CPU, memory, macOS version, display scale, power state, build configuration, and database fixture size.
- Warm/cold launch and cache state.
- Median, p95, and worst observed value across at least 30 measured iterations for interaction timings.
- `HELM_DB_PATH` pointing to an isolated test database for development/automation.
- Representative small fixture and stress fixture. Stress baseline: 30 sources, 20,000 consolidated package rows, 2,000 activity records, 100 plan steps, and one active output stream.

Release sign-off runs a production-equivalent build where permitted, but agent/development inspection never uses the stable user database.

## Accessibility and Input Budget

### Traversal and focus

| Requirement | Objective pass condition |
|---|---|
| Tab / Shift-Tab | With Full Keyboard Access enabled, every enabled interactive control in the current surface is reachable exactly once in logical reading order. Reverse traversal is the exact inverse. |
| Focus groups | Control Center order is toolbar -> sidebar -> content -> visible inspector. Tab enters/leaves groups; arrows navigate collections. No more than one Tab stop per passive row. |
| No traps | 100% of sheets, popovers, menus, output views, and inspectors can be exited by documented keyboard action without closing unrelated work. |
| Initial focus | Window opens on last meaningful content selection; a first-run legal sheet focuses its heading/group before the default action; destructive action is never initial focus. |
| Focus restoration | Closing a sheet/popover/alert returns focus to the invoking control or selected row in 100% of release-critical flows. Removed triggers return to their containing collection. |
| Visible focus | Focus indicator is visible at 100% display scaling in light, dark, inactive/key transitions, and Increased Contrast. Focus and selection remain distinguishable. |
| Refresh continuity | Refresh/poll/stream updates produce zero unexpected focus moves in a 10-minute active-work test. |

### Collection navigation

| Requirement | Objective pass condition |
|---|---|
| Arrow keys | Sidebar, tables, lists, outlines, segmented controls, radio groups, and pane selectors support platform-standard arrows. |
| Home/End/Page | Long package, activity, source, and plan collections support expected beginning/end/page navigation where the native control provides it. |
| Type selection | Package/source tables support type-to-select when focus is in the collection and search is not active. |
| Selection persistence | Selection survives sorting, non-destructive filtering when still visible, refresh, inactive/active window change, inspector hide/show, and window close/reopen. |
| Reorder alternatives | Every drag reorder that remains has keyboard and menu commands, including Move Up/Down or Restore Default. |
| Context commands | Selection-scoped commands are available through a context menu and at least one discoverable toolbar/menu/inspector route. |

### Default and cancel actions

- Every bounded confirmation has at most one default action.
- Return never triggers a destructive action while focus is in a multiline field, search results, table, or disclosure.
- Escape closes the topmost bounded presentation before the parent window.
- Cancel states consequences: unstarted work stops, active work enters Cancelling, and completed changes remain unless a verified restore runs.
- Destructive actions use destructive roles and never receive implicit default focus.
- Closing a window is not cancellation.

### VoiceOver semantics

| Element | Requirement |
|---|---|
| Window/destination | Window title and selected destination are announced once on activation/navigation. |
| Headings | One H1-equivalent per content view; sections and inspector groups expose ordered headings. |
| Lists/tables/outlines | Collection label, row count when stable, selected row, column headers, hierarchy level, disclosure state, and sort state are exposed. |
| Status | Label + value, for example `Verification, failed`; never icon/color alone. |
| Controls | Localized label, role, current value/state, disabled reason where API permits, and concise help for unfamiliar icon-only commands. |
| Groups | Finding, plan consequence, before/after, recovery, and receipt regions are navigable groups rather than one giant combined element. |
| Actions | Custom rows expose only valid actions and do not hide primary operations inside unlabeled gestures. |
| Streaming | Announce stage start, batched progress summary, approval/failure, and terminal outcome. Never announce each manager row, log line, polling tick, or percentage. |
| Announcement rate | Routine streaming announcements no more than one per 2 seconds and only when summary meaning changes; urgent approval/failure may interrupt once. |
| Output | Live output is readable/selectable on demand but does not act as a live region by default. |

VoiceOver pass gate:

- All six primary jobs and Project WOW first run complete with VoiceOver without pointer use.
- Zero unlabeled enabled controls in Accessibility Inspector scan of release-critical fixtures.
- Zero duplicate names that make adjacent actions indistinguishable.
- Rotor headings and controls produce a usable, correctly ordered summary.

### Visual accessibility

| Preference/condition | Requirement |
|---|---|
| Increased Contrast | Status, selection, focus, dividers, fields, and disabled controls remain distinguishable. Custom 1 px low-opacity borders are not the sole boundary. |
| Differentiate Without Color | Every health, task, block, verification, and selection state uses text/icon/shape or position in addition to color. |
| Reduce Motion | No spring, bounce, scale, spotlight travel, blur transition, or auto-scrolling layout motion. Use immediate change or opacity no longer than 150 ms where continuity benefits. |
| Reduce Transparency | Popover, sidebar, toolbar, sheets, and overlays use opaque semantic backgrounds with clear boundaries. No content depends on blur/vibrancy. |
| Dark/light | Text meets 4.5:1 for normal text and 3:1 for large text; essential control/focus/state boundaries meet 3:1 against adjacent colors, measured on custom surfaces. |
| Inactive window | Selection remains identifiable but follows system inactive appearance; destructive/primary custom tint does not imply the window accepts input. |
| Control size | Prefer Apple's 28x28 pt default. No pointer target below Apple's 20x20 pt macOS minimum; adjacent compact controls have sufficient separation. |
| Text size | Default body follows system 13 pt. No essential text below 10 pt. Do not use scaling transforms to shrink native controls. |

### Localization and text expansion

- All seven current locales pass the same workflows at `860x600`, `1280x800`, and Settings minimum.
- Pseudolocalized Latin text at +40% length and a 2x-long safety-message fixture cause zero clipped safety text, hidden default/cancel actions, overlapping controls, or inaccessible horizontal-only content.
- Japanese and German are included in every critical sheet/window pass; Hungarian remains included despite staged rollout status.
- Titles and row labels wrap or truncate only when full text remains available through inspector/help/accessibility value.
- Safety consequence, permission/policy reason, verification, and rollback-limit text never truncates without an immediately accessible full representation.
- Date, count, version, path, and list formatting use locale-appropriate APIs while machine identifiers remain selectable monospaced content where useful.

### Pointer-independent and application-menu coverage

100% keyboard/menu coverage is required for:

- Open/focus Control Center and Settings.
- Navigate Health, Updates, Packages, Activity, and Sources.
- Focus and clear search; accept a result; cancel remote search.
- Refresh/revalidate current scope.
- Review, confirm, and cancel an update/setup plan.
- Select and inspect package, activity, source, finding, and plan rows.
- Install/update/pin/unpin/uninstall where action is valid.
- Cancel/stop, retry, resume, respond to timeout, and open diagnostics.
- Copy/export redacted receipt or diagnostics.
- Show/hide sidebar and inspector.
- Reorder/restore source priority if reordering remains.

Application menu minimum is defined in `NATIVE_MACOS_INFORMATION_ARCHITECTURE.md`. Toolbar-only or hover-only commands fail this budget.

## Perceived-Performance Budget

### Timing definitions

- **Input acknowledgement**: visual/control response to input.
- **First useful render (FUR)**: stable structure plus enough cached/local content to make the next decision.
- **Complete refreshed state (CRS)**: all declared scope for the operation is terminal and coverage/freshness is visible.
- FUR and CRS are always measured and reported separately.

### Interaction budgets

| Surface/action | Input acknowledgement | First useful render | Complete refreshed state / follow-up |
|---|---:|---:|---:|
| Popover, warm cached | <=50 ms p95 | <=150 ms p95 | Cached condition/rows complete <=250 ms p95; background refresh has separate scope SLA. |
| Popover, cold no cache | <=100 ms p95 | Shell <=300 ms p95 | First local condition follows Project WOW personalized-result budget, not a blank panel. |
| Control Center, warm cached | <=50 ms p95 | <=600 ms p95 | Cached destination/selection/inspector <=900 ms p95; refresh continues visibly. |
| Control Center, cold | <=100 ms p95 | Interactive shell <=1,000 ms p95 | First destination local/cached content <=1,500 ms p95 when available. |
| Destination change | <=50 ms p95 | <=100 ms p95 for cached structure/data | <=250 ms p95 for derived local state; remote refresh remains asynchronous. |
| Inspector selection | Selected row <=50 ms p95 | Core fields <=100 ms p95 | Deferred description/log content shows progress within 150 ms and loads independently. |
| Local search | Keystroke <=16 ms p95 | Results <=100 ms p95, <=50 ms median | Complete local/cached set <=150 ms p95 on stress fixture. |
| Remote search | Local results remain | Progress/deferred scope appears <=350 ms after final keystroke | First remote result <=2 s p95 on controlled 100 ms RTT fixture; each source terminal state visible; cancel acknowledgement <=100 ms and terminal/cancelling state <=250 ms. |
| Plan revalidation | Button state <=50 ms | Existing plan remains with `Revalidating` <=100 ms | Local plan p95 <=1 s after prerequisite snapshots; manager operations retain their own timeout/state. |
| Task/progress update | No blocked input | UI reflects received revision <=250 ms p95 | Terminal result + verification transition <=500 ms p95 after authoritative state arrives. |
| Settings pane change | <=50 ms | <=100 ms p95 | Local saved value visible <=250 ms p95; service-backed save shows Saving then terminal truth. |

Project WOW budgets remain:

- Interactive shell p95 under 1 second.
- First personalized result p50 under 5 seconds and p95 under 15 seconds.
- Complete local Environment Brief p95 under 45 seconds.
- No mandatory network operation.

### Scrolling and frame stability

- On a 60 Hz display, at least 95% of measured scroll frames complete within 16.7 ms on the stress fixture after initial row realization.
- No user-visible frame exceeds 50 ms during normal scrolling; any frame over 100 ms is a release-blocking stall.
- Selection, hover, focus ring, and toolbar input remain responsive while task snapshots update.
- Rich description rendering and log formatting happen outside the scroll-critical path or use bounded cache/precomputation.
- Scrolling does not trigger remote work per row.

### Layout stability

- After FUR, the toolbar, primary action, selected row, and top visible content anchor move 0 points due solely to asynchronous data.
- Count/status text may update within reserved bounds. If content legitimately expands, no focused/selected element shifts more than 4 points unless the user opens a disclosure.
- Streaming Environment Brief inserts findings in a reserved/sorted list without changing the user's scroll offset when they are not at the insertion edge.
- Showing progress never replaces a control with a differently sized control; use reserved labels/accessories.
- Inspector loading cannot resize the window or main split panes.

### Idle resource budget

Measured after five idle minutes with Control Center and popover closed, no tasks, no search, no refresh due, over a 30-minute sample:

| Resource | Budget |
|---|---|
| App + service average CPU | <=1.0% of one logical core combined; no sustained sample >3% for more than 5 seconds without attributed maintenance. |
| Wakeups | <=2 wakeups/second combined average. |
| Memory growth | <=20 MB combined growth over 30 minutes after warm-up; no unbounded task-output or rendered-description growth. |
| Disk writes | Zero periodic writes when state is unchanged; logs/poll metadata write only on meaningful events under retention policy. |
| Network | Zero Helm-initiated network requests while offline mode is selected or no network task is scheduled. |

Visible-idle budget with Control Center open, no work:

- Average CPU <=2% of one logical core combined.
- Poll/snapshot updates produce no full-window layout invalidation detectable as selection/focus movement.
- Memory remains within 10% of post-open steady-state over 30 minutes.

## Verification Methods

Required evidence by v0.22:

- Signposted timing around status-item action, window `makeKeyAndOrderFront`, destination selection, search query revision, first result, snapshot receipt, render-ready state, and verification result.
- Instruments Time Profiler and SwiftUI/AppKit rendering investigation on stress fixture.
- XCTest/UI automation where stable for warm/cached timings; local benchmark harness for search/derivation.
- Accessibility Inspector scan plus manual VoiceOver and Full Keyboard Access protocol.
- Appearance matrix captures from deterministic fixtures.
- Activity Monitor or `powermetrics`-appropriate operator record for idle CPU/wakeups, without adding telemetry transport.

## Failure Policy

- Report the first failing metric and test condition; do not average away p95 failures.
- A timing budget may be revised only with measured evidence and a documented user-visible tradeoff.
- Remote manager duration is not a UI failure when FUR, progress, cancellation, and coverage remain within budget.
- No budget authorizes hiding stale, partial, failed, or unverified truth to appear faster.
