# Security Advisory System (CVE)

This document defines Helm’s vulnerability awareness system.

---

## 1. Purpose

Helm provides visibility into known vulnerabilities affecting installed packages.

It does not enforce actions.

---

## 2. Design Principles

- Advisory only
- Local-first evaluation
- Deterministic results
- Transparent output

---

## 3. Data Sources

Potential sources:

- OSV.dev
- NVD (National Vulnerability Database)
- Manager-specific feeds

Data may be aggregated.

---

## 4. Data Flow

```

CVE Sources → Local Cache → Matching Engine → UI

````

---

## 5. Matching Engine

Matches:

- Package name
- Version range

Produces:

```rust
struct SecurityAdvisory {
    package: String,
    affected_versions: String,
    severity: Severity,
    description: String,
    fixed_version: Option<String>,
}
````

---

## 6. Output

For each affected package:

* Vulnerability status
* Severity
* Recommended actions

---

## 7. Recommendations

Helm may suggest:

* Upgrade
* Pin
* Avoid upgrade
* Remove package

Helm does not enforce actions.

---

## 8. Storage

* Local database (SQLite)
* Cached advisory data
* Periodic refresh

---

## 9. Entitlement

This system is part of **Pro edition**.

---

## 10. Constraints

* Must work offline
* Must not block operations
* Must not execute external code

---

## 11. Future Enhancements

* Dependency graph analysis
* Risk scoring
* Historical tracking
