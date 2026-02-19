# Helm Information Architecture

## IA Goal

Organize Helm around operational decisions, not manager internals. Users start with risk posture, then drill into the specific manager/package/task causing that posture.

## Screen Hierarchy

1. Menu Bar Layer
- Menu bar icon status
- Status popover (triage)

2. Control Center Window
- Sidebar domains
- Main content pane
- Context/detail inspector

3. Modal/Sheet Layer
- Upgrade plan confirmation
- Error detail and remediation
- First-launch onboarding

## Navigation Model

Top-level domains in sidebar:
- Overview
- Updates
- Packages
- Tasks
- Managers
- Settings

Global controls:
- Search field (packages + managers)
- Refresh
- Upgrade All

Context navigation:
- Selecting a manager filters packages/tasks to that manager.
- Selecting a package opens package detail with available capabilities.

## Top-Level vs Secondary Placement

Top-level:
- Overview: health posture and action queue summary.
- Updates: pending upgrades and bulk-plan builder.
- Packages: discovery/install/uninstall actions.
- Tasks: live execution and outcomes.
- Managers: capability and health by source.

Secondary (within detail panes or settings):
- Pin policy details
- Retry backoff controls
- Localization and accessibility preferences
- Diagnostics export

## Key Entities

Manager:
- id
- display name
- authority level
- capabilities
- health state

Package:
- identifier
- manager id
- installed version
- latest version
- pin state
- search metadata (source query, timestamp)

UpdatePlan:
- selected packages grouped by manager
- authority-ordered execution stages
- risk flags (privileged/reboot)

Task:
- id
- type
- manager
- target package (optional)
- lifecycle state
- timestamps
- error payload

Incident:
- task failure or manager degradation snapshot
- suggested remediation action

## Relationships

- One manager has many packages.
- One update plan has many staged operations.
- One task belongs to one manager and may target one package.
- One incident links to one or more failed tasks.

## Primary Workflows Mapped to IA

Check health:
- Menu bar icon -> popover summary -> Overview

Upgrade all:
- Popover/Updates -> execution plan -> task monitor

Single package update:
- Search or Packages -> package detail -> update action -> task row

Error recovery:
- Tasks failure state -> incident detail -> retry/open manager guidance

First launch:
- Onboarding modal -> manager detection -> policy defaults -> Overview

After onboarding wizard (Welcome -> Detection -> Configure), a guided walkthrough highlights key UI elements across the popover (6 steps) and control center (7 steps).
