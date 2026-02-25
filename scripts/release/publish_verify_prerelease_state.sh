#!/usr/bin/env bash
set -euo pipefail

RC_VERSION="${1:-}"
LATEST_PRERELEASE_VERSION="${2:-}"
OPEN_HEADS_RAW="${3:-}"

is_rc_semver() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+-rc\.[0-9]+$ ]]
}

compare_rc_semver() {
  python3 - "$1" "$2" <<'PY'
import re
import sys

pattern = re.compile(r"^(\d+)\.(\d+)\.(\d+)-rc\.(\d+)$")

def parse(value: str):
    m = pattern.match(value.strip())
    if not m:
        raise SystemExit(2)
    return tuple(int(part) for part in m.groups())

a = parse(sys.argv[1])
b = parse(sys.argv[2])
if a < b:
    print("-1")
elif a > b:
    print("1")
else:
    print("0")
PY
}

print_state() {
  local status="$1"
  local reason="$2"
  local target="$3"
  local matching="$4"
  printf 'STATUS=%s\n' "$status"
  printf 'REASON=%s\n' "$reason"
  printf 'TARGET_VERSION=%s\n' "$target"
  printf 'MATCHING_HEADS=%s\n' "$matching"
}

if [ -n "$RC_VERSION" ] && ! is_rc_semver "$RC_VERSION"; then
  print_state "invalid" "rc metadata version is not prerelease semver" "" ""
  exit 0
fi

if [ -n "$LATEST_PRERELEASE_VERSION" ] && ! is_rc_semver "$LATEST_PRERELEASE_VERSION"; then
  print_state "invalid" "latest prerelease tag version is not prerelease semver" "" ""
  exit 0
fi

if [ -z "$RC_VERSION" ] && [ -z "$LATEST_PRERELEASE_VERSION" ]; then
  print_state "synced" "no prerelease metadata and no prerelease release tag" "" ""
  exit 0
fi

if [ "$RC_VERSION" = "$LATEST_PRERELEASE_VERSION" ]; then
  print_state "synced" "prerelease metadata and latest prerelease tag match" "$RC_VERSION" ""
  exit 0
fi

TARGET_VERSION="$RC_VERSION"
if [ -z "$TARGET_VERSION" ]; then
  TARGET_VERSION="$LATEST_PRERELEASE_VERSION"
elif [ -n "$LATEST_PRERELEASE_VERSION" ]; then
  cmp="$(compare_rc_semver "$RC_VERSION" "$LATEST_PRERELEASE_VERSION")"
  if [ "$cmp" = "-1" ]; then
    TARGET_VERSION="$LATEST_PRERELEASE_VERSION"
  fi
fi

EXPECTED_HEAD="chore/publish-cli-updates-v${TARGET_VERSION}-rc"
MATCHING_HEADS=()
while IFS= read -r head; do
  [ -n "$head" ] || continue
  if [ "$head" = "$EXPECTED_HEAD" ]; then
    MATCHING_HEADS+=("$head")
  fi
done <<< "$OPEN_HEADS_RAW"

if [ "${#MATCHING_HEADS[@]}" -gt 0 ]; then
  IFS=,
  print_state "pending" "prerelease metadata mismatch is pending open publish PR merge" "$TARGET_VERSION" "${MATCHING_HEADS[*]}"
  exit 0
fi

print_state "mismatch" "prerelease metadata mismatch has no open publish PR counterpart" "$TARGET_VERSION" ""

