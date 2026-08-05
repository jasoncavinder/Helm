# Fleet Managed-Configuration Boundary

This document defines the minimal, vendor-neutral contract required to ensure Helm can coexist with enterprise MDM and Fleet distribution systems in the future.

## 1. Core Principle
**External MDM and software-distribution authority remains controlling.** Helm does not attempt to fight, override, or usurp policies deployed via Jamf, Intune, Kandji, Munki, or Apple Profiles.

## 2. Managed Values and Precedence
- `authority`: Indicates the source of the configuration (`mdm_profile`, `local_override`, `fleet_policy`).
- **Most-Restrictive-Wins**: If local user preferences conflict with MDM policies (e.g., user wants to allow beta updates, MDM forbids it), the most restrictive policy always wins.

## 3. Enforcement Mode
- `read_only`: Helm displays the managed state but allows the user to view alternatives (though they cannot apply them).
- `required`: Helm strictly enforces the policy. **Consumer questions are skipped because policy supplied an answer.** For example, if MDM mandates Homebrew installation, the first-run setup skips asking the user for Homebrew consent.

## 4. Immutable Safety Minimums
Helm maintains strict safety minimums that cannot be downgraded by policy:
- Cryptographic signature validation for downloaded binaries.
- Apple Notarization checks.
- Disallowing arbitrary string-concatenated shell execution.

## 5. Offline Behavior and Expiry
- Managed configurations are cached locally in the SQLite store.
- If the device is offline, Helm operates using the last known valid policy.
- Expiry mechanisms (if defined by the MDM profile) will gracefully downgrade to restrictive defaults if a required heartbeat is missed.

## 6. Auditability
- All actions executed under a managed configuration include the `authority` source in their `Action Receipt`.
- The `Redacted Summary` includes an aggregate of managed enforcement events.

## 7. Exclusions
- There are **no bespoke APIs** (e.g., direct Kandji/Jamf integrations) in this closure.
- There is **no Fleet runtime implementation** in `v0.18.x`. This boundary solely defines the data contract for future `0.19.x+` integration.
