#!/usr/bin/env python3
"""Validate Helm's append-only SQLite migration identity manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path
from typing import Any


ROOT = Path(__file__).resolve().parents[2]
MIGRATIONS_PATH = ROOT / "core/rust/crates/helm-core/src/sqlite/migrations.rs"
MANIFEST_PATH = (
    ROOT / "core/rust/crates/helm-core/resources/sqlite_migration_manifest.json"
)
MANIFEST_RELATIVE = MANIFEST_PATH.relative_to(ROOT).as_posix()
MANIFEST_SCHEMA = "helm.sqlite-migration-manifest"
MIGRATION_PATTERN = re.compile(
    r"const MIGRATION_(\d{4}): SqliteMigration = SqliteMigration \{\s*"
    r"version: (\d+),\s*"
    r'name: "([^"]+)",\s*'
    r'up_sql: r#"(.*?)"#,\s*'
    r'down_sql: r#"(.*?)"#,\s*'
    r"\};",
    re.DOTALL,
)


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def definition_checksum(version: int, name: str, up_sql: str, down_sql: str) -> str:
    digest = hashlib.sha256()
    digest.update(b"helm.sqlite-migration.v1\0")
    digest.update(version.to_bytes(8, byteorder="big", signed=True))
    for field in (name.encode(), up_sql.encode(), down_sql.encode()):
        digest.update(len(field).to_bytes(8, byteorder="big"))
        digest.update(field)
    return digest.hexdigest()


def source_entries() -> list[dict[str, Any]]:
    source = MIGRATIONS_PATH.read_text(encoding="utf-8")
    matches = MIGRATION_PATTERN.findall(source)
    if not matches:
        fail(f"no migrations parsed from {MIGRATIONS_PATH.relative_to(ROOT)}")

    entries: list[dict[str, Any]] = []
    for index, (suffix, raw_version, name, up_sql, down_sql) in enumerate(matches, 1):
        version = int(raw_version)
        if int(suffix) != version or version != index:
            fail(f"migration sequence is not contiguous at version {index}")
        entries.append(
            {
                "version": version,
                "name": name,
                "definition_sha256": definition_checksum(
                    version, name, up_sql, down_sql
                ),
            }
        )
    return entries


def manifest_document(entries: list[dict[str, Any]]) -> dict[str, Any]:
    return {
        "schema": MANIFEST_SCHEMA,
        "schema_version": 1,
        "migrations": entries,
    }


def load_manifest(raw: str, source: str) -> dict[str, Any]:
    try:
        document = json.loads(raw)
    except json.JSONDecodeError as error:
        fail(f"invalid migration manifest from {source}: {error}")
    if not isinstance(document, dict):
        fail(f"migration manifest from {source} must be an object")
    if document.get("schema") != MANIFEST_SCHEMA or document.get("schema_version") != 1:
        fail(f"unsupported migration manifest schema in {source}")
    migrations = document.get("migrations")
    if not isinstance(migrations, list):
        fail(f"migration manifest entries in {source} must be an array")
    return document


def validate_current_manifest(entries: list[dict[str, Any]]) -> dict[str, Any]:
    manifest = load_manifest(MANIFEST_PATH.read_text(encoding="utf-8"), MANIFEST_RELATIVE)
    if manifest["migrations"] != entries:
        fail(
            "SQLite migration definitions differ from the committed manifest; "
            "released entries are immutable and new entries must be appended"
        )
    return manifest


def validate_append_only(base_ref: str, current: dict[str, Any]) -> None:
    base_commit = subprocess.run(
        ["git", "cat-file", "-e", f"{base_ref}^{{commit}}"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if base_commit.returncode != 0:
        fail(f"cannot resolve migration manifest base ref {base_ref!r}")

    base_manifest = subprocess.run(
        ["git", "cat-file", "-e", f"{base_ref}:{MANIFEST_RELATIVE}"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if base_manifest.returncode != 0:
        print(
            "SQLite migration manifest is new on the base ref; accepting initial bootstrap."
        )
        return

    result = subprocess.run(
        ["git", "show", f"{base_ref}:{MANIFEST_RELATIVE}"],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        fail(f"cannot read migration manifest from base ref {base_ref!r}")

    base = load_manifest(result.stdout, f"{base_ref}:{MANIFEST_RELATIVE}")
    base_entries = base["migrations"]
    current_entries = current["migrations"]
    if len(current_entries) < len(base_entries):
        fail("SQLite migration manifest entries may not be removed")
    if current_entries[: len(base_entries)] != base_entries:
        fail("existing SQLite migration manifest entries may not be modified or reordered")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--base-ref",
        help="Git ref whose committed manifest must remain an exact prefix",
    )
    parser.add_argument(
        "--emit",
        action="store_true",
        help="print a manifest generated from the current Rust definitions",
    )
    args = parser.parse_args()

    entries = source_entries()
    if args.emit:
        print(json.dumps(manifest_document(entries), indent=2) + "\n", end="")
        return

    current = validate_current_manifest(entries)
    if args.base_ref:
        validate_append_only(args.base_ref, current)
    print(f"SQLite migration manifest validated ({len(entries)} immutable entries).")


if __name__ == "__main__":
    main()
