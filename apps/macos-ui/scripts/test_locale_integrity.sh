#!/usr/bin/env bash
# shellcheck disable=SC2016,SC2001
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_SCRIPT="${SCRIPT_DIR}/check_locale_integrity.sh"

PASS=0
FAIL=0

run_test() {
  local name="$1"
  local expected_exit="$2"
  shift 2
  # Remaining args: alternating --setup, <cmd>, --expect, <pattern>, --expect, <pattern>, ...
  # --expect_exit <code> overrides expected_exit

  local tmpdir
  tmpdir="$(mktest_dir)"

  # Evaluate setup commands
  eval "$1"
  shift

  # Collect expected patterns
  local -a expects=()
  local -a ordered_expects=()

  while [[ $# -gt 0 ]]; do
    case "$1" in
      --expect)
        shift
        expects+=("$1")
        ;;
      --expect_order)
        shift
        ordered_expects+=("$1")
        ;;
      --expect_exit)
        shift
        expected_exit="$1"
        ;;
      *)
        echo "FAIL: ${name} (unknown arg: $1)"
        FAIL=$((FAIL + 1))
        rm -rf "${tmpdir}"
        return
        ;;
    esac
    shift
  done

  # Run the check script
  local actual_output actual_exit
  actual_output="$(LOCALES_DIR="${tmpdir}/locales" bash "${CHECK_SCRIPT}" 2>&1)" && actual_exit=0 || actual_exit=$?

  # Validate exit code
  local pass=true
  if [[ ${actual_exit} -ne ${expected_exit} ]]; then
    pass=false
  fi

  # Validate expected patterns
  for pat in "${expects[@]}"; do
    if ! echo "${actual_output}" | grep -qF "${pat}"; then
      pass=false
    fi
  done

# Validate ordered patterns by comparing their first matching line numbers
local previous_line=0
local current_line

for pat in "${ordered_expects[@]}"; do
  current_line="$(
    printf '%s\n' "${actual_output}" |
      grep -nF -m 1 -- "${pat}" |
      cut -d: -f1 ||
      true
  )"

  if [[ -z "${current_line}" || ${current_line} -le ${previous_line} ]]; then
    pass=false
    break
  fi

    previous_line="${current_line}"
  done

  if [[ "${pass}" == true ]]; then
    echo "PASS: ${name}"
    PASS=$((PASS + 1))
  else
    echo "FAIL: ${name}"
    echo "  Expected exit: ${expected_exit}, got: ${actual_exit}"
    echo "  Expected output to contain:"
    for pat in "${expects[@]}"; do
      echo "    '${pat}'"
    done

    if ((${#ordered_expects[@]} > 0)); then
      echo "  Expected output in this order:"
      for pat in "${ordered_expects[@]}"; do
        echo "    '${pat}'"
      done
    fi

    echo "  Actual output:"
    echo "${actual_output}" | sed 's/^/    /'
    FAIL=$((FAIL + 1))
  fi

  rm -rf "${tmpdir}"
}

mktest_dir() {
  mktemp -d
}

# ---- Test 1: Valid empty base + empty locale → passes ----
run_test "empty_base_empty_locale" 0 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{}" > "${tmpdir}/locales/en/app.json"
   echo "{}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "Locale integrity checks passed."

# ---- Test 2: Valid empty base + locale with extra keys → reports extra_key ----
run_test "empty_base_extra_keys" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{}" > "${tmpdir}/locales/en/app.json"
   echo "{\"extra\": \"value\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "extra_key locale=fr file=app.json key=extra"

# ---- Test 3: Non-empty base + missing localized keys ----
run_test "missing_keys" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"greeting\": \"Hello\", \"farewell\": \"Goodbye\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"greeting\": \"Bonjour\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "missing_key locale=fr file=app.json key=farewell"

# ---- Test 4: Non-empty base + locale extra keys ----
run_test "extra_keys" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"greeting\": \"Hello\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"greeting\": \"Bonjour\", \"extra\": \"value\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "extra_key locale=fr file=app.json key=extra"

# ---- Test 5: Placeholder mismatches ----
run_test "placeholder_mismatch" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"msg\": \"Hello {name}\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"msg\": \"Bonjour {nom}\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "placeholder_mismatch locale=fr file=app.json key=msg"

# ---- Test 6: Invalid base JSON ----
run_test "invalid_base_json" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "not json" > "${tmpdir}/locales/en/app.json"
   echo "{\"greeting\": \"Bonjour\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "invalid_json locale=en file=app.json"

# ---- Test 7: Invalid localized JSON ----
run_test "invalid_locale_json" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"greeting\": \"Hello\"}" > "${tmpdir}/locales/en/app.json"
   echo "not json" > "${tmpdir}/locales/fr/app.json"' \
  --expect "invalid_json locale=fr file=app.json"

# ---- Test 8: Missing localized file ----
run_test "missing_locale_file" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"greeting\": \"Hello\"}" > "${tmpdir}/locales/en/app.json"' \
  --expect "missing_file locale=fr file=app.json"

# ---- Test 9: Multiple locales, all pass ----
run_test "multiple_locales_pass" 0 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr" "${tmpdir}/locales/es"
   echo "{\"greeting\": \"Hello {name}\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"greeting\": \"Bonjour {name}\"}" > "${tmpdir}/locales/fr/app.json"
   echo "{\"greeting\": \"Hola {name}\"}" > "${tmpdir}/locales/es/app.json"' \
  --expect "Locale integrity checks passed."

# ---- Test 10: Empty base with multiple locales, one has extra keys ----
run_test "empty_base_multi_locale_extra" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr" "${tmpdir}/locales/es"
   echo "{}" > "${tmpdir}/locales/en/app.json"
   echo "{}" > "${tmpdir}/locales/fr/app.json"
   echo "{\"unexpected\": \"value\"}" > "${tmpdir}/locales/es/app.json"' \
  --expect "extra_key locale=es file=app.json key=unexpected"

# ---- Test 11: Output order: missing, extra, placeholder ----
run_test "output_order" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"a\": \"Hello {x}\", \"b\": \"World\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"a\": \"Bonjour {y}\", \"c\": \"Extra\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect_order "missing_key locale=fr file=app.json key=b" \
  --expect_order "extra_key locale=fr file=app.json key=c" \
  --expect_order "placeholder_mismatch locale=fr file=app.json key=a"

# ---- Test 12: Base with placeholders, locale without → mismatch ----
run_test "base_has_placeholder_locale_none" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"msg\": \"Hello {name}\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"msg\": \"Bonjour\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "placeholder_mismatch locale=fr file=app.json key=msg"

# ---- Test 13: Base without placeholders, locale has → mismatch ----
run_test "base_no_placeholder_locale_has" 2 \
  'mkdir -p "${tmpdir}/locales/en" "${tmpdir}/locales/fr"
   echo "{\"msg\": \"Hello\"}" > "${tmpdir}/locales/en/app.json"
   echo "{\"msg\": \"Bonjour {name}\"}" > "${tmpdir}/locales/fr/app.json"' \
  --expect "placeholder_mismatch locale=fr file=app.json key=msg"

# ---- Report ----
echo ""
echo "Results: ${PASS} passed, ${FAIL} failed"

if [[ ${FAIL} -gt 0 ]]; then
  exit 1
fi
