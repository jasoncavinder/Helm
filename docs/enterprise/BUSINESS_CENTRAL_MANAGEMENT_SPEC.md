# Helm Business Central Management Specification

This document defines the business-tier central management model for keeping developer environments consistent across enterprise scopes.

## 1. Goals

- Enforce reproducible package and toolchain baselines across teams.
- Support department and environment-specific policy differences without manual scripting.
- Detect and report drift with actionable remediation state.
- Preserve Helm safety guarantees and architecture boundaries.

## 2. Core Entities

1. Tenant
- Top-level organization boundary.

2. Scope
- Hierarchical targeting units:
  - organization
  - department
  - team
  - environment
  - device group

3. Baseline Profile
- Declares desired package/toolchain state for a scope.

4. Policy Bundle
- Declarative rules governing what is allowed, required, pinned, and blocked.

5. Ring
- Progressive rollout stage (canary, pilot, broad).

6. Compliance Record
- Device-level evaluation result and drift details.

## 3. Policy Model

Policy bundle fields (minimum):

- `policy_id`
- `policy_version`
- `scope_bindings`
- `required_packages`
- `allowed_packages`
- `denied_packages`
- `pin_rules`
- `upgrade_windows`
- `guarded_action_rules`
- `auto_remediation_mode`

Policy semantics:

- Required package missing => drift.
- Denied package installed => drift, optionally block upgrades until resolved.
- Pin rules override default floating upgrades.
- Guarded actions (for example OS updates) still require explicit confirmations per safety policy.

## 4. Scope Resolution and Precedence

Evaluation order:

1. Organization default
2. Department
3. Team
4. Environment
5. Device-group override

Conflict rule:

- More specific scope wins.
- Deny rules win over allow rules at equal specificity.
- Pin rules are explicit and must be traceable to scope origin.

## 5. Drift and Compliance

Drift categories:

- missing_required
- present_denied
- version_out_of_range
- pin_mismatch
- unmanaged_toolchain

Compliance states:

- compliant
- warning
- non_compliant
- unknown

Each compliance record includes:

- evaluated policy version
- timestamp
- device identifier
- manager/action attribution for each violation

## 6. Remediation Modes

- `observe`: report drift, no blocking
- `assist`: suggest remediations, user confirms
- `enforce`: block disallowed actions and optionally auto-remediate in allowed windows

Enforcement never bypasses process-level cancellation and safety guardrails.

## 7. Rollout Rings

Baseline/policy updates roll out by ring:

1. Canary
2. Pilot
3. Broad

Required controls:

- explicit promotion between rings
- pause/rollback support
- per-ring success/failure metrics

## 8. Event and Audit Schema (Minimum)

Event types:

- policy_applied
- policy_rejected
- compliance_evaluated
- drift_detected
- remediation_started
- remediation_completed
- remediation_failed

Required event fields:

- tenant_id
- scope_id
- device_id
- manager
- task_id
- action
- policy_version
- outcome
- timestamp

## 9. Identity and Access

Business mode administration requires:

- SSO-backed identity for operators
- RBAC roles (owner, admin, auditor, operator, viewer)
- immutable audit trail for policy changes and approvals

## 10. MDM Integration Contract

Managed deployment should support:

- App install/update via MDM package distribution
- Managed configuration keys for enrollment and mode selection
- Optional scope hints from device groups

MDM remains authoritative for device posture and enrollment.
Helm is authoritative for package-manager policy execution.

## 11. Offline and Failure Behavior

- Last valid signed policy snapshot remains active offline.
- If no valid policy snapshot exists in enforced mode:
  - read-only operations are allowed
  - mutating operations are blocked with explicit error context
- Control-plane failures must degrade gracefully without UI hangs.

## 12. Delivery Phasing (1.x)

Business central management features are delivered across three post-1.0 milestones (see `docs/ROADMAP.md`):

1. **1.2.x** (Editions and Entitlement Foundations): entitlement scaffolding + managed bootstrap + policy snapshot store
2. **1.5.x** (Business Policy and Drift Management): scoped policy evaluation + drift detection + compliance reporting
3. **1.6.x** (Enterprise Rollout, Approvals, and Audit): rollout rings + approvals + audit export integrations

Note: Milestones 1.1.x (Globalization Expansion), 1.3.x (Security Advisory System, Pro tier), and 1.4.x (Shared Brain infrastructure) are interleaved between these but are not part of the business central management scope.
