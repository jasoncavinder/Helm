#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
PROJECT_FILE="$ROOT_DIR/apps/macos-ui/Helm.xcodeproj/project.pbxproj"
DESTINATION_SCRIPT="$ROOT_DIR/apps/macos-ui/scripts/macos_xcode_destination.sh"
RUST_BUILD_SCRIPT="$ROOT_DIR/apps/macos-ui/scripts/build_rust.sh"
EXPECTED_ARCH="arm64"
EXPECTED_TARGET="13.0"

host_arch="$(uname -m)"
if [[ "$host_arch" != "$EXPECTED_ARCH" ]]; then
  echo "error: Ventura compatibility lane requires an arm64 host; found $host_arch" >&2
  exit 1
fi

destination="$(HELM_XCODE_ARCH="$EXPECTED_ARCH" "$DESTINATION_SCRIPT")"
if [[ "$destination" != "platform=macOS,arch=$EXPECTED_ARCH" ]]; then
  echo "error: unexpected Xcode destination: $destination" >&2
  exit 1
fi

deployment_targets="$({
  sed -n 's/^[[:space:]]*MACOSX_DEPLOYMENT_TARGET = \([^;]*\);/\1/p' "$PROJECT_FILE"
} | sort -u)"
if [[ "$deployment_targets" != "$EXPECTED_TARGET" ]]; then
  echo "error: every Xcode configuration must target macOS $EXPECTED_TARGET; found:" >&2
  printf '%s\n' "$deployment_targets" >&2
  exit 1
fi

expected_rust_default='export MACOSX_DEPLOYMENT_TARGET="${MACOSX_DEPLOYMENT_TARGET:-13.0}"'
if ! grep -Fq "$expected_rust_default" "$RUST_BUILD_SCRIPT"; then
  echo "error: Rust bridge no longer defaults to macOS $EXPECTED_TARGET" >&2
  exit 1
fi

echo "[ventura-compat] host=$host_arch destination=$destination deployment_target=$EXPECTED_TARGET"
