#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/sparkle_recovery_decision.sh"
FIXTURE_DIR="${ROOT_DIR}/scripts/release/tests/fixtures/sparkle"
REPORT_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/release/tests/sparkle_recovery_contract.sh [--report-path <path>]
USAGE
}

fail() {
  printf '[sparkle-recovery-contract] error: %s\n' "$1" >&2
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
    "target_version": fields.get("TARGET_VERSION", ""),
    "appcast_version": fields.get("APPCAST_VERSION", ""),
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
    "schema": "helm.release.sparkle_recovery_contract_report",
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

  payload="$($SCRIPT_PATH --appcast "$FIXTURE_DIR/appcast-current.xml" --state "$FIXTURE_DIR/state-interrupted-download.json")"
  assert_field "$payload" "STATUS" "recoverable"
  assert_field "$payload" "ACTION" "retry_download"
  assert_field "$payload" "SIGNAL" "interrupted_download_recoverable"
  record_result "$tmp_jsonl" "interrupted_download" "$payload"

  payload="$($SCRIPT_PATH --appcast "$FIXTURE_DIR/appcast-current.xml" --state "$FIXTURE_DIR/state-interrupted-apply.json")"
  assert_field "$payload" "STATUS" "recoverable"
  assert_field "$payload" "ACTION" "retry_apply"
  assert_field "$payload" "SIGNAL" "interrupted_apply_recoverable"
  record_result "$tmp_jsonl" "interrupted_apply" "$payload"

  payload="$($SCRIPT_PATH --appcast "$FIXTURE_DIR/appcast-stale.xml" --state "$FIXTURE_DIR/state-interrupted-download.json")"
  assert_field "$payload" "STATUS" "needs_appcast_refresh"
  assert_field "$payload" "ACTION" "refresh_then_retry_download"
  assert_field "$payload" "SIGNAL" "stale_appcast_after_interrupted_download"
  record_result "$tmp_jsonl" "stale_appcast_download" "$payload"

  payload="$($SCRIPT_PATH --appcast "$FIXTURE_DIR/appcast-invalid.xml" --state "$FIXTURE_DIR/state-interrupted-download.json")"
  assert_field "$payload" "STATUS" "invalid"
  assert_field "$payload" "ACTION" "halt"
  assert_field "$payload" "SIGNAL" "invalid_appcast_metadata"
  record_result "$tmp_jsonl" "invalid_appcast_metadata" "$payload"

  payload="$($SCRIPT_PATH --appcast "$FIXTURE_DIR/appcast-current.xml" --state "$FIXTURE_DIR/state-invalid-metadata.json")"
  assert_field "$payload" "STATUS" "manual_review"
  assert_field "$payload" "ACTION" "halt"
  assert_field "$payload" "SIGNAL" "invalid_update_metadata"
  record_result "$tmp_jsonl" "invalid_update_metadata" "$payload"

  if [ -n "$REPORT_PATH" ]; then
    if [[ "$REPORT_PATH" != /* ]]; then
      REPORT_PATH="${ROOT_DIR}/${REPORT_PATH}"
    fi
    local report_resolved
    report_resolved="$(write_report "$tmp_jsonl" "$REPORT_PATH")"
    printf '[sparkle-recovery-contract] report written: %s\n' "$report_resolved"
  fi

  printf '[sparkle-recovery-contract] passed\n'
}

main "$@"
