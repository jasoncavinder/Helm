# Fleet Managed-Configuration Boundary

This document defines the minimal, vendor-neutral contract required to ensure Helm can coexist with enterprise MDM and Fleet distribution systems in the future.

## 1. Core Principle
**External MDM and software-distribution authority remains controlling.** Helm does not attempt to fight, override, or usurp policies deployed via Jamf, Intune, Kandji, Munki, or Apple Profiles.

## 2. Managed Values and Precedence
- `authority`: identifies Apple/MDM, signed Fleet policy, distribution assignment, or local-user input.
- Effective precedence is Apple/MDM enforcement, locally verified Fleet policy, distribution assignment, Helm core safety minimums, Helm defaults, then user preferences where permitted.
- **Most-Restrictive-Wins**: a lower authority cannot widen network, privilege, mutation, source, or retention permissions denied by a higher authority or core safety.
- Policy identity, revision, issue/expiry time, and local verification evidence support deterministic offline reconciliation. Payload claims do not self-assign trust.

## 3. Enforcement Mode
- `advisory`: Helm may recommend the managed preference but the user retains control where higher authority permits.
- `read_only`: Helm displays managed state and alternatives but cannot apply a conflicting local choice.
- `required`: consumer preference questions already answered by verified policy may be skipped. This does not bypass macOS authorization, Helm preflight/revalidation, typed-action admission, verification, or recovery truth. A required but unsupported action remains blocked rather than becoming executable.

## 4. Immutable Safety Minimums
Helm maintains strict safety minimums that cannot be downgraded by policy:
- Finite typed actions only; no arbitrary commands, arguments, scripts, or payload-selected executables.
- Current state and policy revalidation immediately before apply.
- Post-verification before a result is labeled Verified.
- Rollback labels only for tested inverses with current ownership/precondition checks.
- Approved-host, integrity, size/time, and platform trust checks for any future downloaded installer workflow.

## 5. Offline Behavior and Expiry
- A future Fleet implementation may cache only locally verified managed configuration in the SQLite store.
- If the device is offline, Helm may use the last valid unexpired policy and must surface its age and authority.
- Expired, malformed, unverifiable, or downgraded policy fails closed to core safety and restrictive defaults; no mutation resumes automatically.

## 6. Auditability
- All actions executed under a managed configuration include the effective `authority` source in their per-action Action Receipt result.
- The `Redacted Summary` includes an aggregate of managed enforcement events.

## 7. Exclusions
- There are **no bespoke APIs** (e.g., direct Kandji/Jamf integrations) in this closure.
- There is **no Fleet runtime implementation** in `v0.18.x`. This boundary solely defines the data contract for future `0.19.x+` integration.
