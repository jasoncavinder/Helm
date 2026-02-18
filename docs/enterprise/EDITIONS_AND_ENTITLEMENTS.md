# Helm Editions & Entitlements

This document defines Helm’s product editions, feature boundaries, and runtime entitlement model.

It is the canonical source of truth for:

- Feature gating
- Edition behavior
- Beta access rules
- Enforcement guarantees

---

## 1. Principles

Helm is a **local control plane** for system and package management.

All editions must preserve the following invariants:

- Local-first execution
- Deterministic behavior
- Explicit user authority
- No hidden automation
- Transparent operations

No edition may violate these principles.

---

## 2. Edition Overview

Helm provides three editions:

| Edition | Purpose |
|--------|--------|
| **Free** | Core execution layer |
| **Pro** | Automation, intelligence, and advanced workflows |
| **Business** | Governance, policy, and compliance |

---

## 3. Free Edition — Execution Layer

The Free edition must remain **fully usable** and **non-degraded**.

It includes all core functionality required to manage a system.

### Included Capabilities

- Manager detection and enable/disable
- Package listing (installed, outdated, available)
- Install / uninstall / upgrade
- Upgrade-all (authority-ordered)
- Search (local + remote enrichment)
- Task system (queue, progress, cancellation)
- Pinning (native + virtual)
- Upgrade preview and dry-run
- Safe mode guardrails
- Diagnostics and logs
- Localization

### Guarantees

- No feature required for normal operation is paywalled
- Free must remain stable even if entitlement fails
- No remote service is required

---

## 4. Pro Edition — Automation & Intelligence Layer

Pro enhances Helm for individual power users.

It adds:

- Automation
- Safety intelligence
- Advanced workflows
- Enhanced visibility

Pro does not introduce cloud dependencies.

---

### 4.1 Automation

- Scheduled refresh (daily/weekly/monthly)
- Scheduled upgrade-all
- Quiet hours
- Conditional execution (network, battery, etc.)
- Deferred execution windows

---

### 4.2 Security Advisory System (CVE Awareness)

Helm Pro includes a **local security advisory system**.

Capabilities:

- Detect vulnerabilities affecting installed packages
- Surface severity (low, medium, high, critical)
- Show affected version ranges
- Identify fixed versions where available

Recommendations (advisory only):

- Upgrade to safe version
- Pin to known-safe version
- Avoid upgrade (risk of regression)
- Remove package

### Constraints

- Advisory only — Helm does not enforce decisions
- Local-first evaluation (no required cloud)
- Deterministic results based on local data

---

### 4.3 Advanced Safety Controls

- Upgrade policies (major/minor restrictions)
- Safe mode profiles
- Risk indicators (breaking changes, major version jumps)
- Enhanced upgrade preview

---

### 4.4 Advanced Visibility

- Historical logs
- Upgrade history timeline
- Failure analytics
- Manager health insights

---

### 4.5 Power UX

- Advanced filtering (e.g., only minor updates)
- Saved views
- Bulk operations
- Enhanced search

---

## 5. Business Edition — Governance Layer

Business edition extends Helm for organizational environments.

It adds:

- Policy enforcement
- Compliance visibility
- Fleet management (future)

---

### 5.1 Policy Enforcement

- Allowed / blocked packages
- Version constraints
- Mandatory updates
- Safe mode enforcement

---

### 5.2 Drift Detection

- Compare actual vs desired state
- Detect non-compliant systems
- Local evaluation of compliance

---

### 5.3 Audit & Reporting

- Event logs
- Exportable reports
- Compliance summaries

---

### 5.4 Managed Configuration

- MDM-based deployment
- Managed settings
- Locked configuration

---

### 5.5 Future Scope (Post-1.0)

- Central policy service
- Rollout strategies
- Approval workflows

---

## 6. Entitlement Model

Helm uses a runtime entitlement system.

---

### 6.1 Edition Enum

```rust
enum Edition {
    Free,
    Pro,
    Business,
}
````

---

### 6.2 Feature Gating

Each feature is mapped to an edition:

```rust
enum Feature {
    Scheduling,
    SecurityAdvisories,
    AdvancedPolicies,
    History,
    AuditLogs,
}
```

```rust
fn is_enabled(feature: Feature, edition: Edition) -> bool
```

---

### 6.3 Enforcement Rules

* Free features must always be available
* Entitlement failure must not break core functionality
* Gating must be deterministic and local

---

## 7. Beta Entitlement Mode

During pre-1.0:

All features are enabled for testing.

---

### 7.1 Behavior

* Pro features are enabled
* Business features may be partially enabled
* No feature is blocked

---

### 7.2 UI Requirements

All non-Free features must be labeled:

* **"Pro (Beta)"**
* **"Business (Beta)"**

Tooltip:

> "Included during beta. Will require a paid license after 1.0."

---

### 7.3 Implementation

```rust
enum EntitlementMode {
    BetaAllUnlocked,
    Licensed,
}
```

---

## 8. Post-1.0 Behavior

After 1.0:

* Feature access is enforced by license
* Free remains fully usable
* Pro requires individual license
* Business requires organizational license

---

## 9. Failure Modes

Entitlement system must fail safely:

* Default to Free capabilities
* Never block critical operations
* Never crash UI

---

## 10. Non-Goals

The entitlement system must not:

* Require constant network access
* Degrade core functionality
* Introduce hidden behavior

---

## 11. Summary

Helm editions are structured as:

* **Free** — Execution
* **Pro** — Intelligence + Automation
* **Business** — Governance

All editions respect Helm’s core principles:

* Local-first
* Deterministic
* Transparent
