#!/usr/bin/env bash
set -euo pipefail

readonly INVALID_ENVIRONMENT_EXIT=75
readonly REQUIRED_READY_SAMPLES=3

usage() {
  cat <<'USAGE'
Usage: scripts/tests/library_performance_budget.sh [--run-only|--reuse-built-product]

The default mode builds and benchmarks in a unique retained temporary directory.
Run-only mode reuses HELM_LIBRARY_BENCHMARK_APP_BINARY or a product under
HELM_LIBRARY_BENCHMARK_DERIVED_DATA. Existing DerivedData, database, and
iteration overrides remain supported.

Readiness controls:
  HELM_LIBRARY_BENCHMARK_READY_TIMEOUT_SECONDS  default 180
  HELM_LIBRARY_BENCHMARK_READY_INTERVAL_SECONDS default 5
  HELM_LIBRARY_BENCHMARK_MAX_LOAD_PER_CPU        default 0.25
  HELM_LIBRARY_BENCHMARK_MAX_LOAD_1M             optional absolute limit

The host must provide three consecutive acceptable load, thermal, and AC-power
samples. An unready host emits HELM_LIBRARY_BENCHMARK_INVALID_ENVIRONMENT and
exits 75 instead of reporting a product performance failure.
USAGE
}

die_usage() {
  printf '%s\n' "$1" >&2
  exit 2
}

is_positive_integer() {
  [[ "$1" =~ ^[1-9][0-9]*$ ]]
}

is_positive_decimal() {
  [[ "$1" =~ ^([0-9]+([.][0-9]*)?|[.][0-9]+)$ ]] \
    && LC_ALL=C /usr/bin/awk -v value="$1" 'BEGIN { exit !(value > 0) }'
}

