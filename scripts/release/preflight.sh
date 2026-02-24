#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REQUIRE_MAIN=1
REQUIRE_CLEAN=1
FETCH_REMOTE=1
CHECK_SECRETS=1
CHECK_WORKFLOWS=1
ALLOW_EXISTING_TAG=0
TAG_NAME=""
ERROR_COUNT=0
WARN_COUNT=0

usage() {
  cat <<'EOF'
Usage:
  scripts/release/preflight.sh [options]

Options:
  --tag <vX.Y.Z|vX.Y.Z-rc.N>  Validate the provided tag format and ensure it does not already exist.
  --allow-non-main            Do not require current branch to be main.
  --allow-dirty               Do not require a clean git working tree.
  --no-fetch                  Skip git fetch origin.
  --skip-secrets              Skip GitHub Actions secret presence checks.
  --skip-workflows            Skip GitHub workflow presence checks.
  --allow-existing-tag        Allow tag to already exist locally/remotely.
  -h, --help                  Show this help.

This script validates release prerequisites before tagging/publishing.
EOF
}

info() {
  printf '[preflight] %s\n' "$1"
}

warn() {
  WARN_COUNT=$((WARN_COUNT + 1))
  printf '[preflight] warning: %s\n' "$1" >&2
}

fail() {
  ERROR_COUNT=$((ERROR_COUNT + 1))
  printf '[preflight] error: %s\n' "$1" >&2
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    fail "required tool not found: ${tool}"
    return 1
  fi
  return 0
}

