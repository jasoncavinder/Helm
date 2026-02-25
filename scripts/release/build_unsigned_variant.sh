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

error() {
  printf '[build-unsigned-variant] error: %s\n' "$1" >&2
}

canonicalize_path() {
  python3 - "$1" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).expanduser().resolve(strict=False))
PY
}

path_within_base() {
  python3 - "$1" "$2" <<'PY'
from pathlib import Path
import sys

base = Path(sys.argv[1]).expanduser().resolve(strict=False)
candidate = Path(sys.argv[2]).expanduser().resolve(strict=False)

try:
    candidate.relative_to(base)
except ValueError:
    raise SystemExit(1)
PY
}

assert_path_within() {
  local base_path="$1"
  local candidate_path="$2"
  local field_name="$3"

  if ! path_within_base "$base_path" "$candidate_path"; then
    error "${field_name} resolves outside allowed output root (base=${base_path}, candidate=${candidate_path})"
    exit 1
  fi
}

if [[ -z "$VARIANT" ]]; then
  error "VARIANT is required (mas|setapp|business|direct)"
  exit 1
fi

if [[ -z "$TAG_NAME" ]]; then
  error "TAG_NAME is required (e.g. v0.17.2)"
  exit 1
fi

if [[ ! "$TAG_NAME" =~ ^v[0-9]+\.[0-9]+\.[0-9]+(-rc\.[0-9]+)?$ ]]; then
  error "TAG_NAME must match vX.Y.Z or vX.Y.Z-rc.N"
  exit 1
fi

if ! command -v python3 >/dev/null 2>&1; then
  error "python3 is required"
  exit 1
fi

PROFILE="$("$ROOT_DIR/scripts/distribution_profile.py" variant "$VARIANT" channel_profile)"

OUT_DIR="$OUTPUT_ROOT/$VARIANT"
OUTPUT_ROOT="$(canonicalize_path "$OUTPUT_ROOT")"
OUT_DIR="$(canonicalize_path "$OUT_DIR")"
assert_path_within "$OUTPUT_ROOT" "$OUT_DIR" "OUT_DIR"
DERIVED_DATA="$OUT_DIR/DerivedData"
APP_PATH="$DERIVED_DATA/Build/Products/Release/Helm.app"
ZIP_PATH="$OUT_DIR/Helm-${TAG_NAME}-${VARIANT}-unsigned.zip"
DERIVED_DATA="$(canonicalize_path "$DERIVED_DATA")"
APP_PATH="$(canonicalize_path "$APP_PATH")"
ZIP_PATH="$(canonicalize_path "$ZIP_PATH")"
assert_path_within "$OUT_DIR" "$DERIVED_DATA" "DERIVED_DATA"
assert_path_within "$OUT_DIR" "$APP_PATH" "APP_PATH"
assert_path_within "$OUT_DIR" "$ZIP_PATH" "ZIP_PATH"

if ! command -v xcodebuild >/dev/null 2>&1; then
  error "xcodebuild is required"
  exit 1
fi

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
    error "pkgbuild is required when INCLUDE_PKG=1"
    exit 1
  fi

  PKG_ROOT="$OUT_DIR/pkgroot"
  PKG_PATH="$OUT_DIR/Helm-${TAG_NAME}-${VARIANT}-unsigned.pkg"
  PKG_ROOT="$(canonicalize_path "$PKG_ROOT")"
  PKG_PATH="$(canonicalize_path "$PKG_PATH")"
  assert_path_within "$OUT_DIR" "$PKG_ROOT" "PKG_ROOT"
  assert_path_within "$OUT_DIR" "$PKG_PATH" "PKG_PATH"
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
