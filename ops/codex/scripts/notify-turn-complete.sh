#!/usr/bin/env bash
set -u

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LOG_DIR="$ROOT_DIR/dev/logs"
LOG_FILE="$LOG_DIR/codex-runs.ndjson"

mkdir -p "$LOG_DIR"

EVENT_NAME="${1:-agent-turn-complete}"
shift || true
SUMMARY_ARG="$(printf '%s ' "$@" 2>/dev/null | sed -E 's/[[:space:]]+/ /g; s/^ //; s/ $//')"

HOOK_PAYLOAD=""
if [ ! -t 0 ]; then
  HOOK_PAYLOAD="$(cat 2>/dev/null || true)"
fi

GIT_BRANCH="$(git -C "$ROOT_DIR" branch --show-current 2>/dev/null || true)"
GIT_HEAD="$(git -C "$ROOT_DIR" rev-parse --short HEAD 2>/dev/null || true)"
GIT_STATUS="$(git -C "$ROOT_DIR" status --porcelain --untracked-files=all 2>/dev/null || true)"
TIMESTAMP="$(date -u +"%Y-%m-%dT%H:%M:%SZ" 2>/dev/null || true)"

if command -v python3 >/dev/null 2>&1; then
  SUMMARY_ARG="$SUMMARY_ARG" \
  HOOK_PAYLOAD="$HOOK_PAYLOAD" \
  EVENT_NAME="$EVENT_NAME" \
  ROOT_DIR="$ROOT_DIR" \
  GIT_BRANCH="$GIT_BRANCH" \
  GIT_HEAD="$GIT_HEAD" \
  GIT_STATUS="$GIT_STATUS" \
  TIMESTAMP="$TIMESTAMP" \
  python3 - "$LOG_FILE" <<'PY' || true
import datetime
import json
import os
import pathlib
import re
import sys

log_file = pathlib.Path(sys.argv[1])

def clean_text(value: str, limit: int = 240) -> str:
    value = re.sub(r"\s+", " ", (value or "").strip())
    if not value:
        return "no summary provided"
    return value if len(value) <= limit else value[: limit - 3] + "..."

summary = os.environ.get("SUMMARY_ARG", "").strip()
payload_raw = os.environ.get("HOOK_PAYLOAD", "")

if payload_raw.strip():
    payload_obj = None
    try:
        payload_obj = json.loads(payload_raw)
    except Exception:
        if not summary:
            summary = payload_raw.strip().splitlines()[-1]

    if payload_obj is not None and not summary:
        keys = ["summary", "last_agent_message", "final_output", "message", "output_text", "text"]

        def walk(node):
            if isinstance(node, dict):
                for key in keys:
                    value = node.get(key)
                    if isinstance(value, str) and value.strip():
                        return value
                for value in node.values():
                    found = walk(value)
                    if found:
                        return found
            elif isinstance(node, list):
                for value in node:
                    found = walk(value)
                    if found:
                        return found
            return ""

        summary = walk(payload_obj)

summary = clean_text(summary)

status_lines = [line for line in os.environ.get("GIT_STATUS", "").splitlines() if line.strip()]
changed_files = []
for line in status_lines:
    path = line[3:] if len(line) > 3 else line
    # Normalize rename lines: "old -> new" => "new"
    if " -> " in path:
        path = path.split(" -> ", 1)[1]
    changed_files.append(path)

record = {
    "timestamp": os.environ.get("TIMESTAMP")
    or datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "event": os.environ.get("EVENT_NAME", "agent-turn-complete"),
    "git_branch": os.environ.get("GIT_BRANCH", ""),
    "working_directory": os.getcwd(),
    "repo_root": os.environ.get("ROOT_DIR", ""),
    "git_head": os.environ.get("GIT_HEAD", ""),
    "changed_files": changed_files[:200],
    "changed_files_count": len(changed_files),
    "summary": summary,
}

log_file.parent.mkdir(parents=True, exist_ok=True)
with log_file.open("a", encoding="utf-8") as f:
    f.write(json.dumps(record, ensure_ascii=True))
    f.write("\n")
PY
else
  printf '{"timestamp":"%s","event":"%s","summary":"%s"}\n' \
    "$TIMESTAMP" "$EVENT_NAME" "notify hook fallback without python3" >> "$LOG_FILE"
fi

# Never fail the caller due to notify logging.
exit 0
