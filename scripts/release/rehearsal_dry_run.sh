#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "${SCRIPT_DIR}/../.." && pwd)"

PREFLIGHT_SCRIPT="${HELM_RELEASE_PREFLIGHT_SCRIPT:-${SCRIPT_DIR}/preflight.sh}"
RUNBOOK_SCRIPT="${HELM_RELEASE_RUNBOOK_SCRIPT:-${SCRIPT_DIR}/runbook.sh}"
VERIFY_STABLE_CONTRACT_SCRIPT="${HELM_RELEASE_VERIFY_STABLE_CONTRACT_SCRIPT:-${SCRIPT_DIR}/tests/publish_verify_state_contract.sh}"
VERIFY_PRERELEASE_CONTRACT_SCRIPT="${HELM_RELEASE_VERIFY_PRERELEASE_CONTRACT_SCRIPT:-${SCRIPT_DIR}/tests/publish_verify_prerelease_state_contract.sh}"

TAG_NAME=""
REPORT_PATH=""
REPORT_DIR=""
LOG_DIR=""
STEPS_JSONL=""
OVERALL_STATUS="passed"

usage() {
  cat <<'USAGE'
Usage:
  scripts/release/rehearsal_dry_run.sh --tag <vX.Y.Z|vX.Y.Z-rc.N> [--report-path <path>]

Runs a non-mutating release rehearsal contract and writes a machine-readable report.

Behavior:
  - preflight (non-destructive flags)
  - runbook prepare (non-destructive flags)
  - verify contract scripts (stable + prerelease publish-verify contracts)

Safety guarantees:
  - no git push/tag creation
  - no GitHub release mutation
  - no publish metadata mutation
USAGE
}

fail() {
  printf '[rehearsal] error: %s\n' "$1" >&2
  exit 1
}

info() {
  printf '[rehearsal] %s\n' "$1"
}

render_command() {
  local rendered=""
  local part
  for part in "$@"; do
    local escaped
    escaped="$(printf '%q' "$part")"
    if [ -z "$rendered" ]; then
      rendered="$escaped"
    else
      rendered+=" $escaped"
    fi
  done
  printf '%s' "$rendered"
}

append_step_record() {
  local name="$1"
  local status="$2"
  local exit_code="$3"
  local command="$4"
  local log_path="$5"
  local started_at="$6"
  local finished_at="$7"

  python3 - "$STEPS_JSONL" "$name" "$status" "$exit_code" "$command" "$log_path" "$started_at" "$finished_at" <<'PY'
import json
import pathlib
import sys

steps_path = pathlib.Path(sys.argv[1])
record = {
    "name": sys.argv[2],
    "status": sys.argv[3],
    "exit_code": int(sys.argv[4]),
    "command": sys.argv[5],
    "log_path": str(pathlib.Path(sys.argv[6]).resolve()),
    "started_at": sys.argv[7],
    "finished_at": sys.argv[8],
}
with steps_path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, ensure_ascii=True))
    handle.write("\n")
PY
}

run_step() {
  local name="$1"
  local command_display="$2"
  shift 2

  local log_path="${LOG_DIR}/${name}.log"
  local started_at
  local finished_at
  local exit_code=0
  local status="passed"

  started_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
  if "$@" >"${log_path}" 2>&1; then
    status="passed"
    exit_code=0
  else
    exit_code=$?
    status="failed"
    OVERALL_STATUS="failed"
  fi
  finished_at="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"

  append_step_record "$name" "$status" "$exit_code" "$command_display" "$log_path" "$started_at" "$finished_at"

  info "step ${name}: ${status} (exit=${exit_code})"
  if [ "$status" = "failed" ]; then
    sed 's/^/[rehearsal]   /' "${log_path}" >&2 || true
  fi
}

execute_verify_contracts() {
  "${VERIFY_STABLE_CONTRACT_SCRIPT}"
  "${VERIFY_PRERELEASE_CONTRACT_SCRIPT}"
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --tag)
        [ $# -ge 2 ] || fail "--tag requires a value"
        TAG_NAME="$2"
        shift 2
        ;;
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

  [ -n "$TAG_NAME" ] || fail "missing required --tag"
}

