# Helm UI Redesign Concept

## Design Philosophy: Quiet Flight Deck

Helm should feel like a flight deck for local system hygiene:
- Quiet when all systems are healthy.
- Immediate and explicit when action is needed.
- Procedural for risky operations.

The UI is not a dashboard to stare at all day. It is a fast control surface opened for brief, high-confidence interventions.

## User Mental Model

What users believe Helm is:
- A trusted operations companion in the menu bar.
- A single place to understand package and toolchain risk.
- A safe orchestrator that respects manager authority and system guardrails.

When users open Helm:
- A menu bar pulse indicates pending work or failures.
- They want to answer one question quickly: "Is my environment healthy enough right now?"

Primary tasks:
- Check overall health and pending updates.
- Run upgrade all safely.
- Resolve one failing manager or package.

Secondary tasks:
- Search and install specific packages.
- Review history and task logs.
- Configure cadence/policy.

## Core UX Principles

1. Status first
Helm opens to health posture and required actions before detail exploration.

2. Progressive disclosure
Popover handles fast triage. Dedicated window handles planning, search, and diagnostics.

3. Safe by construction
Risky operations (OS updates, large mutating plans) always show explicit impact summaries before execution.

4. Deterministic feedback
Every action maps to task states: queued, running, success, partial failure, failure, canceled.

5. Local-first responsiveness
Cached status appears instantly, then refreshes incrementally as managers report.

6. Keyboard-complete operations
All core workflows are reachable without pointer-only interaction.

## Interaction Model Summary

Menu bar popover:
- Instant health summary.
- Fast actions: Refresh, Upgrade All, Open Control Center.
- Short live task strip.

Control Center window:
- Sidebar navigation by domain: Overview, Packages, Updates, Tasks, Managers, Settings.
- Main pane optimized for one workflow at a time.
- Right contextual panel for selected item details and actions.

## Key Tradeoffs and Decisions

Decision: keep popover intentionally narrow in scope.
- Benefit: low cognitive load and very fast checks.
- Cost: deeper operations require opening the window.

Decision: separate "Updates" from "Packages".
- Benefit: high-frequency maintenance flow is optimized.
- Cost: some entity duplication across sections.

Decision: show partial failures as first-class outcome.
- Benefit: truthful state and easier recovery.
- Cost: more nuanced status language and visuals.

Decision: explicit "Execution Plan" before bulk actions.
- Benefit: trust and predictability.
- Cost: one extra step before action.

## Comparison to Typical System Tools

Like Activity Monitor:
- Clear hierarchy and live status surfaces.

Like Software Update:
- Explicit confirmation and reboot awareness for system-level changes.

Unlike generic dashboards:
- No dense chart wall.
- No "always open" workflow assumption.
- Focus on short, operational loops.
