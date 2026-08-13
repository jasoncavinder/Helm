#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PROJECT_FILE="$ROOT_DIR/apps/macos-ui/Helm.xcodeproj/project.pbxproj"
DAEMON_PLIST="$ROOT_DIR/apps/macos-ui/HelmPrivilegedHelper/com.jasoncavinder.Helm.PrivilegedHelper.plist"
RELEASE_WORKFLOW="$ROOT_DIR/.github/workflows/release-macos-dmg.yml"
DMG_VERIFIER="$ROOT_DIR/apps/macos-ui/scripts/verify_release_dmg.sh"
CHANNEL_FILTER_SCRIPT="$ROOT_DIR/apps/macos-ui/scripts/strip_privileged_helper_for_non_developer_channel.sh"

python3 - "$DAEMON_PLIST" <<'PY'
import plistlib
import sys

with open(sys.argv[1], "rb") as handle:
    plist = plistlib.load(handle)

label = "com.jasoncavinder.Helm.PrivilegedHelper"
assert plist["Label"] == label
assert plist["BundleProgram"] == "Contents/Library/LaunchServices/HelmPrivilegedHelper"
assert plist["ProgramArguments"] == ["HelmPrivilegedHelper", "--daemon"]
assert plist["MachServices"] == {label: True}
PY

grep -Fq 'productType = "com.apple.product-type.tool";' "$PROJECT_FILE"
grep -Fq 'dstPath = Contents/Library/LaunchServices;' "$PROJECT_FILE"
grep -Fq 'dstPath = Contents/Library/LaunchDaemons;' "$PROJECT_FILE"
grep -Fq 'ATTRIBUTES = (CodeSignOnCopy, );' "$PROJECT_FILE"
grep -Fq 'Strip Privileged Helper For Non-Developer Channel' "$PROJECT_FILE"
grep -Fq 'strip_privileged_helper_for_non_developer_channel.sh' "$PROJECT_FILE"
grep -Fq 'HELM_DISTRIBUTION_CHANNEL:-developer_id' "$CHANNEL_FILTER_SCRIPT"
grep -Fq 'Contents/Library/LaunchServices/HelmPrivilegedHelper' "$RELEASE_WORKFLOW"
grep -Fq 'Identifier=com.jasoncavinder.Helm.PrivilegedHelper' "$RELEASE_WORKFLOW"
grep -Fq 'Contents/Library/LaunchServices/HelmPrivilegedHelper' "$DMG_VERIFIER"
grep -Fq 'Contents/Library/LaunchDaemons/com.jasoncavinder.Helm.PrivilegedHelper.plist' "$DMG_VERIFIER"

echo "Privileged helper packaging contract checks passed."
