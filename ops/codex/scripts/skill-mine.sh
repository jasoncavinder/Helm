#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
INPUT_PATH="$ROOT_DIR/dev/logs/codex-runs.ndjson"
OUTPUT_PATH="$ROOT_DIR/ops/codex/docs/SKILL_CANDIDATES.md"
TOP_N=10

usage() {
  cat <<USAGE
Usage:
  $0 [--input <ndjson>] [--output <markdown>] [--top <n>]

Defaults:
  --input  $ROOT_DIR/dev/logs/codex-runs.ndjson
  --output $ROOT_DIR/ops/codex/docs/SKILL_CANDIDATES.md
  --top    10
USAGE
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --input)
      INPUT_PATH="${2:-}"
      shift 2
      ;;
    --output)
      OUTPUT_PATH="${2:-}"
      shift 2
      ;;
    --top)
      TOP_N="${2:-}"
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

mkdir -p "$(dirname "$OUTPUT_PATH")"

python3 - "$INPUT_PATH" "$OUTPUT_PATH" "$TOP_N" <<'PY'
import json
import math
import re
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path

input_path = Path(sys.argv[1])
output_path = Path(sys.argv[2])
top_n = int(sys.argv[3])

now = datetime.now(timezone.utc)

STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "be", "by", "for", "from", "in", "into", "is", "it", "of", "on", "or", "that", "the", "to", "with",
    "this", "these", "those", "now", "then", "than", "using", "use", "used", "run", "runs", "running", "update", "updated", "updating",
    "create", "created", "creating", "implement", "implemented", "implementing", "refine", "refined", "refining", "fix", "fixed", "work", "workflow",
    "codex", "model", "system", "task", "tasks", "session", "sessions", "report", "reports", "change", "changes", "file", "files",
    "add", "added", "adding", "set", "setting", "settings", "new", "existing", "final", "summary", "scaffold", "scaffolded", "done",
}

NAME_STOPWORDS = STOPWORDS | {
    "quality", "gate", "docs", "skill", "skills", "candidate", "candidates", "notify", "logging", "script", "scripts",
}

VERB_HINTS = {
    "analyze": "Analyze recurring context and scope.",
    "review": "Review relevant code and documentation context.",
    "implement": "Implement scoped updates for the workflow objective.",
    "update": "Update source-of-truth docs and supporting assets.",
    "create": "Create required artifacts or scaffolding.",
    "validate": "Run targeted validation and capture results.",
    "generate": "Generate deterministic outputs from inputs.",
    "triage": "Triage issues and isolate root cause.",
    "test": "Run focused tests for touched areas.",
    "mine": "Mine telemetry/history for repeatable patterns.",
    "draft": "Draft reusable assets for operator review.",
}


def parse_ts(value):
    if not value:
        return None
    text = str(value).strip()
    if not text:
        return None
    try:
        if text.endswith("Z"):
            text = text[:-1] + "+00:00"
        return datetime.fromisoformat(text).astimezone(timezone.utc)
    except ValueError:
        return None


def tokenize_summary(summary):
    words = re.findall(r"[a-z0-9]+", (summary or "").lower())
    return {w for w in words if len(w) > 2 and w not in STOPWORDS}


def normalize_components(paths):
    comps = set()
    for raw in paths:
        if not isinstance(raw, str):
            continue
        path = raw.strip()
        if not path:
            continue
        if " -> " in path:
            path = path.split(" -> ", 1)[1]

        if path.startswith("core/rust/"):
            comps.add("core/rust")
        elif path.startswith("apps/macos-ui/"):
            comps.add("apps/macos-ui")
        elif path.startswith("service/macos-service/"):
            comps.add("service/macos-service")
        elif path.startswith("scripts/release/"):
            comps.add("scripts/release")
        elif path.startswith("scripts/tests/"):
            comps.add("scripts/tests")
        elif path.startswith("scripts/"):
            comps.add("scripts")
        elif path.startswith("docs/"):
            comps.add("docs")
        elif path.startswith("web/"):
            comps.add("web")
        elif path.startswith(".github/workflows/"):
            comps.add(".github/workflows")
        else:
            parts = path.split("/")
            if len(parts) >= 2:
                comps.add(f"{parts[0]}/{parts[1]}")
            else:
                comps.add(parts[0])
    return comps


