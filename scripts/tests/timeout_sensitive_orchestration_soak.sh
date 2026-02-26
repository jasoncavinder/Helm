#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CARGO_MANIFEST_PATH="${ROOT_DIR}/core/rust/Cargo.toml"
SOAK_RUNS="${HELM_TIMEOUT_SENSITIVE_SOAK_RUNS:-5}"
FAILURE_BUDGET="${HELM_TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET:-0}"

if ! [[ "${SOAK_RUNS}" =~ ^[0-9]+$ ]] || [[ "${SOAK_RUNS}" -eq 0 ]]; then
  echo "[soak] invalid HELM_TIMEOUT_SENSITIVE_SOAK_RUNS='${SOAK_RUNS}' (expected integer > 0)" >&2
  exit 2
fi

if ! [[ "${FAILURE_BUDGET}" =~ ^[0-9]+$ ]]; then
  echo "[soak] invalid HELM_TIMEOUT_SENSITIVE_SOAK_FAILURE_BUDGET='${FAILURE_BUDGET}' (expected integer >= 0)" >&2
  exit 2
fi

TARGETS=(
  "end_to_end_mise:mise_timeout_sensitive_orchestration_soak_budget"
  "end_to_end_rustup:rustup_timeout_sensitive_orchestration_soak_budget"
)

target_count=${#TARGETS[@]}
total_runs=$((SOAK_RUNS * target_count))
passes=0
failures=0

echo "[soak] timeout-sensitive orchestration repeat run"
echo "[soak] runs=${SOAK_RUNS} targets=${target_count} total=${total_runs} budget=${FAILURE_BUDGET}"

for ((run = 1; run <= SOAK_RUNS; run++)); do
  echo "[soak] run ${run}/${SOAK_RUNS}"
  for target in "${TARGETS[@]}"; do
    test_bin="${target%%:*}"
    test_name="${target##*:}"
    echo "[soak]   target=${test_bin} test=${test_name}"
    if cargo test --manifest-path "${CARGO_MANIFEST_PATH}" -p helm-core --test "${test_bin}" "${test_name}"; then
      passes=$((passes + 1))
    else
      failures=$((failures + 1))
    fi
  done
done

echo "[soak] summary passes=${passes} failures=${failures} budget=${FAILURE_BUDGET}"
if (( failures > FAILURE_BUDGET )); then
  echo "[soak] failed: timeout-sensitive soak exceeded failure budget" >&2
  exit 1
fi

echo "[soak] passed: timeout-sensitive soak is within failure budget"
