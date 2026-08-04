#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
cd "$ROOT_DIR"

echo "[sqlite-compat] validating immutable migration manifest"
python3 scripts/ci/check_sqlite_migration_manifest.py

echo "[sqlite-compat] running core migration matrix"
cargo test \
  --manifest-path core/rust/Cargo.toml \
  -p helm-core \
  --test sqlite_migrations_contract \
  -- \
  --test-threads=1

echo "[sqlite-compat] running real CLI recovery boundary"
cargo test \
  --manifest-path core/rust/Cargo.toml \
  -p helm-cli \
  --test migration_recovery_cli \
  -- \
  --test-threads=1

echo "[sqlite-compat] running isolated FFI initialization boundary"
cargo test \
  --manifest-path core/rust/Cargo.toml \
  -p helm-ffi \
  tests::ffi_init_ \
  -- \
  --test-threads=1

echo "[sqlite-compat] migration compatibility gate passed"
