# Native macOS Information Architecture Decision

Status: approved Original Wayfinder direction
Implementation: staged across v0.19-v0.22

## Primary Jobs

The IA is evaluated against six release-critical jobs:

1. Determine whether the environment needs attention.
2. Review and run a safe, authority-ordered update plan.
3. Find, inspect, install, update, pin, or remove software without first knowing its manager.
4. Understand active work and recover from failure or interruption.
5. Understand a manager installation, provenance, policy, and source choice.
6. Complete first run from Environment Brief through verified value and Action Receipt.

Durable preferences are necessary but are not an operational job. Diagnostics is supporting evidence attached to Dashboard, Activity, and Environment rather than a destination users must learn before they have a problem.

## Shared Responsibility Model

| Surface | Primary question | Contains | Excludes |
|---|---|---|---|
| Menu bar icon | Does Helm need my attention? | Highest-priority condition and accessible summary | Multiple simultaneous metrics or detailed controls |
| Menu bar popover | What needs attention now, and what is the next action? | Cached condition, freshness, one primary action, short active-work list, Open Helm | Settings, package catalog, long logs, deep manager controls, full search |
| Dashboard | What is true, what can I do, what happened, and how do I recover? | Dashboard, Plan, Library, Activity, contextual Environment, search, diagnostics context, receipts | Durable app preferences |
| Settings | How should Helm behave over time? | General, update policy, source defaults, CLI integration, support/export preferences | Refresh, run plan, repair, task logs, reset flow, live service health |
| Sheet | What bounded decision must I make before returning? | Legal acceptance, final confirmation, install choices, privilege consequence | Browsing, long diagnostics, multi-destination navigation |
| Contextual detail | What is the context for the selected item? | Attributes, consequences, domain actions, recovery context | Independent navigation tree or unrelated global controls |

## Approved Wayfinder Model

```text
Dashboard
Plan
Library
Activity

Settings...        separate window
Environment        persistent contextual entry at sidebar foot
Search             toolbar/menu command, results in Library or scoped overlay
Diagnostics        contextual under Dashboard, Activity, or Environment
Receipts           Activity filter/detail, deep-linkable
```

Definitions:

- **Dashboard**: one dominant environment truth, Project WOW Environment Brief, Course Indicator, route/coverage, prioritized findings, and current activity.
- **Plan**: current reviewed authority-ordered update plan, scope, consequences, apply, verification, and recovery.
- **Library**: installed/upgradable/available software catalog, consolidated search, filters, comparison, and actions across apps, tools, and packages.
- **Activity**: active workflows/tasks, approvals, failures, interruptions, history, verification, Action Receipts, diagnostics.
- **Environment**: contextual manager/source coverage, install instances, provenance, eligibility, dependencies, policy, and setup/repair.

Strengths:

- Names user intent rather than exposing manager taxonomy at the workspace level.
- Removes Settings and source administration from peer operational navigation without hiding either.
- Gives failures, verification, receipts, and recovery one continuous Activity home.
- Keeps Plan separate from Library because reviewing an authority-ordered multi-manager plan is a distinct safety workflow.
- Makes Helm's System -> Toolchains -> Applications -> Packages environment route visible without requiring manager knowledge.

Costs:

- Environment is less prominent than a peer destination and therefore requires a persistent labeled sidebar entry, deep links, and strong contextual routing.
- `Library` requires explicit content to make its applications/toolchains/package breadth clear.
- Search results need an explicit origin/return model because Search is not a destination.

## Decision

Approve **Original Wayfinder: four job-first workspaces plus contextual Environment**.

This supersedes the earlier v0.18 five-destination planning decision:

- Remove Settings from the sidebar and popover overlay.
- Redefine Overview/Health as Dashboard state, including the Course Indicator, Environment Brief, route, and coverage.
- Redefine Updates as Plan because the reviewed plan is a cross-manager safety object, not a package filter.
- Redefine Packages as Library because the workspace includes applications and toolchains as well as language packages.
- Retain Activity for workflow, verification, receipts, interruption, and history.
- Move Sources/Managers into contextual Environment, emphasizing provenance, authority, policy, and install instances.
- Keep Search as a command rather than a sixth destination.
- Place diagnostics contextually, not as a top-level dumping ground.

## Dashboard Hierarchy

Expanded width:

```text
Window frame / native toolbar
  [Sidebar] [Title]                  [Search] [Refresh] [Detail] [Primary]

Navigation source list | Content or contextual list/table    | Optional detail
Dashboard              | environment brief and findings      | selected finding
Plan                   | authority plan outline              | selected plan step
Library                | software table/search               | selected software
Activity               | active/history outline              | task/workflow/receipt
Environment (footer)   | manager/source coverage             | selected source/instance
```

Minimum width:

```text
Toolbar: [Sidebar] [Title] [Search icon] [More]
Sidebar or content; contextual detail hidden
Selected detail opens as a replacement detail or sheet only when the task is bounded
```

Pane rules:

- Sidebar is visible by default, user-hideable, and restorable through View > Show/Hide Sidebar.
- Contextual detail is absent on Dashboard until selection requires it and is user-hideable where Plan, Library, Activity, or Environment reveal it.
- Selected content remains selected when contextual detail hides or snapshot data refreshes.
- Sidebar and contextual-detail widths restore within safe bounds.
- A content list/table owns selection. Buttons inside a row do not become an alternate selection model.
- No more than two navigation levels appear in the sidebar. Deeper detail belongs in content and contextual detail.

## Destination Contracts

### Dashboard

Content order:

