#!/usr/bin/env bash
set -euo pipefail

TAG_NAME=""
EVENT_PRERELEASE=""
EVENT_PRERELEASE_SET=0

usage() {
  cat <<'EOF'
Usage:
  scripts/release/validate_github_release_state.sh --tag <vX.Y.Z|vX.Y.Z-rc.N> [--event-prerelease <true|false>]

Validates that a GitHub release is published and matches Helm's stable/RC model:
  - stable tags are non-prerelease releases and own releases/latest
  - RC tags are prerelease releases and never own releases/latest
EOF
}

fail() {
  printf '[release-state] error: %s\n' "$1" >&2
  exit 1
}

while [ $# -gt 0 ]; do
  case "$1" in
    --tag)
      [ $# -ge 2 ] || fail "--tag requires a value"
      TAG_NAME="$2"
      shift 2
      ;;
    --event-prerelease)
      [ $# -ge 2 ] || fail "--event-prerelease requires a value"
      EVENT_PRERELEASE="$2"
      EVENT_PRERELEASE_SET=1
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[ -n "$TAG_NAME" ] || fail "missing required --tag"

EXPECTED_PRERELEASE=""
RELEASE_CHANNEL=""
if [[ "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  EXPECTED_PRERELEASE="false"
  RELEASE_CHANNEL="stable"
elif [[ "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]; then
  EXPECTED_PRERELEASE="true"
  RELEASE_CHANNEL="rc"
else
  fail "unsupported tag format '${TAG_NAME}' (expected vX.Y.Z or vX.Y.Z-rc.N)"
fi

if [ "$EVENT_PRERELEASE_SET" -eq 1 ]; then
  case "$EVENT_PRERELEASE" in
    true|false) ;;
    *) fail "release-event prerelease state must be true or false (actual=${EVENT_PRERELEASE:-<empty>})" ;;
  esac
  if [ "$EVENT_PRERELEASE" != "$EXPECTED_PRERELEASE" ]; then
    fail "tag ${TAG_NAME} requires prerelease=${EXPECTED_PRERELEASE}, but the release event reported prerelease=${EVENT_PRERELEASE}"
  fi
fi

if ! RELEASE_JSON="$(gh release view "$TAG_NAME" --json tagName,url,isDraft,isPrerelease,publishedAt)"; then
  fail "GitHub release not found for ${TAG_NAME}"
fi

IFS=$'\t' read -r ACTUAL_TAG RELEASE_URL RELEASE_DRAFT RELEASE_PRERELEASE PUBLISHED_AT <<< "$(
  RELEASE_JSON="$RELEASE_JSON" python3 - <<'PY'
import json
import os

release = json.loads(os.environ["RELEASE_JSON"])
values = (
    str(release.get("tagName") or ""),
    str(release.get("url") or ""),
    "true" if release.get("isDraft") else "false",
    "true" if release.get("isPrerelease") else "false",
    str(release.get("publishedAt") or ""),
)
print("\t".join(values))
PY
)"

[ "$ACTUAL_TAG" = "$TAG_NAME" ] || fail "release tag mismatch (expected=${TAG_NAME} actual=${ACTUAL_TAG:-<empty>})"
[ "$RELEASE_DRAFT" = "false" ] || fail "release ${TAG_NAME} is still a draft"
[ -n "$PUBLISHED_AT" ] || fail "release ${TAG_NAME} is not published"
if [ "$RELEASE_PRERELEASE" != "$EXPECTED_PRERELEASE" ]; then
  fail "release ${TAG_NAME} has prerelease=${RELEASE_PRERELEASE}; expected ${EXPECTED_PRERELEASE} for the ${RELEASE_CHANNEL} channel"
fi

LATEST_TAG=""
if ! LATEST_TAG="$(gh api "repos/{owner}/{repo}/releases/latest" --jq '.tag_name')"; then
  fail "unable to resolve GitHub releases/latest"
fi
if [ "$LATEST_TAG" = "null" ]; then
  LATEST_TAG=""
fi

if [ "$RELEASE_CHANNEL" = "stable" ]; then
  [ "$LATEST_TAG" = "$TAG_NAME" ] || fail "stable release ${TAG_NAME} must own releases/latest (actual=${LATEST_TAG:-<none>})"
elif [ "$LATEST_TAG" = "$TAG_NAME" ]; then
  fail "RC release ${TAG_NAME} must never own releases/latest"
fi

printf '[release-state] validated channel=%s tag=%s prerelease=%s latest=%s published_at=%s url=%s\n' \
  "$RELEASE_CHANNEL" \
  "$TAG_NAME" \
  "$RELEASE_PRERELEASE" \
  "${LATEST_TAG:-<none>}" \
  "$PUBLISHED_AT" \
  "$RELEASE_URL"
