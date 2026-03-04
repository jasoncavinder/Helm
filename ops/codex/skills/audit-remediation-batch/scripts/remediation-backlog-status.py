#!/usr/bin/env python3
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[3]
BACKLOG = ROOT / "docs" / "audits" / "remediation-backlog.md"

if not BACKLOG.exists():
    print(f"error: missing backlog file: {BACKLOG}", file=sys.stderr)
    raise SystemExit(1)

text = BACKLOG.read_text(encoding="utf-8")
lines = text.splitlines()

# IDs explicitly marked done in status section.
done_ids = set(re.findall(r"- `([A-Z]+-\d+[A-Z]?)` — Done", text))

# Parse prioritized backlog markdown table.
rows = []
in_table = False
for line in lines:
    if line.startswith("| ID | Severity | Category"):
        in_table = True
        continue
    if in_table and not line.startswith("|"):
        break
    if not in_table:
        continue
    if line.startswith("|---"):
        continue

    parts = [part.strip() for part in line.strip("|").split("|")]
    if len(parts) < 9:
        continue
    item_id = parts[0]
    if not re.fullmatch(r"[A-Z]+-\d+[A-Z]?", item_id):
        continue
    rows.append(
        {
            "id": item_id,
            "severity": parts[1],
            "category": parts[2],
            "area": parts[3],
            "effort": parts[5],
            "risk": parts[6],
            "status": "done" if item_id in done_ids else "open",
            "acceptance": parts[8],
        }
    )

requested = set(arg.upper() for arg in sys.argv[1:])
if requested:
    rows = [row for row in rows if row["id"] in requested]

if not rows:
    print("No matching backlog rows found.")
    raise SystemExit(0)

print("| ID | Status | Severity | Category | Effort | Risk | Area |")
print("|---|---|---|---|---|---|---|")
for row in rows:
    print(
        f"| {row['id']} | {row['status']} | {row['severity']} | {row['category']} | {row['effort']} | {row['risk']} | {row['area']} |"
    )
