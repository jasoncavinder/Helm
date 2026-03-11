#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"

required_files=(
  AGENTS.md
  docs/PROJECT_BRIEF.md
  docs/CURRENT_STATE.md
  docs/NEXT_STEPS.md
  docs/ARCHITECTURE.md
  docs/DECISIONS.md
  docs/ROADMAP.md
  docs/RELEASE_CHECKLIST.md
  docs/VERSIONING.md
)

for file in "${required_files[@]}"; do
  if [ ! -f "$ROOT_DIR/$file" ]; then
    echo "missing required docs file: $file" >&2
    exit 1
  fi
done

if rg -n "^(<<<<<<<|=======|>>>>>>>)" "$ROOT_DIR/docs" "$ROOT_DIR/AGENTS.md" "$ROOT_DIR/README.md"; then
  echo "merge markers found in docs scope" >&2
  exit 1
fi

"$ROOT_DIR/scripts/release/check_release_line_copy.sh"

echo "docs-sync checks passed"
