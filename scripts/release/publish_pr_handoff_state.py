#!/usr/bin/env python3
import json
import sys


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def main() -> None:
    if len(sys.argv) != 2:
        fail("expected one GitHub pull-request JSON payload")

    try:
        payload = json.loads(sys.argv[1])
    except json.JSONDecodeError as error:
        fail(f"invalid pull-request JSON: {error}")

    if not isinstance(payload, dict):
        fail("pull-request JSON must be an object")

    state = str(payload.get("state") or "").strip().upper()
    merged_at = str(payload.get("mergedAt") or "").strip()
    url = str(payload.get("url") or "").strip()
    if state not in {"OPEN", "CLOSED", "MERGED"}:
        fail(f"unsupported pull-request state: {state or '<empty>'}")
    if not url:
        fail("pull-request URL is missing")

    if state == "MERGED" and not merged_at:
        fail("merged pull request is missing mergedAt")
    if state != "MERGED" and merged_at:
        fail(f"{state.lower()} pull request unexpectedly has mergedAt")

    if state == "MERGED":
        status = "merged"
    elif state == "OPEN":
        status = "open"
    else:
        status = "closed_unmerged"

    print(f"PR_STATE={state}")
    print(f"MERGED_AT={merged_at}")
    print(f"PR_URL={url}")
    print(f"HANDOFF_STATUS={status}")


if __name__ == "__main__":
    main()
