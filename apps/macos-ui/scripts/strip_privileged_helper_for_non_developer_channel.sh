#!/usr/bin/env bash
set -euo pipefail

if [[ -z "${TARGET_BUILD_DIR:-}" || -z "${CONTENTS_FOLDER_PATH:-}" ]]; then
  echo "error: missing app bundle build paths for privileged helper channel filter" >&2
  exit 1
fi

if [[ "${HELM_DISTRIBUTION_CHANNEL:-developer_id}" == "developer_id" ]]; then
  exit 0
fi

APP_CONTENTS_DIR="${TARGET_BUILD_DIR}/${CONTENTS_FOLDER_PATH}"
HELPER_BIN="$APP_CONTENTS_DIR/Library/LaunchServices/HelmPrivilegedHelper"
HELPER_PLIST="$APP_CONTENTS_DIR/Library/LaunchDaemons/com.jasoncavinder.Helm.PrivilegedHelper.plist"

rm -f "$HELPER_BIN" "$HELPER_PLIST"
rmdir "$APP_CONTENTS_DIR/Library/LaunchServices" 2>/dev/null || true
rmdir "$APP_CONTENTS_DIR/Library/LaunchDaemons" 2>/dev/null || true
rmdir "$APP_CONTENTS_DIR/Library" 2>/dev/null || true
