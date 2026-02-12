# Helm Versioning Strategy

Helm follows Semantic Versioning 2.0.0:

MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]

Example:
0.3.0-alpha.1
0.5.0-beta.2
0.8.0-rc.1
1.0.0

---

## Pre-1.0 Rules (0.x.y)

Before 1.0, Helm is considered unstable and may introduce breaking changes.

Version meaning:

- MINOR (0.X.0)
  Represents a capability milestone or architectural layer.
  Minor bumps may include breaking changes.

- PATCH (0.X.Y)
  Bug fixes, small improvements, or incremental changes within a milestone.

- PRERELEASE TAGS
  - alpha: early milestone work; incomplete; unstable
  - beta: feature-complete for milestone; may contain defects
  - rc: release candidate; expected to be production-ready unless defects are found

Examples:
0.2.0-alpha.1 → first working adapter
0.3.0-beta.1 → orchestration engine complete
0.8.0-rc.1 → near-1.0 stability

Pre-1.0 milestones beyond 0.10 may be used to represent capability layers.
Version numbers are not constrained to single digits.

---

## 1.0.0 Meaning

1.0.0 represents:

- Stable core architecture
- Stable manager adapter interface
- Stable task orchestration model
- Functional UI
- Production-safe execution semantics
- Documented limitations

Breaking architectural changes after 1.0 require a MAJOR version bump.

---

## Version Bump Rules

- Breaking change (post-1.0): MAJOR
- New feature: MINOR
- Bug fix or internal improvement: PATCH

Pre-1.0:
- Capability milestone → bump MINOR
- Iteration within milestone → bump PATCH

---

## Tagging Rules

- Only tag from `main`.
- Do not tag from `dev`.
- Format: vX.Y.Z[-tag]

Examples:
v0.3.0-beta.1
v0.4.0
v1.0.0

---

## Release Flow

1. Complete milestone work on `dev`.
2. Update version in:
   - Cargo.toml
   - Swift bundle version
3. Merge `dev` → `main`.
4. Create annotated tag.
5. Push tag to GitHub.
6. Generate release notes.

---

## Stability Promise (Post-1.0)

After 1.0:
- Public core APIs are stable.
- Adapter trait changes require MAJOR bump.
- Orchestration semantics remain backward compatible.
