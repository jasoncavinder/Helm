#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKFLOWS_DIR="${ROOT_DIR}/.github/workflows"
EXPECTED_RUST_TOOLCHAIN="1.97.1"
EXPECTED_SWIFTLINT_VERSION="0.59.1"
EXPECTED_SWIFTLINT_SHA256="58f9be8a4677900c945e2c618168223f4dd620a0cc65c9ccc5ea0f70433e89c1"
EXPECTED_CHECKOUT_SHA="3d3c42e5aac5ba805825da76410c181273ba90b1"
EXPECTED_SETUP_NODE_SHA="a0853c24544627f65ddf259abe73b1d18a591444"
EXPECTED_CACHE_SHA="caa296126883cff596d87d8935842f9db880ef25"
EXPECTED_UPLOAD_ARTIFACT_SHA="b7c566a772e6b6bfb58ed0dc250532a479d7789f"
EXPECTED_DEPENDENCY_REVIEW_SHA="a1d282b36b6f3519aa1f3fc636f609c47dddb294"

has_pattern() {
  local pattern="$1"
  local file="$2"
  if command -v rg >/dev/null 2>&1; then
    rg -q "$pattern" "$file"
  else
    grep -Eq "$pattern" "$file"
  fi
}

list_files_with_pattern() {
  local pattern="$1"
  shift
  if command -v rg >/dev/null 2>&1; then
    rg -l "$pattern" "$@"
  else
    grep -lE "$pattern" "$@" || true
  fi
}

if command -v rg >/dev/null 2>&1; then
  rg -n 'toolchain:\s*stable' "${WORKFLOWS_DIR}"/*.yml >/tmp/helm_toolchain_stable_hits.txt || true
else
  grep -nE 'toolchain:[[:space:]]*stable' "${WORKFLOWS_DIR}"/*.yml >/tmp/helm_toolchain_stable_hits.txt || true
fi

if [ -s /tmp/helm_toolchain_stable_hits.txt ]; then
  echo "error: found floating Rust toolchain selectors (toolchain: stable)." >&2
  cat /tmp/helm_toolchain_stable_hits.txt >&2
  exit 1
fi

while IFS= read -r workflow; do
  [ -n "${workflow}" ] || continue
  if ! has_pattern "toolchain:[[:space:]]*${EXPECTED_RUST_TOOLCHAIN}" "${workflow}"; then
    echo "error: ${workflow} does not pin Rust toolchain ${EXPECTED_RUST_TOOLCHAIN}" >&2
    exit 1
  fi
done < <(list_files_with_pattern 'dtolnay/rust-toolchain' "${WORKFLOWS_DIR}"/*.yml)

SWIFTLINT_WORKFLOW="${WORKFLOWS_DIR}/swiftlint.yml"
has_pattern "SWIFTLINT_VERSION:[[:space:]]*\"${EXPECTED_SWIFTLINT_VERSION}\"" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must pin SWIFTLINT_VERSION=${EXPECTED_SWIFTLINT_VERSION}" >&2
  exit 1
}

has_pattern "SWIFTLINT_PORTABLE_SHA256:[[:space:]]*\"${EXPECTED_SWIFTLINT_SHA256}\"" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must pin SWIFTLINT_PORTABLE_SHA256=${EXPECTED_SWIFTLINT_SHA256}" >&2
  exit 1
}

has_pattern "releases/download/\\$\\{SWIFTLINT_VERSION\\}/portable_swiftlint.zip" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must install the portable release artifact for the pinned version." >&2
  exit 1
}

validate_action_pin() {
  local action="$1"
  local expected_sha="$2"
  local expected_version="$3"
  local found=0

  while IFS= read -r workflow; do
    [ -n "${workflow}" ] || continue
    found=1
    has_pattern "${action}@${expected_sha}" "${workflow}" || {
      echo "error: ${workflow} must pin ${action} ${expected_version}." >&2
      exit 1
    }
  done < <(list_files_with_pattern "${action}@" "${WORKFLOWS_DIR}"/*.yml)

  if [ "${found}" -ne 1 ]; then
    echo "error: expected at least one ${action} workflow reference." >&2
    exit 1
  fi
}

validate_action_pin "actions/checkout" "${EXPECTED_CHECKOUT_SHA}" "v7.0.1"
validate_action_pin "actions/setup-node" "${EXPECTED_SETUP_NODE_SHA}" "v5.0.0"
validate_action_pin "actions/cache" "${EXPECTED_CACHE_SHA}" "v5.1.0"
validate_action_pin "actions/upload-artifact" "${EXPECTED_UPLOAD_ARTIFACT_SHA}" "v6.0.0"
validate_action_pin \
  "actions/dependency-review-action" \
  "${EXPECTED_DEPENDENCY_REVIEW_SHA}" \
  "v5.0.0"

echo "CI toolchain contracts validated."
