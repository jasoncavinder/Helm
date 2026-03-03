#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

REQUIRE_MAIN=1
REQUIRE_CLEAN=1
FETCH_REMOTE=1
CHECK_SECRETS=1
CHECK_WORKFLOWS=1
CHECK_RULESET_POLICY=1
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
  --skip-ruleset-policy       Skip main-branch ruleset bypass policy checks.
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

normalize_locale_environment() {
  local available selected=""
  available="$(locale -a 2>/dev/null | tr '[:upper:]' '[:lower:]' || true)"

  if printf '%s\n' "$available" | grep -Eq '^en_us\.(utf-8|utf8)$'; then
    selected="en_US.UTF-8"
  elif printf '%s\n' "$available" | grep -Eq '^c\.(utf-8|utf8)$'; then
    selected="C.UTF-8"
  fi

  if [ -n "$selected" ]; then
    export LANG="$selected"
    export LC_ALL="$selected"
    return
  fi

  export LANG="C"
  unset LC_ALL || true
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

extract_appcast_version_from_origin_main() {
  local payload
  payload="$(git show "origin/main:web/public/updates/appcast.xml" 2>/dev/null || true)"
  APPCAST_PAYLOAD="$payload" python3 - <<'PY'
import os
import xml.etree.ElementTree as ET

sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
payload = os.environ.get("APPCAST_PAYLOAD", "").strip()
if not payload:
    print("")
    raise SystemExit(0)

try:
    root = ET.fromstring(payload)
except ET.ParseError:
    print("")
    raise SystemExit(0)

item = root.find("./channel/item")
if item is None:
    print("")
    raise SystemExit(0)

enclosure = item.find("enclosure")
if enclosure is not None:
    version = enclosure.attrib.get(f"{{{sparkle_ns}}}shortVersionString", "").strip()
    if version:
        print(version)
        raise SystemExit(0)

title = (item.findtext("title") or "").strip()
if title.lower().startswith("helm "):
    print(title[5:].strip())
else:
    print("")
PY
}

extract_json_version_from_origin_main() {
  local path="$1"
  local payload
  payload="$(git show "origin/main:${path}" 2>/dev/null || true)"
  JSON_PAYLOAD="$payload" python3 - <<'PY'
import json
import os

payload = os.environ.get("JSON_PAYLOAD", "").strip()
if not payload:
    print("")
    raise SystemExit(0)

try:
    obj = json.loads(payload)
except json.JSONDecodeError:
    print("")
    raise SystemExit(0)

print(str(obj.get("version", "")).strip())
PY
}

compare_stable_versions() {
  local a="$1"
  local b="$2"
  python3 - "$a" "$b" <<'PY'
import re
import sys

def parse(value: str):
    if not re.fullmatch(r"\d+\.\d+\.\d+", value):
        return None
    return tuple(int(part) for part in value.split("."))

a = parse(sys.argv[1].strip())
b = parse(sys.argv[2].strip())
if a is None or b is None:
    print("invalid")
    raise SystemExit(0)

if a < b:
    print("-1")
elif a > b:
    print("1")
else:
    print("0")
PY
}

check_pre_tag_metadata_snapshot() {
  if [ -z "$TAG_NAME" ]; then
    return
  fi

  if [[ "$TAG_NAME" =~ -rc\.[0-9]+$ ]]; then
    info "rc tag detected (${TAG_NAME}); skipping stable metadata snapshot ordering checks"
    return
  fi

  if ! git rev-parse --verify --quiet origin/main >/dev/null; then
    if [ "$FETCH_REMOTE" -eq 0 ]; then
      warn "origin/main is unavailable in --no-fetch mode; skipping stable metadata snapshot ordering checks"
      return
    fi
    fail "origin/main is unavailable; cannot validate stable metadata snapshot ordering"
    return
  fi

  local expected_version appcast_version cli_version compare_result
  expected_version="${TAG_NAME#v}"
  appcast_version="$(extract_appcast_version_from_origin_main)"
  cli_version="$(extract_json_version_from_origin_main "web/public/updates/cli/latest.json")"

  if [ -z "$appcast_version" ]; then
    fail "unable to read top appcast stable version from origin/main:web/public/updates/appcast.xml"
    return
  fi

  if [ -z "$cli_version" ]; then
    fail "unable to read stable CLI metadata version from origin/main:web/public/updates/cli/latest.json"
    return
  fi

  info "pre-tag metadata snapshot: appcast=${appcast_version}, cli_latest=${cli_version}, target=${expected_version}"

  if [ "$appcast_version" != "$cli_version" ]; then
    fail "stable metadata on origin/main is not synchronized (appcast=${appcast_version}, cli_latest=${cli_version})"
    return
  fi

  compare_result="$(compare_stable_versions "$appcast_version" "$expected_version")"
  case "$compare_result" in
  -1)
    info "stable metadata on origin/main is behind target tag (${appcast_version} < ${expected_version})"
    ;;
  0)
    fail "stable metadata on origin/main already matches target version ${expected_version}; choose the next tag or resolve existing publication state"
    ;;
  1)
    fail "stable metadata on origin/main is ahead of target version (${appcast_version} > ${expected_version})"
    ;;
  *)
    fail "unable to compare stable metadata snapshot (${appcast_version}) with target version (${expected_version}); expected stable semver X.Y.Z"
    ;;
  esac
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
  local auth_status_output=""
  if ! auth_status_output="$(gh auth status 2>&1)"; then
    auth_status_output="$(printf '%s' "$auth_status_output" | tr '\n' ' ' | xargs || true)"
    fail "gh auth status failed: ${auth_status_output:-unknown error}"
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

