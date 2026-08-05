# Native macOS Information Architecture Decision

Status: approved v0.18 planning direction
Implementation: deferred to v0.19-v0.22

## Primary Jobs

The IA is evaluated against six release-critical jobs:

1. Determine whether the environment needs attention.
2. Review and run a safe, authority-ordered update plan.
3. Find, inspect, install, update, pin, or remove a package.
4. Understand active work and recover from failure or interruption.
5. Understand a manager installation, provenance, policy, and source choice.
6. Complete first run from Environment Brief through verified value and Action Receipt.

Durable preferences are necessary but are not an operational job. Diagnostics is supporting evidence attached to health, activity, and sources rather than a destination users must learn before they have a problem.

## Shared Responsibility Model

| Surface | Primary question | Contains | Excludes |
|---|---|---|---|
| Menu bar icon | Does Helm need my attention? | Highest-priority condition and accessible summary | Multiple simultaneous metrics or detailed controls |
| Menu bar popover | What needs attention now, and what is the next action? | Cached condition, freshness, one primary action, short active-work list, Open Helm | Settings, package catalog, long logs, deep manager controls, full search |
| Control Center | What is true, what can I do, what happened, and how do I recover? | Health, Updates, Packages, Activity, Sources, search, diagnostics context, receipts | Durable app preferences |
| Settings | How should Helm behave over time? | General, update policy, source defaults, CLI integration, support/export preferences | Refresh, run plan, repair, task logs, reset flow, live service health |
| Sheet | What bounded decision must I make before returning? | Legal acceptance, final confirmation, install choices, privilege consequence | Browsing, long diagnostics, multi-destination navigation |
| Inspector | What is the context for the selected item? | Attributes, consequences, domain actions, recovery context | Independent navigation tree or unrelated global controls |

## Option A: Job-First Five Destinations

```text
Health
Updates
Packages
Activity
Sources

Settings...        separate window
Search             toolbar/menu command, results in Packages or scoped overlay
Diagnostics        contextual under Health, Activity, or Sources
Receipts           Activity filter/detail, deep-linkable
```

Definitions:

- **Health**: prioritized findings, scan coverage/freshness, current condition, Project WOW Environment Brief entry and completion summary.
- **Updates**: current reviewed plan, authority stages, scope, risk, apply, verification, and plan recovery.
- **Packages**: installed/upgradable/available catalog, package search, filters, comparison, package actions.
- **Activity**: active workflows/tasks, approvals, failures, interruptions, history, verification, Action Receipts, diagnostics.
- **Sources**: managers, install instances, provenance, eligibility, dependencies, source policy, manager-level setup/repair.

Strengths:

- Directly names the five recurring operational jobs.
- Removes Settings from operational navigation without hiding it.
- Gives failures, verification, receipts, and recovery one continuous Activity home.
- Renames ambiguous implementation terms: Overview becomes Health, Tasks becomes Activity, Managers becomes Sources.
- Keeps Updates separate from Packages because reviewing an authority-ordered multi-manager plan is a distinct high-frequency safety workflow.

Costs:

- Five top-level destinations remain.
- Users may initially look for manager-specific failures in Health or Activity rather than Sources; deep links and selection breadcrumbs must resolve this.
- Search results need an explicit origin/return model because Search is not a destination.

## Option B: Workbench with Three Destination Groups

```text
Status
Library
Operations

Status
  Environment Brief
  Findings

Library
  Updates
  Packages
  Sources

Operations
  Active
  History
  Receipts
  Diagnostics
```

Strengths:

- Only three top-level concepts.
- Accommodates future Pro/Fleet collections without adding peers.
- Makes Activity history and receipt hierarchy explicit.

Costs:

- Adds a second navigation level before Updates, Packages, or Sources.
- `Library` is not a natural term for update planning or manager provenance.
- Novices must understand Helm's taxonomy before acting.
- Frequent jobs require extra navigation and can become hidden when the sidebar is collapsed.

## Evaluation

Score: 1 poor, 3 acceptable, 5 strong.

| Criterion | Weight | Option A | Option B | Notes |
|---|---:|---:|---:|---|
| Health in under 5 seconds | 5 | 5 | 4 | Both lead with status; A uses explicit Health. |
| Safe update-plan access | 5 | 5 | 3 | A is one selection; B nests under Library. |
| Package search/action efficiency | 4 | 5 | 3 | A has direct Packages plus toolbar search. |
| Failure/recovery continuity | 5 | 5 | 5 | Both give Activity/Operations a durable role. |
| Manager-source comprehension | 4 | 5 | 3 | Sources is direct in A; Library is ambiguous in B. |
| Novice predictability | 5 | 5 | 3 | Job labels outperform taxonomy groups. |
| Expert keyboard efficiency | 4 | 5 | 4 | Both can have shortcuts; A needs fewer steps. |
| Window-width adaptability | 3 | 4 | 4 | Both fit a source-list sidebar. |
| Project WOW compatibility | 5 | 5 | 4 | Health and Activity map directly to brief and receipt. |
| Future expansion restraint | 3 | 4 | 5 | B has more grouping capacity; A requires discipline. |
| Weighted result | 43 | **207/215** | **155/215** | Option A approved. |