MODE="build-and-run"
if (( $# > 1 )); then
  usage >&2
  exit 2
fi
if (( $# == 1 )); then
  case "$1" in
    --run-only|--reuse-built-product) MODE="run-only" ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; exit 2 ;;
  esac
fi

ITERATIONS="${HELM_LIBRARY_BENCHMARK_ITERATIONS:-30}"
READY_TIMEOUT_SECONDS="${HELM_LIBRARY_BENCHMARK_READY_TIMEOUT_SECONDS:-180}"
READY_INTERVAL_SECONDS="${HELM_LIBRARY_BENCHMARK_READY_INTERVAL_SECONDS:-5}"
MAX_LOAD_PER_CPU="${HELM_LIBRARY_BENCHMARK_MAX_LOAD_PER_CPU:-0.25}"
MAX_LOAD_1M_OVERRIDE="${HELM_LIBRARY_BENCHMARK_MAX_LOAD_1M:-}"

if ! is_positive_integer "$ITERATIONS" || (( ITERATIONS < 30 || ITERATIONS > 100 )); then
  die_usage "HELM_LIBRARY_BENCHMARK_ITERATIONS must be an integer from 30 through 100."
fi
if ! is_positive_integer "$READY_TIMEOUT_SECONDS" || (( READY_TIMEOUT_SECONDS > 3600 )); then
  die_usage "HELM_LIBRARY_BENCHMARK_READY_TIMEOUT_SECONDS must be an integer from 1 through 3600."
fi
if ! is_positive_integer "$READY_INTERVAL_SECONDS" || (( READY_INTERVAL_SECONDS > 60 )); then
  die_usage "HELM_LIBRARY_BENCHMARK_READY_INTERVAL_SECONDS must be an integer from 1 through 60."
fi
if (( READY_TIMEOUT_SECONDS < READY_INTERVAL_SECONDS * (REQUIRED_READY_SAMPLES - 1) )); then
  die_usage "The readiness timeout must allow three samples at the configured interval."
fi
if ! is_positive_decimal "$MAX_LOAD_PER_CPU" \
    || ! LC_ALL=C /usr/bin/awk -v value="$MAX_LOAD_PER_CPU" 'BEGIN { exit !(value <= 1) }'; then
  die_usage "HELM_LIBRARY_BENCHMARK_MAX_LOAD_PER_CPU must be greater than 0 and at most 1."
fi
if [[ -n "$MAX_LOAD_1M_OVERRIDE" ]] && ! is_positive_decimal "$MAX_LOAD_1M_OVERRIDE"; then
  die_usage "HELM_LIBRARY_BENCHMARK_MAX_LOAD_1M must be a positive number when set."
fi
if [[ "$MODE" == "run-only" \
    && -z "${HELM_LIBRARY_BENCHMARK_APP_BINARY:-}" \
    && -z "${HELM_LIBRARY_BENCHMARK_DERIVED_DATA:-}" ]]; then
  die_usage "Run-only mode requires HELM_LIBRARY_BENCHMARK_APP_BINARY or HELM_LIBRARY_BENCHMARK_DERIVED_DATA."
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
TEMP_BASE="${TMPDIR:-/tmp}"
ARTIFACT_ROOT="$(mktemp -d "${TEMP_BASE%/}/helm-v020-library-benchmark.XXXXXX")"
DERIVED_DATA_PATH="${HELM_LIBRARY_BENCHMARK_DERIVED_DATA:-$ARTIFACT_ROOT/DerivedData}"
DATABASE_PATH="${HELM_LIBRARY_BENCHMARK_DB:-$ARTIFACT_ROOT/helm.sqlite}"
PROFILE_PATH="${HELM_LIBRARY_BENCHMARK_PROFILE_PATH:-$ARTIFACT_ROOT/helm-%p.profraw}"
APP_BINARY="${HELM_LIBRARY_BENCHMARK_APP_BINARY:-$DERIVED_DATA_PATH/Build/Products/Debug/Helm.app/Contents/MacOS/Helm}"

printf 'HELM_LIBRARY_BENCHMARK_ARTIFACTS root=%q derived_data=%q database=%q app_binary=%q\n' \
  "$ARTIFACT_ROOT" "$DERIVED_DATA_PATH" "$DATABASE_PATH" "$APP_BINARY" >&2

if [[ "$MODE" == "build-and-run" ]]; then
  cd "$REPO_ROOT"
  xcodebuild -quiet \
    -project apps/macos-ui/Helm.xcodeproj \
    -scheme Helm \
    -configuration Debug \
    -destination 'platform=macOS,arch=arm64' \
    -derivedDataPath "$DERIVED_DATA_PATH" \
    CODE_SIGNING_ALLOWED=NO \
    CODE_SIGNING_REQUIRED=NO \
    SWIFT_OPTIMIZATION_LEVEL=-O \
    build
fi

if [[ ! -x "$APP_BINARY" ]]; then
  die_usage "Library performance app binary is missing or not executable: $APP_BINARY"
fi

LAST_LOAD_1M="unavailable"
MAX_LOAD_1M="unavailable"
LAST_THERMAL_LEVEL="unavailable"
LAST_POWER_SOURCE="unavailable"
LAST_READINESS_DETAIL="not_sampled"

invalid_environment() {
  printf '%s\n' \
    "HELM_LIBRARY_BENCHMARK_INVALID_ENVIRONMENT {\"reason\":\"${LAST_READINESS_DETAIL}\",\"timeout_seconds\":${READY_TIMEOUT_SECONDS},\"load_1m\":\"${LAST_LOAD_1M}\",\"max_load_1m\":\"${MAX_LOAD_1M}\",\"thermal_level\":\"${LAST_THERMAL_LEVEL}\",\"power_source\":\"${LAST_POWER_SOURCE}\"}" \
    >&2
  exit "$INVALID_ENVIRONMENT_EXIT"
}

append_readiness_detail() {
  LAST_READINESS_DETAIL="${LAST_READINESS_DETAIL:+${LAST_READINESS_DETAIL},}$1"
}

collect_readiness_sample() {
  local load_output
  local sysload_output
  local power_output

  LAST_READINESS_DETAIL=""
  if load_output="$(LC_ALL=C /usr/sbin/sysctl -n vm.loadavg 2>/dev/null)"; then
    LAST_LOAD_1M="$(LC_ALL=C /usr/bin/awk '
      { gsub(/[{}]/, " "); for (field = 1; field <= NF; field += 1) {
          if ($field ~ /^[0-9]+([.][0-9]+)?$/) { print $field; exit }
        }
      }
    ' <<<"$load_output")"
  else
    LAST_LOAD_1M="unavailable"
  fi
  if [[ "$LAST_LOAD_1M" == "unavailable" || -z "$LAST_LOAD_1M" ]]; then
    LAST_LOAD_1M="unavailable"
    append_readiness_detail "load_average_unavailable"
  elif ! LC_ALL=C /usr/bin/awk -v load="$LAST_LOAD_1M" -v maximum="$MAX_LOAD_1M" \
      'BEGIN { exit !(load <= maximum) }'; then
    append_readiness_detail "load_above_limit"
  fi

  if sysload_output="$(LC_ALL=C /usr/bin/pmset -g sysload 2>/dev/null)"; then
    LAST_THERMAL_LEVEL="$(LC_ALL=C /usr/bin/awk -F '=' '
      /thermal level[[:space:]]*=/ {
        value = $2; gsub(/^[[:space:]]+|[[:space:]]+$/, "", value); print value; exit
      }
    ' <<<"$sysload_output")"
  else
    LAST_THERMAL_LEVEL="unavailable"
  fi
  case "$LAST_THERMAL_LEVEL" in
    Great|OK) ;;
    unavailable|"") LAST_THERMAL_LEVEL="unavailable"; append_readiness_detail "thermal_state_unavailable" ;;
    *) append_readiness_detail "thermal_state_not_ready" ;;
  esac

  if power_output="$(LC_ALL=C /usr/bin/pmset -g batt 2>/dev/null)"; then
    LAST_POWER_SOURCE="$(LC_ALL=C /usr/bin/awk -F "'" \
      '/Now drawing from/ { print $2; exit }' <<<"$power_output")"
  else
    LAST_POWER_SOURCE="unavailable"
  fi
  if [[ "$LAST_POWER_SOURCE" != "AC Power" ]]; then
    [[ -n "$LAST_POWER_SOURCE" ]] || LAST_POWER_SOURCE="unavailable"
    append_readiness_detail "ac_power_required"
  fi

  [[ -z "$LAST_READINESS_DETAIL" ]]
}

