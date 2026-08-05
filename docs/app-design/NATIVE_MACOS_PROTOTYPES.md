# Native macOS Experience Prototypes

Status: annotated low/high-fidelity planning prototypes
Implementation: no production SwiftUI is included

These wireframes specify hierarchy, state, input, and adaptive behavior. They intentionally use repository-native text so contracts can be reviewed before production implementation. Visual styling follows system typography, colors, controls, source lists, toolbars, tables, forms, sheets, and inspectors. Helm Blue is an accent; Rope Gold is not used for ordinary updates or warnings.

## Shared Prototype Rules

- Window minimum: `860x600` points. Expanded reference: `1280x800` points.
- Popover reference: 380-400 points wide; content height is stable within a condition class and capped to the visible screen.
- Settings reference: 680 points wide, pane-height adaptive within sensible bounds.
- First useful content never waits for complete refresh.
- Toolbar, menu, and context commands share one validated command route.
- Tab moves among major focus groups in reading order. Arrow keys move selection within source lists, tables, outlines, radio groups, and segmented controls.
- Inspector is shown at expanded widths, hidden at minimum width, and remembers selection.
- `Return` activates a sheet's default action only. `Escape` dismisses the top bounded presentation and restores focus to its trigger.
- All examples are localized content contracts, not final English copy.

## 1. Menu Bar Popover

### Low fidelity

```text
+--------------------------------------+
| Attention required                   |
| 2 failures need review               |
| Updated 2 min ago | 9/11 sources     |
|                                      |
| [Review Failures]                    |
+--------------------------------------+
| Active                               |
| ~ Updating Homebrew          2 of 6 |
| ! npm verification failed           |
+--------------------------------------+
| Open Helm                      Cmd-O?   |
+--------------------------------------+
```

The final shortcut is assigned only after command-conflict validation.

### High-fidelity annotation

| Region | Native pattern | Content/behavior |
|---|---|---|
| Condition | `VStack` with semantic `Label` and system text styles | One prioritized condition. Healthy state says `No action needed`; it does not show three zero cards. |
| Freshness | Secondary text | `Current`, `Cached 18 min ago`, `7 of 9 sources`, or `Offline; network checks deferred`. |
| Primary action | One standard prominent button | Review Failures, Review Updates, Respond, Resume, Retry, or Refresh. Never `Fix Automatically`. |
| Active work | Native list rows, maximum three | Workflow name, stage, progress/state. Selecting opens Activity context. |
| Footer | Standard link/button or menu | Open Helm. Settings remains in the app/status menu, not the body. |

Focus order:

1. Primary action.
2. Active-work rows, if any.
3. Open Helm.

The condition text is a heading/summary, not a focus stop unless it exposes details.

State variants:

```text
HEALTHY
No action needed
Last checked 4 min ago | 11 sources
[Open Helm]

CACHED / REFRESHING
No urgent issues in cached results
Cached 18 min ago | Refreshing 7 of 11 sources
[Open Helm]

OFFLINE
Local status available
Offline | 3 network checks deferred
[View Local Results]

APPROVAL REQUIRED
Homebrew needs more time
Running 3h 52m | no output for 28m
[Respond]

PARTIAL FAILURE
2 sources need retry
9 of 11 sources mapped | cached values retained
[Review Failures]
```

Dark/inactive/contrast:

- System popover/material appearance; opaque fallback under Reduce Transparency.
- Inactive is not applicable while the panel is key; if a retained panel can become inactive, actions and selection follow system inactive appearance.
- Increased Contrast adds system-defined boundaries, not a second custom tint.

## 2. Control Center Shell

### Expanded `1280x800`

