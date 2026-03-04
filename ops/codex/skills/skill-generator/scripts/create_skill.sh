#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TEMPLATE_PATH="$ROOT_DIR/ops/codex/skills/skill-generator/templates/SKILL.template.md"
SKILLS_ROOT="$ROOT_DIR/skills"

SPEC_FILE=""
FORCE=0
DRY_RUN=0
CONFIRM_NAME=0
INIT_SPEC_PATH=""

usage() {
  cat <<USAGE
Usage:
  $0 --spec <workflow-spec.md> [options]
  $0 --init-spec <path> [--force]

Required workflow:
  conversation/workflow -> WORKFLOW SPEC -> generated skill

Options:
      --spec <path>             Path to WORKFLOW SPEC file
      --confirm-name            Confirm skill name non-interactively
      --skills-root <path>      Override skills root directory (default: $ROOT_DIR/skills)
      --force                   Overwrite existing SKILL.md when target skill exists
      --dry-run                 Render output only; do not write files
      --init-spec <path>        Write a WORKFLOW SPEC scaffold template to path
  -h, --help                    Show this help
USAGE
}

confirm_name_interactive() {
  local skill_name="$1"
  if [ -t 0 ]; then
    printf "Confirm skill name '%s'? [y/N]: " "$skill_name" >&2
    read -r answer
    case "$answer" in
      y|Y|yes|YES) return 0 ;;
      *) return 1 ;;
    esac
  fi
  return 1
}

confirm_overwrite() {
  local target="$1"
  if [ -t 0 ]; then
    printf "Skill '%s' already exists. Overwrite SKILL.md? [y/N]: " "$target" >&2
    read -r answer
    case "$answer" in
      y|Y|yes|YES) return 0 ;;
      *) return 1 ;;
    esac
  fi
  return 1
}

init_spec_template() {
  local path="$1"
  local dir
  dir="$(dirname "$path")"
  mkdir -p "$dir"

  if [ -f "$path" ] && [ "$FORCE" -ne 1 ]; then
    echo "error: spec file already exists at $path (use --force to overwrite)" >&2
    exit 1
  fi

  cat > "$path" <<'SPEC'
WORKFLOW SPEC

Name:
<kebab-case skill name>

Purpose:
One or two sentence description.

Inputs:
- input 1
- input 2

Outputs:
- output 1
- output 2

Steps:
1. First workflow step.
2. Second workflow step.

Safety Constraints:
- Constraint 1
- Constraint 2

Optional Scripts:
no

Optional Resources:
no
SPEC

  echo "[skill-generator] wrote spec scaffold: $path"
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --spec)
      SPEC_FILE="${2:-}"
      shift 2
      ;;
    --confirm-name)
      CONFIRM_NAME=1
      shift
      ;;
    --skills-root)
      SKILLS_ROOT="${2:-}"
      shift 2
      ;;
    --force)
      FORCE=1
      shift
      ;;
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --init-spec)
      INIT_SPEC_PATH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage >&2
      exit 64
      ;;
  esac
done

if [ -n "$INIT_SPEC_PATH" ]; then
  init_spec_template "$INIT_SPEC_PATH"
  exit 0
fi

if [ -z "$SPEC_FILE" ]; then
  echo "error: --spec is required" >&2
  usage >&2
  exit 64
fi

if [ ! -f "$SPEC_FILE" ]; then
  echo "error: spec file not found: $SPEC_FILE" >&2
  exit 1
fi

if [ ! -f "$TEMPLATE_PATH" ]; then
  echo "error: missing template: $TEMPLATE_PATH" >&2
  exit 1
fi

PARSED_JSON="$(SPEC_FILE="$SPEC_FILE" TEMPLATE_PATH="$TEMPLATE_PATH" python3 - <<'PY'
import json
import os
import re
import sys
from pathlib import Path

spec_path = Path(os.environ["SPEC_FILE"])
template_path = Path(os.environ["TEMPLATE_PATH"])
text = spec_path.read_text(encoding="utf-8")

if "WORKFLOW SPEC" not in text:
    print("error: missing 'WORKFLOW SPEC' header", file=sys.stderr)
    sys.exit(1)

allowed_headers = {
    "Name",
    "Purpose",
    "When to Use",
    "Inputs",
    "Outputs",
    "Steps",
    "Safety Constraints",
    "Optional Scripts",
    "Optional Resources",
}

sections = {h: [] for h in allowed_headers}
current = None

for raw_line in text.splitlines():
    line = raw_line.rstrip("\n")
    stripped = line.strip()

    if stripped == "WORKFLOW SPEC":
        continue

    m = re.match(r"^(Name|Purpose|When to Use|Inputs|Outputs|Steps|Safety Constraints|Optional Scripts|Optional Resources):\s*(.*)$", stripped)
    if m:
        current = m.group(1)
        value = m.group(2)
        if value:
            sections[current].append(value)
        continue

    if current is not None:
        sections[current].append(line)

