#!/usr/bin/env bash
set -euo pipefail

DEFAULT_ENDPOINT="https://helmapp.dev/updates/cli/latest.json"
ALLOWED_HOSTS="helmapp.dev github.com objects.githubusercontent.com github-releases.githubusercontent.com release-assets.githubusercontent.com"
DEFAULT_CONNECT_TIMEOUT_SECS="10"
DEFAULT_MAX_TIME_SECS="60"

if [ -z "${HOME:-}" ]; then
  echo "error: HOME is not set." >&2
  exit 1
fi

DEFAULT_BIN_DIR="$HOME/.local/bin"
DEFAULT_MARKER_PATH="$HOME/.config/helm/install.json"

ENDPOINT="${HELM_CLI_UPDATE_ENDPOINT:-$DEFAULT_ENDPOINT}"
BIN_DIR="${HELM_INSTALL_BIN_DIR:-$DEFAULT_BIN_DIR}"
MARKER_PATH="${HELM_INSTALL_MARKER_PATH:-$DEFAULT_MARKER_PATH}"
FORCED_ARCH="${HELM_INSTALL_FORCE_ARCH:-}"
ALLOW_INSECURE_URLS="${HELM_INSTALL_ALLOW_INSECURE_URLS:-}"
ALLOW_ROOT_INSTALL="${HELM_ALLOW_ROOT_INSTALL:-}"
CONNECT_TIMEOUT_SECS="${HELM_INSTALL_CONNECT_TIMEOUT_SECS:-$DEFAULT_CONNECT_TIMEOUT_SECS}"
MAX_TIME_SECS="${HELM_INSTALL_MAX_TIME_SECS:-$DEFAULT_MAX_TIME_SECS}"

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool '$tool'." >&2
    exit 1
  fi
}

require_tool curl
require_tool python3
require_tool shasum
require_tool install
require_tool id

if [ "${EUID:-$(id -u)}" = "0" ] && [ "$ALLOW_ROOT_INSTALL" != "1" ]; then
  echo "error: running install.sh as root is blocked by default." >&2
  echo "set HELM_ALLOW_ROOT_INSTALL=1 to explicitly opt in." >&2
  exit 1
fi

map_arch() {
  local raw="${1:-}"
  case "$raw" in
  arm64 | aarch64)
    echo "arm64"
    ;;
  x86_64 | amd64)
    echo "x86_64"
    ;;
  *)
    echo ""
    ;;
  esac
}

ARCH="$(map_arch "${FORCED_ARCH:-$(uname -m)}")"
if [ -z "$ARCH" ]; then
  echo "error: unsupported architecture '$(uname -m)' (set HELM_INSTALL_FORCE_ARCH=arm64|x86_64)." >&2
  exit 1
fi

WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/helm-install.XXXXXX")"
MANIFEST_PATH="$WORK_DIR/latest.json"
DOWNLOAD_PATH="$WORK_DIR/helm"

cleanup() {
  rm -rf "$WORK_DIR"
}
trap cleanup EXIT

validate_url_policy() {
  local url="$1"
  local label="$2"
  python3 - "$url" "$label" "$ALLOW_INSECURE_URLS" "$ALLOWED_HOSTS" <<'PY'
import sys
from urllib.parse import urlparse

url, label, allow_insecure_raw, allowed_hosts_raw = sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4]
allow_insecure = allow_insecure_raw.strip().lower() in {"1", "true", "yes", "on"}
allowed_hosts = {item.strip().lower() for item in allowed_hosts_raw.split() if item.strip()}

parsed = urlparse(url.strip())
scheme = (parsed.scheme or "").lower()
host = (parsed.hostname or "").lower()

if scheme == "file":
    if allow_insecure:
        raise SystemExit(0)
    print(
        f"error: {label} URL uses file://; set HELM_INSTALL_ALLOW_INSECURE_URLS=1 for explicit test-only override.",
        file=sys.stderr,
    )
    raise SystemExit(1)

if scheme != "https":
    print(f"error: {label} URL must use https.", file=sys.stderr)
    raise SystemExit(1)