```text
+- window frame / toolbar ---------------------------------------------------+
| [Sidebar] Health                     [Search Helm] [Refresh] [Inspector] [Action] |
+---------------+--------------------------------------+----------------------+
| Health        | Attention required                   | Finding              |
| Updates   12  | 2 items need review                  | npm verification     |
| Packages      |                                      | failed               |
| Activity   1  | NEEDS ATTENTION                      |                      |
| Sources       | ! npm verification failed            | What happened        |
|               | ! Homebrew permission denied         | ...                  |
|               |                                      |                      |
|               | FINDINGS                             | Next actions         |
|               | > Duplicate rustup installations     | [Retry] [Diagnostics]|
|               | > 12 updates ready                   |                      |
|               |                                      | Rollback             |
|               | COVERAGE                             | No package rollback  |
|               | 9 of 11 current | 2 cached           | is available.        |
+---------------+--------------------------------------+----------------------+
```

### Minimum `860x600`

```text
+----------------------------------------------------------------------------+
| [Sidebar] Health                       [Search] [Refresh] [More]                  |
+---------------+------------------------------------------------------------+
| Health        | Attention required                                         |
| Updates   12  | 2 items need review                                        |
| Packages      |                                                            |
| Activity   1  | ! npm verification failed                                  |
| Sources       | ! Homebrew permission denied                               |
|               | ...                                                        |
+---------------+------------------------------------------------------------+
```

At minimum width the inspector is hidden, its toolbar toggle remains available, and opening detail either reveals the inspector by replacing content width or uses a detail navigation transition. It never overlays critical controls unexpectedly.

Toolbar placement:

- Leading: sidebar toggle and destination title.
- Center/trailing: global search, Refresh, inspector toggle, optional More overflow.
- One trailing primary action only when the current destination has an unambiguous next step, such as Review Plan. Run remains inside reviewed context/final confirmation.
- Every item exists in an application menu; the toolbar is not required for command access.

Window behavior:

- Minimize and zoom enabled; full-screen behavior validated rather than disabled by default.
- Restore frame to an available display, destination, sidebar/inspector visibility and widths, selection, filters, and scroll anchor.
- Inactive selection uses system gray/inactive appearance. Custom status text remains legible but subdued.

Focus order:

1. Toolbar focus group.
2. Sidebar focus group.
3. Content selection/list group.
4. Inspector group when visible.

Tab enters a group; arrows navigate within it. A toolbar search result returns focus to the selected content after acceptance.

## 3. Settings Window

### High fidelity

```text
+--------------------------------------------------------------+
|                    Helm Settings                             |
| [General] [Updates] [Sources] [CLI] [Support]                |
+--------------------------------------------------------------+
| General                                                      |
|                                                              |
| Launch at login                              [switch]         |
| Language                                     [System Default]|
| Density                                      [Comfortable v] |
|                                                              |
| Appearance                                                   |
| Follow system appearance                    [selected]        |
+--------------------------------------------------------------+
```

Rules:

- Standard Settings scene/window, App menu item, Command-Comma.
- Noncustomizable pane toolbar or native Settings navigation appropriate to supported OS.
- No Control Center inspector, health badge, task count, Refresh, Quit, repair, or live diagnostics.
- Pane height may adapt; minimize/zoom follow Apple Settings guidance and OS behavior.
- Forms use full labels, standard controls, help text, and wrapping rather than two-column custom button grids.

Representative offline/failure behavior:

- Settings that persist locally remain editable offline.
- A preference requiring service persistence shows `Could not save` inline with Retry; it does not pretend to be applied.
- Managed settings show `Managed by your organization`, controlling authority, and disabled control explanation.

Focus order follows visual form order. Tab/Shift-Tab reaches every enabled control under Full Keyboard Access. Pane buttons use arrows when exposed as a toolbar selection group.

## 4. Project WOW Environment Brief

### First useful render

```text
+----------------------------------------------------------------------------+
| Health                                                     [Cancel Scan]   |
+----------------------------------------------------------------------------+
| Your environment, mapping...                                           H1  |
| Local scan | No changes | No network                                      |
|                                                                            |
| [x] macOS 15.6 | Apple silicon | zsh                                         |
| [x] 4 managers found                                                         |
| ~ Mapping packages                                                         |
|                                                                            |
| [Use Helm Now]                                       details: What checked |
+----------------------------------------------------------------------------+
```

