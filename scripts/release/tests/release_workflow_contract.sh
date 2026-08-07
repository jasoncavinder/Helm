#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKFLOWS_DIR="${ROOT_DIR}/.github/workflows"
ALL_VARIANTS_WORKFLOW="${WORKFLOWS_DIR}/release-all-variants.yml"
CLI_WORKFLOW="${WORKFLOWS_DIR}/release-cli-direct.yml"
DMG_WORKFLOW="${WORKFLOWS_DIR}/release-macos-dmg.yml"
PUBLISH_VERIFY_WORKFLOW="${WORKFLOWS_DIR}/release-publish-verify.yml"
CANARY_WORKFLOW="${WORKFLOWS_DIR}/release-macos-canary.yml"
AUTH_CHECK_WORKFLOW="${WORKFLOWS_DIR}/release-publish-auth-check.yml"
PREFLIGHT_SCRIPT="${ROOT_DIR}/scripts/release/preflight.sh"
WEB_BUILD_WORKFLOW="${WORKFLOWS_DIR}/web-build.yml"

fail() {
  printf '[release-workflow-contract] error: %s\n' "$1" >&2
  exit 1
}

expect_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  grep -Eq -- "$pattern" "$file" || fail "$description"
}

reject_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  if grep -Eq -- "$pattern" "$file"; then
    fail "$description"
  fi
}

expect_pattern 'verify-published-release:' "$ALL_VARIANTS_WORKFLOW" "all-variants workflow must verify an existing release"
expect_pattern 'gh release view "\$TAG_NAME" --json isDraft' "$ALL_VARIANTS_WORKFLOW" "all-variants workflow must inspect existing release state"
expect_pattern 'default: false' "$ALL_VARIANTS_WORKFLOW" "unsigned auxiliary release uploads must default off"
reject_pattern 'gh release create' "$ALL_VARIANTS_WORKFLOW" "all-variants workflow must not create releases"
reject_pattern 'release-macos-dmg\.yml' "$ALL_VARIANTS_WORKFLOW" "all-variants workflow must not invoke the DMG builder"
reject_pattern 'release-cli-direct\.yml' "$ALL_VARIANTS_WORKFLOW" "all-variants workflow must not invoke the CLI builder"

for workflow in "$CLI_WORKFLOW" "$DMG_WORKFLOW"; do
  expect_pattern 'git push -u origin "\$PUBLISH_BRANCH" --force-with-lease' "$workflow" "metadata publication must use force-with-lease"
  reject_pattern 'git push.*--force($|[[:space:]])' "$workflow" "metadata publication must not fall back to unconditional force"
  expect_pattern 'FALLBACK_GH_TOKEN: \$\{\{ github\.token \}\}' "$workflow" "metadata publication must retry with github.token"
  expect_pattern 'PUBLISH_AUTH_MODE=github_token_fallback' "$workflow" "metadata publication must record fallback authentication"
  expect_pattern 'publish_pr_handoff_state\.py' "$workflow" "metadata publication must classify the full publish PR JSON snapshot"
  expect_pattern 'HANDOFF_STATUS.*=.*merged' "$workflow" "metadata publication must recognize a merged publish PR"
  expect_pattern 'HANDOFF_STATUS.*=.*open' "$workflow" "metadata publication must preserve an open publish PR while polling"
  expect_pattern 'HANDOFF_STATUS.*closed_unmerged' "$workflow" "metadata publication must preserve a non-red closed-PR follow-up state"
  reject_pattern 'mergedAt.*@tsv' "$workflow" "metadata publication must not parse nullable mergedAt through collapsing TSV fields"
done