if [[ -n "$MAX_LOAD_1M_OVERRIDE" ]]; then
  MAX_LOAD_1M="$MAX_LOAD_1M_OVERRIDE"
else
  if ! LOGICAL_CPU_COUNT="$(LC_ALL=C /usr/sbin/sysctl -n hw.logicalcpu 2>/dev/null)" \
      || ! is_positive_integer "$LOGICAL_CPU_COUNT"; then
    LAST_READINESS_DETAIL="logical_cpu_count_unavailable"
    invalid_environment
  fi
  MAX_LOAD_1M="$(LC_ALL=C /usr/bin/awk \
    -v cpu_count="$LOGICAL_CPU_COUNT" -v load_per_cpu="$MAX_LOAD_PER_CPU" \
    'BEGIN { printf "%.3f", cpu_count * load_per_cpu }')"
fi

deadline=$((SECONDS + READY_TIMEOUT_SECONDS))
consecutive_ready=0
while (( consecutive_ready < REQUIRED_READY_SAMPLES )); do
  if collect_readiness_sample; then
    consecutive_ready=$((consecutive_ready + 1))
    printf 'Library benchmark host-ready sample %d/%d: load=%s (max %s), thermal=%s, power=%s.\n' \
      "$consecutive_ready" "$REQUIRED_READY_SAMPLES" "$LAST_LOAD_1M" "$MAX_LOAD_1M" \
      "$LAST_THERMAL_LEVEL" "$LAST_POWER_SOURCE" >&2
  else
    consecutive_ready=0
    printf 'Waiting for benchmark host readiness: load=%s (max %s), thermal=%s, power=%s, detail=%s.\n' \
      "$LAST_LOAD_1M" "$MAX_LOAD_1M" "$LAST_THERMAL_LEVEL" "$LAST_POWER_SOURCE" \
      "$LAST_READINESS_DETAIL" >&2
  fi
  (( consecutive_ready >= REQUIRED_READY_SAMPLES )) && break

  remaining=$((deadline - SECONDS))
  (( remaining > 0 )) || invalid_environment
  sleep_seconds="$READY_INTERVAL_SECONDS"
  (( sleep_seconds <= remaining )) || sleep_seconds="$remaining"
  sleep "$sleep_seconds"
done

benchmark_output="$({
  HELM_DB_PATH="$DATABASE_PATH" \
  HELM_LIBRARY_BENCHMARK_ITERATIONS="$ITERATIONS" \
  LLVM_PROFILE_FILE="$PROFILE_PATH" \
    "$APP_BINARY"
} 2>&1)"
benchmark_line="$(printf '%s\n' "$benchmark_output" | LC_ALL=C /usr/bin/awk '
  /^HELM_LIBRARY_BENCHMARK / { line = $0 }
  END { print line }
')"

if [[ -z "$benchmark_line" ]]; then
  printf '%s\n' "$benchmark_output" >&2
  echo "Library performance benchmark did not emit a result." >&2
  exit 1
fi

printf '%s\n' "$benchmark_line"
case "$benchmark_line" in
  *'"passed":true'*) ;;
  *)
    echo "Library performance budget failed." >&2
    exit 1
    ;;
esac
