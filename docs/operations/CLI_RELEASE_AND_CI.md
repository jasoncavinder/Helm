# CLI Release & CI Operations

Status: Active operational guide  
Owner: Helm Release Engineering  
Last Updated: 2026-02-24

---

## 1. Scope

This guide covers:

- CLI direct-release automation (`release-cli-direct.yml`)
- website metadata publication for CLI self-update (`/updates/cli/latest.json` and `/updates/cli/latest-rc.json`)
- `install.sh` validation and maintainer actions outside CI
- all-variant release orchestration (`release-all-variants.yml`)

Mandatory release preflight tooling:

- `scripts/release/preflight.sh` (scope/auth/workflow/secret/git checks)
- `scripts/release/runbook.sh` (prepare/tag/publish/verify wrappers)

Reference contracts:

- `docs/architecture/BUILD_VARIANTS.md`
- `.github/workflows/release-cli-direct.yml`
- `.github/workflows/release-publish-verify.yml`
- `.github/workflows/cli-installer-checks.yml`
- `.github/workflows/release-all-variants.yml`

---

## 2. CLI Update Metadata Endpoint Contract

Endpoints:

- `https://helmapp.dev/updates/cli/latest.json`
- repository path: `web/public/updates/cli/latest.json`
- prerelease endpoint: `https://helmapp.dev/updates/cli/latest-rc.json`
- prerelease repository path: `web/public/updates/cli/latest-rc.json`

Availability note:

- `latest.json` is required and must stay publishable/non-404 for stable direct installs.
- `latest-rc.json` is published only after the first prerelease tag flow (`vX.Y.Z-rc.N`).

Schema:

```json
{
  "version": "0.17.3",
  "channel": "stable",
  "published_at": "2026-02-23T00:00:00Z",
  "downloads": {
    "universal": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.3/helm-cli-v0.17.3-darwin-universal",
      "sha256": "<hex>"
    },
    "arm64": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.3/helm-cli-v0.17.3-darwin-arm64",
      "sha256": "<hex>"
    },
    "x86_64": {
      "url": "https://github.com/<owner>/<repo>/releases/download/v0.17.3/helm-cli-v0.17.3-darwin-x86_64",
      "sha256": "<hex>"
    }
  }
}
```

Compatibility guarantees:

- `version` is semver-compatible text consumed by CLI comparison logic.
- `channel` is `stable` for `latest.json` and `rc` for `latest-rc.json`.
- `downloads.universal` is preferred by installer/updater when present.
- `downloads.arm64` and `downloads.x86_64` are fallback architecture-specific entries.
- Additional JSON fields are allowed and ignored by current CLI/install script.
- Existing fields should not be removed without a coordinated CLI contract update.

Channel caveats:

- Package-manager wrapper channels (Homebrew/Cargo/MacPorts) may expose new CLI versions on different timelines than the direct endpoint.
- MAS GUI version visibility may normalize prerelease labels differently than direct prerelease channels; this does not change CLI update policy routing.

---

## 3. How CLI Self-Update Uses This Endpoint

`helm self update` behavior (read-only and mutating modes):

- resolves install provenance (`~/.config/helm/install.json` with heuristic fallback)
- only allows direct self-update for `channel=direct-script` + `update_policy=self`
- `--force` is only honored for `direct-script` installs (it no longer bypasses package-manager channels)
- fetches `latest.json` for stable/direct-script policy; prerelease metadata is published to `latest-rc.json` and does not move stable pointer automatically
- enforces endpoint/download URL policy:
  - default: `https` + allowlisted hosts (`helmapp.dev`, GitHub release hosts)
  - local test override: `HELM_CLI_ALLOW_INSECURE_UPDATE_URLS=1` to permit `file://...` URLs
- enforces bounded asset size on direct binary downloads:
  - default max: `64 MiB`
  - override: `HELM_CLI_SELF_UPDATE_MAX_DOWNLOAD_BYTES`
- compares `version` against current CLI version
- if update is available:
  - selects `universal` asset when available, otherwise architecture-specific asset
  - downloads binary
  - verifies `sha256`
  - atomically replaces executable
  - refreshes provenance marker for direct-script channel
- mutating self-update blocks root execution by default (`HELM_ALLOW_ROOT_SELF_UPDATE=1` is explicit override)
- mutating self-update refuses symlink/non-file replacement targets for executable path safety

Blocked channels (non-self policy):

