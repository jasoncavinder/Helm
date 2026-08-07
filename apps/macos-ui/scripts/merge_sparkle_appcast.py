#!/usr/bin/env python3
import argparse
import copy
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


SPARKLE_NS = "http://www.andymatuschak.org/xml-namespaces/sparkle"
CHANNEL_TAG = f"{{{SPARKLE_NS}}}channel"
SUPPORTED_CHANNELS = {"default", "beta"}


def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    raise SystemExit(1)


def item_channel(item: ET.Element) -> str:
    return (item.findtext(CHANNEL_TAG) or "").strip() or "default"


def read_items(path: Path) -> tuple[ET.ElementTree, ET.Element, list[ET.Element]]:
    try:
        tree = ET.parse(path)
    except (OSError, ET.ParseError) as error:
        fail(f"unable to parse appcast {path}: {error}")
    channel = tree.getroot().find("channel")
    if channel is None:
        fail(f"missing <channel> in appcast: {path}")
    return tree, channel, channel.findall("item")


def validate_unique_channels(items: list[ET.Element], source: str) -> dict[str, ET.Element]:
    by_channel: dict[str, ET.Element] = {}
    for item in items:
        channel_name = item_channel(item)
        if channel_name not in SUPPORTED_CHANNELS:
            fail(f"unsupported channel '{channel_name}' in {source}")
        if channel_name in by_channel:
            fail(f"multiple '{channel_name}' items in {source}")
        by_channel[channel_name] = item
    return by_channel


def main() -> None:
    parser = argparse.ArgumentParser(description="Merge a signed Sparkle candidate into Helm's channel-aware appcast")
    parser.add_argument("--base-appcast", required=True, type=Path)
    parser.add_argument("--candidate-appcast", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()

    base_tree, base_channel, base_items = read_items(args.base_appcast)
    _, _, candidate_items = read_items(args.candidate_appcast)
    if len(candidate_items) != 1:
        fail(f"candidate appcast must contain exactly one item, found {len(candidate_items)}")

    merged = validate_unique_channels(base_items, str(args.base_appcast))
    candidate_by_channel = validate_unique_channels(candidate_items, str(args.candidate_appcast))
    merged.update(candidate_by_channel)
    if "default" not in merged:
        fail("merged appcast would not contain a stable default-channel item")

    for item in base_items:
        base_channel.remove(item)
    for channel_name in ("default", "beta"):
        if channel_name in merged:
            base_channel.append(copy.deepcopy(merged[channel_name]))

    ET.register_namespace("sparkle", SPARKLE_NS)
    ET.indent(base_tree, space="  ")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    base_tree.write(args.output, encoding="utf-8", xml_declaration=True)
    print(f"Merged Sparkle appcast: {args.output}")


if __name__ == "__main__":
    main()