1. Course Indicator, dominant environment truth, freshness, and one primary action.
2. System -> Toolchains -> Applications -> Packages route and coverage.
3. Required approvals, failures, and actionable findings under `Needs you`.
4. One current Activity summary.
5. Healthy state that removes zero-value exception content rather than displaying empty metric cards.

Inspector:

- Finding evidence, controlling manager/authority, consequence, recommendation, and recovery.
- `Open in Activity`, `Open Environment`, or `Review Plan` deep links retain finding selection.

Course Indicator:

- Consumes one shared revisioned state projection used by Dashboard and popover.
- Represents the highest-priority environment condition, not a synthetic aggregate score.
- Uses determinate progress only when backend-owned completed/total values are trustworthy.
- Exposes mode, textual explanation, freshness, primary action, and optional authority stage accessibly without relying on ring color or motion.
- Provides static Reduce Motion behavior and legible Increased Contrast/Reduce Transparency variants.

### Plan

Content:

- Reviewed plan outline grouped by authority phase.
- Scope/filter controls in toolbar or table header.
- Columns for item, source, current/target, status, consequence.
- Explicit empty, stale, offline-deferred, blocked, cancelled, failed-verification, and recovery states.

Inspector:

- Why included/excluded, pin policy, privilege/network/restart needs, verification, rollback limits.

### Library

Content:

- Table/list of consolidated software identities across applications, toolchains, and packages.
- Filters: status and source; saved density is optional later.
- Local search response appears before remote enrichment.
- Search scope labels identify local, cached, and remote results.

Inspector:

- Manager member/version selection, provenance summary, software action, pinning, history, and description.

### Activity

Content groups:

- Needs Attention: approvals, failure, failed verification, interrupted sessions.
- Active: queued/running/verifying/cancelling.
- History: verified, completed-with-limits, cancelled, deferred.
- Receipts filter: Action Receipts and setup-session summaries.

Inspector/detail:

- Workflow hierarchy first, raw task/log detail second.
- Before/after, verification, diagnostics, retry/resume, and honest rollback limits.

### Environment

Content:

- Contextual source/manager table grouped or sortable by authority.
- Columns for source, status, enabled policy, active provenance, packages/updates.
- Multi-instance attention and setup-required states are visible without opening inspector.

Inspector:

- Install instances and active choice, evidence/confidence, eligibility, dependency, install/update/uninstall/repair actions.
- Long logs route to Activity diagnostics rather than expanding indefinitely.

## Search Model

- Command-F focuses toolbar search from any destination.
- Empty search does not navigate.
- Typing shows immediate local results without destroying current destination/selection.
- If the query clearly targets software, accepting a result opens Library and selects the software identity.
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
| Menu bar condition | `Dashboard/finding-id` | Open/focus Dashboard, select finding, show contextual detail if space permits. |
| Popover active task | `Activity/workflow-id/task-id` | Open Dashboard, preserve active group, and reveal task without losing live progress. |
| Timeout notification | `Activity/task-id/approval` | Activate app, select approval, focus Wait/Stop group; notification action may respond directly through core. |
| Completion notification | `Activity/workflow-id/receipt` | Open verified result or recovery state, not generic Activity root. |
| Update notification | `Plan/plan-id` | Show current revalidated plan; stale plan state cannot execute silently. |
| Source context action | `Environment/manager-id/instance-id` | Open Environment and select source/instance detail. |
| Software context action | `Library/consolidated-id/member-id` | Preserve manager member choice and Library filter origin. |
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
| File | Open Helm/Dashboard; Close Window `Command-W`; Export/Copy Receipt when applicable |
| Edit | Undo/Redo where meaningful; Cut/Copy/Paste/Select All; Find `Command-F` |
| View | Dashboard/Plan/Library/Activity destinations; Open Environment; Show/Hide Sidebar; Show/Hide contextual detail; Refresh `Command-R`; density if approved |
| Activity | Review Plan; Run Approved Plan; Cancel/Stop; Retry; Resume; View Receipt, context validated |
| Window | Minimize; Zoom; Bring All to Front; Dashboard and Settings entries |
| Help | Helm Help; Search Help; Diagnostics/Troubleshooting; optional contextual tour; Report a Problem |

Shortcut candidates must be validated for conflicts. Do not assign Command-1 through Command-5 until the real app menu and text-editing behavior are tested on the minimum OS.

## Intentional Departures from macOS Conventions

Approved departures are limited:

| Departure | Helm-specific benefit | Required guardrail |
|---|---|---|
| Menu-bar-first accessory activation instead of Dock-first app | Ambient package/update monitoring without Dock clutter | Standard application menu whenever active; normal Dashboard window behavior; no hidden-only commands. |
| Domain-specific status and Action Receipt views | Native controls do not express authority, verification, and rollback truth as one model | All interaction and appearance budgets apply; use native containers, text, selection, and commands around custom content. |
| Authority-stage update outline | Helm must show deterministic execution ordering | Backend order is authoritative; UI cannot reorder execution policy. |

Custom cards, nonstandard pointers, fixed windows, empty Settings scenes, and missing menu commands are not approved departures.

## Validation Gate

The IA is design-approved for implementation planning, but human validation remains required:

- Owner-run moderated checkpoint in `NATIVE_MACOS_RESEARCH_VALIDATION.md`.
- Compare Option A labels and findability with Option B during first-click tasks.
- If fewer than 8 of 10 participants find Updates, package install, failure recovery, source provenance, and Settings without assistance, reopen labels/placement before v0.20 UI lock.
