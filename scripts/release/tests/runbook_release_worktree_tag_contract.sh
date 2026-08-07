#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUNBOOK_SOURCE="${ROOT_DIR}/scripts/release/runbook.sh"
TMP_DIR="$(mktemp -d)"
STUB_BIN="${TMP_DIR}/bin"
CALL_LOG="${TMP_DIR}/calls.log"
TEST_REPO="${TMP_DIR}/repo"
RUNBOOK_PATH="${TEST_REPO}/scripts/release/runbook.sh"
PREFLIGHT_STUB="${TEST_REPO}/scripts/release/preflight.sh"
trap 'rm -rf "$TMP_DIR"' EXIT

fail() {
  printf '[runbook-release-worktree-tag-contract] error: %s\n' "$1" >&2
  exit 1
}

expect_log() {
  local pattern="$1"
  local message="$2"
  if ! grep -Eq -- "$pattern" "$CALL_LOG"; then
    fail "$message"
  fi
}

reject_log() {
  local pattern="$1"
  local message="$2"
  if grep -Eq -- "$pattern" "$CALL_LOG"; then
    fail "$message"
  fi
}

reset_log() {
  : >"$CALL_LOG"
}

if grep -Fq 'HELM_RELEASE_PREFLIGHT_SCRIPT' "$RUNBOOK_SOURCE"; then
  fail "mutating runbook permits replacing its required preflight script"
fi

mkdir -p "$STUB_BIN" "$(dirname "$RUNBOOK_PATH")"
cp "$RUNBOOK_SOURCE" "$RUNBOOK_PATH"

cat >"${STUB_BIN}/git" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'git %s\n' "$*" >>"$HELM_STUB_CALL_LOG"

case "${1:-}" in
diff)
  if [ "${HELM_STUB_DIRTY:-0}" = "1" ]; then
    exit 1
  fi
  ;;
ls-files)
  if [ "${HELM_STUB_UNTRACKED:-0}" = "1" ]; then
    printf 'untracked.txt\n'
  fi
  ;;
fetch)
  if [ "${HELM_STUB_FETCH_FAIL:-0}" = "1" ]; then
    exit 1
  fi
  ;;
rev-parse)
  if [ "${2:-}" = "HEAD" ]; then
    printf '%s\n' "${HELM_STUB_HEAD_SHA:?}"
  elif [ "${2:-}" = "--verify" ] && [ "${3:-}" = "origin/main" ]; then
    printf '%s\n' "${HELM_STUB_MAIN_SHA:?}"
  else
    exit 1
  fi
  ;;
tag | push)
  ;;
*)
  printf 'unexpected git invocation: %s\n' "$*" >&2
  exit 1
  ;;
esac
EOF

cat >"$PREFLIGHT_STUB" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf 'preflight %s\n' "$*" >>"$HELM_STUB_CALL_LOG"
exit "${HELM_STUB_PREFLIGHT_EXIT_CODE:-0}"
EOF

chmod +x "${STUB_BIN}/git" "$PREFLIGHT_STUB" "$RUNBOOK_PATH"

runbook() {
  HELM_STUB_CALL_LOG="$CALL_LOG" \
    HELM_STUB_HEAD_SHA="${HELM_STUB_HEAD_SHA:-same-sha}" \
    HELM_STUB_MAIN_SHA="${HELM_STUB_MAIN_SHA:-same-sha}" \
    HELM_STUB_DIRTY="${HELM_STUB_DIRTY:-0}" \
    HELM_STUB_UNTRACKED="${HELM_STUB_UNTRACKED:-0}" \
    HELM_STUB_FETCH_FAIL="${HELM_STUB_FETCH_FAIL:-0}" \
    HELM_STUB_PREFLIGHT_EXIT_CODE="${HELM_STUB_PREFLIGHT_EXIT_CODE:-0}" \
    PATH="${STUB_BIN}:$PATH" \
    "$RUNBOOK_PATH" "$@"
}

reset_log
runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null
expect_log '^preflight --tag v99\.99\.99 --allow-non-main$' "worktree mode did not enable only the non-main preflight override"
expect_log '^git fetch origin --quiet$' "worktree mode did not refresh origin/main"
expect_log '^git rev-parse HEAD$' "worktree mode did not resolve HEAD"
expect_log '^git rev-parse --verify origin/main$' "worktree mode did not resolve origin/main"
expect_log '^git tag -a v99\.99\.99 -m Helm 99\.99\.99$' "worktree mode did not create the expected annotated tag"
expect_log '^git push origin v99\.99\.99$' "worktree mode did not push the expected tag"

reset_log
HELM_STUB_HEAD_SHA="stale-sha" HELM_STUB_MAIN_SHA="main-sha" \
  runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null 2>&1 &&
  fail "worktree mode accepted a HEAD that did not equal origin/main"
reject_log '^git tag ' "SHA mismatch created a tag"
reject_log '^git push ' "SHA mismatch pushed a tag"

reset_log
HELM_STUB_DIRTY=1 runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null 2>&1 &&
  fail "worktree mode accepted tracked changes"
reject_log '^git fetch ' "dirty worktree reached remote validation"
reject_log '^git tag ' "dirty worktree created a tag"
reject_log '^git push ' "dirty worktree pushed a tag"

reset_log
HELM_STUB_UNTRACKED=1 runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null 2>&1 &&
  fail "worktree mode accepted untracked files"
reject_log '^git fetch ' "worktree with untracked files reached remote validation"
reject_log '^git tag ' "worktree with untracked files created a tag"
reject_log '^git push ' "worktree with untracked files pushed a tag"

reset_log
HELM_STUB_FETCH_FAIL=1 runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null 2>&1 &&
  fail "worktree mode continued after origin refresh failed"
reject_log '^git rev-parse ' "failed origin refresh reached commit validation"
reject_log '^git tag ' "failed origin refresh created a tag"
reject_log '^git push ' "failed origin refresh pushed a tag"

reset_log
HELM_STUB_PREFLIGHT_EXIT_CODE=42 \
  runbook tag --tag v99.99.99 --allow-release-worktree >/dev/null 2>&1 &&
  fail "worktree mode continued after release preflight failed"
reject_log '^git ' "failed release preflight reached a git mutation or validation"

reset_log
HELM_STUB_PREFLIGHT_EXIT_CODE=42 runbook tag --tag v99.99.99 >/dev/null 2>&1 &&
  fail "strict mode continued after release preflight failed"
reject_log '^git ' "failed strict release preflight reached a git mutation"

reset_log
runbook tag --tag v99.99.99 >/dev/null
expect_log '^preflight --tag v99\.99\.99$' "strict mode changed its preflight arguments"
reject_log '^git fetch ' "strict mode unexpectedly used the worktree override validation"
expect_log '^git tag -a v99\.99\.99 -m Helm 99\.99\.99$' "strict mode did not create the expected annotated tag"
expect_log '^git push origin v99\.99\.99$' "strict mode did not push the expected tag"

printf '[runbook-release-worktree-tag-contract] passed\n'
