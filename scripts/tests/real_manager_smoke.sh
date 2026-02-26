#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

BREW_BIN="${HELM_REAL_MANAGER_BREW_BIN:-brew}"
NODE_BIN="${HELM_REAL_MANAGER_NODE_BIN:-node}"
NPM_BIN="${HELM_REAL_MANAGER_NPM_BIN:-npm}"
PYTHON_BIN="${HELM_REAL_MANAGER_PYTHON_BIN:-python3}"
PIP_BIN="${HELM_REAL_MANAGER_PIP_BIN:-pip3}"
RUBY_BIN="${HELM_REAL_MANAGER_RUBY_BIN:-ruby}"
GEM_BIN="${HELM_REAL_MANAGER_GEM_BIN:-gem}"

REPORT_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/tests/real_manager_smoke.sh [--report-path <path>]

Runs non-destructive version probes for real-manager canary validation:
  - Homebrew manager: brew
  - Node managers: node + npm
  - Python managers: python3 + pip3
  - Ruby managers: ruby + gem

Exit behavior:
  - 0 when all manager groups pass
  - 1 when any manager group fails
USAGE
}

info() {
  printf '[real-manager-smoke] %s\n' "$1"
}

fail() {
  printf '[real-manager-smoke] error: %s\n' "$1" >&2
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

trim_output() {
  local text="$1"
  python3 - "$text" <<'PY'
import sys

text = sys.argv[1].replace("\r", "").strip()
if not text:
    print("")
    raise SystemExit(0)
line = text.splitlines()[0].strip()
if len(line) > 240:
    line = line[:237] + "..."
print(line)
PY
}

append_probe_record() {
  local jsonl_path="$1"
  local manager="$2"
  local probe="$3"
  local status="$4"
  local exit_code="$5"
  local command="$6"
  local detail="$7"

  python3 - "$jsonl_path" "$manager" "$probe" "$status" "$exit_code" "$command" "$detail" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
record = {
    "manager": sys.argv[2],
    "probe": sys.argv[3],
    "status": sys.argv[4],
    "exit_code": int(sys.argv[5]),
    "command": sys.argv[6],
    "detail": sys.argv[7],
}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, ensure_ascii=True))
    handle.write("\n")
PY
}

append_manager_record() {
  local jsonl_path="$1"
  local manager="$2"
  local status="$3"
  local passed="$4"
  local failed="$5"

  python3 - "$jsonl_path" "$manager" "$status" "$passed" "$failed" <<'PY'
import json
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
record = {
    "manager": sys.argv[2],
    "status": sys.argv[3],
    "passed_probes": int(sys.argv[4]),
    "failed_probes": int(sys.argv[5]),
}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, ensure_ascii=True))
    handle.write("\n")
PY
}

probe_command() {
  local manager="$1"
  local probe="$2"
  local probe_jsonl="$3"
  local bin="$4"
  shift 4

  local resolved=""
  local output=""
  local detail=""
  local status="passed"
  local exit_code=0
  local command_display

  command_display="$(printf '%q ' "$bin" "$@" | xargs)"

  if ! resolved="$(command -v "$bin" 2>/dev/null || true)"; then
    resolved=""
  fi

  if [ -z "$resolved" ]; then
    status="failed"
    exit_code=127
    detail="binary not found: ${bin}"
  else
    if output="$($resolved "$@" 2>&1)"; then
      status="passed"
      exit_code=0
      detail="$(trim_output "$output")"
      if [ -z "$detail" ]; then
        detail="command succeeded"
      fi
    else
      exit_code=$?
      status="failed"
      detail="$(trim_output "$output")"
      if [ -z "$detail" ]; then
        detail="command failed"
      fi
    fi
  fi

  append_probe_record "$probe_jsonl" "$manager" "$probe" "$status" "$exit_code" "$command_display" "$detail"

  if [ "$status" = "passed" ]; then
    info "${manager}/${probe}: passed (${detail})"
    return 0
  fi

  info "${manager}/${probe}: failed (${detail})"
  return 1
}

