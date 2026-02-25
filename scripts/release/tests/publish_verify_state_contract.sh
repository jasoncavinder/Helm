#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/publish_verify_state.sh"

fail() {
  printf '[publish-verify-state-contract] error: %s\n' "$1" >&2
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

case_synced="$("$SCRIPT_PATH" "0.17.6" "0.17.6" "")"
assert_field "$case_synced" "STATUS" "synced"
assert_field "$case_synced" "TARGET_VERSION" "0.17.6"

case_pending_cli_first="$("$SCRIPT_PATH" "0.17.6" "0.17.7" $'chore/publish-updates-v0.17.7\nother/branch')"
assert_field "$case_pending_cli_first" "STATUS" "pending"
assert_field "$case_pending_cli_first" "TARGET_VERSION" "0.17.7"

case_pending_appcast_first="$("$SCRIPT_PATH" "0.17.7" "0.17.6" "chore/publish-cli-updates-v0.17.7-stable")"
assert_field "$case_pending_appcast_first" "STATUS" "pending"
assert_field "$case_pending_appcast_first" "TARGET_VERSION" "0.17.7"

case_unmatched="$("$SCRIPT_PATH" "0.17.7" "0.17.6" "")"
assert_field "$case_unmatched" "STATUS" "mismatch"
assert_field "$case_unmatched" "TARGET_VERSION" "0.17.7"

case_invalid="$("$SCRIPT_PATH" "0.17.7-rc.1" "0.17.6" "")"
assert_field "$case_invalid" "STATUS" "invalid"

printf '[publish-verify-state-contract] passed\n'

