#!/usr/bin/env bash
# Compilation only. Print an immutable library directory for SwiftPM's linker.
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
if [ "$#" -ne 2 ]; then
  echo "usage: build_external_update_bridge.sh arm64|x86_64 debug|release" >&2
  exit 2
fi
case "$1" in
  arm64) target=aarch64-apple-darwin ;;
  x86_64) target=x86_64-apple-darwin ;;
  *) echo "unsupported bridge architecture: $1" >&2; exit 2 ;;
esac
case "$2" in
  debug) profile=dev ;;
  release) profile=release ;;
  *) echo "unsupported bridge profile: $2" >&2; exit 2 ;;
esac

target_dir="${ROOT_DIR}/artifacts/external-updater-rust"
MACOSX_DEPLOYMENT_TARGET=13.0 cargo build --locked \
  --manifest-path "${ROOT_DIR}/core/rust/Cargo.toml" --target-dir "${target_dir}" \
  --target "${target}" --profile "${profile}" -p helm-external-update-bridge >&2
library="${target_dir}/${target}/$2/libhelm_external_update_bridge.a"
digest="$(shasum -a 256 "${library}" | awk '{print $1}')"
destination="${ROOT_DIR}/artifacts/external-updater-libraries/${target}/$2/${digest}"
mkdir -p "${destination}"
if [ ! -e "${destination}/libhelm_external_update_bridge.a" ]; then
  cp "${library}" "${destination}/libhelm_external_update_bridge.a"
fi
if [ "$(shasum -a 256 "${destination}/libhelm_external_update_bridge.a" | awk '{print $1}')" != "${digest}" ] \
  || ! cmp -s "${library}" "${destination}/libhelm_external_update_bridge.a"; then
  echo "immutable external updater bridge archive differs from its build" >&2
  exit 1
fi
printf '%s\n' "${destination}"