## Decision

Approve **Option A: Job-First Five Destinations**.

This does not merely preserve the current six sections:

- Remove Settings from the sidebar and popover overlay.
- Redefine Overview as prioritized Health, including Environment Brief and coverage.
- Redefine Tasks as Activity, including workflow, verification, receipts, interruption, and history.
- Redefine Managers as Sources, emphasizing provenance, authority, policy, and install instances rather than implementation objects.
- Keep Updates separate because the plan is a cross-manager safety object, not a package filter.
- Keep Search as a command rather than a sixth destination.
- Place diagnostics contextually, not as a top-level dumping ground.

## Control Center Hierarchy

Expanded width:

```text
Window frame / native toolbar
  [Sidebar] [Title]                  [Search] [Refresh] [Inspector] [Primary]

Navigation source list | Content list/table or Health content | Inspector
Health                 | prioritized findings                | selected finding
Updates                | authority plan outline              | selected plan step
Packages               | package table                       | selected package
Activity               | active/history outline              | task/workflow/receipt
Sources                | manager/source table                | selected source/instance
```

Minimum width:

```text
Toolbar: [Sidebar] [Title] [Search icon] [More]
Sidebar or content; inspector hidden
Selected detail opens as a replacement detail or sheet only when the task is bounded
```

Pane rules:

- Sidebar is visible by default, user-hideable, and restorable through View > Show/Hide Sidebar.
- Inspector is contextual and user-hideable through View > Show/Hide Inspector.
- Selected content remains selected when inspector hides or snapshot data refreshes.
- Sidebar and inspector widths restore within safe bounds.
- A content list/table owns selection. Buttons inside a row do not become an alternate selection model.
- No more than two navigation levels appear in the sidebar. Deeper detail belongs in content and inspector.

## Destination Contracts

### Health

Content order:

1. Current condition and last useful/correctness state.
2. Required approvals or failures.
3. Actionable findings.
4. Coverage, freshness, deferred checks, and unchanged state.
5. Healthy summary collapsed by default.

Inspector:

- Finding evidence, controlling manager/authority, consequence, recommendation, and recovery.
- `Open in Activity`, `Open Source`, or `Review Plan` deep links retain finding selection.

### Updates

Content:

- Reviewed plan outline grouped by authority phase.
- Scope/filter controls in toolbar or table header.
- Columns for item, source, current/target, status, consequence.
- Explicit empty, stale, offline-deferred, blocked, cancelled, failed-verification, and recovery states.

Inspector:

- Why included/excluded, pin policy, privilege/network/restart needs, verification, rollback limits.

### Packages

Content:

- Table/list of consolidated package identities.
- Filters: status and source; saved density is optional later.
- Local search response appears before remote enrichment.
- Search scope labels identify local, cached, and remote results.

Inspector:

- Manager member/version selection, provenance summary, package action, pinning, history, and description.

### Activity

Content groups:

- Needs Attention: approvals, failure, failed verification, interrupted sessions.
- Active: queued/running/verifying/cancelling.
- History: verified, completed-with-limits, cancelled, deferred.
- Receipts filter: Action Receipts and setup-session summaries.

Inspector/detail:

- Workflow hierarchy first, raw task/log detail second.
- Before/after, verification, diagnostics, retry/resume, and honest rollback limits.

### Sources

Content:

- Source table grouped or sortable by authority.
- Columns for source, status, enabled policy, active provenance, packages/updates.
- Multi-instance attention and setup-required states are visible without opening inspector.

Inspector:

- Install instances and active choice, evidence/confidence, eligibility, dependency, install/update/uninstall/repair actions.
- Long logs route to Activity diagnostics rather than expanding indefinitely.

## Search Model

- Command-F focuses toolbar search from any destination.
- Empty search does not navigate.
- Typing shows immediate local results without destroying current destination/selection.
- If the query clearly targets packages, accepting a result opens Packages and selects the package.
- Source, activity, setting, and command matches appear as labeled result groups when implemented.
- Remote package enrichment begins after the existing debounce and reports `Searching remote sources...`, source coverage, and cancellation.
- Offline remote work is `Deferred until online`; local/cached results remain usable.
- Escape clears the results overlay first, restores focus to the originating content, and does not discard the originating selection.
- Recent queries are local only and require a separate privacy decision before persistence; no history is required for v0.19.

## Deep-Link Contract

Every deep link resolves to:

```text
destination + stable entity/workflow ID + optional inspector subview + focus target
```