required = ["Name", "Purpose", "Inputs", "Outputs", "Steps", "Safety Constraints", "Optional Scripts"]
missing = [key for key in required if not "\n".join(sections[key]).strip()]
if missing:
    print(f"error: missing required spec sections: {', '.join(missing)}", file=sys.stderr)
    sys.exit(1)

def normalize_kebab(value: str) -> str:
    value = value.strip().lower()
    value = re.sub(r"[ _]+", "-", value)
    value = re.sub(r"[^a-z0-9-]", "", value)
    value = re.sub(r"-+", "-", value).strip("-")
    return value

name_raw = "\n".join(sections["Name"]).strip().splitlines()[0]
skill_name = normalize_kebab(name_raw)
if not skill_name:
    print("error: invalid skill name after kebab-case normalization", file=sys.stderr)
    sys.exit(1)

purpose = "\n".join(sections["Purpose"]).strip()
when_to_use = "\n".join(sections["When to Use"]).strip()
if not when_to_use:
    when_to_use = f"Use when you need to run the '{skill_name}' workflow repeatedly and consistently."

inputs_raw = "\n".join(sections["Inputs"]).strip()
outputs_raw = "\n".join(sections["Outputs"]).strip()
steps_raw = "\n".join(sections["Steps"]).strip()
safety_raw = "\n".join(sections["Safety Constraints"]).strip()
optional_scripts_raw = "\n".join(sections["Optional Scripts"]).strip().lower()
optional_resources_raw = "\n".join(sections["Optional Resources"]).strip().lower()


def split_lines(block: str):
    lines = [ln.strip() for ln in block.splitlines() if ln.strip()]
    return lines


def to_bullets(block: str) -> str:
    lines = split_lines(block)
    if not lines:
        return "- none specified"
    out = []
    for line in lines:
        if re.match(r"^[-*]\s+", line) or re.match(r"^\d+\.\s+", line):
            out.append(line)
        else:
            out.append(f"- {line}")
    return "\n".join(out)


def to_ordered(block: str) -> str:
    lines = split_lines(block)
    if not lines:
        return "1. No workflow steps provided."
    out = []
    idx = 1
    for line in lines:
        if re.match(r"^\d+\.\s+", line):
            out.append(line)
        else:
            out.append(f"{idx}. {line}")
            idx += 1
    return "\n".join(out)

inputs_md = to_bullets(inputs_raw)
outputs_md = to_bullets(outputs_raw)
safety_md = to_bullets(safety_raw)
steps_md = to_ordered(steps_raw)

text_for_checks = "\n".join(split_lines(inputs_raw) + split_lines(steps_raw) + [purpose]).lower()
step_lines = split_lines(steps_raw)

unsafe_reasons = []

# Secrets/credentials handling.
secret_action_patterns = [
    r"\b(use|set|export|store|load|copy|paste|inject|provide|read|write|upload|download|share)\b.*\b(api[-_ ]?key|secret|token|password|private key|\.p12|provisioning profile|notarization key|signing cert|certificate)\b",
    r"\b(api[-_ ]?key|secret|token|password|private key|\.p12|provisioning profile|notarization key|signing cert|certificate)\b.*\b(use|set|export|store|load|copy|paste|inject|provide|read|write|upload|download|share)\b",
]
for pat in secret_action_patterns:
    if re.search(pat, text_for_checks):
        unsafe_reasons.append("spec appears to include secret/credential handling")
        break

# Automatic publish/release/appcast/deploy paths.
auto_publish_patterns = [
    r"\bgh\s+release\s+create\b",
    r"\bpublish\b.*\brelease\b",
    r"\bappcast\b.*\bpublish\b",
    r"\bdeploy\b.*\bwebsite\b",
    r"\bnotariz(e|ation)\b.*\bsubmit\b",
]
for pat in auto_publish_patterns:
    if re.search(pat, text_for_checks):
        unsafe_reasons.append("spec appears to automate release/appcast/deploy publication")
        break

# Destructive commands without explicit confirmation language.
destructive_patterns = [
    r"\brm\s+-rf\b",
    r"\bgit\s+reset\s+--hard\b",
    r"\bgit\s+clean\s+-fd\b",
    r"\bgit\s+branch\s+-D\b",
]
for line in step_lines:
    lower = line.lower()
    for pat in destructive_patterns:
        if re.search(pat, lower):
            if not re.search(r"\b(confirm|confirmation|explicit approval|user approval)\b", lower):
                unsafe_reasons.append("destructive operation listed without explicit confirmation guard")
            break

