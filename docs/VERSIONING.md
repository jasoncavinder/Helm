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

Advisory (non-required) release monitors:

- `Release Publish Verify`
- `Appcast Drift Guard`
- `CLI Update Metadata Drift Guard`

These workflows validate post-merge publication convergence and should stay advisory so release publish PR merge order does not block unrelated PR merges.

Operational settings:

- auto-merge enabled
- update-branch enabled
- auto-delete merged branches disabled (to protect `main`/`dev`/`docs`/`web`)

`Policy Gate` is the authoritative branch-target/scope guardrail for PRs.

---

## Release Flow

1. Complete milestone work on `dev`.
2. Update version in:
   - `core/rust/Cargo.toml` (workspace version)
   - Generated files: `apps/macos-ui/Generated/HelmVersion.swift` and `apps/macos-ui/Generated/HelmVersion.xcconfig` (auto-generated from build script)
3. Update changelog and release checklist.
4. Run mandatory preflight:
   - `scripts/release/preflight.sh --tag <tag>`
   - or wrapper: `scripts/release/runbook.sh prepare --tag <tag>`
5. Open and merge PR `dev` -> `main` (auto-merge is preferred once required checks are green).
6. If release-critical docs/website deltas were developed on `docs` or `web`, merge those branches into `main` via PR before tagging.
7. Create annotated tag.
8. Push tag to GitHub.
9. Publish GitHub release notes.
10. Let release workflows publish metadata via PR branches:
   - `release-macos-dmg.yml` -> `chore/publish-updates-<tag>`
   - `release-cli-direct.yml` -> `chore/publish-cli-updates-<tag>-<channel>`
11. Review release workflow summary output:
   - `Artifacts uploaded: yes/no`
   - `Publish PR opened: yes/no`
   - `Main metadata synced: yes/no`
12. If the workflow reports follow-up required (publish PR still open), merge the publish PR and rerun the workflow to confirm metadata sync.
13. Confirm post-merge publish verification is green (`Release Publish Verify`).
14. Confirm drift checks remain green (`Appcast Drift Guard`, `CLI Update Metadata Drift Guard`).

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
