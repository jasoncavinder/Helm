#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCAN_DIR="${SCAN_DIR:-${ROOT_DIR}/apps/macos-ui/Helm}"

# Matches SwiftUI views/helpers that take a literal string argument.
# Lines containing ".localized" are excluded because they use L10n keys.
PATTERN='Text\("[A-Za-z]|Button\("[A-Za-z]|Toggle\("[A-Za-z]|TextField\("[A-Za-z]|\.alert\("[A-Za-z]|\.help\("[A-Za-z]'

if ! command -v grep >/dev/null 2>&1; then
  echo "::error::grep is required for the hardcoded UI string check" >&2
  exit 2
fi

RAW_MATCHES_FILE="$(mktemp)"
trap 'rm -f "${RAW_MATCHES_FILE}"' EXIT

# Run the primary scan separately so its exit status cannot be hidden
# by the subsequent .localized filtering step.
set +e
grep -Ern -- "${PATTERN}" "${SCAN_DIR}" >"${RAW_MATCHES_FILE}"
SCAN_EXIT=$?
set -e

case "${SCAN_EXIT}" in
  0)
    # The scanner found candidate matches. Filter legitimate localized uses
    # and independently validate the filter's exit status.
    set +e
    MATCHES="$(grep -vF '.localized' "${RAW_MATCHES_FILE}")"
    FILTER_EXIT=$?
    set -e

    case "${FILTER_EXIT}" in
      0)
        echo "Found hardcoded UI strings; use L10n keys instead."
        echo
        printf '%s\n' "${MATCHES}"
        exit 1
        ;;

      1)
        echo "No hardcoded UI strings found."
        exit 0
        ;;

      *)
        echo "::error::hardcoded UI string result filtering failed with status ${FILTER_EXIT}" >&2
        exit 2
        ;;
    esac
    ;;

  1)
    echo "No hardcoded UI strings found."
    exit 0
    ;;

  *)
    echo "::error::hardcoded UI string scan failed with status ${SCAN_EXIT}" >&2
    exit 2
    ;;
esac
