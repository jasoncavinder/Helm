#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
MERGE_SCRIPT="${ROOT_DIR}/apps/macos-ui/scripts/merge_sparkle_appcast.py"
VERIFY_SCRIPT="${ROOT_DIR}/apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

fail() {
  printf '[sparkle-appcast-channels-contract] error: %s\n' "$1" >&2
  exit 1
}

write_appcast() {
  local path="$1"
  local version="$2"
  local build_version="$3"
  local channel="$4"
  local channel_element=""
  case "$channel" in
    default) ;;
    explicit-default) channel_element="<sparkle:channel>default</sparkle:channel>" ;;
    *) channel_element="<sparkle:channel>${channel}</sparkle:channel>" ;;
  esac

  cat > "$path" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Helm Updates</title>
    <link>https://helmapp.dev/updates/appcast.xml</link>
    <description>Helm direct-channel updates</description>
    <language>en</language>
    <item>
      <title>Helm ${version}</title>
      ${channel_element}
      <enclosure url="https://example.com/Helm-${version}.dmg" sparkle:version="${build_version}" sparkle:shortVersionString="${version}" sparkle:edSignature="test-signature" length="100" type="application/octet-stream"/>
    </item>
  </channel>
</rss>
XML
}

BASE_APPCAST="${TMP_DIR}/base.xml"
BETA_CANDIDATE="${TMP_DIR}/beta.xml"
STABLE_CANDIDATE="${TMP_DIR}/stable.xml"
MERGED_APPCAST="${TMP_DIR}/merged.xml"
UPDATED_APPCAST="${TMP_DIR}/updated.xml"
INVALID_APPCAST="${TMP_DIR}/invalid.xml"
EXPLICIT_DEFAULT_APPCAST="${TMP_DIR}/explicit-default.xml"
NEWER_BETA_CANDIDATE="${TMP_DIR}/newer-beta.xml"
NEWER_MERGED_APPCAST="${TMP_DIR}/newer-merged.xml"

write_appcast "$BASE_APPCAST" "0.18.2" "18002900" "default"
write_appcast "$BETA_CANDIDATE" "0.19.0-rc.1" "19000601" "beta"

"$MERGE_SCRIPT" \
  --base-appcast "$BASE_APPCAST" \
  --candidate-appcast "$BETA_CANDIDATE" \
  --output "$MERGED_APPCAST"
"$VERIFY_SCRIPT" "$MERGED_APPCAST" --expected-version "0.18.2" --expected-channel default
"$VERIFY_SCRIPT" "$MERGED_APPCAST" --expected-version "0.19.0-rc.1" --expected-channel beta

write_appcast "$STABLE_CANDIDATE" "0.19.0" "19000900" "default"
"$MERGE_SCRIPT" \
  --base-appcast "$MERGED_APPCAST" \
  --candidate-appcast "$STABLE_CANDIDATE" \
  --output "$UPDATED_APPCAST"
"$VERIFY_SCRIPT" "$UPDATED_APPCAST" --expected-version "0.19.0" --expected-channel default
"$VERIFY_SCRIPT" "$UPDATED_APPCAST" --expected-version "0.19.0-rc.1" --expected-channel beta

python3 - "$UPDATED_APPCAST" <<'PY'
import sys
import xml.etree.ElementTree as ET

sparkle_ns = "http://www.andymatuschak.org/xml-namespaces/sparkle"
items = ET.parse(sys.argv[1]).getroot().findall("./channel/item")
channels = [
    (item.findtext(f"{{{sparkle_ns}}}channel") or "").strip() or "default"
    for item in items
]
if channels != ["default", "beta"]:
    raise SystemExit(f"unexpected channel order: {channels}")
PY

write_appcast "$INVALID_APPCAST" "0.19.0-rc.1" "19000601" "default"
if "$VERIFY_SCRIPT" "$INVALID_APPCAST" >/dev/null 2>&1; then
  fail "verifier accepted a prerelease version in the default channel"
fi
if "$VERIFY_SCRIPT" "$MERGED_APPCAST" --expected-version "0.19.0-rc.2" --expected-channel beta >/dev/null 2>&1; then
  fail "verifier accepted an unexpected beta-channel version"
fi
if "$VERIFY_SCRIPT" "$MERGED_APPCAST" --expected-version "0.19.0-rc.1" >/dev/null 2>&1; then
  fail "verifier accepted an expected version without an expected channel"
fi

write_appcast "$EXPLICIT_DEFAULT_APPCAST" "0.19.0" "19000900" "explicit-default"
if "$VERIFY_SCRIPT" "$EXPLICIT_DEFAULT_APPCAST" >/dev/null 2>&1; then
  fail "verifier accepted an explicit Sparkle default channel"
fi
if "$MERGE_SCRIPT" \
  --base-appcast "$EXPLICIT_DEFAULT_APPCAST" \
  --candidate-appcast "$BETA_CANDIDATE" \
  --output "$TMP_DIR/explicit-default-merged.xml" >/dev/null 2>&1; then
  fail "merger accepted an explicit Sparkle default channel"
fi

write_appcast "$NEWER_BETA_CANDIDATE" "0.19.0-rc.2" "19000602" "beta"
"$MERGE_SCRIPT" \
  --base-appcast "$UPDATED_APPCAST" \
  --candidate-appcast "$NEWER_BETA_CANDIDATE" \
  --output "$NEWER_MERGED_APPCAST"
if "$MERGE_SCRIPT" \
  --base-appcast "$NEWER_MERGED_APPCAST" \
  --candidate-appcast "$BETA_CANDIDATE" \
  --output "$TMP_DIR/downgraded.xml" >/dev/null 2>&1; then
  fail "merger accepted a beta-channel version regression"
fi
if "$MERGE_SCRIPT" \
  --base-appcast "$UPDATED_APPCAST" \
  --candidate-appcast "$BASE_APPCAST" \
  --output "$TMP_DIR/stable-downgraded.xml" >/dev/null 2>&1; then
  fail "merger accepted a stable-channel version regression"
fi

printf '[sparkle-appcast-channels-contract] passed\n'
