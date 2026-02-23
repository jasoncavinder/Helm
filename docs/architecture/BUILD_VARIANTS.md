# Build Variants / Distribution Profiles

Status: Active contract (implementation in progress for some channels)  
Owner: Helm Core + Release Engineering  
Last Updated: 2026-02-23

---

## 1. Scope

This document is the source of truth for Helm distribution profiles, channel wrappers, and CLI install provenance/update policy.

Terminology:

- `variant`: top-level distribution profile (`direct`, `mas`, `setapp`, `business`).
- `channel wrapper`: package ecosystem wrapper around a variant artifact (Homebrew, Cargo, MacPorts).

Internal channel-key mapping used in app build settings:

- `direct` -> `developer_id`
- `mas` -> `app_store`
- `setapp` -> `setapp`
- `business` -> `fleet` (legacy key; kept for compatibility)

---

## 2. Distribution Profile Matrix

| Variant | Update Mechanism | `helm self update` | Licensing / Gating Assumption | Primary Artifacts |
|---|---|---|---|---|
| `direct` | GUI: Sparkle (Developer ID DMG). CLI direct installer: Helm CLI self-update endpoint. | Allowed only for CLI installs with `update_policy=self` (direct script) or explicit `--force` override on non-managed channels. | Consumer channel (Free + Pro entitlement model, post-1.0). | `Helm.app`, `Helm.dmg`, `helm` CLI binaries, install script |
| `mas` | Mac App Store-managed | Not allowed (channel-managed). | Consumer MAS commerce/receipt authority. | MAS app package (`Helm.app` via App Store pipeline) |
| `setapp` | Setapp-managed | Not allowed (channel-managed). | Consumer Setapp subscription authority (Pro-equivalent channel behavior). | Setapp app artifact (`Helm.app` for Setapp ingestion) |
| `business` | Admin-controlled rollout (PKG/MDM managed) | Blocked by default (`update_policy=managed`). | Separate Helm Business lifecycle with managed policy defaults. | Signed `Helm Business` PKG (`.pkg`) and managed app payload |

Rules:

- Sparkle is direct GUI only.
- Sparkle is excluded from `mas`, `setapp`, and `business`.
- Update authority and licensing authority remain decoupled.

---

## 3. Channel Wrappers

| Wrapper | Artifact Type | Installed Command / App | Update Authority | CLI Provenance Channel |
|---|---|---|---|---|
| Homebrew Cask | GUI app cask | `Helm.app` | Homebrew cask | `brew` (channel-managed) |
| Homebrew Formula | CLI formula (`helm-cli`) | `helm` | Homebrew formula | `brew` (channel-managed) |
| Cargo | CLI crate (`helm-cli`) | `helm` (typically `~/.cargo/bin/helm`) | Cargo (`cargo install`) | `cargo` (channel-managed) |
| cargo-binstall | Prebuilt CLI binary install (`helm-cli`) | `helm` | Channel-managed by binstall package source | `cargo` unless explicit marker says otherwise |
| MacPorts (initial target: CLI-first) | Port package | `helm` (CLI), GUI optional later | MacPorts | `macports` (channel-managed) |
| Direct Script | CLI direct binary install | `helm` | Helm direct CLI endpoint | `direct-script` (`update_policy=self`) |

Naming contract:

- Distribution package name may be `helm-cli`.
- Installed executable name is `helm`.
- GUI app bundle name remains `Helm.app`.

---

## 4. GUI / CLI Relationship

Contract:

- GUI and CLI share the same core task/orchestration authority model.
- GUI distribution may include an embedded CLI payload.
- GUI may install an optional shim at `~/.local/bin/helm` that points to bundled CLI runtime bits.

Provenance implications:

- GUI-installed shim should write provenance channel `app-bundle-shim` with `update_policy=channel`.
- App-bundle shim never self-updates independently; GUI channel remains update authority.

---

## 5. CLI Provenance Marker Contract

Marker path:

- `~/.config/helm/install.json`

Marker schema:

```json
{
  "channel": "direct-script",
  "artifact": "helm-cli",
  "installed_at": "2026-02-23T12:34:56Z",
  "update_policy": "self",
  "version": "0.17.2"
}
```

Fields:

- `channel`:
  - `direct-script`
  - `app-bundle-shim`
  - `brew`
  - `macports`
  - `cargo`
  - `managed`
  - `unknown`
- `artifact`:
  - currently `helm-cli`
- `installed_at`:
  - ISO-8601 UTC timestamp
