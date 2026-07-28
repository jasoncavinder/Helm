#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKFLOWS_DIR="${ROOT_DIR}/.github/workflows"
ALL_VARIANTS_WORKFLOW="${WORKFLOWS_DIR}/release-all-variants.yml"
CLI_WORKFLOW="${WORKFLOWS_DIR}/release-cli-direct.yml"
DMG_WORKFLOW="${WORKFLOWS_DIR}/release-macos-dmg.yml"
WEB_BUILD_WORKFLOW="${WORKFLOWS_DIR}/web-build.yml"

fail() {
  printf '[release-workflow-contract] error: %s\n' "$1" >&2
  exit 1
}

expect_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  rg -q -- "$pattern" "$file" || fail "$description"
}

reject_pattern() {
  local pattern="$1"
  local file="$2"
  local description="$3"
  if rg -q -- "$pattern" "$file"; then
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
done

expect_pattern 'branches: \[web, dev, main\]' "$WEB_BUILD_WORKFLOW" "web build must cover web, dev, and main promotion branches"
expect_pattern '"web/\*\*"' "$WEB_BUILD_WORKFLOW" "web build must filter for web paths"

printf '[release-workflow-contract] passed\n'