if host not in allowed_hosts:
    print(f"error: {label} URL host '{host}' is not allowlisted.", file=sys.stderr)
    raise SystemExit(1)
PY
}

curl_fetch() {
  local url="$1"
  local out="$2"
  curl -fsSL \
    --connect-timeout "$CONNECT_TIMEOUT_SECS" \
    --max-time "$MAX_TIME_SECS" \
    --retry 2 \
    --retry-delay 1 \
    --retry-all-errors \
    "$url" -o "$out"
}

validate_url_policy "$ENDPOINT" "endpoint"
curl_fetch "$ENDPOINT" "$MANIFEST_PATH"

read_manifest_field() {
  local field="$1"
  python3 - "$MANIFEST_PATH" "$ARCH" "$field" <<'PY'
import json
import sys

manifest_path, arch, field = sys.argv[1], sys.argv[2], sys.argv[3]

with open(manifest_path, "r", encoding="utf-8") as f:
    payload = json.load(f)

downloads = payload.get("downloads") or {}
asset = downloads.get("universal") or downloads.get(arch)

if field == "version":
    value = payload.get("version", "")
elif field == "url":
    value = (asset or {}).get("url", "")
elif field == "sha256":
    value = (asset or {}).get("sha256", "")
else:
    value = ""

if not isinstance(value, str):
    value = ""

print(value.strip())
PY
}

VERSION="$(read_manifest_field version)"
URL="$(read_manifest_field url)"
EXPECTED_SHA="$(read_manifest_field sha256)"

if [ -z "$VERSION" ]; then
  echo "error: update manifest is missing version." >&2
  exit 1
fi
if [ -z "$URL" ] || [ -z "$EXPECTED_SHA" ]; then
  echo "error: update manifest is missing download metadata for arch '$ARCH'." >&2
  exit 1
fi

validate_url_policy "$URL" "download"
curl_fetch "$URL" "$DOWNLOAD_PATH"

ACTUAL_SHA="$(shasum -a 256 "$DOWNLOAD_PATH" | awk '{print $1}' | tr '[:upper:]' '[:lower:]')"
NORMALIZED_EXPECTED_SHA="$(echo "$EXPECTED_SHA" | tr '[:upper:]' '[:lower:]' | sed -e 's/^sha256://')"

if [ "$ACTUAL_SHA" != "$NORMALIZED_EXPECTED_SHA" ]; then
  echo "error: checksum mismatch for downloaded helm binary." >&2
  echo "expected: $NORMALIZED_EXPECTED_SHA" >&2
  echo "actual:   $ACTUAL_SHA" >&2
  exit 1
fi

install -d "$BIN_DIR"
install -m 0755 "$DOWNLOAD_PATH" "$BIN_DIR/helm"

install -d "$(dirname "$MARKER_PATH")"
INSTALLED_AT="$(date -u +"%Y-%m-%dT%H:%M:%SZ")"
python3 - "$MARKER_PATH" "$VERSION" "$INSTALLED_AT" <<'PY'
import json
import os
import sys
from pathlib import Path

marker_path, version, installed_at = sys.argv[1], sys.argv[2], sys.argv[3]
target = Path(marker_path)
parent = target.parent
parent.mkdir(parents=True, exist_ok=True)

if target.exists() and target.is_symlink():
    raise SystemExit("error: refusing to write provenance marker through symlink path.")

temp_path = target.with_name(f"{target.name}.tmp-{os.getpid()}")

payload = {
    "channel": "direct-script",
    "artifact": "helm-cli",
    "installed_at": installed_at,
    "update_policy": "self",
    "version": version,
}

with open(temp_path, "x", encoding="utf-8") as f:
    json.dump(payload, f, indent=2, sort_keys=False)
    f.write("\n")
os.replace(temp_path, target)
PY

echo "Installed Helm CLI ${VERSION} to ${BIN_DIR}/helm"
echo "Provenance marker written to ${MARKER_PATH}"

case ":$PATH:" in
*":$BIN_DIR:"*)
  ;;
*)
  echo "PATH hint: add '${BIN_DIR}' to PATH to run 'helm' directly."
  ;;
esac
