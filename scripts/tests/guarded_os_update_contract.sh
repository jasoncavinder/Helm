#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/tests/guarded_os_update_decision.sh"
FIXTURE_DIR="${ROOT_DIR}/scripts/tests/fixtures/guarded_os_update"
REPORT_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/tests/guarded_os_update_contract.sh [--report-path <path>]
USAGE
}

fail() {
  printf '[guarded-os-update-contract] error: %s\n' "$1" >&2
  exit 1
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --report-path)
        [ $# -ge 2 ] || fail "--report-path requires a value"
        REPORT_PATH="$2"
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

record_result() {
  local jsonl_path="$1"
  local scenario="$2"
  local payload="$3"
  python3 - "$jsonl_path" "$scenario" "$payload" <<'PY'
import json
import pathlib
import sys

jsonl_path = pathlib.Path(sys.argv[1])
scenario = sys.argv[2]
payload = sys.argv[3]
fields = {}
for line in payload.splitlines():
    if "=" not in line:
        continue
    key, value = line.split("=", 1)
    fields[key] = value
record = {
    "scenario": scenario,
    "status": fields.get("STATUS", ""),
    "action": fields.get("ACTION", ""),
    "signal": fields.get("SIGNAL", ""),
    "operation": fields.get("OPERATION", ""),
}
with jsonl_path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, ensure_ascii=True))
    handle.write("\n")
PY
}

write_report() {
  local jsonl_path="$1"
  local report_path="$2"
  python3 - "$jsonl_path" "$report_path" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

jsonl_path = pathlib.Path(sys.argv[1])
report_path = pathlib.Path(sys.argv[2])
records = []
for line in jsonl_path.read_text(encoding="utf-8").splitlines():
    line = line.strip()
    if line:
        records.append(json.loads(line))
report = {
    "schema": "helm.tests.guarded_os_update_contract_report",
    "schema_version": 1,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "overall_status": "passed",
    "scenario_results": records,
}
report_path.parent.mkdir(parents=True, exist_ok=True)
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(str(report_path.resolve()))
PY
}

main() {
  parse_args "$@"

  local tmp_jsonl
  tmp_jsonl="$(mktemp)"
  trap 'rm -f "${tmp_jsonl:-}"' EXIT

  local payload

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/read_only_refresh.json")"
  assert_field "$payload" "STATUS" "allowed"
  assert_field "$payload" "ACTION" "run"
  assert_field "$payload" "SIGNAL" "guarded_read_only_allowed"
  record_result "$tmp_jsonl" "read_only_refresh" "$payload"

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/mutating_confirmation_missing.json")"
  assert_field "$payload" "STATUS" "needs_confirmation"
  assert_field "$payload" "ACTION" "halt"
  assert_field "$payload" "SIGNAL" "confirmation_required"
  record_result "$tmp_jsonl" "mutating_confirmation_missing" "$payload"

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/mutating_isolation_missing.json")"
  assert_field "$payload" "STATUS" "denied"
  assert_field "$payload" "ACTION" "halt"
  assert_field "$payload" "SIGNAL" "isolation_required"
  record_result "$tmp_jsonl" "mutating_isolation_missing" "$payload"

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/mutating_snapshot_missing.json")"
  assert_field "$payload" "STATUS" "denied"
  assert_field "$payload" "ACTION" "halt"
  assert_field "$payload" "SIGNAL" "snapshot_required"
  record_result "$tmp_jsonl" "mutating_snapshot_missing" "$payload"

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/mutating_failed_after_mutation.json")"
  assert_field "$payload" "STATUS" "rollback_required"
  assert_field "$payload" "ACTION" "rollback"
  assert_field "$payload" "SIGNAL" "mutation_failed_rollback_required"
  record_result "$tmp_jsonl" "mutating_failed_after_mutation" "$payload"

  payload="$($SCRIPT_PATH --state "$FIXTURE_DIR/mutating_ready.json")"
  assert_field "$payload" "STATUS" "allowed"
  assert_field "$payload" "ACTION" "run"
  assert_field "$payload" "SIGNAL" "guarded_mutation_allowed"
  record_result "$tmp_jsonl" "mutating_ready" "$payload"

  if [ -n "$REPORT_PATH" ]; then
    if [[ "$REPORT_PATH" != /* ]]; then
      REPORT_PATH="${ROOT_DIR}/${REPORT_PATH}"
    fi
    local report_resolved
    report_resolved="$(write_report "$tmp_jsonl" "$REPORT_PATH")"
    printf '[guarded-os-update-contract] report written: %s\n' "$report_resolved"
  fi

  printf '[guarded-os-update-contract] passed\n'
}

main "$@"
