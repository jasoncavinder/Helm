#!/usr/bin/env python3
"""Generate a lightweight release-notes HTML page from CHANGELOG.md."""

from __future__ import annotations

import argparse
import datetime as dt
import html
import pathlib
import re
import sys
from typing import Iterable, List


HEADING_RE = re.compile(r"^## \[(?P<version>[^\]]+)\] - (?P<date>\d{4}-\d{2}-\d{2})\s*$")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate release-notes HTML from a Keep-a-Changelog section."
    )
    parser.add_argument(
        "--changelog-path",
        required=True,
        help="Path to CHANGELOG.md",
    )
    parser.add_argument(
        "--tag",
        required=True,
        help="Release tag (for example: v0.17.0-rc.3)",
    )
    parser.add_argument(
        "--output-path",
        required=True,
        help="Output HTML file path",
    )
    parser.add_argument(
        "--canonical-url",
        default="",
        help="Optional canonical URL for the generated page",
    )
    parser.add_argument(
        "--fallback-release-url",
        default="",
        help="Optional fallback URL to include in the page footer",
    )
    return parser.parse_args()


def normalize_version(tag: str) -> str:
    if tag.startswith("v") and len(tag) > 1:
        return tag[1:]
    return tag


def render_inline(text: str) -> str:
    """Render inline code spans and escape all other HTML-sensitive characters."""
    parts = text.split("`")
    rendered: List[str] = []
    for index, part in enumerate(parts):
        escaped = html.escape(part, quote=True)
        if index % 2 == 1:
            rendered.append(f"<code>{escaped}</code>")
        else:
            rendered.append(escaped)
    return "".join(rendered)


def extract_section(changelog_text: str, version: str) -> tuple[str, List[str]]:
    lines = changelog_text.splitlines()
    start_index = -1
    release_date = ""

    for idx, line in enumerate(lines):
        match = HEADING_RE.match(line.strip())
        if match and match.group("version") == version:
            start_index = idx + 1
            release_date = match.group("date")
            break

    if start_index == -1:
        raise ValueError(f"Could not find release section for version '{version}' in changelog.")

    end_index = len(lines)
    for idx in range(start_index, len(lines)):
        if lines[idx].startswith("## ["):
            end_index = idx
            break

    section_lines = lines[start_index:end_index]
    while section_lines and section_lines[0].strip() == "":
        section_lines.pop(0)
    while section_lines and section_lines[-1].strip() == "":
        section_lines.pop()

    return release_date, section_lines


def render_section_body(lines: Iterable[str]) -> str:
    blocks: List[str] = []
    list_open = False

    def close_list() -> None:
        nonlocal list_open
        if list_open:
            blocks.append("    </ul>")
            list_open = False

    for raw_line in lines:
        line = raw_line.rstrip()
        stripped = line.strip()

        if stripped == "":
            close_list()
            continue

        if stripped.startswith("### "):
            close_list()
            heading = render_inline(stripped[4:].strip())
            blocks.append(f"    <h2>{heading}</h2>")
            continue

        bullet_match = re.match(r"^\s*-\s+(.*)$", line)
        if bullet_match:
            if not list_open:
                blocks.append("    <ul>")
                list_open = True
            blocks.append(f"      <li>{render_inline(bullet_match.group(1).strip())}</li>")
            continue

        close_list()
        blocks.append(f"    <p>{render_inline(stripped)}</p>")

    close_list()
    return "\n".join(blocks)


def format_release_date(raw_date: str) -> str:
    try:
        parsed = dt.date.fromisoformat(raw_date)
    except ValueError:
        return raw_date
    return f"{parsed.strftime('%B')} {parsed.day}, {parsed.year}"


def build_html(
    version: str,
    release_date: str,
    body_html: str,
    canonical_url: str,
    fallback_release_url: str,
) -> str:
    canonical_tag = (
        f'  <link rel="canonical" href="{html.escape(canonical_url, quote=True)}">\n'
        if canonical_url
        else ""
    )
    fallback_link = ""
    if fallback_release_url:
        safe_url = html.escape(fallback_release_url, quote=True)
        fallback_link = (
            f'    <p class="meta">Need full context? '
            f'<a href="{safe_url}" rel="noopener noreferrer">View this release on GitHub</a>.</p>\n'
        )

    safe_date = html.escape(release_date, quote=True)
    return f"""<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Helm {html.escape(version, quote=True)} Release Notes</title>
{canonical_tag}  <style>
    :root {{
      color-scheme: light dark;
    }}
    body {{
      margin: 0;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Inter, sans-serif;
      line-height: 1.55;
      background: #f7f8fa;
      color: #111827;
    }}
    main {{
      max-width: 860px;
      margin: 0 auto;
      padding: 24px;
    }}
    article {{
      background: #ffffff;
      border: 1px solid #e5e7eb;
      border-radius: 12px;
      padding: 24px;
    }}
    h1 {{
      margin: 0 0 4px;
      font-size: 1.6rem;
      line-height: 1.3;
    }}
    h2 {{
      margin: 24px 0 8px;
      font-size: 1.1rem;
    }}
    p, ul {{
      margin: 8px 0;
    }}
    ul {{
      padding-left: 20px;
    }}
    .meta {{
      color: #4b5563;
      font-size: 0.95rem;
      margin: 0 0 12px;
    }}
    code {{
      font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
      background: #f3f4f6;
      border-radius: 6px;
      padding: 0.1rem 0.35rem;
      font-size: 0.92em;
    }}
    footer {{
      margin-top: 16px;
      color: #6b7280;
      font-size: 0.9rem;
    }}
    @media (prefers-color-scheme: dark) {{
      body {{
        background: #0b1220;
        color: #e5e7eb;
      }}
      article {{
        background: #0f172a;
        border-color: #1f2937;
      }}
      .meta {{
        color: #9ca3af;
      }}
      code {{
        background: #111827;
      }}
      footer {{
        color: #9ca3af;
      }}
      a {{
        color: #93c5fd;
      }}
    }}
  </style>
</head>
<body>
  <main>
    <article>
      <h1>Helm {html.escape(version, quote=True)} Release Notes</h1>
      <p class="meta">Release date: {safe_date}</p>
{body_html}
{fallback_link}    </article>
    <footer>Generated from <code>CHANGELOG.md</code>.</footer>
  </main>
</body>
</html>
"""


def main() -> int:
    args = parse_args()
    changelog_path = pathlib.Path(args.changelog_path)
    output_path = pathlib.Path(args.output_path)
    version = normalize_version(args.tag)

    if not changelog_path.is_file():
        print(f"error: changelog not found: {changelog_path}", file=sys.stderr)
        return 1

    changelog_text = changelog_path.read_text(encoding="utf-8")
    try:
        raw_date, section_lines = extract_section(changelog_text, version)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 1

    body_html = render_section_body(section_lines)
    release_date = format_release_date(raw_date)
    page = build_html(
        version=version,
        release_date=release_date,
        body_html=body_html,
        canonical_url=args.canonical_url.strip(),
        fallback_release_url=args.fallback_release_url.strip(),
    )

    output_path.parent.mkdir(parents=True, exist_ok=True)
    output_path.write_text(page, encoding="utf-8")
    print(f"Generated release notes page: {output_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
