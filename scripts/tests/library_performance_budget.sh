#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DERIVED_DATA_PATH="${HELM_LIBRARY_BENCHMARK_DERIVED_DATA:-/tmp/helm-v020-library-benchmark-deriveddata}"
DATABASE_PATH="${HELM_LIBRARY_BENCHMARK_DB:-/tmp/helm-v020-library-benchmark.sqlite}"
ITERATIONS="${HELM_LIBRARY_BENCHMARK_ITERATIONS:-30}"
PROFILE_PATH="${TMPDIR:-/tmp}/helm-v020-library-benchmark-%p.profraw"
APP_BINARY="$DERIVED_DATA_PATH/Build/Products/Debug/Helm.app/Contents/MacOS/Helm"

if [[ ! "$ITERATIONS" =~ ^[0-9]+$ ]] || (( ITERATIONS < 30 || ITERATIONS > 100 )); then
  echo "HELM_LIBRARY_BENCHMARK_ITERATIONS must be an integer from 30 through 100." >&2
  exit 2
fi

cd "$REPO_ROOT"

xcodebuild -quiet \
  -project apps/macos-ui/Helm.xcodeproj \
  -scheme Helm \
  -configuration Debug \
  -destination 'platform=macOS,arch=arm64' \
  -derivedDataPath "$DERIVED_DATA_PATH" \
  CODE_SIGNING_ALLOWED=NO \
  CODE_SIGNING_REQUIRED=NO \
  SWIFT_OPTIMIZATION_LEVEL=-O \
  build

benchmark_output="$({
  HELM_DB_PATH="$DATABASE_PATH" \
  HELM_LIBRARY_BENCHMARK_ITERATIONS="$ITERATIONS" \
  LLVM_PROFILE_FILE="$PROFILE_PATH" \
    "$APP_BINARY"
} 2>&1)"
benchmark_line="$(printf '%s\n' "$benchmark_output" | awk '
  /^HELM_LIBRARY_BENCHMARK / { line = $0 }
  END { print line }
')"

if [[ -z "$benchmark_line" ]]; then
  printf '%s\n' "$benchmark_output" >&2
  echo "Library performance benchmark did not emit a result." >&2
  exit 1
fi

printf '%s\n' "$benchmark_line"
case "$benchmark_line" in
  *'"passed":true'*) ;;
  *)
    echo "Library performance budget failed." >&2
    exit 1
    ;;
esac
