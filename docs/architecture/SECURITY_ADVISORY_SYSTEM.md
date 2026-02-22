# Security Advisory System (v1.3.x, Helm Pro)

This document defines Helm's local-first advisory model planned for `v1.3.x`.

It is explicitly separate from the `v1.4.x` Shared Brain system.

---

## 1. Scope

The Security Advisory System provides CVE/advisory awareness for installed packages and toolchains.

It is advisory-only and does not enforce actions automatically.

---

## 2. Version Boundaries

Stage 0 (`<=0.16.x`):
- Planning/documentation only

Stage 1 (`0.18.x`):
- Internal local groundwork only (no public feature)

Stage 2 (`1.3.x`):
- Security Advisory System (this document)

Stage 3 (`1.4.x`):
- Shared Brain (separate document/scope)

---

## 3. Design Principles

- Local-first evaluation
- Deterministic matching behavior
- Transparent recommendations
- Non-blocking execution
- Offline-safe behavior from cached advisory data

---

## 4. Data Sources

Supported source strategy:

- Local advisory cache (primary at runtime)
- Optional pull from public advisory APIs (for example OSV.dev, GitHub Advisory Database, NVD, manager-specific feeds)

No Helm-operated central advisory database is required for `1.3.x`.

---

## 5. Local Cache and TTL

Advisory records are cached in SQLite with freshness metadata:

- source
- query or manager origin
- fetched timestamp
- expiry timestamp (TTL)

Cache rules:

- Read from cache first
- Refresh on TTL expiry or explicit refresh request
- Use stale cache when offline (with stale marker in UI metadata)

---

## 6. Matching Engine

Match inputs:

- manager
- normalized package name
- installed version
- advisory affected range

Match outputs:

```rust
struct SecurityAdvisory {
    manager: String,
    package: String,
    affected_versions: String,
    severity: Severity,
    summary: String,
    fixed_version: Option<String>,
    source: String,
    fetched_at_epoch_ms: i64,
    expires_at_epoch_ms: i64,
}
```

---

## 7. Recommendations

For affected packages, Helm may recommend:

- update to fixed version
- pin temporarily
- avoid risky upgrade path
- remove package

These recommendations are operator guidance only.

---

## 8. Entitlement and UX

- Feature tier: Helm Pro (`1.3.x`)
- Free tier: no advisory UI surfaces
- Advisory evaluation must never block install/upgrade/uninstall/refresh tasks

---

## 9. Explicit Non-Goals for v1.3.x

The following are out of scope until `1.4.x` Shared Brain:

- fingerprint sharing
- known-fix crowdsourced lookup
- centralized Postgres backend
- App Attest-based install authentication
- request signing/nonce/replay protection for centralized APIs

---

## 10. Relationship to Shared Brain (`1.4.x`)

Security Advisory System is standalone and useful without Shared Brain.

Shared Brain is additive:

- augments local advisory context with shared signals
- requires new backend/auth/security infrastructure
- must not be a hard dependency for local advisory evaluation
