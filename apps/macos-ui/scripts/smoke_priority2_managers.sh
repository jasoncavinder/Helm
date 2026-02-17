#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
OUT_FILE="${ROOT_DIR}/docs/validation/v0.11.0-beta.2-smoke-matrix.md"

mkdir -p "${ROOT_DIR}/docs/validation"

run_with_timeout() {
  local timeout_s="$1"
  shift
  perl -e 'alarm shift; exec @ARGV' "$timeout_s" "$@"
}

probe() {
  local manager="$1"
  local detect_cmd="$2"
  local list_cmd="$3"
  local outdated_cmd="$4"

  local detected="no"
  local version="n/a"
  local list_status="not-run"
  local outdated_status="not-run"

  if /bin/bash -lc "$detect_cmd" >/tmp/helm_${manager}_detect.out 2>/tmp/helm_${manager}_detect.err; then
    detected="yes"
    version="$(awk 'NF && $0 ~ /[Vv]ersion/ { print; exit }' /tmp/helm_${manager}_detect.out \
      | tr -d '\r' | sed 's/[[:space:]]\+/ /g' | sed 's/^ //; s/ $//')"
    if [[ -z "${version}" ]]; then
      version="$(awk 'NF && $0 !~ /not writable/ { print; exit }' /tmp/helm_${manager}_detect.out \
        | tr -d '\r' | sed 's/[[:space:]]\+/ /g' | sed 's/^ //; s/ $//')"
    fi
    if [[ -z "${version}" ]]; then
      version="(detected)"
    fi

    if run_with_timeout 20 /bin/bash -lc "$list_cmd" >/tmp/helm_${manager}_list.out 2>/tmp/helm_${manager}_list.err; then
      list_status="ok"
    else
      list_status="fail"
    fi

    if run_with_timeout 20 /bin/bash -lc "$outdated_cmd" >/tmp/helm_${manager}_outdated.out 2>/tmp/helm_${manager}_outdated.err; then
      outdated_status="ok"
    else
      outdated_status="fail"
    fi
  fi

  printf '| %s | %s | %s | %s | %s |\n' "$manager" "$detected" "$version" "$list_status" "$outdated_status"
}

{
  echo '# v0.11.0-beta.2 Priority 2 Smoke Matrix'
  echo
  echo "Generated: $(date -u '+%Y-%m-%d %H:%M:%S UTC')"
  echo
  echo '| manager | detected | version | list_installed | list_outdated |'
  echo '|---|---|---|---|---|'

  probe "pnpm" "pnpm --version" "pnpm list -g --depth=0 --json" "pnpm outdated -g --json"
  probe "yarn" "yarn --version" "yarn global list --depth=0 --json" "yarn outdated --json"
  probe "poetry" "poetry --version" "poetry self show plugins --no-ansi" "poetry self show plugins --outdated --no-ansi"
  probe "rubygems" "gem --version" "gem list --local" "gem outdated"
  probe "bundler" "bundle --version" "gem list --local bundler" "gem outdated bundler"

  echo
  echo '## Notes'
  echo '- `detected=no` means the manager binary was not available on PATH in this host environment.'
  echo '- `list_*` marked `fail` means the command returned non-zero or timed out (>20s).'
} >"$OUT_FILE"

echo "Wrote ${OUT_FILE}"