expect_pattern 'APPCAST_CHANNEL="beta"' "$DMG_WORKFLOW" "RC releases must select the Sparkle beta channel"
expect_pattern '--channel "\$APPCAST_CHANNEL"' "$DMG_WORKFLOW" "appcast generation must receive the selected channel"
expect_pattern 'merge_sparkle_appcast\.py' "$DMG_WORKFLOW" "appcast publication must merge the candidate with the existing feed"
expect_pattern '--base-appcast "\$APPCAST_BASE_PATH"' "$DMG_WORKFLOW" "appcast publication must preserve items from the existing feed"
expect_pattern 'git fetch --no-tags --depth=1 origin main' "$DMG_WORKFLOW" "appcast generation must fetch the current main publication base"
expect_pattern 'git show "FETCH_HEAD:\$\{FEED_PATH\}" > "\$APPCAST_BASE_PATH"' "$DMG_WORKFLOW" "appcast generation must read the feed from current main"
expect_pattern '--base-appcast "\$PUBLISH_DIR/\$FEED_PATH"' "$DMG_WORKFLOW" "appcast publication must re-merge against the freshly cloned main feed"
expect_pattern 'EXPECTED_CHANNEL="beta"' "$DMG_WORKFLOW" "RC publication verification must select the beta channel"
expect_pattern 'APPCAST_BETA_VERSION' "$PUBLISH_VERIFY_WORKFLOW" "post-publication verification must inspect GUI beta metadata"
expect_pattern 'publish_verify_prerelease_pair_state\.sh' "$PUBLISH_VERIFY_WORKFLOW" "post-publication verification must require coherent GUI and CLI prerelease metadata"
expect_pattern 'invalid\|incomplete\|mismatch' "$PUBLISH_VERIFY_WORKFLOW" "post-publication verification must reject one-sided or unexplained GUI/CLI RC drift"
expect_pattern 'pending the exact counterpart publish PR merge' "$PUBLISH_VERIFY_WORKFLOW" "post-publication verification may defer only for the exact counterpart publish PR"

expect_pattern 'runs-on: macos-26' "$CANARY_WORKFLOW" "release canary must use the supported macOS runner"
expect_pattern 'EXPECTED_XCODE_MAJOR: "26"' "$CANARY_WORKFLOW" "release canary must pin the expected Xcode major"
reject_pattern 'xcodebuild -version.*awk.*exit' "$CANARY_WORKFLOW" "release canary must not close the xcodebuild version pipe early"
expect_pattern 'cargo test --workspace --manifest-path core/rust/Cargo.toml -- --test-threads=1' "$CANARY_WORKFLOW" "release canary must run the serialized Rust release gate"
expect_pattern 'Build unsigned universal release app' "$CANARY_WORKFLOW" "release canary must build an unsigned universal app"

expect_pattern 'RELEASE_PUBLISH_PAT' "$AUTH_CHECK_WORKFLOW" "release auth check must validate the publish credential"
expect_pattern 'git ls-remote --exit-code' "$AUTH_CHECK_WORKFLOW" "release auth check must verify Git authentication"
expect_pattern 'write_probe:' "$AUTH_CHECK_WORKFLOW" "release auth check must require explicit write-probe opt-in"
expect_pattern 'repos/\$\{GITHUB_REPOSITORY\}/pulls' "$AUTH_CHECK_WORKFLOW" "release auth check must validate pull-request permission"
expect_pattern 'git/refs/heads/\$\{PROBE_BRANCH\}' "$AUTH_CHECK_WORKFLOW" "release auth check must clean up its probe branch"
expect_pattern 'GITHUB_RUN_ATTEMPT' "$AUTH_CHECK_WORKFLOW" "release auth probe branches must be unique across reruns"
expect_pattern 'trap on_exit EXIT' "$AUTH_CHECK_WORKFLOW" "release auth check must fail when probe cleanup fails"

for secret in ASC_KEY_ID ASC_ISSUER_ID ASC_PRIVATE_KEY_BASE64 RELEASE_PUBLISH_PAT; do
  expect_pattern "\"${secret}\"" "$PREFLIGHT_SCRIPT" "release preflight must require ${secret}"
done

expect_pattern 'branches: \[dev, main\]' "$WEB_BUILD_WORKFLOW" "web build must cover dev and main integration branches"
expect_pattern '"web/\*\*"' "$WEB_BUILD_WORKFLOW" "web build must filter for web paths"
expect_pattern 'actions/workflows/\$\{wf\}' "$PREFLIGHT_SCRIPT" "release preflight must query required workflow state"
expect_pattern 'required workflow is not active' "$PREFLIGHT_SCRIPT" "release preflight must reject disabled required workflows"

printf '[release-workflow-contract] passed\n'
