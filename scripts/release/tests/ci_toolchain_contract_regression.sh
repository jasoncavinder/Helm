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

printf '[ci-toolchain-contract-regression] passed\n'
