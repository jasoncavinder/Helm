#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PREFLIGHT_SCRIPT="${SCRIPT_DIR}/preflight.sh"

usage() {
  cat <<'EOF'
Usage:
  scripts/release/runbook.sh prepare --tag <tag> [--allow-non-main] [--allow-dirty] [--no-fetch] [--skip-secrets] [--skip-workflows] [--skip-ruleset-policy]
  scripts/release/runbook.sh tag --tag <tag>
  scripts/release/runbook.sh publish --tag <tag>
  scripts/release/runbook.sh verify --tag <tag>

Commands:
  prepare   Run release preflight checks.
  tag       Create and push an annotated release tag.
  publish   Create (or confirm) GitHub release for the tag.
  verify    Verify release assets and update metadata on main.
EOF
}

fail() {
  printf '[runbook] error: %s\n' "$1" >&2
  exit 1
}

phase_info() {
  local phase="$1"
  local message="$2"
  printf '[%s] %s\n' "$phase" "$message"
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

is_rc_tag() {
  local tag="$1"
  [[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]
}

require_tag_arg() {
  local tag="$1"
  if [ -z "$tag" ]; then
    fail "missing required --tag argument"
  fi
}

parse_common_tag_args() {
  local tag=""
  while [ $# -gt 0 ]; do
    case "$1" in
    --tag)
      [ $# -ge 2 ] || fail "--tag requires a value"
      tag="$2"
      shift 2
      ;;
    *)
      fail "unknown argument: $1"
      ;;
    esac
  done
  printf '%s' "$tag"
}

cmd_prepare() {
  local args=()
  while [ $# -gt 0 ]; do
    case "$1" in
    --tag)
      [ $# -ge 2 ] || fail "--tag requires a value"
      args+=("--tag" "$2")
      shift 2
      ;;
    --allow-non-main | --allow-dirty | --no-fetch | --skip-secrets | --skip-workflows | --skip-ruleset-policy)
      args+=("$1")
      shift
      ;;
    *)
      fail "unknown argument for prepare: $1"
      ;;
    esac
  done

  phase_info "preflight" "running release preflight checks"
  "${PREFLIGHT_SCRIPT}" "${args[@]}"
}

cmd_tag() {
  local tag
  tag="$(parse_common_tag_args "$@")"
  require_tag_arg "$tag"

  phase_info "preflight" "running release preflight for ${tag}"
  "${PREFLIGHT_SCRIPT}" --tag "$tag"

  phase_info "publish" "creating annotated tag ${tag}"
  git tag -a "$tag" -m "Helm ${tag#v}"

  phase_info "publish" "pushing tag ${tag}"
  git push origin "$tag"
}

cmd_publish() {
  local tag
  tag="$(parse_common_tag_args "$@")"
  require_tag_arg "$tag"

  phase_info "preflight" "running release preflight for ${tag}"
  "${PREFLIGHT_SCRIPT}" --tag "$tag" --allow-non-main --allow-existing-tag

  if gh release view "$tag" >/dev/null 2>&1; then
    phase_info "publish" "GitHub release already exists for ${tag}"
    gh release view "$tag" --json url,isDraft,isPrerelease,publishedAt --jq '{url, draft:.isDraft, prerelease:.isPrerelease, published_at:.publishedAt}'
    return
  fi

  phase_info "publish" "creating GitHub release ${tag}"
  if is_rc_tag "$tag"; then
    gh release create "$tag" \
      --verify-tag \
      --title "Helm ${tag#v}" \
      --generate-notes \
      --prerelease
  else
    gh release create "$tag" \
      --verify-tag \
      --title "Helm ${tag#v}" \
      --generate-notes \
      --latest
  fi
}

release_has_assets() {
  local tag="$1"
  local csv="$2"
  local asset_names
  asset_names="$(gh release view "$tag" --json assets --jq '.assets[].name')"
  python3 - "$csv" "$asset_names" <<'PY'
import sys

required = [item for item in sys.argv[1].split(",") if item]
present = {line.strip() for line in sys.argv[2].splitlines() if line.strip()}
missing = [name for name in required if name not in present]
if missing:
    print(",".join(missing))
    raise SystemExit(1)
print("")
PY
}

extract_appcast_version() {
  local payload
  payload="$(git show origin/main:web/public/updates/appcast.xml || true)"
  APPCAST_PAYLOAD="$payload" python3 - <<'PY'
import os
import sys
import xml.etree.ElementTree as ET

sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
payload = os.environ.get("APPCAST_PAYLOAD", "")
if not payload:
    print("")
    raise SystemExit(0)

root = ET.fromstring(payload)
item = root.find("./channel/item")
if item is None:
    print("")
    raise SystemExit(0)

enclosure = item.find("enclosure")
if enclosure is not None:
    value = enclosure.attrib.get(f"{{{sparkle_ns}}}shortVersionString", "").strip()
    if value:
        print(value)
        raise SystemExit(0)

title = (item.findtext("title") or "").strip()
if title.lower().startswith("helm "):
    print(title[5:].strip())
else:
    print("")
PY
}

extract_json_version_from_main() {
  local path="$1"
  local payload
  payload="$(git show "origin/main:${path}" || true)"
  JSON_PAYLOAD="$payload" python3 - <<'PY'
import json
import os
import sys

payload = os.environ.get("JSON_PAYLOAD", "")
if not payload:
    print("")
    raise SystemExit(0)

obj = json.loads(payload)
print(str(obj.get("version", "")).strip())
PY
}

cmd_verify() {
  local tag
  tag="$(parse_common_tag_args "$@")"
  require_tag_arg "$tag"

  local expected_version="${tag#v}"
  local errors=0
  local missing=""

  phase_info "preflight" "running release preflight for ${tag}"
  "${PREFLIGHT_SCRIPT}" --tag "$tag" --allow-non-main --allow-dirty --skip-secrets --skip-workflows --allow-existing-tag

  if ! gh release view "$tag" >/dev/null 2>&1; then
    fail "GitHub release not found for ${tag}"
  fi

  if ! git fetch origin --quiet; then
    fail "failed to fetch origin before verification"
  fi

  local required_assets
  required_assets="helm-cli-${tag}-darwin-universal,helm-cli-${tag}-darwin-arm64,helm-cli-${tag}-darwin-x86_64,helm-cli-${tag}-checksums.txt,Helm-${tag}-macos-universal.dmg"
  missing="$(release_has_assets "$tag" "$required_assets" 2>/dev/null || true)"
  if [ -n "${missing:-}" ]; then
    printf '[verify] error: release %s is missing required assets: %s\n' "$tag" "$missing" >&2
    errors=$((errors + 1))
  else
    phase_info "verify" "required release assets are present"
  fi

  local cli_path cli_version
  if is_rc_tag "$tag"; then
    cli_path="web/public/updates/cli/latest-rc.json"
  else
    cli_path="web/public/updates/cli/latest.json"
  fi
  cli_version="$(extract_json_version_from_main "$cli_path" || true)"
  if [ "$cli_version" != "$expected_version" ]; then
    printf '[verify] error: %s version mismatch on main (expected=%s actual=%s)\n' "$cli_path" "$expected_version" "${cli_version:-<empty>}" >&2
    errors=$((errors + 1))
  else
    phase_info "verify" "${cli_path} matches ${expected_version}"
  fi

  if ! is_rc_tag "$tag"; then
    local appcast_version
    appcast_version="$(extract_appcast_version || true)"
    if [ "$appcast_version" != "$expected_version" ]; then
      printf '[verify] error: appcast version mismatch on main (expected=%s actual=%s)\n' "$expected_version" "${appcast_version:-<empty>}" >&2
      errors=$((errors + 1))
    else
      phase_info "verify" "appcast version matches ${expected_version}"
    fi
  else
    phase_info "verify" "rc tag detected; skipping stable appcast version verification"
  fi

  local open_publish_prs pr_json
  pr_json="$(gh pr list --state open --base main --json number,headRefName,title || true)"
  open_publish_prs="$(python3 - "$tag" "$pr_json" <<'PY'
import json
import sys

tag = sys.argv[1]
payload = json.loads(sys.argv[2] or "[]")
lines = []
for pr in payload:
    head = pr.get("headRefName", "")
    if f"chore/publish-updates-{tag}" in head or f"chore/publish-cli-updates-{tag}" in head:
        lines.append(f"{pr.get('number')}\t{head}\t{pr.get('title','')}")
print("\n".join(lines))
PY
)"
  if [ -n "$open_publish_prs" ]; then
    printf '[verify] error: open release publish PR(s) remain:\n%s\n' "$open_publish_prs" >&2
    errors=$((errors + 1))
  else
    phase_info "verify" "no open publish PRs for ${tag}"
  fi

  if [ "$errors" -gt 0 ]; then
    printf '[verify] failed with %d issue(s)\n' "$errors" >&2
    exit 1
  fi

  phase_info "verify" "verification passed for ${tag}"
}

main() {
  cd "$ROOT_DIR"
  normalize_locale_environment

  [ $# -ge 1 ] || {
    usage
    exit 1
  }

  local cmd="$1"
  shift || true

  case "$cmd" in
  prepare)
    cmd_prepare "$@"
    ;;
  tag)
    cmd_tag "$@"
    ;;
  publish)
    cmd_publish "$@"
    ;;
  verify)
    cmd_verify "$@"
    ;;
  -h | --help | help)
    usage
    ;;
  *)
    fail "unknown command: ${cmd}"
    ;;
  esac
}

main "$@"
