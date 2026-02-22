# Helm Versioning Strategy

Helm follows Semantic Versioning 2.0.0:

MAJOR.MINOR.PATCH[-PRERELEASE][+BUILD]

Example:
0.3.0-alpha.1
0.5.0-beta.2
0.8.0
0.7.0
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
0.8.0 → pinning and policy enforcement milestone complete
0.13.0-rc.1 → UI/UX redesign release candidate

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

- Stable app releases: tag from `main` after merging from `dev`.
- Pre-release tags (alpha, beta, rc): may be tagged from `dev` lineage when the release is not yet merged to `main`.
- Format: vX.Y.Z[-tag]

Examples:
v0.3.0-beta.1
v0.13.0-rc.1
v0.4.0
v1.0.0

---

## Branch Integration Model

- `dev` is the integration branch for app/runtime/core code.
- `docs` is the integration branch for documentation/policy/licensing docs.
- `web` is the integration branch for website implementation under `web/`.
- `main` remains the stable/releasable branch.

Promotion flow:

- code release content: `dev` -> `main`
- docs-only publication content: `docs` -> `main`
- website-only publication content: `web` -> `main`

Only app releases are version-tagged by default; standalone docs/website publications generally do not require a SemVer tag unless explicitly tied to a release cut.

---

## GitHub Guardrails

Helm uses GitHub rulesets and workflow checks to enforce branch policy.

Required checks by protected branch:

- `main`: `Policy Gate`, `Rust Core Tests`, `Xcode Build Check`, `hardcoded-ui-strings`, `Semgrep scan`, `Lint Swift`
- `dev`: `Policy Gate`, `Rust Core Tests`, `Xcode Build Check`, `hardcoded-ui-strings`, `Semgrep scan`, `Lint Swift`
- `docs`: `Policy Gate`, `Docs Checks`
- `web`: `Policy Gate`, `Web Build`

Operational settings:

- auto-merge enabled
- update-branch enabled
- auto-delete merged branches enabled

`Policy Gate` is the authoritative branch-target/scope guardrail for PRs.

---

## Release Flow

1. Complete milestone work on `dev`.
2. Update version in:
   - `core/rust/Cargo.toml` (workspace version)
   - Generated files: `apps/macos-ui/Generated/HelmVersion.swift` and `apps/macos-ui/Generated/HelmVersion.xcconfig` (auto-generated from build script)
3. Update changelog and release checklist.
4. Open and merge PR `dev` -> `main` (auto-merge is preferred once required checks are green).
5. If release-critical docs/website deltas were developed on `docs` or `web`, merge those branches into `main` via PR before tagging.
6. Create annotated tag.
7. Push tag to GitHub.
8. Publish GitHub release notes.
9. Let `release-macos-dmg.yml` publish appcast/release notes via PR branch `chore/publish-updates-<tag>` and wait for merge (no direct-push fallback).
10. Confirm drift checks remain green (`Appcast Drift Guard`).

Release checklist document:
- `docs/RELEASE_CHECKLIST.md`

---

## Stability Promise (Post-1.0)

After 1.0:
- Public core APIs are stable.
- Adapter trait changes require MAJOR bump.
- Orchestration semantics remain backward compatible.

---

## Licensing Note (Pre-1.0)

Pre-1.0 versions of Helm are distributed under a source-available, non-commercial license.

Licensing terms may change at or after 1.0.
