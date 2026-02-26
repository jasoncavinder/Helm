#!/usr/bin/env bash
set -euo pipefail

STATE_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/tests/guarded_os_update_decision.sh --state <path>

Evaluates fixture-driven guarded OS update decision state.
Outputs key=value records:
  STATUS
  ACTION
  SIGNAL
  OPERATION
USAGE
}

fail() {
  printf '[guarded-os-update-decision] error: %s\n' "$1" >&2
  exit 1
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --state)
        [ $# -ge 2 ] || fail "--state requires a value"
        STATE_PATH="$2"
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

  [ -n "$STATE_PATH" ] || fail "missing required --state"
  [ -f "$STATE_PATH" ] || fail "state file not found: ${STATE_PATH}"
}

main() {
  parse_args "$@"

  python3 - "$STATE_PATH" <<'PY'
import json
import pathlib
import sys

state_path = pathlib.Path(sys.argv[1])

def emit(status: str, action: str, signal: str, operation: str) -> None:
    print(f"STATUS={status}")
    print(f"ACTION={action}")
    print(f"SIGNAL={signal}")
    print(f"OPERATION={operation}")

try:
    state = json.loads(state_path.read_text(encoding="utf-8"))
except Exception:
    emit("invalid", "halt", "invalid_state_payload", "")
    raise SystemExit(0)

operation = str(state.get("operation", "")).strip()
operation_class = str(state.get("operation_class", "")).strip()
requires_confirmation = bool(state.get("requires_confirmation", False))
operator_confirmed = bool(state.get("operator_confirmed", False))
vm_isolated = bool(state.get("vm_isolated", False))
snapshot_ready = bool(state.get("snapshot_ready", False))
simulated_result = str(state.get("simulated_result", "ready")).strip()

if operation_class not in {"read_only", "mutating"}:
    emit("invalid", "halt", "invalid_operation_class", operation)
    raise SystemExit(0)

if operation_class == "read_only":
    emit("allowed", "run", "guarded_read_only_allowed", operation)
    raise SystemExit(0)

if not vm_isolated:
    emit("denied", "halt", "isolation_required", operation)
    raise SystemExit(0)

if not snapshot_ready:
    emit("denied", "halt", "snapshot_required", operation)
    raise SystemExit(0)

if requires_confirmation and not operator_confirmed:
    emit("needs_confirmation", "halt", "confirmation_required", operation)
    raise SystemExit(0)

if simulated_result == "failed_after_mutation":
    emit("rollback_required", "rollback", "mutation_failed_rollback_required", operation)
    raise SystemExit(0)

emit("allowed", "run", "guarded_mutation_allowed", operation)
PY
}

main "$@"
