#!/usr/bin/env bash
set -euo pipefail

APPCAST_VERSION="${1:-}"
CLI_VERSION="${2:-}"
OPEN_HEADS_RAW="${3:-}"

is_semver() {
  [[ "$1" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]
}

compare_semver() {
  python3 - "$1" "$2" <<'PY'
import sys

def parse(value: str):
    return tuple(int(part) for part in value.split("."))

a = parse(sys.argv[1].strip())
b = parse(sys.argv[2].strip())
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

if ! is_semver "$APPCAST_VERSION"; then
  print_state "invalid" "appcast version is not stable semver" "" ""
  exit 0
fi

if ! is_semver "$CLI_VERSION"; then
  print_state "invalid" "stable CLI metadata version is not stable semver" "" ""
  exit 0
fi

if [ "$APPCAST_VERSION" = "$CLI_VERSION" ]; then
  print_state "synced" "stable metadata versions match" "$APPCAST_VERSION" ""
  exit 0
fi

cmp="$(compare_semver "$APPCAST_VERSION" "$CLI_VERSION")"
if [ "$cmp" = "-1" ]; then
  TARGET_VERSION="$CLI_VERSION"
elif [ "$cmp" = "1" ]; then
  TARGET_VERSION="$APPCAST_VERSION"
else
  TARGET_VERSION="$APPCAST_VERSION"
fi

EXPECTED_UPDATES_HEAD="chore/publish-updates-v${TARGET_VERSION}"
EXPECTED_CLI_HEAD="chore/publish-cli-updates-v${TARGET_VERSION}-stable"
MATCHING_HEADS=()

while IFS= read -r head; do
  [ -n "$head" ] || continue
  if [ "$head" = "$EXPECTED_UPDATES_HEAD" ] || [ "$head" = "$EXPECTED_CLI_HEAD" ]; then
    MATCHING_HEADS+=("$head")
  fi
done <<< "$OPEN_HEADS_RAW"

if [ "${#MATCHING_HEADS[@]}" -gt 0 ]; then
  IFS=,
  print_state "pending" "stable metadata mismatch is pending open publish PR merge" "$TARGET_VERSION" "${MATCHING_HEADS[*]}"
  exit 0
fi

print_state "mismatch" "stable metadata mismatch has no open publish PR counterpart" "$TARGET_VERSION" ""