### Complete local brief

```text
+----------------------------------------------------------------------------+
| Health                                                       [Check Now]   |
+----------------------------------------------------------------------------+
| Your environment, mapped.                                             H1   |
| Local scan complete | No changes made | No network used                    |
|                                                                            |
| 9 managers ready | 214 packages organized                                 |
| 2 items need attention | 7 of 9 sources current, 2 partial                 |
|                                                                            |
| NEEDS ATTENTION                                                            |
| > rustup has two installations                    [Review]                  |
| > mise shell setup is incomplete                  [Review]                  |
|                                                                            |
| DEFAULTS APPLIED                                                           |
| Eligible managers monitored | guarded actions require confirmation         |
|                                                                            |
| [Use Helm Now] [Customize]                            [Review Plan] primary |
+----------------------------------------------------------------------------+
```

Streaming rules:

- Summary geometry reserves stable rows; counts update without moving primary actions.
- New findings enter a sorted region; focus and user scroll never jump.
- VoiceOver announces stage summary, not each row: `Local mapping update: 7 of 9 sources complete; 2 findings.`
- Cancel stops cancellable discovery and leaves partial results. `Use Helm Now` remains available unless the legal gate is incomplete.
- Network Stage D begins only through `Check Now` and updates trust copy before work starts.

Failure/offline examples:

```text
PARTIAL
7 of 9 sources mapped
Homebrew timed out | npm returned incomplete data
Cached values shown where available
[Retry 2 Sources] [Use Helm Now]

OFFLINE
Local environment mapped
Update checks and remote catalogs deferred until online
[Use Helm Now] [Review Deferred Work]

SERVICE UNAVAILABLE
Cached environment available
Helm could not start local discovery
[Retry] [Open Diagnostics] [Use Cached Results]
```

## 5. Reviewed First-Run Plan

### Plan content with inspector

```text
+----------------------------------------------------------------------------+
| Review Plan                                  [Back]              [Continue] |
+----------------------------------------------+-----------------------------+
| 1  Repair stale mise executable              | Why this applies            |
|    No network | No authorization             | Selected path no longer...  |
|                                              |                             |
| 2  Install Helm CLI shim                     | Action                      |
|    Local file change | Reversible             | select_manager_executable   |
|                                              |                             |
| 3  Complete rustup shell setup               | Changes                     |
|    Shell file change | Review backup          | Helm preference only        |
|                                              |                             |
| 4  Review 12 updates                         | Verification                |
|    Network later | No update runs now         | Re-detect active path       |
|                                              |                             |
| [ ] Include optional CLI shim                | Recovery                    |
|                                              | Prior preference restored   |
+----------------------------------------------+-----------------------------+
```

Final confirmation sheet:

```text
Apply 2 approved changes?

Network: No
Administrator authorization: No
Files/settings: Helm preference and ~/.local/bin/helm
Verification: Re-detect executable and CLI ownership
Rollback: Both changes are reversible while Helm ownership matches

[Cancel]                                           [Apply Changes]
```

Plan rules:

- Recommendations are visibly not applied.
- Every row carries typed action ID, manager/authority, network, privilege, changes, verification, and rollback limit in detail.
- Continue opens a bounded confirmation only after required rows are reviewed.
- Stale preflight returns to plan with `Plan changed`, preserving still-valid selections.
- Policy-blocked items cannot be selected and identify controlling authority. Permission-required items remain selectable with just-in-time authorization disclosure.

## 6. Verified-Improvement Progress

