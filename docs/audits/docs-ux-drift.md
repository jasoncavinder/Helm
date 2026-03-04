# Docs and UX Drift Audit

Date: 2026-02-25  
Scope: README, website-facing docs, CLI help/error output consistency, terminology consistency.

## Method

- Reviewed source docs and website content:
  - `README.md`
  - `web/src/components/starlight/Banner.astro`
  - `web/src/content/docs/guides/{installation,usage,faq}.md`
  - `web/src/content/docs/{index,overview,roadmap,changelog}.md[x]`
- Verified live CLI help/behavior from local binary:
  - `core/rust/target/debug/helm --help`
  - `core/rust/target/debug/helm onboarding --help`
  - `core/rust/target/debug/helm managers --help`
  - `core/rust/target/debug/helm updates --help`
  - Error-path checks:
    - `helm bogus`
    - `helm --timeout 0 status`
    - `helm managers show`
    - `helm updates run`

## Findings

### 1) Stable vs beta wording drift on website surfaces

- Severity: Medium
- Impact: Users see conflicting release posture (stable in README/docs vs "beta testing" language on website banner/installation/FAQ).
- Evidence:
  - `web/src/components/starlight/Banner.astro` used "Help test Helm v0.17.6" + "latest beta".
  - `web/src/content/docs/guides/installation.md` used "latest beta DMG".
  - `web/src/content/docs/guides/faq.md` used "Helm beta builds".
- Recommended fix: Normalize wording to "latest release" while preserving pre-1.0 context.
- Status: Fixed in this pass.

### 2) README download label drift

- Severity: Low
- Impact: README framed stable release as "Beta Download" despite `v0.17.6` being documented as current stable on `main`.
- Evidence:
  - `README.md` section header was `## Beta Download`.
- Recommended fix: Rename to release-oriented wording.
- Status: Fixed in this pass (`## Download`, "latest release DMG").

### 3) Support channel visibility mismatch between app/README and website docs

- Severity: Low
- Impact: Support options were complete in app/README, but website FAQ did not expose all six support channels in one place.
- Evidence:
  - README has six channels (GitHub Sponsors, Patreon, Buy Me a Coffee, Ko-fi, PayPal, Venmo).
  - Website FAQ lacked a direct "how to support" list.
- Recommended fix: Add a dedicated FAQ entry listing all six support channels.
- Status: Fixed in this pass.

### 4) Timeout behavior documentation drift

- Severity: Medium
- Impact: FAQ described static timeout values (30s/300s) that no longer match current activity-aware hard+idle timeout behavior and manager-tunable profiles.
- Evidence:
  - `web/src/content/docs/guides/faq.md` previously stated fixed timeout numbers.
  - Current state docs describe activity-aware timeout semantics and per-manager tuning.
- Recommended fix: Update FAQ wording to describe hard/idle timeouts and manager-level tuning.
- Status: Fixed in this pass.

### 5) CLI behavior discoverability gap in website docs

- Severity: Low
- Impact: CLI help and command surface exist, but usage docs lacked a concise CLI/TUI behavior section.
- Evidence:
  - `helm --help` documents CLI/TUI behavior and extensive command surface.
  - `web/src/content/docs/guides/usage.md` previously focused only on GUI flows.
- Recommended fix: Add a short CLI/TUI section with core behavior and starter commands.
- Status: Fixed in this pass.

### 6) CLI error strings are correct but not consistently actionable

- Severity: Low
- Impact: Some common CLI errors do not guide recovery steps.
- Evidence:
  - `helm bogus` -> `unknown command 'bogus'`
  - `helm managers show` -> `managers show requires a manager id`
  - `helm updates run` -> `updates run requires --yes`
- Recommended fix:
  - Add quick hints, for example:
    - unknown command: "run `helm help`"
    - missing manager id: "run `helm managers list`"
    - missing `--yes`: "run `helm updates preview` first"
- Status: Not implemented in this pass (kept as follow-up copy-hardening item).

### 7) Terminology consistency (manager vs adapter vs task vs service/daemon)

- Severity: Low
- Impact: Minor cognitive friction where internal terms may surface without context.
- Evidence:
  - User-facing docs mostly use "manager" and "task".
  - Architecture docs correctly use "adapter" and "service/XPC boundary".
  - `docs/PROJECT_BRIEF.md` includes "daemon / XPC boundary" phrasing while most user-facing docs use "service" or "XPC".
- Recommended fix:
  - Keep user-facing docs on "manager", "task", and "service (XPC)".
  - Reserve "adapter" for architecture/developer docs and define once where needed.
- Status: Acceptable now; monitor during future doc edits.

### 8) Website build emits duplicate-content-id warnings

- Severity: Low
- Impact: Build still succeeds, but warnings can mask future real content-registry issues.
- Evidence:
  - `npm run build` emitted:
    - duplicate id `guides/faq`
    - duplicate id `guides/installation`
    - duplicate id `guides/usage`
- Recommended fix:
  - Inspect Starlight content loader/config for duplicate registration sources and resolve to warning-free builds.
- Status: Not fixed in this pass (outside this localized copy sweep).

## Changes Applied

- `README.md`
  - "Beta Download" -> "Download"
  - "latest beta DMG" -> "latest release DMG"
  - Added `Helm CLI (Bundled)` section with behavior-aligned quick commands
- `web/src/components/starlight/Banner.astro`
  - Updated global banner to release wording
- `web/src/content/docs/guides/installation.md`
  - Updated beta wording to release wording
- `web/src/content/docs/guides/faq.md`
  - Updated pre-1.0/beta wording
  - Added dedicated six-channel support methods section
  - Updated timeout explanation to activity-aware semantics + tunable profiles
- `web/src/content/docs/guides/usage.md`
  - Added CLI/TUI behavior section and starter commands

## Follow-up Recommendations

1. Apply low-risk CLI error-copy hint improvements in `core/rust/crates/helm-cli/src/main.rs`.
2. Consider centralizing "current release line" copy for website banner/docs to reduce future version-string drift.
3. Resolve Starlight duplicate-content-id warnings for `guides/*` during website builds.
