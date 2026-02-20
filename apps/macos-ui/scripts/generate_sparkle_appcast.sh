#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat >&2 <<'USAGE'
Usage: generate_sparkle_appcast.sh \
  --app-path <Helm.app> \
  --dmg-path <Helm.dmg> \
  --output-path <appcast.xml> \
  --download-url <https://.../Helm.dmg> \
  --appcast-url <https://.../appcast.xml> \
  [--release-notes-url <https://...>] \
  [--sparkle-bin-dir <Sparkle bin directory>] \
  [--sparkle-package-path <Sparkle checkout path, legacy fallback>]

Environment:
  HELM_SPARKLE_PRIVATE_ED_KEY   Base64 Sparkle private EdDSA key secret
USAGE
}

APP_PATH=""
DMG_PATH=""
OUTPUT_PATH=""
DOWNLOAD_URL=""
APPCAST_URL=""
RELEASE_NOTES_URL=""
SPARKLE_PACKAGE_PATH=""
SPARKLE_BIN_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app-path)
      APP_PATH="$2"
      shift 2
      ;;
    --dmg-path)
      DMG_PATH="$2"
      shift 2
      ;;
    --output-path)
      OUTPUT_PATH="$2"
      shift 2
      ;;
    --download-url)
      DOWNLOAD_URL="$2"
      shift 2
      ;;
    --appcast-url)
      APPCAST_URL="$2"
      shift 2
      ;;
    --release-notes-url)
      RELEASE_NOTES_URL="$2"
      shift 2
      ;;
    --sparkle-bin-dir)
      SPARKLE_BIN_DIR="$2"
      shift 2
      ;;
    --sparkle-package-path)
      SPARKLE_PACKAGE_PATH="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      exit 64
      ;;
  esac
done

if [[ -z "$APP_PATH" || -z "$DMG_PATH" || -z "$OUTPUT_PATH" || -z "$DOWNLOAD_URL" || -z "$APPCAST_URL" ]]; then
  usage
  exit 64
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: app bundle not found: $APP_PATH" >&2
  exit 1
fi

if [[ ! -f "$DMG_PATH" ]]; then
  echo "error: DMG not found: $DMG_PATH" >&2
  exit 1
fi

if [[ -z "${HELM_SPARKLE_PRIVATE_ED_KEY:-}" ]]; then
  echo "error: HELM_SPARKLE_PRIVATE_ED_KEY is required" >&2
  exit 1
fi

if [[ -z "$SPARKLE_BIN_DIR" && -n "$SPARKLE_PACKAGE_PATH" ]]; then
  LEGACY_BIN_DIR="$(cd "$SPARKLE_PACKAGE_PATH/.." >/dev/null 2>&1 && pwd)/artifacts/sparkle/bin"
  if [[ -x "$LEGACY_BIN_DIR/sign_update" ]]; then
    SPARKLE_BIN_DIR="$LEGACY_BIN_DIR"
  fi
fi

if [[ -z "$SPARKLE_BIN_DIR" ]]; then
  if [[ -d "$PWD/build/DerivedData/SourcePackages/artifacts/sparkle/bin" ]]; then
    SPARKLE_BIN_DIR="$PWD/build/DerivedData/SourcePackages/artifacts/sparkle/bin"
  else
    SPARKLE_BIN_DIR="$(
      find "$HOME/Library/Developer/Xcode/DerivedData" \
        -type f \
        -path '*/SourcePackages/artifacts/sparkle/bin/sign_update' \
        -print -quit 2>/dev/null | sed 's#/sign_update##'
    )"
  fi
fi

SPARKLE_SIGN_UPDATE_BIN="$SPARKLE_BIN_DIR/sign_update"
if [[ ! -x "$SPARKLE_SIGN_UPDATE_BIN" ]]; then
  echo "error: Sparkle sign_update binary not found at $SPARKLE_SIGN_UPDATE_BIN" >&2
  exit 1
fi

INFO_PLIST="$APP_PATH/Contents/Info.plist"
if [[ ! -f "$INFO_PLIST" ]]; then
  echo "error: missing Info.plist in app bundle: $INFO_PLIST" >&2
  exit 1
fi

SHORT_VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleShortVersionString" "$INFO_PLIST")
BUNDLE_VERSION=$(/usr/libexec/PlistBuddy -c "Print :CFBundleVersion" "$INFO_PLIST")
MIN_SYSTEM_VERSION=$(/usr/libexec/PlistBuddy -c "Print :LSMinimumSystemVersion" "$INFO_PLIST" 2>/dev/null || true)

if [[ -z "$RELEASE_NOTES_URL" ]]; then
  RELEASE_NOTES_URL="$APPCAST_URL"
fi

SIGNATURE_RAW="$(
  printf '%s\n' "$HELM_SPARKLE_PRIVATE_ED_KEY" |
    "$SPARKLE_SIGN_UPDATE_BIN" --ed-key-file - -p "$DMG_PATH"
)"
SIGNATURE="$(printf '%s\n' "$SIGNATURE_RAW" | tail -n1 | tr -d '\r\n')"

if [[ ! "$SIGNATURE" =~ ^[A-Za-z0-9+/=]+$ ]]; then
  echo "error: failed to parse Sparkle signature output" >&2
  printf '%s\n' "$SIGNATURE_RAW" >&2
  exit 1
fi

LENGTH=$(stat -f%z "$DMG_PATH")
PUB_DATE=$(LC_ALL=C date -u +"%a, %d %b %Y %H:%M:%S +0000")
MIN_SYSTEM_VERSION_ATTR=""
if [[ -n "$MIN_SYSTEM_VERSION" ]]; then
  MIN_SYSTEM_VERSION_ATTR=" sparkle:minimumSystemVersion=\"$MIN_SYSTEM_VERSION\""
fi

mkdir -p "$(dirname "$OUTPUT_PATH")"

cat > "$OUTPUT_PATH" <<XML
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Helm Updates</title>
    <link>$APPCAST_URL</link>
    <description>Helm direct-channel updates</description>
    <language>en</language>
    <item>
      <title>Helm $SHORT_VERSION</title>
      <pubDate>$PUB_DATE</pubDate>
      <sparkle:releaseNotesLink>$RELEASE_NOTES_URL</sparkle:releaseNotesLink>
      <enclosure
        url="$DOWNLOAD_URL"
        sparkle:version="$BUNDLE_VERSION"
        sparkle:shortVersionString="$SHORT_VERSION"
        sparkle:edSignature="$SIGNATURE"
        $MIN_SYSTEM_VERSION_ATTR
        length="$LENGTH"
        type="application/octet-stream"/>
XML

cat >> "$OUTPUT_PATH" <<'XML'
    </item>
  </channel>
</rss>
XML

echo "Generated Sparkle appcast: $OUTPUT_PATH"