- `brew`, `macports`, `cargo`, `app-bundle-shim`, and `managed`
- CLI prints recommended channel action (for example `brew upgrade helm-cli`)

---

## 4. Direct Installer (`install.sh`) Behavior

`install.sh`:

- fetches the same endpoint (`HELM_CLI_UPDATE_ENDPOINT` override supported)
- enforces endpoint/download URL policy:
  - default: `https` only + allowlisted hosts (`helmapp.dev` and GitHub release hosts)
  - local test override: `HELM_INSTALL_ALLOW_INSECURE_URLS=1` to allow `file://...` URLs
- blocks root execution by default:
  - explicit override: `HELM_ALLOW_ROOT_INSTALL=1`
- installs binary to:
  - default: `~/.local/bin/helm`
  - override: `HELM_INSTALL_BIN_DIR`
- writes provenance marker:
  - default path: `~/.config/helm/install.json`
  - override: `HELM_INSTALL_MARKER_PATH`
  - channel=`direct-script`, update_policy=`self`
  - schema contract: `docs/contracts/install-marker.schema.json`
- supports architecture override for testing:
  - `HELM_INSTALL_FORCE_ARCH=arm64|x86_64`
- supports network timeout overrides:
  - `HELM_INSTALL_CONNECT_TIMEOUT_SECS`
  - `HELM_INSTALL_MAX_TIME_SECS`

---

## 5. Maintainer Actions Required Outside CI

### 5.0 Run Release Preflight First (Required)

Before tagging or publishing, run:

```bash
scripts/release/preflight.sh --tag vX.Y.Z
```

or the wrapper form:

```bash
scripts/release/runbook.sh prepare --tag vX.Y.Z
```

If your shell prints locale warnings, normalize locale env before release commands:

```bash
export LANG=en_US.UTF-8
export LC_ALL=en_US.UTF-8
```

Expected outcome:

- zero preflight errors
- token scopes include `repo` and `workflow`
- release workflows are discoverable by `gh`
- required DMG signing/update secrets are present
- stable metadata snapshot on `origin/main` is synchronized (`appcast.xml` and `cli/latest.json`) and behind the target stable tag

If preflight fails, resolve failures before creating/pushing tags.

### 5.1 Authenticate `gh` with Maintainer PAT

Required scopes (minimum):

- `repo`
- `workflow`

Without these, release operators cannot reliably rerun/dispatch workflows (`Resource not accessible by personal access token`).

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

### 5.2.1 Verify Main Ruleset Publish-PR Bypass Policy

`scripts/release/preflight.sh` now validates `main` ruleset policy for release publish branches.

Expected policy (least privilege):

- `pull_request` and `required_status_checks` rules are present on `refs/heads/main`
- required checks include `Policy Gate`
- bypass actor policy uses pull-request-only mode (never `always`)
- preferred: bypass actor includes GitHub Actions app in pull-request mode:
  - `actor_type=Integration`
  - `actor_id=15368` (`github-actions`)
  - `bypass_mode=pull_request`
- fallback (when GitHub rejects integration actor for repository-owned rulesets): `Repository admin` role in `pull_request` mode

Quick verification:

```bash
gh api repos/jasoncavinder/Helm/rulesets/13089765 --jq '{id,name,bypass_actors,rules:[.rules[].type],required_checks:(.rules[] | select(.type=="required_status_checks") | .parameters.required_status_checks | map(.context))}'
```

If remediation is needed, update the `Protect main branch` ruleset so bypass actors use `pull_request` mode (not `always`), and apply the best available actor path:

- preferred: GitHub Actions integration actor
- fallback: `Repository admin` role with `pull_request` bypass mode only

UI remediation path:

1. GitHub repository `Settings` -> `Rules` -> `Rulesets`.
2. Open `Protect main branch`.
3. In `Bypass list`, add `GitHub Actions` and set bypass mode to `Pull requests only`.
4. In `Bypass list`, change any broad role bypass from `Always` to `Pull requests only` (or remove it).
5. Save ruleset changes and rerun preflight.

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

Use this to backfill existing tags (for example `v0.17.3`) or rerun publication:

```bash
gh workflow run release-cli-direct.yml -f tag=v0.17.3
gh run list --workflow "Release CLI Direct Installer" --limit 5
gh run view <run-id> --log
```

Tag policy used by `release-cli-direct.yml`:

- stable tags: `vX.Y.Z` -> publish `web/public/updates/cli/latest.json`
- prerelease tags: `vX.Y.Z-rc.N` -> publish `web/public/updates/cli/latest-rc.json`
- unsupported tag formats are rejected

### 5.5 Interpret Release Workflow Publication Summaries

Both direct release workflows now emit a publication summary with:

- `Artifacts uploaded: yes/no`
- `Publish PR opened: yes/no`
- `Main metadata synced: yes/no`

Outcome semantics:

- hard failures are retained for build/signing/notarization/upload/PR-creation faults
- if publication PR automation succeeds but PR merge is still pending, the run can complete with follow-up required (non-red terminal state)
- when follow-up is required: merge the publish PR and rerun the workflow to confirm `Main metadata synced: yes`
- release logs now use phase prefixes to simplify triage:
  - `[preflight]` for auth/scope/policy setup checks
  - `[build]` for compile/package/notarization execution
  - `[publish]` for release asset and publish-PR operations
  - `[verify]` for metadata consistency and final checkpoint output

### 5.6 Verify Publish-PR Merge Checkpoint

`release-publish-verify.yml` runs automatically on `main` pushes touching publish metadata paths and can be run manually.

It verifies:

- top `appcast.xml` version matches `cli/latest.json` stable version
- matched stable version maps to a non-draft, non-prerelease GitHub release tag
- matching release-notes artifact exists under `web/public/updates/release-notes/<tag>.html`
- `cli/latest-rc.json` (when present) maps to a non-draft prerelease tag with `channel=rc`

Manual trigger:

```bash
gh workflow run release-publish-verify.yml
gh run list --workflow "Release Publish Verify" --limit 5
gh run view <run-id> --log
```

Optional strict target check:

```bash
gh workflow run release-publish-verify.yml -f tag=vX.Y.Z
```

### 5.7 Trigger All-Variant Build/Release Orchestration

This workflow runs:

- direct GUI DMG release flow
- direct CLI release flow
- MAS profile unsigned build artifact
- Setapp profile unsigned build artifact
- business profile unsigned `.app` zip + unsigned `.pkg` artifact

Command:

```bash
gh workflow run release-all-variants.yml -f tag=v0.17.3 -f upload_auxiliary_assets=true
gh run list --workflow "Release All Variants" --limit 5
gh run view <run-id> --log
```

Notes:

- `release-all-variants.yml` ensures a release exists for the tag (creates draft if missing).
- direct channel jobs keep existing release workflows unchanged.
- MAS/Setapp/business orchestration shares one matrix-driven build path and one helper (`scripts/release/build_unsigned_variant.sh`) keyed by `docs/contracts/distribution-profiles.json`.
- MAS/Setapp/business outputs are intentionally unsigned in the baseline orchestration workflow.
- signed store/vendor pipelines remain a separate follow-up.

### 5.8 Promote Recurring Release Friction Into Permanent Docs

`TMP_RELEASE_FRICTION` is temporary capture only and should not be committed.

Promotion path after each release:

1. append concrete friction entries during execution (`symptom`, `root cause`, `workaround`, `date`, `run/pr reference`)
2. mark entries as recurring when they repeat or cause release delay
3. promote recurring items into durable docs:
   - policy/process decisions -> `docs/DECISIONS.md`
   - operator/runbook/checklist updates -> this file and `docs/RELEASE_CHECKLIST.md`
4. link the fixing PR/commit in the promoted entry and close the temporary friction item

---

## 6. Install Script CI Responsibilities

`cli-installer-checks.yml` validates:

- `shellcheck` lint on `install.sh` and `scripts/build.sh`
- deterministic install smoke test into temp prefix
- provenance marker emission/contents
- provenance marker schema conformance via `scripts/validate_install_marker_schema.py`

Additional metadata guard:

- `release-publish-verify.yml` validates publish-PR merge outcomes against GitHub release state on every relevant `main` metadata push.
- `cli-update-drift.yml` validates that stable/prerelease CLI metadata pointers align with latest GitHub releases.
- release workflows pin release-critical third-party actions to immutable SHAs and use explicit per-job token write scopes.
- `release-cli-direct.yml` verifies the built universal binary reports a version matching the target tag before asset publication.

This workflow is intentionally scoped to installer surface changes.

---

## 7. Non-Goals for This Milestone

- No MAS CI packaging implementation
- No Setapp CI packaging implementation
- No Business PKG/notarization CI implementation
- No extended MDM feature implementation beyond managed-policy provenance placeholder