```text
+----------------------------------------------------------------------------+
| Improving this Mac                                              [Stop]     |
+----------------------------------------------------------------------------+
| 1  Revalidate plan                                      [x] Verified         |
| 2  Repair stale mise executable                         [x] Applied          |
| 3  Verify active executable                             ~ Verifying        |
| 4  Record Action Receipt                                Waiting            |
|                                                                            |
| Verifying mise at the selected path...                                     |
| No administrator authorization requested.                                  |
|                                                                            |
| Details > command intent, task output, elapsed time                         |
+----------------------------------------------------------------------------+
```

Failed verification:

```text
Change applied; verification failed
The preference was saved, but mise still resolves to the previous executable.

[Retry Verification] [Restore Prior Preference] [Keep Change] [Diagnostics]
```

Rules:

- Applying and verifying are distinct states.
- `Stop` explains whether current process stops immediately and whether applied work remains.
- Stage selection and details remain keyboard-accessible while progress updates.
- Announce stage transitions once. Do not announce every output line or percentage tick.
- Closing the window does not imply cancellation; reopening/deep link restores workflow context.

## 7. Action Receipt and Recovery Context

```text
+----------------------------------------------+-----------------------------+
| Action Receipt                              | Selected change             |
| Aug 4, 2026 at 4:32 PM | Verified           | mise executable preference  |
|                                             |                             |
| BEFORE                                      | Before                      |
| mise path unavailable                       | PATH default                |
|                                             |                             |
| CHANGES                                     | After                       |
| [x] Selected /opt/homebrew/bin/mise            | /opt/homebrew/bin/mise      |
| [x] Installed Helm-owned CLI shim              |                             |
|                                             | Verification                |
| AFTER                                       | Detected and version read   |
| mise detected and ready                     |                             |
| Helm CLI available                          | Recovery                    |
|                                             | Restore prior preference    |
| UNCHANGED                                   | available                   |
| No packages upgraded                        |                             |
| No administrator authorization requested    | Limits                      |
|                                             | No package rollback needed  |
| [Copy Redacted Summary] [Export] [Done]      |                             |
+----------------------------------------------+-----------------------------+
```

Interrupted recovery:

```text
Setup was interrupted after 1 of 3 changes

Verified: mise executable preference
Unverified: Helm CLI shim
Not started: rustup shell setup

[Resume] [Retry Unverified] [Keep Verified Changes] [Review Rollback]
```

Honest rollback limit:

```text
Rollback is not available for this package upgrade.
Helm can stop remaining work and preserve diagnostics, but the manager does not
provide a verified downgrade path.

[Keep Completed Changes] [Stop Remaining Work] [Export Diagnostics]
```

Receipt rules:

- Receipt status is Verified, Partially Verified, Failed Verification, Interrupted, or No Changes.
- Copy is redacted by default according to the shared schema. Paths, usernames, package names, and organization IDs require explicit inclusion.
- Recovery commands are available only when core contract says they are valid now.
- Receipt stays in Activity and deep links from notifications/popover.

## Appearance Variants

| Variant | Prototype requirement |
|---|---|
| Light | System window/content backgrounds, standard accent selection, restrained semantic fills. |
| Dark | System dark materials/colors; output surfaces maintain readable code contrast; no blanket navy tint over controls. |
| Inactive | System list/table selections become inactive; custom receipt/status regions reduce accent saturation without obscuring state words. |
| Increased Contrast | Boundaries and focus become stronger; warning/error remain icon + word + structure; custom thin rails are not required. |
| Reduce Motion | Cross-destination continuity uses no scale/spring/blur; immediate or short opacity transitions only. |
| Reduce Transparency | Popover/sidebar/toolbar receive opaque semantic backgrounds and separators. |

## Prototype Review Checklist

- Minimum and expanded sizes preserve all safety information.
- Toolbar items have menu equivalents and overflow behavior.
- Focus order matches reading order; arrows work within every collection.
- Selection remains visible and distinct from focus in key and inactive windows.
- Every failure/offline/blocked example identifies what remains true and the next action.
- Environment Brief, plan, progress, and receipt use shared Project WOW IDs and state terms after contract integration.
- No prototype implies production behavior exists in v0.18.