prepare_paths() {
  local stamp
  local tag_slug

  stamp="$(date -u +"%Y%m%dT%H%M%SZ")"
  tag_slug="$(printf '%s' "$TAG_NAME" | tr '/ ' '__')"

  if [ -z "$REPORT_PATH" ]; then
    REPORT_PATH="${ROOT_DIR}/artifacts/release-rehearsal/report-${tag_slug}-${stamp}.json"
  elif [[ "$REPORT_PATH" != /* ]]; then
    REPORT_PATH="${ROOT_DIR}/${REPORT_PATH}"
  fi

  REPORT_DIR="$(cd "$(dirname "$REPORT_PATH")" && pwd)"
  mkdir -p "$REPORT_DIR"

  LOG_DIR="${REPORT_DIR}/logs-${tag_slug}-${stamp}"
  mkdir -p "$LOG_DIR"

  STEPS_JSONL="${LOG_DIR}/steps.jsonl"
  : > "$STEPS_JSONL"
}

check_contract_paths() {
  local path
  for path in "$PREFLIGHT_SCRIPT" "$RUNBOOK_SCRIPT" "$VERIFY_STABLE_CONTRACT_SCRIPT" "$VERIFY_PRERELEASE_CONTRACT_SCRIPT"; do
    if [ ! -x "$path" ]; then
      fail "required executable contract path is missing or not executable: ${path}"
    fi
  done
}

write_report() {
  python3 - "$STEPS_JSONL" "$REPORT_PATH" "$TAG_NAME" "$OVERALL_STATUS" <<'PY'
import json
import pathlib
import sys
from datetime import datetime, timezone

steps_path = pathlib.Path(sys.argv[1])
report_path = pathlib.Path(sys.argv[2])
tag = sys.argv[3]
overall_status = sys.argv[4]

steps = []
if steps_path.exists():
    for line in steps_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            steps.append(json.loads(line))

report = {
    "schema": "helm.release.rehearsal_report",
    "schema_version": 1,
    "generated_at": datetime.now(timezone.utc).isoformat(),
    "tag": tag,
    "dry_run": True,
    "overall_status": overall_status,
    "non_mutation_guards": [
        "No git push or tag creation commands are executed.",
        "No GitHub release create/edit/delete/upload commands are executed.",
        "No release metadata files are modified by rehearsal steps.",
    ],
    "steps": steps,
}

report_path.parent.mkdir(parents=True, exist_ok=True)
report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
print(str(report_path.resolve()))
PY
}

main() {
  cd "$ROOT_DIR"

  parse_args "$@"
  prepare_paths
  check_contract_paths

  local -a preflight_cmd=(
    "$PREFLIGHT_SCRIPT"
    --tag "$TAG_NAME"
    --allow-non-main
    --allow-dirty
    --no-fetch
    --skip-secrets
    --skip-workflows
    --skip-ruleset-policy
    --allow-existing-tag
  )

  local -a prepare_cmd=(
    "$RUNBOOK_SCRIPT"
    prepare
    --tag "$TAG_NAME"
    --allow-non-main
    --allow-dirty
    --no-fetch
    --skip-secrets
    --skip-workflows
    --skip-ruleset-policy
  )

  local verify_display
  verify_display="$(render_command "$VERIFY_STABLE_CONTRACT_SCRIPT") && $(render_command "$VERIFY_PRERELEASE_CONTRACT_SCRIPT")"

  run_step "preflight" "$(render_command "${preflight_cmd[@]}")" "${preflight_cmd[@]}"
  run_step "prepare" "$(render_command "${prepare_cmd[@]}")" "${prepare_cmd[@]}"
  run_step "verify" "$verify_display" execute_verify_contracts

  local report_resolved
  report_resolved="$(write_report)"
  info "report written: ${report_resolved}"

  if [ "$OVERALL_STATUS" != "passed" ]; then
    fail "one or more rehearsal phases failed"
  fi

  info "release rehearsal dry run passed"
}

main "$@"
