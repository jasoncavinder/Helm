#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/build_unsigned_variant.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

fail() {
  printf '[build-unsigned-variant-contract] error: %s\n' "$1" >&2
  exit 1
}

expect_failure() {
  local label="$1"
  shift

  if "$@" >"${TMP_DIR}/stdout.log" 2>"${TMP_DIR}/stderr.log"; then
    fail "${label}: expected failure but command succeeded"
  fi
}

expect_failure \
  "invalid-tag-rejected" \
  env VARIANT="mas" TAG_NAME="v0.17.6/../../escape" OUTPUT_ROOT="${TMP_DIR}/out-a" "${SCRIPT_PATH}"

if ! grep -Fq "TAG_NAME must match vX.Y.Z or vX.Y.Z-rc.N" "${TMP_DIR}/stderr.log"; then
  fail "invalid-tag-rejected: missing expected validation error"
fi

mkdir -p "${TMP_DIR}/safe-root" "${TMP_DIR}/escaped-root"
ln -s "${TMP_DIR}/escaped-root" "${TMP_DIR}/safe-root/mas"

expect_failure \
  "symlinked-output-root-rejected" \
  env VARIANT="mas" TAG_NAME="v0.17.6" OUTPUT_ROOT="${TMP_DIR}/safe-root" "${SCRIPT_PATH}"

if ! grep -Fq "OUT_DIR resolves outside allowed output root" "${TMP_DIR}/stderr.log"; then
  fail "symlinked-output-root-rejected: missing expected path containment error"
fi

printf '[build-unsigned-variant-contract] passed\n'
