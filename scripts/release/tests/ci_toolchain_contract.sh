#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WORKFLOWS_DIR="${ROOT_DIR}/.github/workflows"
EXPECTED_RUST_TOOLCHAIN="1.93.1"
EXPECTED_SWIFTLINT_VERSION="0.59.1"
EXPECTED_SWIFTLINT_SHA256="58f9be8a4677900c945e2c618168223f4dd620a0cc65c9ccc5ea0f70433e89c1"

if rg -n 'toolchain:\s*stable' "${WORKFLOWS_DIR}"/*.yml >/tmp/helm_toolchain_stable_hits.txt; then
  echo "error: found floating Rust toolchain selectors (toolchain: stable)." >&2
  cat /tmp/helm_toolchain_stable_hits.txt >&2
  exit 1
fi

while IFS= read -r workflow; do
  if ! rg -q "toolchain:\\s*${EXPECTED_RUST_TOOLCHAIN}" "${workflow}"; then
    echo "error: ${workflow} does not pin Rust toolchain ${EXPECTED_RUST_TOOLCHAIN}" >&2
    exit 1
  fi
done < <(rg -l 'dtolnay/rust-toolchain' "${WORKFLOWS_DIR}"/*.yml)

SWIFTLINT_WORKFLOW="${WORKFLOWS_DIR}/swiftlint.yml"
rg -q "SWIFTLINT_VERSION: \"${EXPECTED_SWIFTLINT_VERSION}\"" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must pin SWIFTLINT_VERSION=${EXPECTED_SWIFTLINT_VERSION}" >&2
  exit 1
}

rg -q "SWIFTLINT_PORTABLE_SHA256: \"${EXPECTED_SWIFTLINT_SHA256}\"" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must pin SWIFTLINT_PORTABLE_SHA256=${EXPECTED_SWIFTLINT_SHA256}" >&2
  exit 1
}

rg -q "releases/download/\\$\\{SWIFTLINT_VERSION\\}/portable_swiftlint.zip" "${SWIFTLINT_WORKFLOW}" || {
  echo "error: swiftlint workflow must install the portable release artifact for the pinned version." >&2
  exit 1
}

echo "CI toolchain contracts validated."
