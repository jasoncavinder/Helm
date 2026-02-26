#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/publish_verify_prerelease_state.sh"

fail() {
  printf '[publish-verify-prerelease-state-contract] error: %s\n' "$1" >&2
  exit 1
}

field_value() {
  local payload="$1"
  local key="$2"
  printf '%s\n' "$payload" | awk -F= -v key="$key" '$1==key {print substr($0, index($0, "=") + 1)}'
}

assert_field() {
  local payload="$1"
  local key="$2"
  local expected="$3"
  local actual
  actual="$(field_value "$payload" "$key")"
  if [ "$actual" != "$expected" ]; then
    fail "expected ${key}=${expected}, got ${actual:-<empty>}"
  fi
}

case_synced_none="$("$SCRIPT_PATH" "" "" "")"
assert_field "$case_synced_none" "STATUS" "synced"
assert_field "$case_synced_none" "TARGET_VERSION" ""

case_synced="$("$SCRIPT_PATH" "0.18.0-rc.2" "0.18.0-rc.2" "")"
assert_field "$case_synced" "STATUS" "synced"
assert_field "$case_synced" "TARGET_VERSION" "0.18.0-rc.2"

case_pending_metadata_behind="$("$SCRIPT_PATH" "0.18.0-rc.1" "0.18.0-rc.2" "chore/publish-cli-updates-v0.18.0-rc.2-rc")"
assert_field "$case_pending_metadata_behind" "STATUS" "pending"
assert_field "$case_pending_metadata_behind" "TARGET_VERSION" "0.18.0-rc.2"
assert_field "$case_pending_metadata_behind" "MATCHING_HEADS" "chore/publish-cli-updates-v0.18.0-rc.2-rc"

case_pending_missing_metadata="$("$SCRIPT_PATH" "" "0.18.0-rc.3" "chore/publish-cli-updates-v0.18.0-rc.3-rc")"
assert_field "$case_pending_missing_metadata" "STATUS" "pending"
assert_field "$case_pending_missing_metadata" "TARGET_VERSION" "0.18.0-rc.3"

case_mismatch="$("$SCRIPT_PATH" "0.18.0-rc.1" "0.18.0-rc.2" "")"
assert_field "$case_mismatch" "STATUS" "mismatch"
assert_field "$case_mismatch" "TARGET_VERSION" "0.18.0-rc.2"

case_invalid="$("$SCRIPT_PATH" "0.18.0" "0.18.0-rc.2" "")"
assert_field "$case_invalid" "STATUS" "invalid"

printf '[publish-verify-prerelease-state-contract] passed\n'

