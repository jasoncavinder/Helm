#!/usr/bin/env bash
set -euo pipefail

GUI_VERSION="${1:-}"
CLI_VERSION="${2:-}"
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
    match = pattern.match(value.strip())
    if not match:
        raise SystemExit(2)
    return tuple(int(part) for part in match.groups())

left = parse(sys.argv[1])
right = parse(sys.argv[2])
print(-1 if left < right else 1 if left > right else 0)
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

if [ -n "$GUI_VERSION" ] && ! is_rc_semver "$GUI_VERSION"; then
  print_state "invalid" "GUI beta version is not prerelease semver" "" ""
  exit 0
fi

if [ -n "$CLI_VERSION" ] && ! is_rc_semver "$CLI_VERSION"; then
  print_state "invalid" "CLI rc version is not prerelease semver" "" ""
  exit 0
fi

if [ -z "$GUI_VERSION" ] && [ -z "$CLI_VERSION" ]; then
  print_state "empty" "no prerelease GUI or CLI metadata" "" ""
  exit 0
fi

if [ "$GUI_VERSION" = "$CLI_VERSION" ]; then
  print_state "synced" "prerelease GUI and CLI metadata match" "$GUI_VERSION" ""
  exit 0
fi

TARGET_VERSION="$GUI_VERSION"
if [ -z "$TARGET_VERSION" ]; then
  TARGET_VERSION="$CLI_VERSION"
elif [ -n "$CLI_VERSION" ] && [ "$(compare_rc_semver "$GUI_VERSION" "$CLI_VERSION")" = "-1" ]; then
  TARGET_VERSION="$CLI_VERSION"
fi

EXPECTED_HEADS=()
if [ "$GUI_VERSION" != "$TARGET_VERSION" ]; then
  EXPECTED_HEADS+=("chore/publish-updates-v${TARGET_VERSION}")
fi
if [ "$CLI_VERSION" != "$TARGET_VERSION" ]; then
  EXPECTED_HEADS+=("chore/publish-cli-updates-v${TARGET_VERSION}-rc")
fi

MATCHING_HEADS=()
while IFS= read -r head; do
  [ -n "$head" ] || continue
  for expected_head in "${EXPECTED_HEADS[@]}"; do
    if [ "$head" = "$expected_head" ]; then
      MATCHING_HEADS+=("$head")
      break
    fi
  done
done <<< "$OPEN_HEADS_RAW"

if [ "${#MATCHING_HEADS[@]}" -gt 0 ]; then
  MATCHING_HEADS_CSV="$(IFS=,; printf '%s' "${MATCHING_HEADS[*]}")"
  print_state "pending" "prerelease GUI/CLI gap is pending the exact counterpart publish PR" "$TARGET_VERSION" "$MATCHING_HEADS_CSV"
  exit 0
fi

if [ -z "$GUI_VERSION" ] || [ -z "$CLI_VERSION" ]; then
  print_state "incomplete" "only one prerelease metadata surface is published" "$TARGET_VERSION" ""
  exit 0
fi

print_state "mismatch" "prerelease GUI and CLI versions differ without the exact counterpart publish PR" "$TARGET_VERSION" ""
