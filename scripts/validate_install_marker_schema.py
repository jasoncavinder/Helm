#!/usr/bin/env python3
"""Validate Helm install marker JSON using the repository schema contract.

Usage:
  python3 scripts/validate_install_marker_schema.py <schema.json> <marker.json>
"""

from __future__ import annotations

import json
import sys
from pathlib import Path


def fail(message: str) -> int:
    print(f"error: {message}", file=sys.stderr)
    return 1


def load_json(path: Path) -> dict:
    with path.open("r", encoding="utf-8") as handle:
        return json.load(handle)


def validate(schema: dict, marker: dict) -> list[str]:
    errors: list[str] = []
    if not isinstance(schema, dict):
        return ["schema root must be an object"]
    if not isinstance(marker, dict):
        return ["marker payload must be an object"]

    properties = schema.get("properties")
    if not isinstance(properties, dict):
        return ["schema is missing object 'properties'"]

    required = schema.get("required", [])
    if not isinstance(required, list):
        return ["schema 'required' must be an array"]

    additional_properties = bool(schema.get("additionalProperties", True))
    if not additional_properties:
        for key in marker:
            if key not in properties:
                errors.append(f"unexpected property '{key}'")

    for key in required:
        if not isinstance(key, str):
            errors.append("schema required list contains non-string entries")
            continue
        if key not in marker:
            errors.append(f"missing required property '{key}'")

    for key, definition in properties.items():
        if key not in marker:
            continue
        value = marker[key]
        if value is None:
            continue
        if not isinstance(definition, dict):
            continue

        declared_type = definition.get("type")
        if declared_type == "string":
            if not isinstance(value, str):
                errors.append(f"property '{key}' must be a string")
                continue
            min_length = definition.get("minLength")
            if isinstance(min_length, int) and len(value) < min_length:
                errors.append(
                    f"property '{key}' must be at least {min_length} characters"
                )
            enum_values = definition.get("enum")
            if isinstance(enum_values, list) and value not in enum_values:
                errors.append(f"property '{key}' has unsupported value '{value}'")

    return errors


def main(argv: list[str]) -> int:
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2

    schema_path = Path(argv[1])
    marker_path = Path(argv[2])

    if not schema_path.exists():
        return fail(f"schema file not found: {schema_path}")
    if not marker_path.exists():
        return fail(f"marker file not found: {marker_path}")

    schema = load_json(schema_path)
    marker = load_json(marker_path)

    errors = validate(schema, marker)
    if errors:
        for item in errors:
            print(f"error: {item}", file=sys.stderr)
        return 1

    print(f"install marker is schema-valid: {marker_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
