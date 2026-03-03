#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TAG_NAME="${1:-}"

info() {
  printf '[sparkle-checklist] %s\n' "$1"
}

run() {
  info "run: $*"
  (cd "$ROOT_DIR" && "$@")
}

# Always run non-destructive local contracts first.
run apps/macos-ui/scripts/verify_sparkle_appcast_policy.sh web/public/updates/appcast.xml
run scripts/release/tests/sparkle_recovery_contract.sh
run scripts/release/check_release_line_copy.sh
run scripts/release/tests/rehearsal_dry_run_contract.sh

# Optional: run full rehearsal only when explicitly enabled.
if [ -n "$TAG_NAME" ] && [ "${HELM_ENABLE_REHEARSAL:-0}" = "1" ]; then
  run scripts/release/rehearsal_dry_run.sh --tag "$TAG_NAME"
else
  info "skip full rehearsal (set HELM_ENABLE_REHEARSAL=1 and pass <tag> to enable)"
fi

info "sparkle/appcast checklist passed"