def extract_tags(obj):
    tags = set()
    for key in ("command_tags", "tags", "commands"):
        value = obj.get(key)
        if isinstance(value, list):
            for item in value:
                if isinstance(item, str) and item.strip():
                    tags.add(item.strip().lower())
        elif isinstance(value, str) and value.strip():
            for piece in re.split(r"[,;\s]+", value.strip().lower()):
                if piece:
                    tags.add(piece)
    return tags


def jaccard(left, right):
    if not left and not right:
        return 0.0
    union = left | right
    if not union:
        return 0.0
    return len(left & right) / len(union)


def slugify(text):
    val = text.strip().lower()
    val = re.sub(r"[^a-z0-9\s-]", "", val)
    val = re.sub(r"[\s_]+", "-", val)
    val = re.sub(r"-+", "-", val).strip("-")
    return val


def top_terms(counter, n, deny=None):
    deny = deny or set()
    out = []
    for term, _ in counter.most_common():
        if term in deny:
            continue
        out.append(term)
        if len(out) >= n:
            break
    return out


def infer_verbs(summaries):
    verbs = Counter()
    for summary in summaries:
        words = re.findall(r"[a-z0-9]+", summary.lower())
        if not words:
            continue
        first = words[0]
        if first in VERB_HINTS:
            verbs[first] += 1
        for w in words[:8]:
            if w in VERB_HINTS:
                verbs[w] += 1
    return [verb for verb, _ in verbs.most_common(4)]


entries = []
if input_path.exists() and input_path.stat().st_size > 0:
    for line_no, line in enumerate(input_path.read_text(encoding="utf-8", errors="replace").splitlines(), start=1):
        raw = line.strip()
        if not raw:
            continue
        try:
            obj = json.loads(raw)
        except json.JSONDecodeError:
            continue

        summary = ""
        for key in ("summary", "last_agent_message", "final_output", "message", "output_text", "text"):
            val = obj.get(key)
            if isinstance(val, str) and val.strip():
                summary = val.strip()
                break

        changed_files = obj.get("changed_files")
        if not isinstance(changed_files, list):
            changed_files = obj.get("changed_files_sample")
        if not isinstance(changed_files, list):
            changed_files = []

        tags = extract_tags(obj)
        tokens = tokenize_summary(summary)
        comps = normalize_components(changed_files)

        if not summary and not comps and not tags:
            continue

        ts = parse_ts(obj.get("timestamp") or obj.get("timestamp_utc"))
        entries.append(
            {
                "summary": summary or "(no summary)",
                "tokens": tokens,
                "components": comps,
                "tags": tags,
                "ts": ts,
                "changed_files": changed_files,
                "line_no": line_no,
            }
        )

lines = []
lines.append("# Skill Candidates")
lines.append("")
lines.append(f"Generated (UTC): `{now.replace(microsecond=0).isoformat().replace('+00:00', 'Z')}`")
lines.append(f"Source log: `{input_path}`")
lines.append("")
lines.append("This report proposes reusable skill candidates from telemetry. It does **not** create skills automatically.")
lines.append("")
lines.append("Approval-required drafting command:")
lines.append("")
lines.append("```bash")
lines.append("ops/codex/scripts/draft-skill-from-candidate.sh <candidate-name>")
lines.append("```")
lines.append("")

if not entries:
    lines.append("## No telemetry entries detected")
    lines.append("")
    lines.append("No NDJSON telemetry records were found. Run some Codex sessions, then rerun:")
    lines.append("")
    lines.append("```bash")
    lines.append("ops/codex/scripts/skill-mine.sh")
    lines.append("```")
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"[skill-mine] wrote {output_path}")
    sys.exit(0)

clusters = []

