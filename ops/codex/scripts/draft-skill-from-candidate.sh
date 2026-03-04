#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CANDIDATES_FILE="$ROOT_DIR/ops/codex/docs/SKILL_CANDIDATES.md"
GENERATOR_SCRIPT="$ROOT_DIR/ops/codex/skills/skill-generator/scripts/create_skill.sh"

CANDIDATE_NAME=""
YES=0
DRY_RUN=0

usage() {
  cat <<USAGE
Usage:
  $0 <candidate-name> [--yes] [--dry-run]

Description:
  Extract a candidate WORKFLOW SPEC from ops/codex/docs/SKILL_CANDIDATES.md and draft a skill
  via the existing skill-generator workflow.

Options:
  --yes      Skip interactive confirmation
  --dry-run  Render only; do not write the skill
USAGE
}

confirm() {
  local prompt="$1"
  if [ "$YES" -eq 1 ]; then
    return 0
  fi
  if [ -t 0 ]; then
    printf "%s [y/N]: " "$prompt" >&2
    read -r answer
    case "$answer" in
      y|Y|yes|YES) return 0 ;;
      *) return 1 ;;
    esac
  fi
  return 1
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --yes)
      YES=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      if [ -z "$CANDIDATE_NAME" ]; then
        CANDIDATE_NAME="$1"
        shift
      else
        echo "error: unknown argument: $1" >&2
        usage >&2
        exit 64
      fi
      ;;
  esac
done

if [ -z "$CANDIDATE_NAME" ]; then
  echo "error: candidate name is required" >&2
  usage >&2
  exit 64
fi

if [ ! -f "$CANDIDATES_FILE" ]; then
  echo "error: missing candidates file: $CANDIDATES_FILE" >&2
  echo "hint: run ops/codex/scripts/skill-mine.sh first" >&2
  exit 1
fi

TMP_SPEC="$(mktemp "${TMPDIR:-/tmp}/skill-candidate-spec.XXXXXX")"
trap 'rm -f "$TMP_SPEC"' EXIT

set +e
awk -v cand="$CANDIDATE_NAME" '
BEGIN { in_candidate=0; in_spec=0; found=0 }
$0 == "## Candidate: " cand { in_candidate=1; next }
in_candidate && /^## Candidate: / {
  if (found) exit
  in_candidate=0
}
in_candidate && /^```text$/ && !in_spec {
  in_spec=1
  found=1
  next
}
in_candidate && in_spec && /^```$/ { exit }
in_candidate && in_spec { print }
END {
  if (!found) exit 42
}
' "$CANDIDATES_FILE" > "$TMP_SPEC"
awk_code=$?
set -e

if [ "$awk_code" -eq 42 ]; then
  echo "error: candidate not found: $CANDIDATE_NAME" >&2
  exit 1
elif [ "$awk_code" -ne 0 ]; then
  echo "error: failed to parse candidate spec for: $CANDIDATE_NAME" >&2
  exit 1
fi

if ! rg -q "^WORKFLOW SPEC$" "$TMP_SPEC"; then
  echo "error: extracted block is not a valid WORKFLOW SPEC" >&2
  exit 1
fi

SPEC_NAME="$(awk '
BEGIN { capture=0 }
/^Name:[[:space:]]*$/ { capture=1; next }
/^Name:[[:space:]]*[^[:space:]].*$/ {
  sub(/^Name:[[:space:]]*/, "", $0)
  print
  exit
}
capture == 1 {
  if (NF > 0) {
    print
    exit
  }
}
' "$TMP_SPEC" | sed -E 's/^[[:space:]]+|[[:space:]]+$//g')"

if [ -z "$SPEC_NAME" ]; then
  echo "error: could not parse Name from candidate WORKFLOW SPEC" >&2
  exit 1
fi

SKILL_DIR="$ROOT_DIR/ops/codex/skills/$SPEC_NAME"
if [ -d "$SKILL_DIR" ]; then
  echo "error: target skill already exists: $SKILL_DIR" >&2
  echo "refusing to overwrite existing skills" >&2
  exit 1
fi

if ! confirm "Draft candidate '$CANDIDATE_NAME' as skill '$SPEC_NAME'?"; then
  echo "error: draft canceled (confirmation required)" >&2
  exit 1
fi

if [ -x "$GENERATOR_SCRIPT" ]; then
  cmd=("$GENERATOR_SCRIPT" --spec "$TMP_SPEC" --confirm-name)
  if [ "$DRY_RUN" -eq 1 ]; then
    cmd+=(--dry-run)
  fi

  echo "[draft-skill] running: ${cmd[*]}"
  "${cmd[@]}"
  exit 0
fi

# Fallback when programmatic generator invocation is unavailable.
echo "[draft-skill] generator script unavailable; printing exact prompt fallback"
echo
echo "Paste this prompt into Codex:"
echo "------------------------------------------------------------"
echo "Use skill: skill-generator. Create a reusable skill from this WORKFLOW SPEC."
echo "Do not overwrite existing skills. Update ops/codex/docs/USAGE.md with invocation guidance."
echo
cat "$TMP_SPEC"
echo "------------------------------------------------------------"
