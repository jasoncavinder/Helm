#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUTPUT_ROOT="${HELM_BUILD_OUTPUT_DIR:-$ROOT_DIR/build/variants}"

declare -a OPTIONAL_FAILURES=()

usage() {
  cat <<'EOF'
Usage:
  scripts/build.sh direct
  scripts/build.sh mas
  scripts/build.sh setapp
  scripts/build.sh business
  scripts/build.sh all
  scripts/build.sh cli

Description:
  Builds Helm artifacts for local validation by distribution profile.
  Outputs are written to build/variants by default.

Notes:
  - These are unsigned local artifacts (not release/notarized builds).
  - The "all" mode is best-effort for profile-specific GUI packaging steps.
EOF
}

info() {
  printf '[build] %s\n' "$1"
}

warn() {
  printf '[build] warning: %s\n' "$1" >&2
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    printf '[build] error: required tool not found: %s\n' "$tool" >&2
    return 1
  fi
}

variant_field() {
  local variant="$1"
  local field="$2"
  python3 "$ROOT_DIR/scripts/distribution_profile.py" variant "$variant" "$field"
}

prepare_output_dir() {
  mkdir -p "$OUTPUT_ROOT"
}

build_cli_release() {
  require_tool cargo
  prepare_output_dir

  info "building helm-cli release binary"
  (
    cd "$ROOT_DIR/core/rust"
    cargo build -p helm-cli --release
  )

  local src="$ROOT_DIR/core/rust/target/release/helm"
  local dst_dir="$OUTPUT_ROOT/cli"
  local dst="$dst_dir/helm"

  mkdir -p "$dst_dir"
  if [ ! -f "$src" ]; then
    printf '[build] error: expected CLI binary not found: %s\n' "$src" >&2
    return 1
  fi

  cp "$src" "$dst"
  chmod +x "$dst"
  info "cli artifact: $dst"
}

render_channel_config() {
  local profile="$1"
  HELM_CHANNEL_PROFILE="$profile" \
    "$ROOT_DIR/apps/macos-ui/scripts/render_channel_xcconfig.sh" \
    "$ROOT_DIR/apps/macos-ui/Generated/HelmChannel.xcconfig" >/dev/null
}

build_gui_variant() {
  local variant="$1"
  local profile
  profile="$(variant_field "$variant" channel_profile)"

  require_tool xcodebuild
  prepare_output_dir
  render_channel_config "$profile"

  local variant_dir="$OUTPUT_ROOT/$variant"
  local derived_data="$variant_dir/DerivedData"
  local built_app="$derived_data/Build/Products/Release/Helm.app"
  local output_app="$variant_dir/Helm.app"

  info "building GUI variant '$variant' (profile=$profile)"
  mkdir -p "$variant_dir"

  (
    cd "$ROOT_DIR"
    HELM_CHANNEL_PROFILE="$profile" \
      xcodebuild \
      -project apps/macos-ui/Helm.xcodeproj \
      -scheme Helm \
      -configuration Release \
      -destination "generic/platform=macOS" \
      -derivedDataPath "$derived_data" \
      CODE_SIGNING_ALLOWED=NO \
      CODE_SIGNING_REQUIRED=NO \
      CODE_SIGN_IDENTITY=- \
      build >/dev/null
  )

  if [ ! -d "$built_app" ]; then
    printf '[build] error: expected app artifact not found: %s\n' "$built_app" >&2
    return 1
  fi

  rm -rf "$output_app"
  cp -R "$built_app" "$output_app"
  info "gui artifact: $output_app"
}

build_direct_dmg() {
  local variant_dir="$OUTPUT_ROOT/direct"
  local app_path="$variant_dir/Helm.app"
  local staging="$variant_dir/dmg-staging"
  local dmg_path="$variant_dir/Helm-unsigned.dmg"

  require_tool hdiutil

  if [ ! -d "$app_path" ]; then
    printf '[build] error: app not found for direct dmg packaging: %s\n' "$app_path" >&2
    return 1
  fi

  rm -rf "$staging" "$dmg_path"
  mkdir -p "$staging"
  cp -R "$app_path" "$staging/"
  ln -s /Applications "$staging/Applications"

  info "packaging unsigned direct dmg"
  hdiutil create \
    -volname "Helm" \
    -srcfolder "$staging" \
    -ov \
    -format UDZO \
    "$dmg_path" >/dev/null

  rm -rf "$staging"
  info "direct dmg artifact: $dmg_path"
}

write_variant_placeholder() {
  local variant="$1"
  local message="$2"
  local variant_dir="$OUTPUT_ROOT/$variant"
  local note_path="$variant_dir/ARTIFACT_NOTES.md"

  mkdir -p "$variant_dir"
  cat >"$note_path" <<EOF
# ${variant} packaging notes

${message}

This local build is intentionally unsigned/unnotarized and exists for profile validation.
Release packaging for this variant requires maintainer credentials and store/vendor pipelines.
EOF
  info "notes artifact: $note_path"
}

run_optional() {
  local label="$1"
  shift

  if "$@"; then
    return 0
  fi

  warn "$label failed (best-effort mode continues)"
  OPTIONAL_FAILURES+=("$label")
  return 0
}

build_direct() {
  local status=0
  if ! build_gui_variant "direct"; then
    status=1
  else
    if ! build_direct_dmg; then
      status=1
    fi
  fi
  return "$status"
}

build_mas() {
  local status=0
  if ! build_gui_variant "mas"; then
    status=1
  fi
  write_variant_placeholder \
    "mas" \
    "MAS packaging/export is not performed by this script. Use App Store Connect distribution tooling for signed store artifacts."
  return "$status"
}

build_setapp() {
  local status=0
  if ! build_gui_variant "setapp"; then
    status=1
  fi
  write_variant_placeholder \
    "setapp" \
    "Setapp ingestion packaging is not performed by this script. Provide signed vendor-specific metadata/artifacts in Setapp pipeline."
  return "$status"
}

build_business() {
  local status=0
  if ! build_gui_variant "business"; then
    status=1
  fi
  write_variant_placeholder \
    "business" \
    "Business PKG generation is not performed by this script. Use maintainer signing + notarization pipeline for managed fleet rollout."
  return "$status"
}

build_all() {
  build_cli_release
  run_optional "direct variant" build_direct
  run_optional "mas variant" build_mas
  run_optional "setapp variant" build_setapp
  run_optional "business variant" build_business

  if [ "${#OPTIONAL_FAILURES[@]}" -gt 0 ]; then
    warn "completed with optional failures: ${OPTIONAL_FAILURES[*]}"
  else
    info "completed all variants"
  fi
}

main() {
  local target="${1:-}"
  require_tool python3
  case "$target" in
  direct)
    build_cli_release
    build_direct
    ;;
  mas)
    build_cli_release
    build_mas
    ;;
  setapp)
    build_cli_release
    build_setapp
    ;;
  business)
    build_cli_release
    build_business
    ;;
  all)
    build_all
    ;;
  cli)
    build_cli_release
    ;;
  help | --help | -h | "")
    usage
    ;;
  *)
    printf '[build] error: unsupported build target: %s\n\n' "$target" >&2
    usage
    exit 1
    ;;
  esac
}

main "${1:-}"
