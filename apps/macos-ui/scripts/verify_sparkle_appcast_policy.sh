#!/usr/bin/env bash
set -euo pipefail

if [ $# -ne 1 ]; then
  echo "Usage: $0 <appcast.xml>" >&2
  exit 64
fi

APPCAST_PATH="$1"
if [ ! -f "$APPCAST_PATH" ]; then
  echo "error: appcast not found: $APPCAST_PATH" >&2
  exit 1
fi

python3 - "$APPCAST_PATH" <<'PY'
import sys
import xml.etree.ElementTree as ET
from urllib.parse import urlparse

appcast_path = sys.argv[1]
sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
sparkle_delta_attr = f"{{{sparkle_ns}}}deltaFrom"
sparkle_sig_attr = f"{{{sparkle_ns}}}edSignature"
sparkle_version_attr = f"{{{sparkle_ns}}}version"
sparkle_short_version_attr = f"{{{sparkle_ns}}}shortVersionString"

def fail(message: str) -> None:
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)

root = ET.parse(appcast_path).getroot()
channel = root.find("channel")
if channel is None:
    fail("missing <channel> node")

items = channel.findall("item")
if len(items) != 1:
    fail(f"expected exactly one appcast <item>, found {len(items)}")

item = items[0]
enclosures = item.findall("enclosure")
if len(enclosures) != 1:
    fail(f"expected exactly one <enclosure>, found {len(enclosures)}")

enclosure = enclosures[0]
for required_attr in (sparkle_sig_attr, sparkle_version_attr, sparkle_short_version_attr):
    if not enclosure.get(required_attr):
        fail(f"missing required enclosure attribute: {required_attr}")

if enclosure.get(sparkle_delta_attr):
    fail("delta updates are disabled by policy; found sparkle:deltaFrom on enclosure")

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

print("Sparkle appcast policy checks passed.")
PY
