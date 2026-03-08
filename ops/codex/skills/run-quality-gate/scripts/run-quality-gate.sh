#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../../.." && pwd)"
SCOPE="${1:-all}"

info() {
  printf '[quality-gate] %s\n' "$1"
}

run() {
  info "run: $*"
  (cd "$ROOT_DIR" && "$@")
}

run_rust() {
  run cargo test --workspace --manifest-path core/rust/Cargo.toml
  run cargo fmt --all --manifest-path core/rust/Cargo.toml -- --check
  run cargo clippy --workspace --manifest-path core/rust/Cargo.toml -- -D warnings
}

run_i18n() {
  run apps/macos-ui/scripts/check_locale_integrity.sh
  run apps/macos-ui/scripts/check_locale_lengths.sh

  for locale in en es de fr pt-BR ja hu; do
    run diff -ru "locales/${locale}" "apps/macos-ui/Helm/Resources/locales/${locale}"
  done
}

run_ui() {
  run apps/macos-ui/scripts/check_channel_policy.sh

  if [ "${HELM_SKIP_XCODE:-0}" = "1" ]; then
    info "skip xcodebuild because HELM_SKIP_XCODE=1"
    return
  fi

  run xcodebuild \
    -project apps/macos-ui/Helm.xcodeproj \
    -scheme Helm \
    -destination "platform=macOS" \
    -configuration Debug \
    CODE_SIGN_IDENTITY=- \
    CODE_SIGNING_REQUIRED=NO \
    CODE_SIGNING_ALLOWED=NO \
    test
}

run_release_contracts() {
  run scripts/release/tests/build_unsigned_variant_contract.sh
  run scripts/release/tests/publish_verify_state_contract.sh
  run scripts/release/tests/publish_verify_prerelease_state_contract.sh
  run scripts/release/tests/ci_toolchain_contract.sh
  run scripts/release/tests/provenance_manifest_contract.sh
  run scripts/release/tests/sparkle_recovery_contract.sh
  run scripts/release/check_release_line_copy.sh

  # Non-destructive release-safety checks only.
  run scripts/release/preflight.sh --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy
  run scripts/release/runbook.sh prepare --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy
}

case "$SCOPE" in
  rust)
    run_rust
    ;;
  i18n)
    run_i18n
    ;;
  ui)
    run_ui
    ;;
  release-contracts)
    run_release_contracts
    ;;
  all)
    run_rust
    run_i18n
    run_ui
    run_release_contracts
    ;;
  *)
    echo "Usage: $0 [rust|i18n|ui|release-contracts|all]" >&2
    exit 64
    ;;
esac

info "scope '${SCOPE}' passed"
