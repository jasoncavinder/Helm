#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/release/rehearsal_dry_run.sh"

fail() {
  printf '[rehearsal-dry-run-contract] error: %s\n' "$1" >&2
  exit 1
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

CALL_LOG="${TMP_DIR}/calls.log"
PREFLIGHT_STUB="${TMP_DIR}/preflight_stub.sh"
RUNBOOK_STUB="${TMP_DIR}/runbook_stub.sh"
VERIFY_STABLE_STUB="${TMP_DIR}/verify_stable_stub.sh"
VERIFY_PRERELEASE_STUB="${TMP_DIR}/verify_prerelease_stub.sh"

cat > "$PREFLIGHT_STUB" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'preflight|%s\n' "$*" >> "$HELM_STUB_CALL_LOG"
exit "${HELM_STUB_PREFLIGHT_EXIT_CODE:-0}"
STUB

cat > "$RUNBOOK_STUB" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'runbook|%s\n' "$*" >> "$HELM_STUB_CALL_LOG"
[ "${1:-}" = "prepare" ] || exit 91
exit "${HELM_STUB_RUNBOOK_EXIT_CODE:-0}"
STUB

cat > "$VERIFY_STABLE_STUB" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'verify_stable|%s\n' "$*" >> "$HELM_STUB_CALL_LOG"
exit "${HELM_STUB_VERIFY_STABLE_EXIT_CODE:-0}"
STUB

cat > "$VERIFY_PRERELEASE_STUB" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
printf 'verify_prerelease|%s\n' "$*" >> "$HELM_STUB_CALL_LOG"
exit "${HELM_STUB_VERIFY_PRERELEASE_EXIT_CODE:-0}"
STUB

chmod +x "$PREFLIGHT_STUB" "$RUNBOOK_STUB" "$VERIFY_STABLE_STUB" "$VERIFY_PRERELEASE_STUB"

SUCCESS_REPORT="${TMP_DIR}/report-success.json"
: > "$CALL_LOG"

HELM_STUB_CALL_LOG="$CALL_LOG" \
HELM_RELEASE_PREFLIGHT_SCRIPT="$PREFLIGHT_STUB" \
HELM_RELEASE_RUNBOOK_SCRIPT="$RUNBOOK_STUB" \
HELM_RELEASE_VERIFY_STABLE_CONTRACT_SCRIPT="$VERIFY_STABLE_STUB" \
HELM_RELEASE_VERIFY_PRERELEASE_CONTRACT_SCRIPT="$VERIFY_PRERELEASE_STUB" \
"$SCRIPT_PATH" --tag v99.99.99 --report-path "$SUCCESS_REPORT"

python3 - "$SUCCESS_REPORT" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert report["schema"] == "helm.release.rehearsal_report"
assert report["schema_version"] == 1
assert report["dry_run"] is True
assert report["overall_status"] == "passed"
assert report["tag"] == "v99.99.99"
steps = report["steps"]
assert [step["name"] for step in steps] == ["preflight", "prepare", "verify"]
for step in steps:
    assert step["status"] == "passed"
    assert step["exit_code"] == 0
PY

grep -F "preflight|--tag v99.99.99 --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy --allow-existing-tag" "$CALL_LOG" >/dev/null || fail "preflight call flags mismatch"
grep -F "runbook|prepare --tag v99.99.99 --allow-non-main --allow-dirty --no-fetch --skip-secrets --skip-workflows --skip-ruleset-policy" "$CALL_LOG" >/dev/null || fail "runbook prepare call flags mismatch"
grep -F "verify_stable|" "$CALL_LOG" >/dev/null || fail "stable verify contract not executed"
grep -F "verify_prerelease|" "$CALL_LOG" >/dev/null || fail "prerelease verify contract not executed"

FAIL_REPORT="${TMP_DIR}/report-failure.json"
: > "$CALL_LOG"
if HELM_STUB_CALL_LOG="$CALL_LOG" \
  HELM_RELEASE_PREFLIGHT_SCRIPT="$PREFLIGHT_STUB" \
  HELM_RELEASE_RUNBOOK_SCRIPT="$RUNBOOK_STUB" \
  HELM_RELEASE_VERIFY_STABLE_CONTRACT_SCRIPT="$VERIFY_STABLE_STUB" \
  HELM_RELEASE_VERIFY_PRERELEASE_CONTRACT_SCRIPT="$VERIFY_PRERELEASE_STUB" \
  HELM_STUB_VERIFY_PRERELEASE_EXIT_CODE=7 \
  "$SCRIPT_PATH" --tag v99.99.99 --report-path "$FAIL_REPORT"; then
  fail "expected failure when prerelease verify contract exits non-zero"
fi

python3 - "$FAIL_REPORT" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert report["overall_status"] == "failed"
verify_step = next(step for step in report["steps"] if step["name"] == "verify")
assert verify_step["status"] == "failed"
assert verify_step["exit_code"] == 7
PY

printf '[rehearsal-dry-run-contract] passed\n'
