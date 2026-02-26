#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'EOF'
Usage:
  generate_provenance_manifest.sh --output <path> --subject <path> [--subject <path> ...] [--tag <vX.Y.Z>]

Examples:
  generate_provenance_manifest.sh \
    --output build/release-assets/provenance-v0.17.6.json \
    --subject build/release-assets/Helm-v0.17.6-macos-universal.dmg \
    --subject build/release-assets/appcast.xml \
    --tag v0.17.6
EOF
}

OUTPUT_PATH=""
RELEASE_TAG="${TAG_NAME:-}"
SUBJECTS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      if [[ $# -lt 2 ]]; then
        usage
        exit 1
      fi
      OUTPUT_PATH="$2"
      shift 2
      ;;
    --subject)
      if [[ $# -lt 2 ]]; then
        usage
        exit 1
      fi
      SUBJECTS+=("$2")
      shift 2
      ;;
    --tag)
      if [[ $# -lt 2 ]]; then
        usage
        exit 1
      fi
      RELEASE_TAG="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument '$1'" >&2
      usage
      exit 1
      ;;
  esac
done

if [[ -z "${OUTPUT_PATH}" ]]; then
  echo "error: --output is required" >&2
  usage
  exit 1
fi

if [[ "${#SUBJECTS[@]}" -eq 0 ]]; then
  echo "error: at least one --subject is required" >&2
  usage
  exit 1
fi

for subject in "${SUBJECTS[@]}"; do
  if [[ ! -f "${subject}" ]]; then
    echo "error: subject file not found: ${subject}" >&2
    exit 1
  fi
done

mkdir -p "$(dirname "${OUTPUT_PATH}")"

python3 - "$OUTPUT_PATH" "$RELEASE_TAG" "${SUBJECTS[@]}" <<'PY'
import datetime
import hashlib
import json
import os
import pathlib
import sys

output_path = pathlib.Path(sys.argv[1])
release_tag = sys.argv[2]
subjects = [pathlib.Path(value) for value in sys.argv[3:]]

def sha256sum(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        while True:
            chunk = handle.read(1024 * 1024)
            if not chunk:
                break
            digest.update(chunk)
    return digest.hexdigest()

subject_entries = []
for subject in subjects:
    subject_entries.append(
        {
            "name": subject.name,
            "path": subject.as_posix(),
            "sha256": sha256sum(subject),
            "size_bytes": subject.stat().st_size,
        }
    )

payload = {
    "schema_version": 1,
    "generated_at": datetime.datetime.now(datetime.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
    "builder": {
        "repository": os.environ.get("GITHUB_REPOSITORY", ""),
        "workflow": os.environ.get("GITHUB_WORKFLOW", ""),
        "run_id": os.environ.get("GITHUB_RUN_ID", ""),
        "run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", ""),
        "sha": os.environ.get("GITHUB_SHA", ""),
        "ref": os.environ.get("GITHUB_REF", ""),
    },
    "release": {
        "tag": release_tag,
    },
    "subjects": subject_entries,
}

with output_path.open("w", encoding="utf-8") as handle:
    json.dump(payload, handle, indent=2)
    handle.write("\n")
PY

echo "[provenance] wrote ${OUTPUT_PATH}"