write_report() {
  local manager_jsonl="$1"
  local probe_jsonl="$2"
  local report_path="$3"

  python3 - "$manager_jsonl" "$probe_jsonl" "$report_path" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

manager_path = pathlib.Path(sys.argv[1])
probe_path = pathlib.Path(sys.argv[2])
report_path = pathlib.Path(sys.argv[3])

manager_results = []
probe_results = []
for line in manager_path.read_text(encoding="utf-8").splitlines():
    line = line.strip()
    if line:
        manager_results.append(json.loads(line))
for line in probe_path.read_text(encoding="utf-8").splitlines():
    line = line.strip()
    if line:
        probe_results.append(json.loads(line))

failed_managers = [item["manager"] for item in manager_results if item["status"] != "passed"]
report = {
    "schema": "helm.tests.real_manager_smoke_report",
    "schema_version": 1,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "overall_status": "passed" if not failed_managers else "failed",
    "failed_managers": failed_managers,
    "manager_results": manager_results,
    "probe_results": probe_results,
}

report_path.parent.mkdir(parents=True, exist_ok=True)
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(str(report_path.resolve()))
PY
}

main() {
  parse_args "$@"

  local manager_jsonl
  local probe_jsonl
  manager_jsonl="$(mktemp)"
  probe_jsonl="$(mktemp)"
  trap 'rm -f "${manager_jsonl:-}" "${probe_jsonl:-}"' EXIT

  local manager_failed=0
  local manager_passed=0

  # Homebrew manager
  local group_failed=0
  local group_passed=0
  if probe_command "homebrew" "brew_version" "$probe_jsonl" "$BREW_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if [ "$group_failed" -eq 0 ]; then
    append_manager_record "$manager_jsonl" "homebrew" "passed" "$group_passed" "$group_failed"
    manager_passed=$((manager_passed + 1))
  else
    append_manager_record "$manager_jsonl" "homebrew" "failed" "$group_passed" "$group_failed"
    manager_failed=$((manager_failed + 1))
  fi

  # Node managers
  group_failed=0
  group_passed=0
  if probe_command "node" "node_version" "$probe_jsonl" "$NODE_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if probe_command "node" "npm_version" "$probe_jsonl" "$NPM_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if [ "$group_failed" -eq 0 ]; then
    append_manager_record "$manager_jsonl" "node" "passed" "$group_passed" "$group_failed"
    manager_passed=$((manager_passed + 1))
  else
    append_manager_record "$manager_jsonl" "node" "failed" "$group_passed" "$group_failed"
    manager_failed=$((manager_failed + 1))
  fi

  # Python managers
  group_failed=0
  group_passed=0
  if probe_command "python" "python3_version" "$probe_jsonl" "$PYTHON_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if probe_command "python" "pip3_version" "$probe_jsonl" "$PIP_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if [ "$group_failed" -eq 0 ]; then
    append_manager_record "$manager_jsonl" "python" "passed" "$group_passed" "$group_failed"
    manager_passed=$((manager_passed + 1))
  else
    append_manager_record "$manager_jsonl" "python" "failed" "$group_passed" "$group_failed"
    manager_failed=$((manager_failed + 1))
  fi

  # Ruby managers
  group_failed=0
  group_passed=0
  if probe_command "ruby" "ruby_version" "$probe_jsonl" "$RUBY_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if probe_command "ruby" "gem_version" "$probe_jsonl" "$GEM_BIN" --version; then
    group_passed=$((group_passed + 1))
  else
    group_failed=$((group_failed + 1))
  fi
  if [ "$group_failed" -eq 0 ]; then
    append_manager_record "$manager_jsonl" "ruby" "passed" "$group_passed" "$group_failed"
    manager_passed=$((manager_passed + 1))
  else
    append_manager_record "$manager_jsonl" "ruby" "failed" "$group_passed" "$group_failed"
    manager_failed=$((manager_failed + 1))
  fi

  info "summary: managers_total=4 managers_passed=${manager_passed} managers_failed=${manager_failed}"

  if [ -n "$REPORT_PATH" ]; then
    if [[ "$REPORT_PATH" != /* ]]; then
      REPORT_PATH="${ROOT_DIR}/${REPORT_PATH}"
    fi
    local report_resolved
    report_resolved="$(write_report "$manager_jsonl" "$probe_jsonl" "$REPORT_PATH")"
    info "report written: ${report_resolved}"
  fi

  if [ "$manager_failed" -gt 0 ]; then
    exit 1
  fi
}

main "$@"
