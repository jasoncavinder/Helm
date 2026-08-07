#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
BIN_DIR="${TMP_DIR}/bin"
TEST_RELEASE_DIR="${TMP_DIR}/repo/scripts/release"
RUNBOOK="${TEST_RELEASE_DIR}/runbook.sh"
PREFLIGHT_STUB="${TEST_RELEASE_DIR}/preflight.sh"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

fail() {
  printf '[runbook-publish-release-state-contract] error: %s\n' "$1" >&2
  exit 1
}

mkdir -p "$BIN_DIR" "$TEST_RELEASE_DIR"
cp "${ROOT_DIR}/scripts/release/runbook.sh" "$RUNBOOK"
cp "${ROOT_DIR}/scripts/release/validate_github_release_state.sh" "${TEST_RELEASE_DIR}/validate_github_release_state.sh"

cat > "$PREFLIGHT_STUB" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
exit 0
EOF
chmod +x "$RUNBOOK" "$PREFLIGHT_STUB" "${TEST_RELEASE_DIR}/validate_github_release_state.sh"

cat > "${BIN_DIR}/gh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

if [ "$1" = "release" ] && [ "$2" = "view" ]; then
  if [ ! -f "${GH_STUB_STATE_DIR}/release-exists" ]; then
    exit 1
  fi
  printf '{"tagName":"%s","url":"https://example.invalid/releases/tag/%s","isDraft":%s,"isPrerelease":%s,"publishedAt":"%s"}\n' \
    "$GH_STUB_TAG" \
    "$GH_STUB_TAG" \
    "$GH_STUB_DRAFT" \
    "$GH_STUB_PRERELEASE" \
    "$GH_STUB_PUBLISHED_AT"
  exit 0
fi

if [ "$1" = "release" ] && [ "$2" = "create" ]; then
  printf '%s\n' "$*" >> "${GH_STUB_STATE_DIR}/calls.log"
  touch "${GH_STUB_STATE_DIR}/release-exists"
  exit 0
fi

if [ "$1" = "api" ]; then
  if [ -z "${GH_STUB_LATEST_TAG:-}" ]; then
    exit 1
  fi
  printf '%s\n' "$GH_STUB_LATEST_TAG"
  exit 0
fi

printf 'unexpected gh invocation: %s\n' "$*" >&2
exit 64
EOF
chmod +x "${BIN_DIR}/gh"

run_case() {
  local name="$1"
  local tag="$2"
  local existing="$3"
  local draft="$4"
  local prerelease="$5"
  local published_at="$6"
  local latest_tag="$7"
  local expected_status="$8"
  local expected_message="$9"
  local state_dir="${TMP_DIR}/${name}"
  local output_path="${state_dir}/output.log"

  mkdir -p "$state_dir"
  if [ "$existing" = "true" ]; then
    touch "${state_dir}/release-exists"
  fi

  local status=0
  if PATH="${BIN_DIR}:${PATH}" \
    GH_STUB_STATE_DIR="$state_dir" \
    GH_STUB_TAG="$tag" \
    GH_STUB_DRAFT="$draft" \
    GH_STUB_PRERELEASE="$prerelease" \
    GH_STUB_PUBLISHED_AT="$published_at" \
    GH_STUB_LATEST_TAG="$latest_tag" \
    "$RUNBOOK" publish --tag "$tag" > "$output_path" 2>&1; then
    status=0
  else
    status=$?
  fi

  if [ "$status" -ne "$expected_status" ]; then
    cat "$output_path" >&2
    fail "${name}: expected status ${expected_status}, got ${status}"
  fi
  if [ -n "$expected_message" ] && ! grep -F "$expected_message" "$output_path" >/dev/null; then
    cat "$output_path" >&2
    fail "${name}: missing expected message: ${expected_message}"
  fi
}

run_case existing_stable_valid v1.2.3 true false false 2026-08-07T00:00:00Z v1.2.3 0 "validated channel=stable"
run_case existing_rc_valid v1.3.0-rc.1 true false true 2026-08-07T00:00:00Z v1.2.3 0 "validated channel=rc"
run_case stable_prerelease_mismatch v1.2.3 true false true 2026-08-07T00:00:00Z v1.2.3 1 "expected false for the stable channel"
run_case rc_release_mismatch v1.3.0-rc.1 true false false 2026-08-07T00:00:00Z v1.2.3 1 "expected true for the rc channel"
run_case draft_rejected v1.2.3 true true false 2026-08-07T00:00:00Z v1.2.3 1 "is still a draft"
run_case unpublished_rejected v1.2.3 true false false "" v1.2.3 1 "is not published"
run_case stale_stable_rejected v1.2.3 true false false 2026-08-07T00:00:00Z v1.2.4 1 "must own releases/latest"
run_case rc_latest_rejected v1.3.0-rc.1 true false true 2026-08-07T00:00:00Z v1.3.0-rc.1 1 "must never own releases/latest"
run_case latest_lookup_failure_rejected v1.3.0-rc.1 true false true 2026-08-07T00:00:00Z "" 1 "unable to resolve GitHub releases/latest"

run_case create_stable v2.0.0 false false false 2026-08-07T00:00:00Z v2.0.0 0 "validated channel=stable"
grep -F -- "--latest" "${TMP_DIR}/create_stable/calls.log" >/dev/null || fail "stable creation must pass --latest"
if grep -F -- "--prerelease" "${TMP_DIR}/create_stable/calls.log" >/dev/null; then
  fail "stable creation must not pass --prerelease"
fi

run_case create_rc v2.1.0-rc.1 false false true 2026-08-07T00:00:00Z v2.0.0 0 "validated channel=rc"
grep -F -- "--prerelease" "${TMP_DIR}/create_rc/calls.log" >/dev/null || fail "RC creation must pass --prerelease"
if grep -F -- "--latest" "${TMP_DIR}/create_rc/calls.log" >/dev/null; then
  fail "RC creation must not pass --latest"
fi

if PATH="${BIN_DIR}:${PATH}" \
  GH_STUB_STATE_DIR="${TMP_DIR}/event-mismatch" \
  GH_STUB_TAG="v2.1.0-rc.1" \
  GH_STUB_DRAFT=false \
  GH_STUB_PRERELEASE=true \
  GH_STUB_PUBLISHED_AT=2026-08-07T00:00:00Z \
  GH_STUB_LATEST_TAG=v2.0.0 \
  "${ROOT_DIR}/scripts/release/validate_github_release_state.sh" \
    --tag v2.1.0-rc.1 \
    --event-prerelease false >/dev/null 2>&1; then
  fail "release-event prerelease mismatch must fail"
fi

printf '[runbook-publish-release-state-contract] passed\n'