| Origin | Example target | Required behavior |
|---|---|---|
| Menu bar condition | `Health/finding-id` | Open/focus Control Center, select finding, show inspector if space permits. |
| Popover active task | `Activity/workflow-id/task-id` | Preserve active group and reveal task without losing live progress. |
| Timeout notification | `Activity/task-id/approval` | Activate app, select approval, focus Wait/Stop group; notification action may respond directly through core. |
| Completion notification | `Activity/workflow-id/receipt` | Open verified result or recovery state, not generic Activity root. |
| Update notification | `Updates/plan-id` | Show current revalidated plan; stale plan state cannot execute silently. |
| Source context action | `Sources/manager-id/instance-id` | Select source and instance in inspector. |
| Package context action | `Packages/consolidated-id/member-id` | Preserve manager member choice and package filter origin. |
| Help/contextual tip | current destination + anchor ID | Never changes operational selection unless user follows an explicit link. |

If an ID is stale, show a nonmodal `This item is no longer available` message and keep the nearest valid destination. Never silently select an unrelated first row.

## Settings Separation

Approved panes:

- **General**: launch at login, language, appearance/density if later approved.
- **Updates**: check cadence, notification behavior, guarded-action defaults, auto-apply policy if implemented.
- **Sources**: durable safe mode and source-policy defaults, not live manager enablement or repair.
- **CLI**: shim preference/status and install/remove if treated as durable integration setup.
- **Support**: diagnostics inclusion/redaction defaults, support links, update channel information where appropriate.

Move out of Settings:

- Refresh -> toolbar/App or File command.
- Live service health and errors -> Health/Activity.
- Copy task/service diagnostics -> Activity/detail or Help menu.
- Reset Local Data -> Help > Troubleshooting or a bounded diagnostics workflow.
- Restore manager priority -> Sources contextual command.
- Replay walkthrough -> Help.
- Quit -> App menu.
- Metrics/deep links -> remove.

## Progressive Disclosure

Novice defaults:

- Health presents one condition and one next action.
- Update plan starts with summary, stages, and consequences; raw task intent and logs are collapsed.
- Sources shows human-readable provenance and policy before scores/margins.
- First run asks no generic preference questions before Environment Brief.

Expert paths:

- Toolbar/menu/context commands and destination shortcuts.
- Inspector exposes versions, source instances, authority, confidence, decision margin, and exact typed intent.
- Activity exposes command intent, stdout/stderr, structured logs, and export.
- Table sorting, source filters, Copy, and Reveal/Open related context are keyboard-accessible.

Progressive disclosure must never hide:

- whether a mutation has occurred
- privilege, network, restart, and external authority
- policy versus permission blocking
- verification state
- rollback limitations
- partial coverage, stale data, or deferred work

## Application Menu and Commands

Minimum pre-1.0 command coverage:

| Menu | Commands |
|---|---|
| Helm | About Helm; Settings... `Command-,`; Hide Helm; Hide Others; Show All; Quit Helm `Command-Q` |
| File | Open Helm/Control Center; Close Window `Command-W`; Export/Copy Receipt when applicable |
| Edit | Undo/Redo where meaningful; Cut/Copy/Paste/Select All; Find `Command-F` |
| View | Health/Updates/Packages/Activity/Sources destinations; Show/Hide Sidebar; Show/Hide Inspector; Refresh `Command-R`; density if approved |
| Activity | Review Plan; Run Approved Plan; Cancel/Stop; Retry; Resume; View Receipt, context validated |
| Window | Minimize; Zoom; Bring All to Front; Control Center and Settings entries |
| Help | Helm Help; Search Help; Diagnostics/Troubleshooting; optional contextual tour; Report a Problem |

Shortcut candidates must be validated for conflicts. Do not assign Command-1 through Command-5 until the real app menu and text-editing behavior are tested on the minimum OS.

## Intentional Departures from macOS Conventions

Approved departures are limited:

| Departure | Helm-specific benefit | Required guardrail |
|---|---|---|
| Menu-bar-first accessory activation instead of Dock-first app | Ambient package/update monitoring without Dock clutter | Standard application menu whenever active; normal Control Center window behavior; no hidden-only commands. |
| Domain-specific status and Action Receipt views | Native controls do not express authority, verification, and rollback truth as one model | All interaction and appearance budgets apply; use native containers, text, selection, and commands around custom content. |
| Authority-stage update outline | Helm must show deterministic execution ordering | Backend order is authoritative; UI cannot reorder execution policy. |

Custom cards, nonstandard pointers, fixed windows, empty Settings scenes, and missing menu commands are not approved departures.

## Validation Gate

The IA is design-approved for implementation planning, but human validation remains required:

- Owner-run moderated checkpoint in `NATIVE_MACOS_RESEARCH_VALIDATION.md`.
- Compare Option A labels and findability with Option B during first-click tasks.
- If fewer than 8 of 10 participants find Updates, package install, failure recovery, source provenance, and Settings without assistance, reopen labels/placement before v0.20 UI lock.