for entry in entries:
    best_idx = -1
    best_score = 0.0

    for idx, cluster in enumerate(clusters):
        c_tokens = set(term for term, _ in cluster["token_counter"].most_common(14))
        c_comps = set(comp for comp, _ in cluster["component_counter"].most_common(8))
        c_tags = set(tag for tag, _ in cluster["tag_counter"].most_common(8))

        s_sim = jaccard(entry["tokens"], c_tokens)
        c_sim = jaccard(entry["components"], c_comps)
        t_sim = jaccard(entry["tags"], c_tags) if (entry["tags"] or c_tags) else 0.0
        score = 0.45 * s_sim + 0.45 * c_sim + 0.10 * t_sim

        if score > best_score:
            best_score = score
            best_idx = idx

    if best_idx >= 0 and best_score >= 0.30:
        cluster = clusters[best_idx]
        cluster["entries"].append(entry)
        cluster["token_counter"].update(entry["tokens"])
        cluster["component_counter"].update(entry["components"])
        cluster["tag_counter"].update(entry["tags"])
        cluster["assign_scores"].append(best_score)
    else:
        clusters.append(
            {
                "entries": [entry],
                "token_counter": Counter(entry["tokens"]),
                "component_counter": Counter(entry["components"]),
                "tag_counter": Counter(entry["tags"]),
                "assign_scores": [1.0],
            }
        )

repeat_clusters = [c for c in clusters if len(c["entries"]) >= 2]

if not repeat_clusters:
    lines.append("## No repeated candidates detected")
    lines.append("")
    lines.append("Telemetry exists, but no repeated clusters exceeded the current detection threshold.")
    lines.append("Try rerunning after more sessions, or lower clustering thresholds in `ops/codex/scripts/skill-mine.sh` if needed.")
    output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"[skill-mine] wrote {output_path}")
    sys.exit(0)

ranked = []
used_names = set()

for cluster in repeat_clusters:
    count = len(cluster["entries"])
    avg_score = sum(cluster["assign_scores"]) / max(1, len(cluster["assign_scores"]))

    latest_ts = None
    for e in cluster["entries"]:
        if e["ts"] and (latest_ts is None or e["ts"] > latest_ts):
            latest_ts = e["ts"]

    if latest_ts is None:
        recency_bonus = 0.0
    else:
        age_days = max(0.0, (now - latest_ts).total_seconds() / 86400.0)
        recency_bonus = max(0.0, 1.0 - min(age_days, 30.0) / 30.0) * 2.0

    priority = count * 2.0 + avg_score * 5.0 + recency_bonus

    top_components = [c for c, _ in cluster["component_counter"].most_common(4)]
    top_tokens = top_terms(cluster["token_counter"], 4, deny=NAME_STOPWORDS)

    if top_tokens:
        name_seed = "-".join(top_tokens[:3])
    elif top_components:
        name_seed = "-".join(re.split(r"[/_-]+", top_components[0])[:3])
    else:
        name_seed = "repeated-workflow"

    skill_name = slugify(name_seed) or "repeated-workflow"
    base_name = skill_name
    suffix = 2
    while skill_name in used_names:
        skill_name = f"{base_name}-{suffix}"
        suffix += 1
    used_names.add(skill_name)

    component_phrase = ", ".join(top_components) if top_components else "mixed components"
    token_phrase = " ".join(top_tokens[:2]) if top_tokens else "repeated"

    purpose = (
        f"Standardize a repeated '{token_phrase}' workflow observed across {component_phrase}. "
        f"This candidate is based on {count} similar telemetry entries."
    )

    inputs = [
        "- task objective and expected outcome",
        f"- target scope/components (common: {component_phrase})",
    ]

    lower_tokens = set(top_tokens)
    if any(t in lower_tokens for t in {"release", "appcast", "publish", "runbook", "tag"}):
        inputs.append("- release tag/channel context (dry-run only unless explicitly approved)")
    if any(t in lower_tokens for t in {"audit", "remediation", "security"}):
        inputs.append("- remediation IDs or audit references")
    if any(t in lower_tokens for t in {"test", "quality", "lint", "build", "verify"}):
        inputs.append("- validation scope (targeted vs full)")
    if cluster["tag_counter"]:
        tag_list = ", ".join(tag for tag, _ in cluster["tag_counter"].most_common(5))
        inputs.append(f"- optional command tags ({tag_list})")

    outputs = [
        "- concise execution summary",
        "- touched component/file summary",
        "- verification results and next-step recommendations",
    ]

    verbs = infer_verbs([e["summary"] for e in cluster["entries"]])
    steps = [
        "1. Confirm the workflow scope, boundaries, and desired outcome.",
        f"2. Gather required context from dominant components ({component_phrase}).",
    ]

    step_idx = 3
    for verb in verbs:
        steps.append(f"{step_idx}. {VERB_HINTS[verb]}")
        step_idx += 1

    steps.extend(
        [
            f"{step_idx}. Execute the recurring core actions inferred from similar summaries.",
            f"{step_idx + 1}. Run targeted validation aligned to impacted components.",
            f"{step_idx + 2}. Record outcomes and capture concise telemetry-style summary.",
        ]
    )

    inferred_step_count = max(len(steps), count * 3)
    if inferred_step_count > 25:
        split_suggestion = (
            "Split recommended: separate this candidate into component-specific sub-skills "
            "(for example core vs docs vs release checks)."
        )
    else:
        split_suggestion = "No split suggested (<=25 inferred steps)."

    safety = [
        "- no secrets, credentials, signing keys, or provisioning material",
        "- no automatic release/appcast/website publication",
        "- no destructive operations without explicit confirmation",
    ]

    optional_scripts = "yes" if (count >= 3 or any(c.startswith("scripts") for c in top_components)) else "no"

    examples = []
    for e in cluster["entries"][:3]:
        examples.append(e["summary"])

    ranked.append(
        {
            "skill_name": skill_name,
            "priority": priority,
            "count": count,
            "components": top_components,
            "purpose": purpose,
            "inputs": inputs,
            "outputs": outputs,
            "steps": steps,
            "safety": safety,
            "optional_scripts": optional_scripts,
            "split_suggestion": split_suggestion,
            "examples": examples,
        }
    )

