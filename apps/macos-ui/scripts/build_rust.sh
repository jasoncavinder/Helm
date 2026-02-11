#!/bin/bash
set -e

# Environment setup
# Assuming the script is run from the project root or we can find it.
# Xcode runs from project dir usually.

echo "Building Rust core..."

# Go to repo root
REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT/core/rust"

# Determine build profile
if [ "$CONFIGURATION" = "Release" ]; then
    PROFILE="release"
    CARGO_FLAGS="--release"
else
    PROFILE="debug"
    CARGO_FLAGS=""
fi

# Build
cargo build -p helm-ffi $CARGO_FLAGS

# Copy artifacts to a build directory inside apps/macos-ui
# ensuring Xcode can find them
DEST_DIR="$REPO_ROOT/apps/macos-ui/Generated"
mkdir -p "$DEST_DIR"

cp "target/$PROFILE/libhelm_ffi.a" "$DEST_DIR/"
cp "crates/helm-ffi/include/helm.h" "$DEST_DIR/"

echo "Rust build complete. Artifacts in $DEST_DIR"
