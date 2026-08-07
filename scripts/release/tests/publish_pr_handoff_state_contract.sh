#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/publish_pr_handoff_state.py"

fail() {
  printf '[publish-pr-handoff-state-contract] error: %s\n' "$1" >&2
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

open_payload="$($SCRIPT_PATH '{"state":"OPEN","mergedAt":null,"url":"https://github.com/example/Helm/pull/330"}')"
assert_field "$open_payload" "PR_STATE" "OPEN"
assert_field "$open_payload" "MERGED_AT" ""
assert_field "$open_payload" "PR_URL" "https://github.com/example/Helm/pull/330"
assert_field "$open_payload" "HANDOFF_STATUS" "open"

merged_payload="$($SCRIPT_PATH '{"state":"MERGED","mergedAt":"2026-08-04T17:00:23Z","url":"https://github.com/example/Helm/pull/330"}')"
assert_field "$merged_payload" "MERGED_AT" "2026-08-04T17:00:23Z"
assert_field "$merged_payload" "HANDOFF_STATUS" "merged"

closed_payload="$($SCRIPT_PATH '{"state":"CLOSED","mergedAt":null,"url":"https://github.com/example/Helm/pull/330"}')"
assert_field "$closed_payload" "HANDOFF_STATUS" "closed_unmerged"

if "$SCRIPT_PATH" '{"state":"OPEN","mergedAt":null,"url":""}' >/dev/null 2>&1; then
  fail "classifier accepted a missing pull-request URL"
fi
if "$SCRIPT_PATH" '{"state":"UNKNOWN","mergedAt":null,"url":"https://github.com/example/Helm/pull/330"}' >/dev/null 2>&1; then
  fail "classifier accepted an unsupported pull-request state"
fi
if "$SCRIPT_PATH" '{"state":"MERGED","mergedAt":null,"url":"https://github.com/example/Helm/pull/330"}' >/dev/null 2>&1; then
  fail "classifier accepted an inconsistent merged pull-request snapshot"
fi
if "$SCRIPT_PATH" '{"state":"OPEN","mergedAt":"2026-08-04T17:00:23Z","url":"https://github.com/example/Helm/pull/330"}' >/dev/null 2>&1; then
  fail "classifier accepted an inconsistent open pull-request snapshot"
fi

printf '[publish-pr-handoff-state-contract] passed\n'
