# Helm Editions and Entitlements

This document defines Helm distribution variants and runtime feature gating from 1.0 onward.

## 1. Build Variants

Helm ships as two build variants:

1. Debug build
- Intended for engineering and internal validation.
- Includes debug diagnostics and development toggles.
- Not supported for production deployment.

2. Release build
- Signed production artifact for public and enterprise distribution.
- Contains all code paths; feature access is controlled by entitlements.

Recommendation: keep a single release artifact and gate features via verified entitlements to reduce packaging drift and support burden.

## 2. Runtime Editions

The release build supports three runtime editions:

1. Free
- Core local package management workflows.
- Local-first search, refresh, install/uninstall/upgrade, baseline pinning.

2. Pro (subscription/pay-gated)
- Advanced individual/team workflows.
- Enhanced scheduling/policy controls for local operations.
- Expanded diagnostics and export features for power users.

3. Business (subscription/pay-gated)
- Central management for enterprise consistency.
- Scoped policy baselines, drift/compliance, rollout rings, and audit export.

## 3. Capability Matrix

| Capability | Debug | Free | Pro | Business |
|---|---|---|---|---|
| Core manager detection, refresh, install/uninstall/upgrade | Yes | Yes | Yes | Yes |
| Local-first progressive search | Yes | Yes | Yes | Yes |
| Local pinning and pin-aware bulk upgrades | Yes | Yes | Yes | Yes |
| Advanced local policy controls (single device/team) | Yes | No | Yes | Yes |
| Extended diagnostics/export UX | Yes | Limited | Yes | Yes |
| Central policy scopes (org/department/team/device groups) | Optional test | No | No | Yes |
| Central compliance/drift reporting | Optional test | No | No | Yes |
| Central rollout rings and approvals | Optional test | No | No | Yes |

Notes:

- "Optional test" means available only in developer/test environments, not licensed production use.
- Exact feature cuts can evolve by minor release, but Free/Pro/Business boundaries must remain explicit.

## 4. Entitlement Model

Entitlement claims should include:

- edition (`free`, `pro`, `business`)
- tenant context (business only)
- enabled feature flags
- issued-at and expiration timestamps
- signature metadata (key id, algorithm)

Validation rules:

- Verify signature before enabling gated features.
- Fail closed for business-only management actions when claims are invalid.
- Keep core read-only functionality available when entitlement checks fail.

## 5. Degradation Behavior

If entitlement becomes invalid/expired:

- Free capabilities continue.
- Pro capabilities downgrade to Free.
- Business central enforcement downgrades to managed read-only or local safe mode (policy dependent).
- All degradations are surfaced in UI and logs with clear cause.

## 6. Distribution and Licensing

- Distribution vehicle: signed release package/app deployable by users or MDM.
- Licensing: runtime activation and periodic entitlement refresh.
- No hidden "enterprise-only" binary fork.

This approach preserves deterministic behavior across all customers and limits support matrix complexity.

## 7. 1.0 Boundary

For 1.0 release readiness:

- Debug and release builds are defined and reproducible.
- Release build supports entitlement-aware gating scaffolding.
- Central business control plane capabilities may be delivered incrementally in 1.x.
