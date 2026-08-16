# Navigator Experience Architecture

Status: approved Original Wayfinder implementation contract

## Surface contract

| Surface | Primary question | Persistent content | Explicit exclusions |
|---|---|---|---|
| Status item | Does Helm need attention? | Quiet template icon plus highest-priority accessible state | Counts as the only meaning, independent context menu IA |
| Popover | What is true now, and what should I do next? | Dominant state, freshness, one action, route orientation, active work, three commands | Manager lists, metric grids, full search results, deep settings, diagnostics |
| Dashboard | What is my environment state, and where do I work? | Dashboard, Plan, Library, Activity, contextual Environment | Durable preferences and promotional content |
| Settings | How should Helm behave over time? | General, schedules, update policy, source defaults, CLI, support preferences | Refresh, repair, plan execution, live health |
| Sheet/alert | What bounded decision is required? | Consequence, scope, default/cancel/destructive action | Browsing and independent navigation |

The popover and Dashboard are the only operational interfaces. Settings and bounded system presentations follow macOS conventions and do not duplicate operational state.

## Navigation model

```text
Status item
  └─ Popover
       ├─ primary condition action ───────────────┐
       ├─ Open Dashboard ────────────────────────┤
       ├─ Find software… ──────── Library search │
       └─ active work ─────────────── Activity   │
                                                  ▼
Dashboard ─ Dashboard / Plan / Library / Activity
  └─ Environment (contextual infrastructure)
```

Deep links preserve the originating condition, selected entity, and return route. Opening Dashboard does not dismiss or reset active work. Closing Dashboard does not cancel a workflow.

## Workspace responsibilities

### Dashboard

- current environment truth and freshness
- first-run Environment Brief and subsequent summary
- route/coverage through System, Toolchains, Applications, Packages
- approvals, failures, and findings that need the user
- one current activity summary
- one contextual primary action

### Plan

- authority-stage ordering and dependency consequences
- included, pinned, unsupported, and guarded exclusions
- network/download, authorization, restart, verification, and recovery expectations
- scoped review and final bounded confirmation
- stage-by-stage execution continuity into Activity

### Library

- consolidated software identity across apps, tools, and packages
- immediate local/cached results and disclosed remote enrichment
- recommendation and source-choice rationale
- install/update/pin/remove actions
- contextual source/provenance detail only when relevant

### Activity

- running/queued/approval states
- history, verification, Action Receipts, and interruptions
- command/output evidence and support diagnostics
- recovery, retry, resume, and cancellation

### Environment

- manager/source coverage and freshness
- install instances, provenance, authority, eligibility, and policy
- source setup, repair, and intentional multi-install understanding
- service/runtime condition when it affects coverage

## Adaptive detail

The Dashboard overview never opens with a permanent inspector. Detail behavior follows the task:

- wide Plan/Library/Activity windows may reveal a contextual trailing pane after selection
- minimum width replaces content with a detail route and preserves Back behavior
- bounded choices use sheets
- long diagnostics use Activity detail or a dedicated diagnostics window
- selection and return context survive refresh, window close/reopen, and deep links

## State priority

Only one state owns the hero and popover primary action:

1. approval required
2. failed/interrupted work
3. active work
4. actionable finding
5. updates ready
6. refreshing/cached/partial
7. healthy

Lower-priority state remains available in `Needs you`, Activity, or the environment route without competing for the primary action.

## Course Indicator

Original Wayfinder's circular Course Indicator renders the single state selected by the priority model above. Dashboard and popover use one shared revisioned projection containing:

- semantic mode: healthy, updates-ready, determinate-work, indeterminate-work, approval, failed/interrupted, or cached/partial/offline
- accessible title and explanation
- freshness and coverage context
- primary action/deep link
- optional backend-owned completed/total progress and current authority stage

The presentation layer may interpolate a supplied determinate value, but it must not manufacture a composite health score, infer progress from elapsed time, or combine unrelated manager counts into a percentage. Updates-ready uses a sea-glass teal segment and explicit count, not a completion arc or an attention treatment. Actionable nonfailure findings use the distinct amber/orange `Needs Review` state. Indeterminate activity uses a system-equivalent motion with a static reduced-motion alternative. Failure, approval, and partial-data modes change symbol and ring treatment as well as color.

## Terminology

- `Control Center` becomes `Dashboard` in product copy.
- `Updates` becomes `Plan` when referring to Helm's reviewed authority-ordered workflow; individual update availability remains an item state.
- `Packages` becomes `Library` at the workspace level because it includes applications and toolchains as well as language packages.
- `Sources` becomes contextual `Environment`; manager and source remain precise domain terms in detail.
- `Health` is a state communicated by Dashboard rather than a navigation destination.
- `Updates Ready` is the canonical compact label for actionable update inventory; `Needs Review` identifies actionable nonfailure findings.

These terminology changes are approved for incremental implementation and require canonical/mirrored locale parity as production strings are introduced.