ranked.sort(key=lambda c: (c["priority"], c["count"]), reverse=True)
ranked = ranked[:top_n]

lines.append("## Top Candidates")
lines.append("")
lines.append("| Rank | Candidate | Priority | Repeats | Components |")
lines.append("|---|---|---:|---:|---|")
for idx, cand in enumerate(ranked, start=1):
    comps = ", ".join(cand["components"][:3]) if cand["components"] else "(none)"
    lines.append(f"| {idx} | `{cand['skill_name']}` | {cand['priority']:.2f} | {cand['count']} | {comps} |")
lines.append("")

for idx, cand in enumerate(ranked, start=1):
    lines.append(f"## Candidate: {cand['skill_name']}")
    lines.append("")
    lines.append(f"- Rank: {idx}")
    lines.append(f"- Priority Score: `{cand['priority']:.2f}`")
    lines.append(f"- Repeated Entries: `{cand['count']}`")
    lines.append(f"- Dominant Components: `{', '.join(cand['components']) if cand['components'] else '(none)'}`")
    lines.append("")
    lines.append("Example summaries:")
    for ex in cand["examples"]:
        lines.append(f"- {ex}")
    lines.append("")
    lines.append("### WORKFLOW SPEC")
    lines.append("")
    lines.append("```text")
    lines.append("WORKFLOW SPEC")
    lines.append("")
    lines.append("Name:")
    lines.append(cand["skill_name"])
    lines.append("")
    lines.append("Purpose:")
    lines.append(cand["purpose"])
    lines.append("")
    lines.append("Inputs:")
    lines.extend(line for line in cand["inputs"])
    lines.append("")
    lines.append("Outputs:")
    lines.extend(line for line in cand["outputs"])
    lines.append("")
    lines.append("Steps:")
    lines.extend(line for line in cand["steps"])
    lines.append("")
    lines.append("Safety Constraints:")
    lines.extend(line for line in cand["safety"])
    lines.append("")
    lines.append("Optional Scripts:")
    lines.append(cand["optional_scripts"])
    lines.append("")
    lines.append("Suggested Split:")
    lines.append(cand["split_suggestion"])
    lines.append("```")
    lines.append("")
    lines.append("Draft command (approval required):")
    lines.append("")
    lines.append("```bash")
    lines.append(f"ops/codex/scripts/draft-skill-from-candidate.sh {cand['skill_name']}")
    lines.append("```")
    lines.append("")

output_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
print(f"[skill-mine] wrote {output_path}")
PY
