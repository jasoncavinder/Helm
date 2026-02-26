#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT_PATH="${ROOT_DIR}/scripts/tests/real_manager_smoke.sh"

fail() {
  printf '[real-manager-smoke-contract] error: %s\n' "$1" >&2
  exit 1
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "${TMP_DIR:-}"' EXIT

make_stub() {
  local path="$1"
  local output="$2"
  cat > "$path" <<STUB
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "${output}"
STUB
  chmod +x "$path"
}

make_stub "${TMP_DIR}/brew" "Homebrew 4.3.0"
make_stub "${TMP_DIR}/node" "v24.13.1"
make_stub "${TMP_DIR}/npm" "11.6.2"
make_stub "${TMP_DIR}/python3" "Python 3.14.3"
make_stub "${TMP_DIR}/pip3" "pip 24.0"
make_stub "${TMP_DIR}/ruby" "ruby 4.0.1"
make_stub "${TMP_DIR}/gem" "3.6.0"

SUCCESS_REPORT="${TMP_DIR}/success-report.json"
HELM_REAL_MANAGER_BREW_BIN="${TMP_DIR}/brew" \
HELM_REAL_MANAGER_NODE_BIN="${TMP_DIR}/node" \
HELM_REAL_MANAGER_NPM_BIN="${TMP_DIR}/npm" \
HELM_REAL_MANAGER_PYTHON_BIN="${TMP_DIR}/python3" \
HELM_REAL_MANAGER_PIP_BIN="${TMP_DIR}/pip3" \
HELM_REAL_MANAGER_RUBY_BIN="${TMP_DIR}/ruby" \
HELM_REAL_MANAGER_GEM_BIN="${TMP_DIR}/gem" \
"$SCRIPT_PATH" --report-path "$SUCCESS_REPORT" >/dev/null

python3 - "$SUCCESS_REPORT" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert report["schema"] == "helm.tests.real_manager_smoke_report"
assert report["schema_version"] == 1
assert report["overall_status"] == "passed"
assert report["failed_managers"] == []
manager_results = report.get("manager_results") or []
assert len(manager_results) == 4
assert all(item.get("status") == "passed" for item in manager_results)
probe_results = report.get("probe_results") or []
assert len(probe_results) == 7
assert all(item.get("status") == "passed" for item in probe_results)
PY

FAIL_REPORT="${TMP_DIR}/fail-report.json"
if HELM_REAL_MANAGER_BREW_BIN="${TMP_DIR}/brew" \
  HELM_REAL_MANAGER_NODE_BIN="${TMP_DIR}/node" \
  HELM_REAL_MANAGER_NPM_BIN="${TMP_DIR}/npm" \
  HELM_REAL_MANAGER_PYTHON_BIN="${TMP_DIR}/python3" \
  HELM_REAL_MANAGER_PIP_BIN="${TMP_DIR}/pip3" \
  HELM_REAL_MANAGER_RUBY_BIN="${TMP_DIR}/ruby" \
  HELM_REAL_MANAGER_GEM_BIN="${TMP_DIR}/missing-gem" \
  "$SCRIPT_PATH" --report-path "$FAIL_REPORT" >/dev/null; then
  fail "expected non-zero exit when ruby/gem probe is missing"
fi

python3 - "$FAIL_REPORT" <<'PY'
import json
import pathlib
import sys

report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
assert report["overall_status"] == "failed"
assert "ruby" in report.get("failed_managers", [])
ruby = next(item for item in report.get("manager_results", []) if item.get("manager") == "ruby")
assert ruby.get("status") == "failed"
probe = next(item for item in report.get("probe_results", []) if item.get("probe") == "gem_version")
assert probe.get("status") == "failed"
assert probe.get("exit_code") == 127
PY

printf '[real-manager-smoke-contract] passed\n'
