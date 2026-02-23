#!/usr/bin/env python3
"""Read Helm distribution profile contract values.

Usage:
  scripts/distribution_profile.py variant <variant-id> <field[.nested]>
  scripts/distribution_profile.py wrapper <wrapper-id> <field[.nested]>
"""

from __future__ import annotations

import json
import pathlib
import sys
from typing import Any


def fail(message: str) -> int:
    print(f"[distribution-profile] error: {message}", file=sys.stderr)
    return 1


def read_contract() -> dict[str, Any]:
    root = pathlib.Path(__file__).resolve().parent.parent
    contract_path = root / "docs" / "contracts" / "distribution-profiles.json"
    try:
        return json.loads(contract_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise RuntimeError(f"contract file not found: {contract_path}")
    except json.JSONDecodeError as exc:
        raise RuntimeError(f"invalid JSON in {contract_path}: {exc}") from exc


def select_domain(contract: dict[str, Any], domain: str, key: str) -> Any:
    if domain == "variant":
        items = contract.get("variants")
    elif domain == "wrapper":
        items = contract.get("channel_wrappers")
    else:
        raise RuntimeError(f"unsupported domain '{domain}' (expected variant|wrapper)")
    if not isinstance(items, dict):
        raise RuntimeError(f"contract missing object for domain '{domain}'")
    if key not in items:
        raise RuntimeError(f"{domain} '{key}' not found in contract")
    return items[key]


def resolve_path(value: Any, path: str) -> Any:
    current = value
    for segment in path.split("."):
        if not isinstance(current, dict):
            raise RuntimeError(
                f"cannot resolve '{path}': segment '{segment}' reached non-object value"
            )
        if segment not in current:
            raise RuntimeError(f"field '{path}' not found (missing segment '{segment}')")
        current = current[segment]
    return current


def render(value: Any) -> str:
    if isinstance(value, bool):
        return "true" if value else "false"
    if isinstance(value, (str, int, float)):
        return str(value)
    return json.dumps(value, separators=(",", ":"), sort_keys=True)


def main(argv: list[str]) -> int:
    if len(argv) != 4:
        return fail("expected exactly 3 arguments: <variant|wrapper> <id> <field>")
    _, domain, item_key, field_path = argv
    try:
        contract = read_contract()
        base_value = select_domain(contract, domain, item_key)
        field_value = resolve_path(base_value, field_path)
    except RuntimeError as exc:
        return fail(str(exc))
    print(render(field_value))
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
