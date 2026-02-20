#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "Usage: $0 <path-to-dmg> [app-name]" >&2
}

if [[ $# -lt 1 || $# -gt 2 ]]; then
  usage
  exit 64
fi

DMG_PATH="$1"
APP_NAME="${2:-Helm.app}"

if [[ ! -f "$DMG_PATH" ]]; then
  echo "error: DMG not found: $DMG_PATH" >&2
  exit 1
fi

normalize_bool() {
  printf '%s' "$1" | tr '[:upper:]' '[:lower:]'
}

is_truthy() {
  case "$(normalize_bool "$1")" in
    yes|true|1)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_falsy() {
  case "$(normalize_bool "$1")" in
    no|false|0)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

plist_value() {
  local key="$1"
  local plist="$2"
  /usr/libexec/PlistBuddy -c "Print :$key" "$plist" 2>/dev/null || true
}

ATTACH_PLIST="$(mktemp "${TMPDIR:-/tmp}/helm-dmg-attach.XXXXXX.plist")"
DEVICE=""
MOUNT_PATH=""

cleanup() {
  if [[ -n "$DEVICE" ]]; then
    hdiutil detach "$DEVICE" >/dev/null 2>&1 || hdiutil detach -force "$DEVICE" >/dev/null 2>&1 || true
  fi
  rm -f "$ATTACH_PLIST"
}
trap cleanup EXIT

hdiutil attach -readonly -nobrowse -plist "$DMG_PATH" > "$ATTACH_PLIST"

ATTACH_INFO_RAW="$(python3 - "$ATTACH_PLIST" <<'PY'
import plistlib
import sys

with open(sys.argv[1], 'rb') as f:
    data = plistlib.load(f)

for entity in data.get('system-entities', []):
    device = entity.get('dev-entry')
    mount_point = entity.get('mount-point')
    if device and mount_point:
        print(device)
        print(mount_point)
        break
PY
)"

DEVICE="$(printf '%s\n' "$ATTACH_INFO_RAW" | sed -n '1p')"
MOUNT_PATH="$(printf '%s\n' "$ATTACH_INFO_RAW" | sed -n '2p')"

if [[ -z "$DEVICE" || -z "$MOUNT_PATH" ]]; then
  echo "error: failed to mount DMG and resolve mount path" >&2
  exit 1
fi

APP_PATH="$MOUNT_PATH/$APP_NAME"
APPLICATIONS_LINK="$MOUNT_PATH/Applications"
BACKGROUND_PATH="$MOUNT_PATH/.background/background.png"
APP_INFO_PLIST="$APP_PATH/Contents/Info.plist"
HELM_BIN="$APP_PATH/Contents/MacOS/Helm"
SPARKLE_FRAMEWORK="$APP_PATH/Contents/Frameworks/Sparkle.framework"

if [[ ! -d "$APP_PATH" ]]; then
  echo "error: missing app in DMG: $APP_PATH" >&2
  exit 1
fi

if [[ ! -L "$APPLICATIONS_LINK" ]]; then
  echo "error: missing /Applications symlink in DMG" >&2
  exit 1
fi

if [[ "$(readlink "$APPLICATIONS_LINK")" != "/Applications" ]]; then
  echo "error: Applications symlink does not point to /Applications" >&2
  exit 1
fi

if [[ ! -f "$BACKGROUND_PATH" ]]; then
  echo "error: missing DMG background image: $BACKGROUND_PATH" >&2
  exit 1
fi

if [[ ! -f "$APP_INFO_PLIST" ]]; then
  echo "error: missing app Info.plist in DMG artifact" >&2
  exit 1
fi

CHANNEL="$(plist_value "HelmDistributionChannel" "$APP_INFO_PLIST")"
SPARKLE_ENABLED="$(plist_value "HelmSparkleEnabled" "$APP_INFO_PLIST")"
SPARKLE_ALLOWS_DOWNGRADES="$(plist_value "SUAllowsDowngrades" "$APP_INFO_PLIST")"
SPARKLE_FEED_URL="$(plist_value "SUFeedURL" "$APP_INFO_PLIST")"
SPARKLE_PUBLIC_ED_KEY="$(plist_value "SUPublicEDKey" "$APP_INFO_PLIST")"

if [[ "$CHANNEL" != "developer_id" ]]; then
  echo "error: HelmDistributionChannel must be developer_id in release DMG, found: $CHANNEL" >&2
  exit 1
fi

if ! is_truthy "$SPARKLE_ENABLED"; then
  echo "error: HelmSparkleEnabled must be true in release DMG, found: $SPARKLE_ENABLED" >&2
  exit 1
fi

if ! is_falsy "$SPARKLE_ALLOWS_DOWNGRADES"; then
  echo "error: SUAllowsDowngrades must be false in release DMG, found: $SPARKLE_ALLOWS_DOWNGRADES" >&2
  exit 1
fi

if [[ "$SPARKLE_FEED_URL" != https://* ]]; then
  echo "error: SUFeedURL must use https:// in release DMG" >&2
  exit 1
fi

if [[ -z "$SPARKLE_PUBLIC_ED_KEY" ]]; then
  echo "error: SUPublicEDKey must be present in release DMG" >&2
  exit 1
fi

if [[ ! -d "$SPARKLE_FRAMEWORK" ]]; then
  echo "error: Sparkle.framework missing from DMG app bundle" >&2
  exit 1
fi

if [[ ! -f "$HELM_BIN" ]]; then
  echo "error: Helm executable missing from DMG app bundle" >&2
  exit 1
fi

if ! otool -L "$HELM_BIN" | grep -q "Sparkle.framework"; then
  echo "error: Helm binary in DMG is not linked against Sparkle.framework" >&2
  exit 1
fi

codesign --verify --deep --strict --verbose=2 "$APP_PATH" >/dev/null

echo "DMG verification passed: $DMG_PATH"
