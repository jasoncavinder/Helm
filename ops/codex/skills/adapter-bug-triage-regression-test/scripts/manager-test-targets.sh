#!/usr/bin/env bash
set -euo pipefail

MANAGER_ID="${1:-}"
if [ -z "$MANAGER_ID" ]; then
  echo "Usage: $0 <manager-id>" >&2
  exit 64
fi

case "$MANAGER_ID" in
  homebrew|homebrew_formula)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test end_to_end_homebrew
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test manager_lifecycle_matrix
OUT
    ;;
  mise)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test end_to_end_mise
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test orchestration_adapter_runtime
OUT
    ;;
  rustup)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test end_to_end_rustup
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test orchestration_runtime_queue
OUT
    ;;
  mas)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test end_to_end_mas
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test manager_lifecycle_matrix
OUT
    ;;
  softwareupdate)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test end_to_end_softwareupdate
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test authority_ordering
OUT
    ;;
  npm|pnpm|yarn|pip|pipx|poetry|rubygems|bundler|cargo|cargo-binstall)
    cat <<'OUT'
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test manager_lifecycle_matrix
cargo test --manifest-path core/rust/Cargo.toml -p helm-core --test adapter_capability_gate
OUT
    ;;
  *)
    cat <<'OUT'
cargo test --workspace --manifest-path core/rust/Cargo.toml
OUT
    ;;
esac
