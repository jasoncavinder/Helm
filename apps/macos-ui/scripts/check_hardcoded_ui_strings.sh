#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCAN_DIR="${ROOT_DIR}/apps/macos-ui/Helm"

# Matches SwiftUI views/helpers that take a literal string argument.
# Lines containing ".localized" are excluded because they use L10n keys.
PATTERN='Text\("[A-Za-z]|Button\("[A-Za-z]|Toggle\("[A-Za-z]|TextField\("[A-Za-z]|\.alert\("[A-Za-z]|\.help\("[A-Za-z]'

if ! command -v grep >/dev/null 2>&1; then
  echo "::error::grep is required for the hardcoded UI string check" >&2
  exit 2
fi

# Temporarily disable errexit so we can capture grep's exit code.
set +e
MATCHES="$(grep -Ern -- "$PATTERN" "${SCAN_DIR}" | grep -v '.localized')"
GREP_EXIT=$?
set -e

if [[ ${GREP_EXIT} -eq 0 ]]; then
  echo "Found hardcoded UI strings; use L10n keys instead."
  echo ""
  echo "${MATCHES}"
  exit 1
elif [[ ${GREP_EXIT} -eq 1 ]]; then
  echo "No hardcoded UI strings found."
  exit 0
else
  echo "::error::scanner exited with unexpected status ${GREP_EXIT}" >&2
  exit 2
fi
