#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/publish_verify_prerelease_pair_state.sh"

fail() {
  printf '[publish-verify-prerelease-pair-state-contract] error: %s\n' "$1" >&2
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

case_empty="$("$SCRIPT_PATH" "" "" "")"
assert_field "$case_empty" "STATUS" "empty"

case_synced="$("$SCRIPT_PATH" "0.19.0-rc.2" "0.19.0-rc.2" "")"
assert_field "$case_synced" "STATUS" "synced"
assert_field "$case_synced" "TARGET_VERSION" "0.19.0-rc.2"

case_gui_only="$("$SCRIPT_PATH" "0.19.0-rc.2" "" "")"
assert_field "$case_gui_only" "STATUS" "incomplete"

case_gui_only_pending="$("$SCRIPT_PATH" "0.19.0-rc.2" "" "chore/publish-cli-updates-v0.19.0-rc.2-rc")"
assert_field "$case_gui_only_pending" "STATUS" "pending"

case_cli_only="$("$SCRIPT_PATH" "" "0.19.0-rc.2" "")"
assert_field "$case_cli_only" "STATUS" "incomplete"

case_cli_only_pending="$("$SCRIPT_PATH" "" "0.19.0-rc.2" "chore/publish-updates-v0.19.0-rc.2")"
assert_field "$case_cli_only_pending" "STATUS" "pending"

case_gui_behind_pending="$("$SCRIPT_PATH" "0.19.0-rc.1" "0.19.0-rc.2" "chore/publish-updates-v0.19.0-rc.2")"
assert_field "$case_gui_behind_pending" "STATUS" "pending"

case_cli_behind_pending="$("$SCRIPT_PATH" "0.19.0-rc.2" "0.19.0-rc.1" "chore/publish-cli-updates-v0.19.0-rc.2-rc")"
assert_field "$case_cli_behind_pending" "STATUS" "pending"

case_wrong_pr="$("$SCRIPT_PATH" "0.19.0-rc.2" "" "chore/publish-cli-updates-v0.19.0-rc.1-rc")"
assert_field "$case_wrong_pr" "STATUS" "incomplete"

case_mismatch="$("$SCRIPT_PATH" "0.19.0-rc.1" "0.19.0-rc.2" "")"
assert_field "$case_mismatch" "STATUS" "mismatch"

case_invalid="$("$SCRIPT_PATH" "0.19.0" "0.19.0-rc.2" "")"
assert_field "$case_invalid" "STATUS" "invalid"

printf '[publish-verify-prerelease-pair-state-contract] passed\n'
