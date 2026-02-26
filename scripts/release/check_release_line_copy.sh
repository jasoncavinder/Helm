#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CONTRACT_PATH="${ROOT_DIR}/docs/contracts/release-line.json"

fail() {
  printf '[release-line-check] error: %s\n' "$1" >&2
  exit 1
}

if [ ! -f "$CONTRACT_PATH" ]; then
  fail "missing release-line contract: ${CONTRACT_PATH}"
fi

stable_version="$(
  python3 - "$CONTRACT_PATH" <<'PY'
import json
import re
import sys

with open(sys.argv[1], "r", encoding="utf-8") as fh:
    payload = json.load(fh)

version = str(payload.get("stable_version", "")).strip()
if not re.fullmatch(r"\d+\.\d+\.\d+", version):
    print("")
    raise SystemExit(0)

print(version)
PY
)"

if [ -z "$stable_version" ]; then
  fail "docs/contracts/release-line.json must define stable_version as X.Y.Z"
fi

stable_tag="v${stable_version}"

assert_contains() {
  local file_path="$1"
  local text="$2"
  if ! grep -Fq "$text" "$file_path"; then
    fail "${file_path} is missing expected release line: ${text}"
  fi
}

assert_contains "${ROOT_DIR}/README.md" "Pre-1.0 &middot; ${stable_tag}"
assert_contains "${ROOT_DIR}/README.md" "stable \`${stable_tag}\` on \`main\`"
assert_contains "${ROOT_DIR}/README.md" "Please test \`${stable_tag}\`"
assert_contains "${ROOT_DIR}/web/src/components/starlight/Banner.astro" "Helm ${stable_tag} is live."
assert_contains "${ROOT_DIR}/docs/CURRENT_STATE.md" "latest stable release currently published on \`main\`: **${stable_version}**"
assert_contains "${ROOT_DIR}/docs/NEXT_STEPS.md" "latest stable release on \`main\`: \`${stable_tag}\`"

printf '[release-line-check] passed for %s\n' "$stable_tag"

