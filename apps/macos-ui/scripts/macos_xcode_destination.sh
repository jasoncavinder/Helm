#!/usr/bin/env bash
set -euo pipefail

arch="${HELM_XCODE_ARCH:-$(uname -m)}"

case "$arch" in
  arm64|x86_64)
    printf 'platform=macOS,arch=%s\n' "$arch"
    ;;
  *)
    printf 'platform=macOS\n'
    ;;
esac
