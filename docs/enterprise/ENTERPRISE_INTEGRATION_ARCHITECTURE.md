# Helm Enterprise Integration Architecture

This document defines how Helm integrates into enterprise macOS environments without violating Helm's core architecture constraints.

## 1. Scope

This architecture is for Helm 1.0 and beyond.

- Helm 1.0 focuses on local deterministic orchestration and safe execution.
- Helm 1.x adds centrally managed enterprise operation through **Helm Business** (fleet product).

Helm does not replace MDM. It complements MDM by governing developer and package-manager software workflows.

## 2. Design Principles

- Keep existing layer boundaries intact:
  - UI (SwiftUI): presentation only.
  - Service boundary (XPC/macOS service): process execution boundary.
  - Core (Rust): orchestration, adapters, persistence, policy enforcement.
- Keep management policy separate from execution:
  - Central services define desired state.
  - Local Helm agent enforces desired state safely.
- Preserve local safety when disconnected:
  - Last known valid policy remains enforceable.
  - Risky operations stay guarded.

## 3. Reference Architecture

```text
Apple Business Manager + ADE
  -> MDM (Jamf/Intune/Kandji/Mosyle/etc.)
  -> Deploy signed Helm Business fleet PKG + managed app configuration
  -> Helm endpoint:
       - SwiftUI app (operator visibility)
       - Background service (execution boundary)
       - Rust core (adapters, orchestration, policy evaluation)
       - SQLite state store (cache, policy snapshot, task history)

Helm Business Control Plane (post-1.0)
  -> Tenant and RBAC administration
  -> Policy authoring and versioning
  -> Scope targeting (org/department/team/device group)
  -> Compliance and drift status
  -> Audit/event export integrations (SIEM/webhooks)
```

## 4. Trust Boundaries

1. Device management boundary (ABM/ADE/MDM)
- Device ownership, enrollment, baseline compliance.
- Helm installation and bootstrap config delivery.

2. Helm execution boundary (local service + core)
- Structured process invocation only.
- Per-manager serialization, cross-manager parallelism.
- Cancellation and privilege guardrails.

3. Helm policy boundary (business control plane)
- Signed policy bundles and entitlement assertions.
- Scoped targeting and drift/compliance evaluation.

## 5. Enrollment and Bootstrap Flow

1. IT deploys Helm Business fleet PKG through MDM.
2. MDM applies managed configuration values (for business mode):
- tenant identifier
- enrollment endpoint
- enrollment token or client credential reference
- bootstrap policy channel
- mode (`unmanaged`, `managed-readonly`, `managed-enforced`)
3. Helm validates enrollment material, including offline organizational licensing artifacts where applicable.
4. Helm fetches effective policy.
5. Helm stores a signed policy snapshot in SQLite.
6. Helm starts policy-aware orchestration.

## 6. Managed Operating Modes

- `unmanaged`:
  - local behavior (free/pro pattern)
  - no central policy enforcement
- `managed-readonly`:
  - policy visible, drift reported
  - user actions are not hard-blocked
- `managed-enforced`:
  - policy evaluated before mutating tasks
  - blocked actions produce explicit, attributed errors

## 7. Policy Enforcement Points

Policy must be evaluated in Rust core before:

- install
- uninstall
- upgrade
- upgrade-all
- pin / unpin changes

Policy cannot bypass existing safety requirements:

- guarded/system actions still require explicit confirmations
- pinned packages remain excluded from bulk upgrades unless explicit override policy is present

## 8. Drift and Compliance Model

Drift is computed locally against the effective scoped baseline:

- required package missing
- forbidden package present
- version outside allowed range
- pin state mismatch

Compliance states:

- compliant
- warning
- non_compliant
- unknown (disconnected/no valid policy)

## 9. Failure and Offline Behavior

- Policy fetch failure:
  - use last known valid signed policy snapshot
  - keep execution deterministic
- Offline license artifact unavailable/invalid:
  - fail closed for fleet-only entitlements
  - keep read-only and core-safe behavior available
- No valid policy snapshot in enforced mode:
  - allow read-only actions
  - block mutating actions with explicit error attribution
- Control-plane unavailability must not crash UI or executor.

## 10. Security and Audit Requirements

- Entitlements and policies are cryptographically verifiable.
- Task logs remain structured: manager, task, action, outcome.
- Business mode adds tenant and policy version context to audit records.
- No command-string shell concatenation at any layer.

## 11. Integration Boundaries

Helm business integration points (1.x):

- MDM profile and bootstrap config consumption
- PKG-based fleet deployment lifecycle
- Offline organizational license-file handling
- SSO/RBAC identity mapping for administrators
- Event export to SIEM/ticketing/webhook endpoints

Out of scope for 1.0:

- cloud policy synchronization as a release gate
- remote command execution outside Helm task model
