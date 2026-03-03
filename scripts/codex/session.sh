#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
export CODEX_HOME="${CODEX_HOME:-$ROOT_DIR/.codex}"

mkdir -p "$CODEX_HOME"

exec codex -C "$ROOT_DIR" "$@"
