#!/usr/bin/env bash
set -euo pipefail

APPCAST_PATH=""
STATE_PATH=""

usage() {
  cat <<'USAGE'
Usage:
  scripts/release/sparkle_recovery_decision.sh --appcast <path> --state <path>

Evaluates fixture-driven Sparkle recovery decision state.
Outputs key=value records:
  STATUS
  ACTION
  SIGNAL
  TARGET_VERSION
  APPCAST_VERSION
USAGE
}

fail() {
  printf '[sparkle-recovery-decision] error: %s\n' "$1" >&2
  exit 1
}

parse_args() {
  while [ $# -gt 0 ]; do
    case "$1" in
      --appcast)
        [ $# -ge 2 ] || fail "--appcast requires a value"
        APPCAST_PATH="$2"
        shift 2
        ;;
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

  [ -n "$APPCAST_PATH" ] || fail "missing required --appcast"
  [ -n "$STATE_PATH" ] || fail "missing required --state"
  [ -f "$APPCAST_PATH" ] || fail "appcast file not found: ${APPCAST_PATH}"
  [ -f "$STATE_PATH" ] || fail "state file not found: ${STATE_PATH}"
}

main() {
  parse_args "$@"

  python3 - "$APPCAST_PATH" "$STATE_PATH" <<'PY'
import json
import re
import sys
import xml.etree.ElementTree as ET

sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
semver_pattern = re.compile(r"^\d+\.\d+\.\d+$")

appcast_path = sys.argv[1]
state_path = sys.argv[2]

def emit(status: str, action: str, signal: str, target: str, appcast_version: str) -> None:
    print(f"STATUS={status}")
    print(f"ACTION={action}")
    print(f"SIGNAL={signal}")
    print(f"TARGET_VERSION={target}")
    print(f"APPCAST_VERSION={appcast_version}")

try:
    state = json.loads(open(state_path, "r", encoding="utf-8").read())
except Exception:
    emit("invalid", "halt", "invalid_state_payload", "", "")
    raise SystemExit(0)

phase = str(state.get("phase", "")).strip()
interrupted = bool(state.get("interrupted", False))
last_error = str(state.get("last_error", "")).strip()
target_version = str(state.get("target_version", "")).strip()

try:
    root = ET.parse(appcast_path).getroot()
    item = root.find("./channel/item")
    enclosure = None if item is None else item.find("enclosure")
    appcast_version = "" if enclosure is None else str(enclosure.attrib.get(f"{{{sparkle_ns}}}shortVersionString", "")).strip()
except Exception:
    emit("invalid", "halt", "invalid_appcast_metadata", target_version, "")
    raise SystemExit(0)

if not semver_pattern.fullmatch(appcast_version):
    emit("invalid", "halt", "invalid_appcast_metadata", target_version, appcast_version)
    raise SystemExit(0)

if last_error in {"invalid_metadata", "signature_mismatch"}:
    emit("manual_review", "halt", "invalid_update_metadata", target_version, appcast_version)
    raise SystemExit(0)

if interrupted and phase == "download":
    if appcast_version == target_version and target_version:
        emit("recoverable", "retry_download", "interrupted_download_recoverable", target_version, appcast_version)
    else:
        emit("needs_appcast_refresh", "refresh_then_retry_download", "stale_appcast_after_interrupted_download", target_version, appcast_version)
    raise SystemExit(0)

if interrupted and phase == "apply":
    if appcast_version == target_version and target_version:
        emit("recoverable", "retry_apply", "interrupted_apply_recoverable", target_version, appcast_version)
    else:
        emit("needs_appcast_refresh", "refresh_then_retry_apply", "stale_appcast_after_interrupted_apply", target_version, appcast_version)
    raise SystemExit(0)

emit("no_recovery_needed", "none", "none", target_version, appcast_version)
PY
}

main "$@"
