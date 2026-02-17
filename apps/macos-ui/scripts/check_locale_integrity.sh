#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LOCALES_DIR="${ROOT_DIR}/locales"
BASE_LOCALE="en"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for locale integrity checks" >&2
  exit 1
fi

if [[ ! -d "${LOCALES_DIR}/${BASE_LOCALE}" ]]; then
  echo "error: missing base locale directory: ${LOCALES_DIR}/${BASE_LOCALE}" >&2
  exit 1
fi

extract_placeholders() {
  local value="$1"
  printf '%s\n' "$value" | grep -oE '\{[A-Za-z0-9_]+\}' | tr -d '{}' | sort -u || true
}

echo "Locale integrity audit"
echo "base=${BASE_LOCALE}"

mapfile -t files < <(find "${LOCALES_DIR}/${BASE_LOCALE}" -maxdepth 1 -type f -name '*.json' -print | sort)
mapfile -t locales < <(find "${LOCALES_DIR}" -mindepth 1 -maxdepth 1 -type d -print | sort)

error_count=0

for locale_path in "${locales[@]}"; do
  locale="$(basename "${locale_path}")"
  [[ "${locale}" == "${BASE_LOCALE}" ]] && continue
  [[ "${locale}" == "_meta" ]] && continue

  for base_file in "${files[@]}"; do
    file_name="$(basename "${base_file}")"
    locale_file="${LOCALES_DIR}/${locale}/${file_name}"

    if [[ ! -f "${locale_file}" ]]; then
      echo "missing_file locale=${locale} file=${file_name}"
      error_count=$((error_count + 1))
      continue
    fi

    if ! jq empty "${base_file}" >/dev/null 2>&1; then
      echo "invalid_json locale=${BASE_LOCALE} file=${file_name}"
      error_count=$((error_count + 1))
      continue
    fi

    if ! jq empty "${locale_file}" >/dev/null 2>&1; then
      echo "invalid_json locale=${locale} file=${file_name}"
      error_count=$((error_count + 1))
      continue
    fi

    mapfile -t base_keys < <(jq -r 'keys[]' "${base_file}" | sort)
    mapfile -t locale_keys < <(jq -r 'keys[]' "${locale_file}" | sort)

    mapfile -t missing_keys < <(comm -23 <(printf '%s\n' "${base_keys[@]}") <(printf '%s\n' "${locale_keys[@]}"))
    mapfile -t extra_keys < <(comm -13 <(printf '%s\n' "${base_keys[@]}") <(printf '%s\n' "${locale_keys[@]}"))

    for key in "${missing_keys[@]}"; do
      [[ -z "${key}" ]] && continue
      echo "missing_key locale=${locale} file=${file_name} key=${key}"
      error_count=$((error_count + 1))
    done

    for key in "${extra_keys[@]}"; do
      [[ -z "${key}" ]] && continue
      echo "extra_key locale=${locale} file=${file_name} key=${key}"
      error_count=$((error_count + 1))
    done

    mapfile -t keys_to_check < <(printf '%s\n' "${base_keys[@]}")
    for key in "${keys_to_check[@]}"; do
      [[ -z "${key}" ]] && continue

      base_value="$(jq -r --arg k "${key}" '.[$k] // ""' "${base_file}")"
      locale_value="$(jq -r --arg k "${key}" '.[$k] // ""' "${locale_file}")"

      base_placeholders="$(extract_placeholders "${base_value}")"
      locale_placeholders="$(extract_placeholders "${locale_value}")"

      if [[ "${base_placeholders}" != "${locale_placeholders}" ]]; then
        echo "placeholder_mismatch locale=${locale} file=${file_name} key=${key} base={${base_placeholders//$'\n'/,}} localized={${locale_placeholders//$'\n'/,}}"
        error_count=$((error_count + 1))
      fi
    done
  done
done

if [[ ${error_count} -eq 0 ]]; then
  echo "Locale integrity checks passed."
else
  echo "Locale integrity checks failed with ${error_count} issue(s)." >&2
  exit 2
fi