validate_tag_format() {
  local tag="$1"
  if [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    return 0
  fi
  if [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]; then
    return 0
  fi
  fail "unsupported tag format '${tag}' (expected vX.Y.Z or vX.Y.Z-rc.N)"
  return 1
}

check_git_state() {
  if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    fail "current directory is not inside a git worktree"
    return
  fi

  if [ "$FETCH_REMOTE" -eq 1 ]; then
    if ! git fetch origin --quiet; then
      fail "failed to fetch origin"
    else
      info "fetched origin"
    fi
  fi

  local branch
  branch="$(git branch --show-current)"
  info "current branch: ${branch}"

  if [ "$REQUIRE_MAIN" -eq 1 ] && [ "$branch" != "main" ]; then
    fail "release preflight requires main branch (use --allow-non-main to override)"
  fi

  if [ "$REQUIRE_CLEAN" -eq 1 ]; then
    local dirty=0
    if ! git diff --quiet --ignore-submodules --; then
      dirty=1
    fi
    if ! git diff --cached --quiet --ignore-submodules --; then
      dirty=1
    fi
    if [ -n "$(git ls-files --others --exclude-standard)" ]; then
      dirty=1
    fi
    if [ "$dirty" -eq 1 ]; then
      fail "working tree is not clean"
      git status --short >&2
    fi
  fi

  local compare_ref=""
  if [ "$REQUIRE_MAIN" -eq 1 ]; then
    compare_ref="origin/main"
  else
    compare_ref="$(git rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)"
    if [ -z "$compare_ref" ]; then
      compare_ref="origin/main"
    fi
  fi

  if git rev-parse --verify --quiet "$compare_ref" >/dev/null; then
    local ahead behind
    read -r ahead behind <<<"$(git rev-list --left-right --count "HEAD...${compare_ref}")"
    if [ "$behind" -gt 0 ]; then
      if [ "$REQUIRE_MAIN" -eq 1 ]; then
        fail "local HEAD is behind ${compare_ref} by ${behind} commit(s)"
      else
        warn "local HEAD is behind ${compare_ref} by ${behind} commit(s)"
      fi
    fi
    if [ "$ahead" -gt 0 ]; then
      warn "local HEAD is ahead of ${compare_ref} by ${ahead} commit(s)"
    fi
  else
    warn "reference '${compare_ref}' not found; skipping ahead/behind check"
  fi
}

check_tag_availability() {
  if [ -z "$TAG_NAME" ]; then
    return
  fi

  if ! validate_tag_format "$TAG_NAME"; then
    return
  fi

  if [ "$ALLOW_EXISTING_TAG" -eq 1 ]; then
    return
  fi

  if git show-ref --tags --verify --quiet "refs/tags/${TAG_NAME}"; then
    fail "tag already exists locally: ${TAG_NAME}"
  fi

  if git ls-remote --exit-code --tags origin "refs/tags/${TAG_NAME}" >/dev/null 2>&1; then
    fail "tag already exists on origin: ${TAG_NAME}"
  fi
}

parse_scopes() {
  local raw="$1"
  printf '%s' "$raw" | tr '[:upper:]' '[:lower:]' | tr ',' ' ' | xargs
}

scope_contains() {
  local haystack="$1"
  local needle="$2"
  if printf ' %s ' "$haystack" | grep -Fq " ${needle} "; then
    return 0
  fi
  return 1
}

check_github_auth() {
  if ! gh auth status >/dev/null 2>&1; then
    fail "gh auth status failed; authenticate with a maintainer token first"
    return
  fi
  info "gh authentication detected"

  local api_headers scope_header scopes
  if ! api_headers="$(gh api -i user 2>/dev/null || true)"; then
    warn "unable to query GitHub API headers for scope verification"
    return
  fi

  scope_header="$(printf '%s\n' "$api_headers" | awk 'BEGIN{IGNORECASE=1}/^x-oauth-scopes:/{sub(/\r$/,"",$0); sub(/^[^:]*:[[:space:]]*/,"",$0); print; exit}')"
  if [ -z "$scope_header" ]; then
    warn "x-oauth-scopes header missing; unable to validate token scopes automatically"
    return
  fi

  scopes="$(parse_scopes "$scope_header")"
  info "token scopes: ${scope_header}"

  if ! scope_contains "$scopes" "repo"; then
    fail "token is missing required scope: repo"
  fi
  if ! scope_contains "$scopes" "workflow"; then
    fail "token is missing required scope: workflow"
  fi
}

check_required_workflows() {
  if [ "$CHECK_WORKFLOWS" -ne 1 ]; then
    return
  fi

  local required_workflows=(
    "release-macos-dmg.yml"
    "release-cli-direct.yml"
    "release-all-variants.yml"
    "appcast-drift.yml"
    "cli-update-drift.yml"
  )

  local wf
  for wf in "${required_workflows[@]}"; do
    if ! gh workflow view "$wf" >/dev/null 2>&1; then
      fail "required workflow not found or inaccessible: ${wf}"
    fi
  done
}

check_required_secrets() {
  if [ "$CHECK_SECRETS" -ne 1 ]; then
    return
  fi

  local secrets_json
  if ! secrets_json="$(gh secret list --json name --limit 200 2>/dev/null)"; then
    fail "unable to list GitHub repository secrets"
    return
  fi

  local missing
  missing="$(python3 - "$secrets_json" <<'PY'
import json
import sys

required = [
    "APPLE_TEAM_ID",
    "MACOS_DEVELOPER_ID_APP_CERT_BASE64",
    "MACOS_DEVELOPER_ID_APP_CERT_PASSWORD",
    "MACOS_KEYCHAIN_PASSWORD",
    "HELM_SPARKLE_FEED_URL",
    "HELM_SPARKLE_PUBLIC_ED_KEY",
    "HELM_SPARKLE_PRIVATE_ED_KEY",
]

payload = json.loads(sys.argv[1])
present = {item["name"] for item in payload}
missing = [name for name in required if name not in present]
print(",".join(missing))
PY
)"

  if [ -n "$missing" ]; then
    fail "missing required GitHub secrets: ${missing}"
  else
    info "required release secrets are present"
  fi
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
    --tag)
      if [ $# -lt 2 ]; then
        fail "--tag requires a value"
        return 1
      fi
      TAG_NAME="$2"
      shift 2
      ;;
    --allow-non-main)
      REQUIRE_MAIN=0
      shift
      ;;
    --allow-dirty)
      REQUIRE_CLEAN=0
      shift
      ;;
    --no-fetch)
      FETCH_REMOTE=0
      shift
      ;;
    --skip-secrets)
      CHECK_SECRETS=0
      shift
      ;;
    --skip-workflows)
      CHECK_WORKFLOWS=0
      shift
      ;;
    --allow-existing-tag)
      ALLOW_EXISTING_TAG=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      return 1
      ;;
    esac
  done
  return 0
}

main() {
  cd "$ROOT_DIR"

  parse_args "$@" || true

  require_tool git || true
  require_tool gh || true
  require_tool python3 || true

  check_git_state
  check_tag_availability
  check_github_auth
  check_required_workflows
  check_required_secrets

  if [ "$ERROR_COUNT" -gt 0 ]; then
    printf '[preflight] failed: %d error(s), %d warning(s)\n' "$ERROR_COUNT" "$WARN_COUNT" >&2
    exit 1
  fi

  printf '[preflight] passed: %d warning(s)\n' "$WARN_COUNT"
}

main "$@"
