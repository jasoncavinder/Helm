# Helm User Flows

## Flow 1: Checking System Status

Goal:
- Determine if action is required in under 5 seconds.

Steps:
1. User sees menu bar icon badge/state.
2. User opens popover.
3. Popover shows cached health immediately.
4. Background refresh begins automatically.
5. UI updates manager cards incrementally.

State behavior:
- Loading: skeleton rows + "Refreshing managers..."
- Success: healthy/outdated counts with last updated time
- Partial failure: warning banner with affected managers count
- Failure: error card with "Retry Refresh"

## Flow 2: Updating All Packages

Goal:
- Execute a safe, authority-ordered bulk update.

Steps:
1. User clicks "Upgrade All" from popover or Updates section.
2. Helm builds execution plan grouped by authority tiers.
3. Confirmation sheet shows:
- manager counts
- package counts
- privileged/reboot warnings
4. User chooses Run Now or Cancel.
5. Task list shows each stage progressing.

State behavior:
- Loading: plan generation spinner with deterministic ordering note
- Success: summary toast + updated health posture
- Partial failure: completed + failed groups split with retry options
- Failure: blocking error with manager attribution

## Flow 3: Updating a Single Tool

Goal:
- Update one package quickly without context switching.

Steps:
1. User searches package in global search.
2. User selects package result.
3. Detail panel shows version delta, manager, pin state, and actions.
4. User clicks "Update".
5. Inline task chip appears on package row and Tasks view.

State behavior:
- Loading: row-level progress and disabled duplicate action
- Success: version label updates inline
- Partial failure: if pre-check passes but execution fails, row shows warning + retry
- Failure: inline error with "View Details"

## Flow 4: Handling Errors

Goal:
- Move from failure to next action with minimal ambiguity.

Steps:
1. Task enters failed state.
2. Failure center card appears in Overview and Tasks.
3. User opens detail:
- manager
- action
- command intent (not raw shell)
- recommended remediation
4. User retries, skips, or opens manager diagnostics.

State behavior:
- Loading: remediation checks if needed
- Success: incident auto-resolves and is archived
- Partial failure: retry succeeds for subset; unresolved items remain visible
- Failure: persistent incident with exportable diagnostics

## Flow 5: First Launch Experience

Goal:
- Reach trustworthy baseline with minimal setup burden.

Steps:
1. Welcome screen explains Helm scope and safety principles.
2. Detection step scans managers and capabilities.
3. User reviews detected managers and enables/disables optional ones.
4. Policy step sets update cadence and auto-apply default (off).
5. Finish routes to Overview with first refresh running.
6. Guided walkthrough begins — SpotlightOverlay highlights key UI elements across the popover (6 steps) and control center (7 steps). Users can skip at any point.

State behavior:
- Loading: manager detection progress by source
- Success: setup complete with immediate baseline status
- Partial failure: unsupported/missing managers shown as non-blocking
- Failure: setup retry path without data loss
