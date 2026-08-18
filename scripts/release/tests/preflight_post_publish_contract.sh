#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
PREFLIGHT_SOURCE="${ROOT_DIR}/scripts/release/preflight.sh"

fail() {
  printf '[preflight-post-publish-contract] error: %s\n' "$1" >&2
  exit 1
}

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TEST_ROOT="${TMP_DIR}/repo"
mkdir -p \
  "${TEST_ROOT}/core/rust" \
  "${TEST_ROOT}/scripts/release" \
  "${TEST_ROOT}/web/public/updates/cli" \
  "${TMP_DIR}/bin"
cp "$PREFLIGHT_SOURCE" "${TEST_ROOT}/scripts/release/preflight.sh"
chmod +x "${TEST_ROOT}/scripts/release/preflight.sh"

cat > "${TEST_ROOT}/core/rust/Cargo.toml" <<'TOML'
[workspace]
members = []

[workspace.package]
version = "0.18.0"
TOML

cat > "${TEST_ROOT}/web/public/updates/appcast.xml" <<'XML'
<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <item>
      <title>Helm 0.18.0</title>
      <enclosure sparkle:shortVersionString="0.18.0" />
    </item>
  </channel>
</rss>
XML

cat > "${TEST_ROOT}/web/public/updates/cli/latest.json" <<'JSON'
{
  "version": "0.18.0",
  "channel": "stable"
}
JSON

cat > "${TMP_DIR}/bin/gh" <<'STUB'
#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "auth" ] && [ "${2:-}" = "status" ]; then
  exit 0
fi
if [ "${1:-}" = "api" ] && [ "${2:-}" = "-i" ] && [ "${3:-}" = "user" ]; then
  printf 'HTTP/2 200\nx-oauth-scopes: repo, workflow\n'
  exit 0
fi
if [ "${1:-}" = "repo" ] && [ "${2:-}" = "view" ]; then
  printf 'example/helm\n'
  exit 0
fi
if [ "${1:-}" = "api" ] && [[ "${2:-}" == repos/example/helm/actions/workflows/* ]]; then
  workflow="${2##*/}"
  if [ "$workflow" = "${HELM_STUB_DISABLED_WORKFLOW:-}" ]; then
    printf 'disabled_inactivity\n'
  else
    printf 'active\n'
  fi
  exit 0
fi
exit 97
STUB
chmod +x "${TMP_DIR}/bin/gh"

git -C "$TEST_ROOT" init -q -b task
git -C "$TEST_ROOT" config user.name "Helm Release Contract"
git -C "$TEST_ROOT" config user.email "release-contract@example.invalid"
git -C "$TEST_ROOT" add core web scripts/release/preflight.sh
git -C "$TEST_ROOT" commit -q -m "test fixture"
git -C "$TEST_ROOT" update-ref refs/remotes/origin/main HEAD

base_args=(
  --tag v0.18.0
  --allow-non-main
  --no-fetch
  --skip-secrets
  --skip-ruleset-policy
  --allow-existing-tag
)
common_args=("${base_args[@]}" --skip-workflows)

if PATH="${TMP_DIR}/bin:${PATH}" "${TEST_ROOT}/scripts/release/preflight.sh" "${common_args[@]}" >"${TMP_DIR}/strict.log" 2>&1; then
  fail "equal published metadata must fail in strict pre-tag mode"
fi
grep -F "stable metadata on origin/main already matches target version 0.18.0" "${TMP_DIR}/strict.log" >/dev/null || \
  fail "strict pre-tag failure reason was not reported"

PATH="${TMP_DIR}/bin:${PATH}" "${TEST_ROOT}/scripts/release/preflight.sh" \
  "${common_args[@]}" --allow-published-metadata >"${TMP_DIR}/post-publish.log" 2>&1
grep -F "source workspace version matches release tag (0.18.0)" \
  "${TMP_DIR}/post-publish.log" >/dev/null || fail "matching source workspace version was not accepted"
grep -F "stable metadata on origin/main matches target version 0.18.0 (post-publication verification)" \
  "${TMP_DIR}/post-publish.log" >/dev/null || fail "post-publication acceptance was not reported"

mismatch_args=(
  --tag v0.18.1
  --allow-non-main
  --no-fetch
  --skip-secrets
  --skip-workflows
  --skip-ruleset-policy
  --allow-existing-tag
)
if PATH="${TMP_DIR}/bin:${PATH}" "${TEST_ROOT}/scripts/release/preflight.sh" \
  "${mismatch_args[@]}" >"${TMP_DIR}/source-mismatch.log" 2>&1; then
  fail "source workspace version mismatch must fail preflight"
fi
grep -F "release tag version 0.18.1 does not match Rust workspace version 0.18.0" \
  "${TMP_DIR}/source-mismatch.log" >/dev/null || fail "source workspace mismatch reason was not reported"

rc_mismatch_args=(
  --tag v0.18.0-rc.1
  --allow-non-main
  --no-fetch
  --skip-secrets
  --skip-workflows
  --skip-ruleset-policy
  --allow-existing-tag
)
if PATH="${TMP_DIR}/bin:${PATH}" "${TEST_ROOT}/scripts/release/preflight.sh" \
  "${rc_mismatch_args[@]}" >"${TMP_DIR}/rc-source-mismatch.log" 2>&1; then
  fail "RC source workspace version mismatch must fail preflight"
fi
grep -F "release tag version 0.18.0-rc.1 does not match Rust workspace version 0.18.0" \
  "${TMP_DIR}/rc-source-mismatch.log" >/dev/null || fail "RC source workspace mismatch reason was not reported"

if HELM_STUB_DISABLED_WORKFLOW=appcast-drift.yml PATH="${TMP_DIR}/bin:${PATH}" \
  "${TEST_ROOT}/scripts/release/preflight.sh" "${base_args[@]}" --allow-published-metadata \
  >"${TMP_DIR}/disabled-workflow.log" 2>&1; then
  fail "disabled required workflow must fail preflight"
fi
grep -F "required workflow is not active: appcast-drift.yml (state=disabled_inactivity)" \
  "${TMP_DIR}/disabled-workflow.log" >/dev/null || fail "disabled workflow failure reason was not reported"

PATH="${TMP_DIR}/bin:${PATH}" "${TEST_ROOT}/scripts/release/preflight.sh" \
  "${base_args[@]}" --allow-published-metadata >"${TMP_DIR}/active-workflows.log" 2>&1

printf '[preflight-post-publish-contract] passed\n'
