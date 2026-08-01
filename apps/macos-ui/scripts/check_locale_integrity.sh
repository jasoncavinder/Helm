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

echo "Locale integrity audit"
echo "base=${BASE_LOCALE}"

mapfile -t files < <(find "${LOCALES_DIR}/${BASE_LOCALE}" -maxdepth 1 -type f -name '*.json' -print | sort)
mapfile -t locales < <(find "${LOCALES_DIR}" -mindepth 1 -maxdepth 1 -type d -print | sort)

error_count=0

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR}"' EXIT

for base_file in "${files[@]}"; do
  file_name="$(basename "${base_file}")"
  
  if ! jq empty "${base_file}" >/dev/null 2>&1; then
    echo "invalid_json locale=${BASE_LOCALE} file=${file_name}"
    error_count=$((error_count + 1))
    touch "${TMP_DIR}/base_${file_name}.pl"
    continue
  fi
  
  jq -r 'to_entries[] | .key + "\t" + (.value | tostring | [match("\\{([A-Za-z0-9_]+)\\}"; "g").captures[0].string] | sort | unique | join(","))' "${base_file}" | sort > "${TMP_DIR}/base_${file_name}.pl"
done

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

    if ! jq empty "${locale_file}" >/dev/null 2>&1; then
      echo "invalid_json locale=${locale} file=${file_name}"
      error_count=$((error_count + 1))
      continue
    fi
    
    if [[ ! -s "${TMP_DIR}/base_${file_name}.pl" ]]; then
      continue
    fi

    jq -r 'to_entries[] | .key + "\t" + (.value | tostring | [match("\\{([A-Za-z0-9_]+)\\}"; "g").captures[0].string] | sort | unique | join(","))' "${locale_file}" | sort > "${TMP_DIR}/locale_${locale}_${file_name}.pl"

    mapfile -t mismatches < <(awk -F'\t' -v loc="${locale}" -v file="${file_name}" '
      NR==FNR {
        base_pl[$1] = $2
        base_order[++n] = $1
        next
      }
      {
        loc_pl[$1] = $2
        loc_order[++m] = $1
      }
      END {
        for (i=1; i<=n; i++) {
          k = base_order[i]
          if (!(k in loc_pl)) {
            print "missing_key locale=" loc " file=" file " key=" k
          }
        }
        for (i=1; i<=m; i++) {
          k = loc_order[i]
          if (!(k in base_pl)) {
            print "extra_key locale=" loc " file=" file " key=" k
          }
        }
        for (i=1; i<=n; i++) {
          k = base_order[i]
          b = base_pl[k]
          if (!(k in loc_pl)) {
            l = ""
          } else {
            l = loc_pl[k]
          }
          if (b != l) {
            print "placeholder_mismatch locale=" loc " file=" file " key=" k " base={" b "} localized={" l "}"
          }
        }
      }
    ' "${TMP_DIR}/base_${file_name}.pl" "${TMP_DIR}/locale_${locale}_${file_name}.pl")

    for mismatch in "${mismatches[@]}"; do
      [[ -z "${mismatch}" ]] && continue
      echo "${mismatch}"
      error_count=$((error_count + 1))
    done
  done
done

if [[ ${error_count} -eq 0 ]]; then
  echo "Locale integrity checks passed."
else
  echo "Locale integrity checks failed with ${error_count} issue(s)." >&2
  exit 2
fi