- `update_policy`:
  - `self` (CLI may self-update directly)
  - `channel` (must use external manager/channel command)
  - `managed` (admin policy; direct self-update blocked)
  - `none` (no update path available)
- `version`:
  - optional install-time version hint

Managed placeholder policy (minimal enterprise scope):

- `channel=managed` + `update_policy=managed` is sufficient to force deny self-update.
- No additional MDM/policy plumbing is required for this milestone.

---

## 6. Provenance Fallback Heuristics (When Marker Is Missing)

Executable realpath rules:

1. under `/opt/local` -> `macports`
2. under Homebrew prefix (`/opt/homebrew`, `/usr/local`, or discovered `brew --prefix`) -> `brew`
3. under `~/.cargo/bin` -> `cargo`
4. inside `*.app/Contents/` -> `app-bundle-shim`
5. otherwise -> `unknown`

Default policy when marker missing:

- `brew`, `macports`, `cargo`, `app-bundle-shim` -> `update_policy=channel`
- `unknown` -> `update_policy=none`

---

## 7. Recommended Channel Actions

| Provenance Channel | Recommended Action |
|---|---|
| `direct-script` | `helm self update` |
| `brew` | `brew upgrade helm-cli` |
| `macports` | `sudo port selfupdate && sudo port upgrade helm-cli` |
| `cargo` | `cargo install --locked helm-cli` |
| `app-bundle-shim` | Update the Helm GUI via its channel (Sparkle/App Store/Setapp) |
| `managed` | Follow organization-managed update workflow |
| `unknown` | Reinstall via supported channel or use direct installer |

---

## 8. Local Build Process (Per-Variant + Build-All)

Primary helper script:

- `scripts/build.sh`

Prerequisites:

- Xcode + Command Line Tools (`xcodebuild`, `hdiutil`)
- Rust toolchain (`cargo`)
- macOS host

Per-variant commands:

- Direct profile:
  - `scripts/build.sh direct`
- MAS profile:
  - `scripts/build.sh mas`
- Setapp profile:
  - `scripts/build.sh setapp`
- Business profile:
  - `scripts/build.sh business`

Build-everything command:

- `scripts/build.sh all`

Behavior and outputs:

- All commands build the release CLI binary (`helm`) and copy it to `build/variants/cli/helm`.
- Direct profile:
  - Builds unsigned app at `build/variants/direct/Helm.app`
  - Builds unsigned DMG at `build/variants/direct/Helm-unsigned.dmg`
- MAS profile:
  - Builds unsigned app at `build/variants/mas/Helm.app`
  - Writes placeholder packaging notes at `build/variants/mas/ARTIFACT_NOTES.md`
- Setapp profile:
  - Builds unsigned app at `build/variants/setapp/Helm.app`
  - Writes placeholder packaging notes at `build/variants/setapp/ARTIFACT_NOTES.md`
- Business profile:
  - Builds unsigned app at `build/variants/business/Helm.app`
  - Writes placeholder packaging notes at `build/variants/business/ARTIFACT_NOTES.md`
- `all` mode is best-effort for variant GUI packaging: it continues through all variants and reports optional failures at the end.

Scope note:

- This script is for local validation and artifact shape checks.
- Signed, notarized, and store/vendor submissions remain release-pipeline responsibilities.

---

## 9. CI Ownership Boundaries (High-Level)

- GUI direct DMG + Sparkle feed publication remains existing release workflow responsibility.
- CLI direct installer publication must publish:
  - release binaries
  - checksums
  - website `latest.json` contract payload
- Future channel CI (MAS, Setapp, business PKG) remains roadmap-scoped until credentials/process are ready.

---

## 10. CLI Update Endpoint Contract

Endpoint:

- `https://helmapp.dev/updates/cli/latest.json`
- repository path: `web/public/updates/cli/latest.json`

Payload requirements:

- `version` (semver-compatible text)
- `published_at` (ISO-8601 UTC timestamp)
- `downloads.universal|arm64|x86_64` entries with:
  - `url`
  - `sha256`

Compatibility rule:

- existing fields above are stable for current CLI self-update and `install.sh`.
- additive fields are allowed.
- field removal/renaming requires coordinated CLI contract/version update.

---

## 11. Operational References

- Maintainer workflow and manual action guide:
  - `docs/operations/CLI_RELEASE_AND_CI.md`
- Future CI milestone tracking:
  - `docs/roadmap/CLI_DISTRIBUTION_CI_MILESTONES.md`
