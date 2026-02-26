#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKFLOWS_DIR="${ROOT_DIR}/.github/workflows"
EXPECTED_RUST_TOOLCHAIN="1.93.1"
EXPECTED_SWIFTLINT_VERSION="0.59.1"
EXPECTED_SWIFTLINT_SHA256="58f9be8a4677900c945e2c618168223f4dd620a0cc65c9ccc5ea0f70433e89c1"

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

echo "CI toolchain contracts validated."
