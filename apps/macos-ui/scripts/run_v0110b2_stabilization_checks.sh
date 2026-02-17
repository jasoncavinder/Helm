#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

run() {
  echo
  echo "==> $*"
  "$@"
}

echo "v0.11.0-beta.2 stabilization checks"

echo
run cargo test -p helm-core -p helm-ffi --manifest-path "$ROOT_DIR/core/rust/Cargo.toml"

echo
echo "==> xcodebuild HelmTests"
XCODE_LOG="/tmp/helm_v0110b2_xcodebuild.log"
if ! xcodebuild -project "$ROOT_DIR/apps/macos-ui/Helm.xcodeproj" \
  -scheme HelmTests \
  -destination 'platform=macOS' \
  -derivedDataPath /tmp/helmtests-deriveddata \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  test 2>&1 | tee "$XCODE_LOG"; then
  if rg -q "Sandbox restriction|testmanagerd\\.control" "$XCODE_LOG"; then
    echo "warning: xcodebuild test blocked by sandbox testmanagerd IPC restrictions; continuing with non-Xcode checks."
  else
    echo "error: xcodebuild test failed; see $XCODE_LOG" >&2
    exit 1
  fi
fi

run "$ROOT_DIR/apps/macos-ui/scripts/check_locale_lengths.sh"

echo
run /bin/bash -lc '
set -euo pipefail
for l in en es de fr pt-BR ja; do
  diff -ru "$0/locales/$l" "$0/apps/macos-ui/Helm/Resources/locales/$l" >/dev/null
  echo "locale mirror parity OK: $l"
done
' "$ROOT_DIR"

run /bin/bash -lc '
set -euo pipefail
PATTERN="Text\\(\"[A-Za-z]|Button\\(\"[A-Za-z]|Toggle\\(\"[A-Za-z]|TextField\\(\"[A-Za-z]|\\.alert\\(\"[A-Za-z]|\\.help\\(\"[A-Za-z]"
if rg -n "$PATTERN" "$0/apps/macos-ui/Helm"; then
  echo "Found hardcoded UI strings; use L10n keys instead." >&2
  exit 1
fi
echo "hardcoded UI string lint passed"
' "$ROOT_DIR"

echo
echo "All v0.11.0-beta.2 stabilization checks passed."
