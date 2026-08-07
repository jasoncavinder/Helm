#!/usr/bin/env bash
set -euo pipefail

APPCAST_PATH=""
EXPECTED_VERSION=""
EXPECTED_CHANNEL=""

if [ $# -lt 1 ]; then
  echo "Usage: $0 <appcast.xml> [--expected-version <version>] [--expected-channel <default|beta>]" >&2
  exit 64
fi

APPCAST_PATH="$1"
shift
while [ $# -gt 0 ]; do
  case "$1" in
    --expected-version)
      [ $# -ge 2 ] || { echo "error: --expected-version requires a value" >&2; exit 64; }
      EXPECTED_VERSION="$2"
      shift 2
      ;;
    --expected-channel)
      [ $# -ge 2 ] || { echo "error: --expected-channel requires a value" >&2; exit 64; }
      EXPECTED_CHANNEL="$2"
      shift 2
      ;;
    *)
      echo "error: unknown option: $1" >&2
      exit 64
      ;;
  esac
done

if [ -n "$EXPECTED_CHANNEL" ] && [ "$EXPECTED_CHANNEL" != "default" ] && [ "$EXPECTED_CHANNEL" != "beta" ]; then
  echo "error: expected channel must be default or beta" >&2
  exit 64
fi
if [ -n "$EXPECTED_VERSION" ] && [ -z "$EXPECTED_CHANNEL" ]; then
  echo "error: --expected-version requires --expected-channel" >&2
  exit 64
fi
if [ ! -f "$APPCAST_PATH" ]; then
  echo "error: appcast not found: $APPCAST_PATH" >&2
  exit 1
fi

python3 - "$APPCAST_PATH" "$EXPECTED_VERSION" "$EXPECTED_CHANNEL" <<'PY'
import sys
import xml.etree.ElementTree as ET
from urllib.parse import urlparse

appcast_path = sys.argv[1]
expected_version = sys.argv[2]
expected_channel = sys.argv[3]
sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
sparkle_delta_attr = f"{{{sparkle_ns}}}deltaFrom"
sparkle_sig_attr = f"{{{sparkle_ns}}}edSignature"
sparkle_version_attr = f"{{{sparkle_ns}}}version"
sparkle_short_version_attr = f"{{{sparkle_ns}}}shortVersionString"
sparkle_channel_tag = f"{{{sparkle_ns}}}channel"

def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)

root = ET.parse(appcast_path).getroot()
channel = root.find("channel")
if channel is None:
    fail("missing <channel> node")

items = channel.findall("item")
if not 1 <= len(items) <= 2:
    fail(f"expected one or two appcast <item> nodes, found {len(items)}")

versions_by_channel = {}
for item in items:
    channel_name = (item.findtext(sparkle_channel_tag) or "").strip() or "default"
    if channel_name not in {"default", "beta"}:
        fail(f"unsupported Sparkle channel: {channel_name}")
    if channel_name in versions_by_channel:
        fail(f"found multiple appcast items for channel: {channel_name}")

    enclosures = item.findall("enclosure")
    if len(enclosures) != 1:
        fail(f"expected exactly one <enclosure> for {channel_name}, found {len(enclosures)}")

    enclosure = enclosures[0]
    for required_attr in (sparkle_sig_attr, sparkle_version_attr, sparkle_short_version_attr):
        if not enclosure.get(required_attr):
            fail(f"missing required enclosure attribute for {channel_name}: {required_attr}")

    if enclosure.get(sparkle_delta_attr):
        fail("delta updates are disabled by policy; found sparkle:deltaFrom on enclosure")

    short_version = enclosure.get(sparkle_short_version_attr, "").strip()
    if channel_name == "default" and "-" in short_version:
        fail(f"default channel must contain a stable version, got: {short_version}")
    if channel_name == "beta" and "-" not in short_version:
        fail(f"beta channel must contain a prerelease version, got: {short_version}")
    versions_by_channel[channel_name] = short_version

    url = enclosure.get("url", "")
    parsed = urlparse(url)
    if parsed.scheme != "https":
        fail(f"enclosure URL must use https://, got: {url}")
    if not parsed.path.lower().endswith(".dmg"):
        fail(f"enclosure URL must target a DMG payload, got: {url}")

    for node in item.iter():
        if node.tag == f"{{{sparkle_ns}}}deltas":
            fail("delta updates are disabled by policy; found sparkle:deltas node")
        for attr_name in node.attrib:
            if attr_name == sparkle_delta_attr:
                fail("delta updates are disabled by policy; found sparkle:deltaFrom attribute")

if "default" not in versions_by_channel:
    fail("missing stable default-channel appcast item")

if expected_channel:
    actual_version = versions_by_channel.get(expected_channel)
    if actual_version is None:
        fail(f"missing expected appcast channel: {expected_channel}")
    if expected_version and actual_version != expected_version:
        fail(
            f"{expected_channel} channel version mismatch: "
            f"expected {expected_version}, got {actual_version}"
        )

print("Sparkle appcast policy checks passed.")
PY
