#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VARIANT="${VARIANT:-}"
TAG_NAME="${TAG_NAME:-}"
OUTPUT_ROOT="${OUTPUT_ROOT:-$ROOT_DIR/build/all-variants}"
INCLUDE_PKG="${INCLUDE_PKG:-}"

info() {
  printf '[build-unsigned-variant] %s\n' "$1" >&2
}

if [[ -z "$VARIANT" ]]; then
  echo "[build-unsigned-variant] error: VARIANT is required (mas|setapp|business|direct)" >&2
  exit 1
fi

if [[ -z "$TAG_NAME" ]]; then
  echo "[build-unsigned-variant] error: TAG_NAME is required (e.g. v0.17.2)" >&2
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "[build-unsigned-variant] error: python3 is required" >&2
  exit 1
fi

if ! command -v xcodebuild >/dev/null 2>&1; then
  echo "[build-unsigned-variant] error: xcodebuild is required" >&2
  exit 1
fi

PROFILE="$("$ROOT_DIR/scripts/distribution_profile.py" variant "$VARIANT" channel_profile)"

OUT_DIR="$OUTPUT_ROOT/$VARIANT"
DERIVED_DATA="$OUT_DIR/DerivedData"
APP_PATH="$DERIVED_DATA/Build/Products/Release/Helm.app"
ZIP_PATH="$OUT_DIR/Helm-${TAG_NAME}-${VARIANT}-unsigned.zip"

info "building variant='$VARIANT' with channel profile='$PROFILE'"

HELM_CHANNEL_PROFILE="$PROFILE" \
  "$ROOT_DIR/apps/macos-ui/scripts/render_channel_xcconfig.sh" \
  "$ROOT_DIR/apps/macos-ui/Generated/HelmChannel.xcconfig"

HELM_CHANNEL_PROFILE="$PROFILE" \
  xcodebuild \
  -project "$ROOT_DIR/apps/macos-ui/Helm.xcodeproj" \
  -scheme Helm \
  -configuration Release \
  -destination "generic/platform=macOS" \
  -derivedDataPath "$DERIVED_DATA" \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  CODE_SIGN_IDENTITY=- \
  build >&2

mkdir -p "$OUT_DIR"
/usr/bin/ditto -c -k --sequesterRsrc --keepParent "$APP_PATH" "$ZIP_PATH"

echo "variant=$VARIANT"
echo "channel_profile=$PROFILE"
echo "out_dir=$OUT_DIR"
echo "app_path=$APP_PATH"
echo "zip_path=$ZIP_PATH"

if [[ -z "$INCLUDE_PKG" ]]; then
  if [[ "$VARIANT" == "business" ]]; then
    INCLUDE_PKG="1"
  else
    INCLUDE_PKG="0"
  fi
fi

if [[ "$INCLUDE_PKG" == "1" ]]; then
  if ! command -v pkgbuild >/dev/null 2>&1; then
    echo "[build-unsigned-variant] error: pkgbuild is required when INCLUDE_PKG=1" >&2
    exit 1
  fi

  PKG_ROOT="$OUT_DIR/pkgroot"
  PKG_PATH="$OUT_DIR/Helm-${TAG_NAME}-${VARIANT}-unsigned.pkg"
  VERSION="${TAG_NAME#v}"

  rm -rf "$PKG_ROOT"
  mkdir -p "$PKG_ROOT/Applications"
  cp -R "$APP_PATH" "$PKG_ROOT/Applications/Helm.app"

  pkgbuild \
    --root "$PKG_ROOT" \
    --identifier "dev.helmapp.helm.$VARIANT" \
    --version "$VERSION" \
    "$PKG_PATH" >&2

  echo "pkg_path=$PKG_PATH"
fi
