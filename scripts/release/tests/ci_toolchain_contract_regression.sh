#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
CONTRACT_PATH="${ROOT_DIR}/scripts/release/tests/ci_toolchain_contract.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

fail() {
  printf '[ci-toolchain-contract-regression] error: %s\n' "$1" >&2
  exit 1
}

cp -R "${ROOT_DIR}/.github/workflows" "${TMP_DIR}/workflows"

CODEQL_WORKFLOW="${TMP_DIR}/workflows/codeql.yml"
MUTATED_WORKFLOW="${TMP_DIR}/codeql.yml"
sed -E \
  's#(github/codeql-action/analyze)@[0-9a-f]{40}#\1@0000000000000000000000000000000000000000#' \
  "${CODEQL_WORKFLOW}" > "${MUTATED_WORKFLOW}"

if cmp -s "${CODEQL_WORKFLOW}" "${MUTATED_WORKFLOW}"; then
  fail "fixture setup did not replace the CodeQL analyze SHA"
fi
mv "${MUTATED_WORKFLOW}" "${CODEQL_WORKFLOW}"

if HELM_CI_WORKFLOWS_DIR="${TMP_DIR}/workflows" \
  "${CONTRACT_PATH}" >"${TMP_DIR}/stdout.log" 2>"${TMP_DIR}/stderr.log"; then
  fail "mixed correct and incorrect CodeQL pins were accepted"
fi

if ! grep -Fq \
  "github/codeql-action/analyze@0000000000000000000000000000000000000000" \
  "${TMP_DIR}/stderr.log"; then
  fail "contract failure did not identify the incorrect CodeQL reference"
fi

for workflow in codeql ci-test; do
  cp "${ROOT_DIR}/.github/workflows/codeql.yml" "${TMP_DIR}/workflows/codeql.yml"
  cp "${ROOT_DIR}/.github/workflows/ci-test.yml" "${TMP_DIR}/workflows/ci-test.yml"
  sed '/swift .*--package-path service\/external-updater --arch arm64/d' \
    "${TMP_DIR}/workflows/${workflow}.yml" > "${TMP_DIR}/missing-package.yml"
  if cmp -s "${TMP_DIR}/workflows/${workflow}.yml" "${TMP_DIR}/missing-package.yml"; then
    fail "fixture setup did not remove the ${workflow} external package command"
  fi
  mv "${TMP_DIR}/missing-package.yml" "${TMP_DIR}/workflows/${workflow}.yml"
  if HELM_CI_WORKFLOWS_DIR="${TMP_DIR}/workflows" \
    "${CONTRACT_PATH}" >"${TMP_DIR}/stdout.log" 2>"${TMP_DIR}/stderr.log"; then
    fail "missing ${workflow} external package command was accepted"
  fi
  if ! grep -Fq "standalone external updater Swift package" "${TMP_DIR}/stderr.log"; then
    fail "missing ${workflow} package command was not identified"
  fi
done

for workflow in codeql ci-test; do
  cp "${ROOT_DIR}/.github/workflows/codeql.yml" "${TMP_DIR}/workflows/codeql.yml"
  cp "${ROOT_DIR}/.github/workflows/ci-test.yml" "${TMP_DIR}/workflows/ci-test.yml"
  sed '/library_dir=.*bash scripts\/build_external_update_bridge.sh arm64 debug/d' \
    "${TMP_DIR}/workflows/${workflow}.yml" > "${TMP_DIR}/missing-bridge.yml"
  mv "${TMP_DIR}/missing-bridge.yml" "${TMP_DIR}/workflows/${workflow}.yml"
  if HELM_CI_WORKFLOWS_DIR="${TMP_DIR}/workflows" \
    "${CONTRACT_PATH}" >"${TMP_DIR}/stdout.log" 2>"${TMP_DIR}/stderr.log"; then
    fail "missing ${workflow} bridge build was accepted"
  fi
  if ! grep -Fq "private external updater Rust bridge" "${TMP_DIR}/stderr.log"; then
    fail "missing ${workflow} bridge build was not identified"
  fi
done

printf '[ci-toolchain-contract-regression] passed\n'
