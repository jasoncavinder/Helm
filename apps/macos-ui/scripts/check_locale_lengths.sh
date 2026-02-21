#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
BASE_LOCALE="en"
LOCALES=(es de fr pt-BR ja hu)
FILES=(common app service)
RATIO_THRESHOLD="1.35"
ABS_THRESHOLD=24

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required for locale overflow checks" >&2
  exit 1
fi

echo "Locale overflow audit"
echo "base=${BASE_LOCALE} ratio>${RATIO_THRESHOLD} abs_delta>${ABS_THRESHOLD}"

overflow_count=0

for locale in "${LOCALES[@]}"; do
  for file in "${FILES[@]}"; do
    base_file="${ROOT_DIR}/apps/macos-ui/Helm/Resources/locales/${BASE_LOCALE}/${file}.json"
    locale_file="${ROOT_DIR}/apps/macos-ui/Helm/Resources/locales/${locale}/${file}.json"

    [[ -f "${base_file}" ]] || continue
    [[ -f "${locale_file}" ]] || continue

    mapfile -t rows < <(
      jq -r -n --slurpfile b "${base_file}" --slurpfile l "${locale_file}" '
        $l[0]
        | to_entries[]
        | [.key, ($b[0][.key] // ""), (.value // "")]
        | @tsv
      '
    )

    for row in "${rows[@]}"; do
      IFS=$'\t' read -r key base_value locale_value <<< "${row}"
      [[ -n "${key}" ]] || continue
      base_len=${#base_value}
      locale_len=${#locale_value}

      if [[ ${base_len} -eq 0 ]]; then
        continue
      fi

      ratio=$(awk -v n="${locale_len}" -v d="${base_len}" 'BEGIN { printf "%.4f", n/d }')
      abs_delta=$((locale_len - base_len))

      ratio_exceeds=$(awk -v r="${ratio}" -v t="${RATIO_THRESHOLD}" 'BEGIN { print (r > t) ? 1 : 0 }')
      if [[ ${ratio_exceeds} -eq 1 && ${abs_delta} -gt ${ABS_THRESHOLD} ]]; then
        printf '%s\t%s\t%s\tbase=%d\tlocalized=%d\tratio=%s\n' \
          "${locale}" "${file}" "${key}" "${base_len}" "${locale_len}" "${ratio}"
        overflow_count=$((overflow_count + 1))
      fi
    done
  done
done

if [[ ${overflow_count} -eq 0 ]]; then
  echo "No high-risk overflow candidates found by heuristic thresholds."
else
  echo "Found ${overflow_count} high-risk overflow candidates."
  exit 2
fi