check_main_ruleset_policy() {
  if [ "$CHECK_RULESET_POLICY" -ne 1 ]; then
    return
  fi

  local repo
  local repo_output=""
  if ! repo_output="$(gh repo view --json nameWithOwner --jq '.nameWithOwner' 2>&1)"; then
    repo_output="$(printf '%s' "$repo_output" | tr '\n' ' ' | xargs || true)"
    fail "unable to resolve repository slug via gh for ruleset policy checks: ${repo_output:-unknown error}"
    return
  fi
  repo="$repo_output"

  local ruleset_json
  local ruleset_ids
  if ! ruleset_ids="$(gh api "repos/${repo}/rulesets" --jq '.[].id' 2>/dev/null)"; then
    fail "unable to query repository rulesets for main-branch policy checks"
    return
  fi

  if [ -z "$ruleset_ids" ]; then
    fail "no repository rulesets returned for policy checks"
    return
  fi

  ruleset_json=""
  local ruleset_id
  local ruleset_detail
  local includes_main
  for ruleset_id in $ruleset_ids; do
    ruleset_detail="$(gh api "repos/${repo}/rulesets/${ruleset_id}" 2>/dev/null || true)"
    if [ -z "$ruleset_detail" ]; then
      continue
    fi

    includes_main="$(
      python3 - "$ruleset_detail" <<'PY'
import json
import sys

payload = json.loads(sys.argv[1])
conditions = payload.get("conditions") or {}
ref_name = conditions.get("ref_name") or {}
includes = ref_name.get("include") or []
print("yes" if "refs/heads/main" in includes else "no")
PY
    )"

    if [ "$includes_main" = "yes" ]; then
      ruleset_json="$ruleset_detail"
      break
    fi
  done

  if [ -z "$ruleset_json" ] || [ "$ruleset_json" = "null" ]; then
    fail "no main-branch ruleset found for policy checks"
    return
  fi

  local policy_result
  if ! policy_result="$(python3 - "$ruleset_json" <<'PY'
import json
import sys

ruleset = json.loads(sys.argv[1])
bypass_actors = ruleset.get("bypass_actors") or []
rules = ruleset.get("rules") or []

rule_types = {rule.get("type") for rule in rules}
required_contexts = []
for rule in rules:
    if rule.get("type") == "required_status_checks":
        for check in (rule.get("parameters", {}).get("required_status_checks") or []):
            context = (check or {}).get("context", "")
            if context:
                required_contexts.append(context)

advisory_contexts = {
    "Release Publish Verify",
    "Appcast Drift Guard",
    "CLI Update Metadata Drift Guard",
}
required_advisory_contexts = sorted(set(required_contexts).intersection(advisory_contexts))

has_actions_pull_request_bypass = any(
    actor.get("actor_type") == "Integration"
    and actor.get("actor_id") == 15368
    and actor.get("bypass_mode") == "pull_request"
    for actor in bypass_actors
)
has_repo_role_pull_request_bypass = any(
    actor.get("actor_type") == "RepositoryRole" and actor.get("bypass_mode") == "pull_request"
    for actor in bypass_actors
)

has_any_always_bypass = any(actor.get("bypass_mode") == "always" for actor in bypass_actors)
has_repo_role_always_bypass = any(
    actor.get("actor_type") == "RepositoryRole" and actor.get("bypass_mode") == "always"
    for actor in bypass_actors
)

