#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/generate_provenance_manifest.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

fail() {
  printf '[provenance-manifest-contract] error: %s\n' "$1" >&2
  exit 1
}

FILE_A="${TMP_DIR}/a.txt"
FILE_B="${TMP_DIR}/b.txt"
OUT_JSON="${TMP_DIR}/provenance.json"

printf 'alpha\n' > "${FILE_A}"
printf 'beta\n' > "${FILE_B}"

env \
  GITHUB_REPOSITORY="jasoncavinder/Helm" \
  GITHUB_WORKFLOW="Release CLI Direct Installer" \
  GITHUB_RUN_ID="123456" \
  GITHUB_RUN_ATTEMPT="2" \
  GITHUB_SHA="0123456789abcdef" \
  GITHUB_REF="refs/tags/v0.17.6" \
  "${SCRIPT_PATH}" \
    --output "${OUT_JSON}" \
    --subject "${FILE_A}" \
    --subject "${FILE_B}" \
    --tag "v0.17.6"

if [[ ! -f "${OUT_JSON}" ]]; then
  fail "manifest was not created"
fi

OUT_JSON="${OUT_JSON}" FILE_A="${FILE_A}" FILE_B="${FILE_B}" python3 - <<'PY'
import hashlib
import json
import os
import pathlib
import sys

out_json = pathlib.Path(os.environ["OUT_JSON"])
file_a = pathlib.Path(os.environ["FILE_A"])
file_b = pathlib.Path(os.environ["FILE_B"])
payload = json.loads(out_json.read_text(encoding="utf-8"))

if payload.get("schema_version") != 1:
    raise SystemExit("schema_version must be 1")

if payload.get("release", {}).get("tag") != "v0.17.6":
    raise SystemExit("release tag mismatch")

subjects = payload.get("subjects") or []
if len(subjects) != 2:
    raise SystemExit("expected exactly 2 subjects")

def sha256(path: pathlib.Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()

expected = {
    file_a.name: (str(file_a), sha256(file_a), file_a.stat().st_size),
    file_b.name: (str(file_b), sha256(file_b), file_b.stat().st_size),
}

for subject in subjects:
    name = subject.get("name")
    if name not in expected:
        raise SystemExit(f"unexpected subject name: {name}")
    expected_path, expected_sha, expected_size = expected[name]
    if subject.get("path") != expected_path:
        raise SystemExit(f"path mismatch for {name}")
    if subject.get("sha256") != expected_sha:
        raise SystemExit(f"sha mismatch for {name}")
    if subject.get("size_bytes") != expected_size:
        raise SystemExit(f"size mismatch for {name}")
PY

if "${SCRIPT_PATH}" --output "${TMP_DIR}/should-fail.json" --subject "${TMP_DIR}/missing.bin" >/dev/null 2>&1; then
  fail "expected missing subject validation failure"
fi

printf '[provenance-manifest-contract] passed\n'