if unsafe_reasons:
    print("error: refusing to generate unsafe skill:\n- " + "\n- ".join(sorted(set(unsafe_reasons))), file=sys.stderr)
    sys.exit(1)

optional_scripts = optional_scripts_raw in {"yes", "y", "true", "1", "required"}
optional_resources = optional_resources_raw in {"yes", "y", "true", "1", "required"}

description = purpose.replace("\n", " ").strip()
if len(description) > 180:
    description = description[:177] + "..."

template = template_path.read_text(encoding="utf-8")
rendered = (
    template.replace("{{skill_name}}", skill_name)
    .replace("{{skill_description}}", description)
    .replace("{{purpose}}", purpose)
    .replace("{{when_to_use}}", when_to_use)
    .replace("{{inputs}}", inputs_md)
    .replace("{{outputs}}", outputs_md)
    .replace("{{safety_rules}}", safety_md)
    .replace("{{workflow_steps}}", steps_md)
)

out = {
    "skill_name": skill_name,
    "normalized_name_changed": skill_name != name_raw,
    "name_raw": name_raw,
    "description": description,
    "optional_scripts": optional_scripts,
    "optional_resources": optional_resources,
    "rendered": rendered,
}
print(json.dumps(out))
PY
)"

SKILL_NAME="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["skill_name"])' <<< "$PARSED_JSON")"
RAW_NAME="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["name_raw"])' <<< "$PARSED_JSON")"
DESC="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["description"])' <<< "$PARSED_JSON")"
WITH_SCRIPTS="$(python3 -c 'import json,sys; print("1" if json.loads(sys.stdin.read())["optional_scripts"] else "0")' <<< "$PARSED_JSON")"
WITH_RESOURCES="$(python3 -c 'import json,sys; print("1" if json.loads(sys.stdin.read())["optional_resources"] else "0")' <<< "$PARSED_JSON")"
RENDERED_CONTENT="$(python3 -c 'import json,sys; print(json.loads(sys.stdin.read())["rendered"])' <<< "$PARSED_JSON")"

if [ "$SKILL_NAME" != "$RAW_NAME" ]; then
  echo "info: normalized skill name '$RAW_NAME' -> '$SKILL_NAME'" >&2
fi

if [ "$CONFIRM_NAME" -ne 1 ]; then
  if ! confirm_name_interactive "$SKILL_NAME"; then
    if [ -t 0 ]; then
      echo "error: skill name not confirmed" >&2
    else
      echo "error: non-interactive execution requires --confirm-name" >&2
    fi
    exit 1
  fi
fi

TARGET_DIR="$SKILLS_ROOT/$SKILL_NAME"
TARGET_SKILL_MD="$TARGET_DIR/SKILL.md"

if [ -d "$TARGET_DIR" ] && [ "$FORCE" -ne 1 ]; then
  if ! confirm_overwrite "$SKILL_NAME"; then
    echo "error: skill already exists at $TARGET_DIR (use --force to overwrite)" >&2
    exit 1
  fi
fi

if [ "$DRY_RUN" -eq 1 ]; then
  echo "[skill-generator] dry-run spec: $SPEC_FILE"
  echo "[skill-generator] skill name: $SKILL_NAME"
  echo "[skill-generator] description: $DESC"
  echo "[skill-generator] scripts/: $([ "$WITH_SCRIPTS" -eq 1 ] && echo yes || echo no)"
  echo "[skill-generator] resources/: $([ "$WITH_RESOURCES" -eq 1 ] && echo yes || echo no)"
  echo "[skill-generator] rendered target: $TARGET_SKILL_MD"
  echo "[skill-generator] rendered content:"
  echo "$RENDERED_CONTENT"
  exit 0
fi

mkdir -p "$TARGET_DIR"
printf '%s\n' "$RENDERED_CONTENT" > "$TARGET_SKILL_MD"

if [ "$WITH_SCRIPTS" -eq 1 ]; then
  mkdir -p "$TARGET_DIR/scripts"
fi

if [ "$WITH_RESOURCES" -eq 1 ]; then
  mkdir -p "$TARGET_DIR/resources"
fi

echo "[skill-generator] created skill: $SKILL_NAME"
echo "[skill-generator] path: $TARGET_DIR"
echo "[skill-generator] SKILL.md: $TARGET_SKILL_MD"
echo "[skill-generator] scripts/: $([ "$WITH_SCRIPTS" -eq 1 ] && echo created || echo not-requested)"
echo "[skill-generator] resources/: $([ "$WITH_RESOURCES" -eq 1 ] && echo created || echo not-requested)"
echo "[skill-generator] next step: update ops/codex/docs/USAGE.md with invocation guidance for '$SKILL_NAME'"
