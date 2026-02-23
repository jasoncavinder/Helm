# CLI Release & CI Operations

Status: Active operational guide  
Owner: Helm Release Engineering  
Last Updated: 2026-02-23

---

## 1. Scope

This guide covers:

- CLI direct-release automation (`release-cli-direct.yml`)
- website metadata publication for CLI self-update (`/updates/cli/latest.json`)
- `install.sh` validation and maintainer actions outside CI
- all-variant release orchestration (`release-all-variants.yml`)

Reference contracts:

- `docs/architecture/BUILD_VARIANTS.md`
- `.github/workflows/release-cli-direct.yml`
- `.github/workflows/cli-installer-checks.yml`
- `.github/workflows/release-all-variants.yml`

---

## 2. CLI Update Metadata Endpoint Contract

Endpoint:

- `https://helmapp.dev/updates/cli/latest.json`
- repository path: `web/public/updates/cli/latest.json`

Schema:

```json
{
  "version": "0.17.2",
  "published_at": "2026-02-23T00:00:00Z",
  "downloads": {
    "universal": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.2/helm-cli-v0.17.2-darwin-universal",
      "sha256": "<hex>"
    },
    "arm64": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.2/helm-cli-v0.17.2-darwin-arm64",
      "sha256": "<hex>"
    },
    "x86_64": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.2/helm-cli-v0.17.2-darwin-x86_64",
      "sha256": "<hex>"
    }
  }
}
```

Compatibility guarantees:

- `version` is semver-compatible text consumed by CLI comparison logic.
- `downloads.universal` is preferred by installer/updater when present.
- `downloads.arm64` and `downloads.x86_64` are fallback architecture-specific entries.
- Additional JSON fields are allowed and ignored by current CLI/install script.
- Existing fields should not be removed without a coordinated CLI contract update.

---

## 3. How CLI Self-Update Uses This Endpoint

`helm self update` behavior (read-only and mutating modes):

- resolves install provenance (`~/.config/helm/install.json` with heuristic fallback)
- only allows direct self-update for `update_policy=self` (or `--force` for non-managed channel installs)
- fetches `latest.json`
- compares `version` against current CLI version
- if update is available:
  - selects `universal` asset when available, otherwise architecture-specific asset
  - downloads binary
  - verifies `sha256`
  - atomically replaces executable
  - refreshes provenance marker for direct-script channel

Blocked channels (non-self policy):

- `brew`, `macports`, `cargo`, `app-bundle-shim`, and `managed`
- CLI prints recommended channel action (for example `brew upgrade helm-cli`)

---

## 4. Direct Installer (`install.sh`) Behavior

`install.sh`:

- fetches the same endpoint (`HELM_CLI_UPDATE_ENDPOINT` override supported)
- installs binary to:
  - default: `~/.local/bin/helm`
  - override: `HELM_INSTALL_BIN_DIR`
- writes provenance marker:
  - default path: `~/.config/helm/install.json`
  - override: `HELM_INSTALL_MARKER_PATH`
  - channel=`direct-script`, update_policy=`self`
- supports architecture override for testing:
  - `HELM_INSTALL_FORCE_ARCH=arm64|x86_64`

---

## 5. Maintainer Actions Required Outside CI

### 5.1 Authenticate `gh` with Maintainer PAT

Required scopes (minimum):

- `repo`
- `workflow`

Commands:

```bash
gh auth logout -h github.com -u || true
gh auth login --hostname github.com --git-protocol https --web
gh auth status
```

If using token mode:

```bash
printf '%s' "$GITHUB_PAT" | gh auth login --with-token
gh auth status
```

### 5.2 Verify Actions Permissions

Repository settings should allow:

- GitHub Actions to create and approve pull requests where required
- workflow token write access for release publication jobs

Quick checks:

```bash
gh repo view --json name,defaultBranchRef
gh workflow list
```

### 5.3 Set/Verify Release Secrets

Existing DMG release workflow (`release-macos-dmg.yml`) still requires current Apple/signing secrets.
CLI release workflow relies on `github.token` for release uploads + PR publication and does not add new required secrets.

To set or rotate secrets:

```bash
gh secret list
gh secret set MACOS_DEVELOPER_ID_APP_CERT_BASE64 < cert.b64
gh secret set MACOS_DEVELOPER_ID_APP_CERT_PASSWORD
gh secret set MACOS_KEYCHAIN_PASSWORD
gh secret set HELM_SPARKLE_FEED_URL
gh secret set HELM_SPARKLE_PUBLIC_ED_KEY
gh secret set HELM_SPARKLE_PRIVATE_ED_KEY
```

### 5.4 Trigger CLI Release Publication Manually

Use this to backfill existing tags (for example `v0.17.2`) or rerun publication:

```bash
gh workflow run release-cli-direct.yml -f tag=v0.17.2
gh run list --workflow "Release CLI Direct Installer" --limit 5
gh run view <run-id> --log
```

### 5.5 Trigger All-Variant Build/Release Orchestration

This workflow runs:

- direct GUI DMG release flow
- direct CLI release flow
- MAS profile unsigned build artifact
- Setapp profile unsigned build artifact
- business profile unsigned `.app` zip + unsigned `.pkg` artifact

Command:

```bash
gh workflow run release-all-variants.yml -f tag=v0.17.2 -f upload_auxiliary_assets=true
gh run list --workflow "Release All Variants" --limit 5
gh run view <run-id> --log
```

Notes:

- `release-all-variants.yml` ensures a release exists for the tag (creates draft if missing).
- direct channel jobs keep existing release workflows unchanged.
- MAS/Setapp/business outputs are intentionally unsigned in the baseline orchestration workflow.
- signed store/vendor pipelines remain a separate follow-up.

---

## 6. Install Script CI Responsibilities

`cli-installer-checks.yml` validates:

- `shellcheck` lint on `install.sh` and `scripts/build.sh`
- deterministic install smoke test into temp prefix
- provenance marker emission/contents

This workflow is intentionally scoped to installer surface changes.

---

## 7. Non-Goals for This Milestone

- No MAS CI packaging implementation
- No Setapp CI packaging implementation
- No Business PKG/notarization CI implementation
- No extended MDM feature implementation beyond managed-policy provenance placeholder