print(f"ruleset_id={ruleset.get('id', '')}")
print(f"ruleset_name={ruleset.get('name', '')}")
print(f"has_pull_request_rule={'yes' if 'pull_request' in rule_types else 'no'}")
print(f"has_required_status_checks_rule={'yes' if 'required_status_checks' in rule_types else 'no'}")
print(f"has_policy_gate_check={'yes' if 'Policy Gate' in required_contexts else 'no'}")
print(f"has_actions_pull_request_bypass={'yes' if has_actions_pull_request_bypass else 'no'}")
print(f"has_repo_role_pull_request_bypass={'yes' if has_repo_role_pull_request_bypass else 'no'}")
print(f"has_any_always_bypass={'yes' if has_any_always_bypass else 'no'}")
print(f"has_repo_role_always_bypass={'yes' if has_repo_role_always_bypass else 'no'}")
print(f"has_advisory_checks_required={'yes' if required_advisory_contexts else 'no'}")
print(f"required_advisory_checks={','.join(required_advisory_contexts)}")
PY
)"; then
    fail "unable to evaluate main-branch ruleset policy details"
    return
  fi

  local ruleset_id=""
  local ruleset_name=""
  local has_pull_request_rule=""
  local has_required_status_checks_rule=""
  local has_policy_gate_check=""
  local has_actions_pull_request_bypass=""
  local has_repo_role_pull_request_bypass=""
  local has_any_always_bypass=""
  local has_repo_role_always_bypass=""
  local has_advisory_checks_required=""
  local required_advisory_checks=""

  while IFS='=' read -r key value; do
    case "$key" in
    ruleset_id)
      ruleset_id="$value"
      ;;
    ruleset_name)
      ruleset_name="$value"
      ;;
    has_pull_request_rule)
      has_pull_request_rule="$value"
      ;;
    has_required_status_checks_rule)
      has_required_status_checks_rule="$value"
      ;;
    has_policy_gate_check)
      has_policy_gate_check="$value"
      ;;
    has_actions_pull_request_bypass)
      has_actions_pull_request_bypass="$value"
      ;;
    has_repo_role_pull_request_bypass)
      has_repo_role_pull_request_bypass="$value"
      ;;
    has_any_always_bypass)
      has_any_always_bypass="$value"
      ;;
    has_repo_role_always_bypass)
      has_repo_role_always_bypass="$value"
      ;;
    has_advisory_checks_required)
      has_advisory_checks_required="$value"
      ;;
    required_advisory_checks)
      required_advisory_checks="$value"
      ;;
    esac
  done <<<"$policy_result"

  info "main ruleset policy check: id=${ruleset_id} name='${ruleset_name}'"

  if [ "$has_pull_request_rule" != "yes" ]; then
    fail "main ruleset is missing pull_request enforcement rule"
  fi
  if [ "$has_required_status_checks_rule" != "yes" ]; then
    fail "main ruleset is missing required_status_checks enforcement rule"
  fi
  if [ "$has_policy_gate_check" != "yes" ]; then
    fail "main ruleset required checks do not include 'Policy Gate'"
  fi
  if [ "$has_any_always_bypass" = "yes" ]; then
    fail "main ruleset has broad bypass actor(s) in always mode; use pull_request mode instead"
  fi
  if [ "$has_repo_role_always_bypass" = "yes" ]; then
    fail "main ruleset has RepositoryRole bypass in always mode; set to pull_request or remove"
  fi
  if [ "$has_advisory_checks_required" = "yes" ]; then
    fail "main ruleset should not require advisory post-publish checks (${required_advisory_checks}); keep required checks limited to merge-gating checks."
  fi
  if [ "$has_actions_pull_request_bypass" != "yes" ] && [ "$has_repo_role_pull_request_bypass" != "yes" ]; then
    fail "main ruleset requires pull_request-only bypass policy (GitHub Actions integration actor or RepositoryRole pull_request bypass)"
  fi

  if [ "$has_actions_pull_request_bypass" = "yes" ]; then
    info "main ruleset includes GitHub Actions integration bypass in pull_request mode"
  elif [ "$has_repo_role_pull_request_bypass" = "yes" ]; then
    warn "main ruleset uses RepositoryRole pull_request bypass fallback; GitHub Actions integration bypass is unavailable in some repository configurations"
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
  if ! secrets_json="$(gh secret list --json name 2>/dev/null)"; then
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
    --skip-ruleset-policy)
      CHECK_RULESET_POLICY=0
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
  normalize_locale_environment

  require_tool git || true
  require_tool gh || true
  require_tool python3 || true

  check_git_state
  check_tag_availability
  check_pre_tag_metadata_snapshot
  check_github_auth
  check_main_ruleset_policy
  check_required_workflows
  check_required_secrets

  if [ "$ERROR_COUNT" -gt 0 ]; then
    printf '[preflight] failed: %d error(s), %d warning(s)\n' "$ERROR_COUNT" "$WARN_COUNT" >&2
    exit 1
  fi

  printf '[preflight] passed: %d warning(s)\n' "$WARN_COUNT"
}

main "$@"
