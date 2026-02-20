#!/bin/bash
set -euo pipefail

echo "Building Rust core..."

# Ensure rustup toolchain is available in Xcode build environment
export PATH="$HOME/.cargo/bin:$PATH"

# Go to repo root
REPO_ROOT=$(git rev-parse --show-toplevel)
cd "$REPO_ROOT/core/rust"

# Determine build profile
if [[ "${CONFIGURATION:-Debug}" == Release* ]]; then
    PROFILE="release"
    CARGO_FLAGS="--release"
else
    PROFILE="debug"
    CARGO_FLAGS=""
fi

# Map Xcode architecture names to Rust target triples.
map_arch_to_target() {
    case "$1" in
        arm64|arm64e)
            echo "aarch64-apple-darwin"
            ;;
        x86_64)
            echo "x86_64-apple-darwin"
            ;;
        *)
            return 1
            ;;
    esac
}

normalize_xcconfig_bool() {
    case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
        1|true|yes|on)
            echo "YES"
            ;;
        *)
            echo "NO"
            ;;
    esac
}

trim_xcconfig_value() {
    printf '%s' "$1" | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*$//'
}

read_xcconfig_value() {
    local xcconfig_file="$1"
    local key="$2"
    awk -F '=' -v key="$key" '
        $1 ~ "^[[:space:]]*" key "[[:space:]]*$" {
            value = $2
            sub(/[[:space:]]*\/\/.*/, "", value)
            sub(/^[[:space:]]+/, "", value)
            sub(/[[:space:]]+$/, "", value)
            print value
            found = 1
            exit
        }
        END {
            if (!found) {
                print ""
            }
        }
    ' "$xcconfig_file"
}

CHANNEL_PROFILE="${HELM_CHANNEL_PROFILE:-developer_id}"
CHANNEL_CONFIG_DIR="$REPO_ROOT/apps/macos-ui/Config/channels"
CHANNEL_TEMPLATE="$CHANNEL_CONFIG_DIR/${CHANNEL_PROFILE}.xcconfig"
if [ ! -f "$CHANNEL_TEMPLATE" ]; then
    echo "Unknown HELM_CHANNEL_PROFILE '$CHANNEL_PROFILE' (expected developer_id, app_store, setapp, or fleet)." >&2
    exit 1
fi

CHANNEL_DISTRIBUTION=$(trim_xcconfig_value "$(read_xcconfig_value "$CHANNEL_TEMPLATE" HELM_DISTRIBUTION_CHANNEL)")
CHANNEL_SPARKLE_ENABLED=$(normalize_xcconfig_bool "$(read_xcconfig_value "$CHANNEL_TEMPLATE" HELM_SPARKLE_ENABLED)")
CHANNEL_SPARKLE_FEED_URL=$(trim_xcconfig_value "$(read_xcconfig_value "$CHANNEL_TEMPLATE" HELM_SPARKLE_FEED_URL)")
CHANNEL_SPARKLE_PUBLIC_ED_KEY=$(trim_xcconfig_value "$(read_xcconfig_value "$CHANNEL_TEMPLATE" HELM_SPARKLE_PUBLIC_ED_KEY)")

if [ -n "${HELM_CHANNEL_OVERRIDE_DISTRIBUTION:-}" ]; then
    CHANNEL_DISTRIBUTION=$(trim_xcconfig_value "$HELM_CHANNEL_OVERRIDE_DISTRIBUTION")
fi
if [ -n "${HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED:-}" ]; then
    CHANNEL_SPARKLE_ENABLED=$(normalize_xcconfig_bool "$HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED")
fi
if [ -n "${HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL:-}" ]; then
    CHANNEL_SPARKLE_FEED_URL=$(trim_xcconfig_value "$HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL")
fi
if [ -n "${HELM_CHANNEL_OVERRIDE_SPARKLE_PUBLIC_ED_KEY:-}" ]; then
    CHANNEL_SPARKLE_PUBLIC_ED_KEY=$(trim_xcconfig_value "$HELM_CHANNEL_OVERRIDE_SPARKLE_PUBLIC_ED_KEY")
fi

if [ "$CHANNEL_DISTRIBUTION" != "developer_id" ]; then
    if [ "$CHANNEL_SPARKLE_ENABLED" = "YES" ]; then
        echo "Invalid channel config: Sparkle must be disabled when HELM_DISTRIBUTION_CHANNEL=$CHANNEL_DISTRIBUTION." >&2
        exit 1
    fi
    if [ -n "$CHANNEL_SPARKLE_FEED_URL" ] || [ -n "$CHANNEL_SPARKLE_PUBLIC_ED_KEY" ]; then
        echo "Invalid channel config: Sparkle feed/signing metadata must be empty for non-developer channels ($CHANNEL_DISTRIBUTION)." >&2
        exit 1
    fi
fi

if [ "$CHANNEL_DISTRIBUTION" = "developer_id" ] && [ "$CHANNEL_SPARKLE_ENABLED" = "YES" ]; then
    if [ -z "$CHANNEL_SPARKLE_FEED_URL" ] || [ -z "$CHANNEL_SPARKLE_PUBLIC_ED_KEY" ]; then
        echo "Invalid channel config: Developer ID channel with Sparkle enabled requires HELM_SPARKLE_FEED_URL and HELM_SPARKLE_PUBLIC_ED_KEY." >&2
        exit 1
    fi
fi

# Xcode provides ARCHS (e.g. "arm64 x86_64") when building for "Any Mac".
REQUESTED_ARCHS="${ARCHS:-$(uname -m)}"
IFS=' ' read -r -a XCODE_ARCHS <<< "$REQUESTED_ARCHS"

RUST_TARGETS=()
for arch in "${XCODE_ARCHS[@]}"; do
    target=$(map_arch_to_target "$arch" || true)
    if [ -n "${target:-}" ]; then
        already_present=0
        for existing in "${RUST_TARGETS[@]:-}"; do
            if [ "$existing" = "$target" ]; then
                already_present=1
                break
            fi
        done
        if [ "$already_present" -eq 0 ]; then
            RUST_TARGETS+=("$target")
        fi
    fi
done

if [ "${#RUST_TARGETS[@]}" -eq 0 ]; then
    host_target=$(map_arch_to_target "$(uname -m)" || true)
    if [ -z "${host_target:-}" ]; then
        echo "Unsupported host architecture: $(uname -m)" >&2
        exit 1
    fi
    RUST_TARGETS+=("$host_target")
fi

echo "Requested Xcode ARCHS: $REQUESTED_ARCHS"
echo "Rust targets: ${RUST_TARGETS[*]}"

LIB_INPUTS=()
installed_targets=""
if command -v rustup >/dev/null 2>&1; then
    installed_targets=$(rustup target list --installed || true)
fi

for target in "${RUST_TARGETS[@]}"; do
    if [ -n "$installed_targets" ] && ! printf '%s\n' "$installed_targets" | grep -qx "$target"; then
        if [ "${HELM_AUTO_INSTALL_RUST_TARGETS:-1}" = "1" ] && command -v rustup >/dev/null 2>&1; then
            echo "Installing missing Rust target: $target"
            rustup target add "$target" || true
            installed_targets=$(rustup target list --installed || true)
        fi
    fi

    if [ -n "$installed_targets" ] && ! printf '%s\n' "$installed_targets" | grep -qx "$target"; then
        if [ "$CONFIGURATION" = "Release" ]; then
            echo "Required Rust target is missing for Release build: $target" >&2
            exit 1
        fi
        echo "Skipping unavailable Rust target in non-Release build: $target"
        continue
    fi

    echo "Building helm-ffi for $target..."
    cargo build -p helm-ffi $CARGO_FLAGS --target "$target"
    LIB_INPUTS+=("target/$target/$PROFILE/libhelm_ffi.a")
done

if [ "${#LIB_INPUTS[@]}" -eq 0 ]; then
    echo "No Rust targets were built. Ensure rustup targets are installed." >&2
    exit 1
fi

# Copy artifacts to a build directory inside apps/macos-ui
# ensuring Xcode can find them
DEST_DIR="$REPO_ROOT/apps/macos-ui/Generated"
mkdir -p "$DEST_DIR"

cat > "$DEST_DIR/HelmChannel.xcconfig" <<XCCONFIG
// Auto-generated by build_rust.sh — do not edit
HELM_DISTRIBUTION_CHANNEL = $CHANNEL_DISTRIBUTION
HELM_SPARKLE_ENABLED = $CHANNEL_SPARKLE_ENABLED
HELM_SPARKLE_FEED_URL = $CHANNEL_SPARKLE_FEED_URL
HELM_SPARKLE_PUBLIC_ED_KEY = $CHANNEL_SPARKLE_PUBLIC_ED_KEY
XCCONFIG

if [ "${#LIB_INPUTS[@]}" -eq 1 ]; then
    cp "${LIB_INPUTS[0]}" "$DEST_DIR/libhelm_ffi.a"
else
    echo "Creating universal static library..."
    lipo -create "${LIB_INPUTS[@]}" -output "$DEST_DIR/libhelm_ffi.a"
fi
cp "crates/helm-ffi/include/helm.h" "$DEST_DIR/"

# Extract version from workspace Cargo.toml and generate Swift constant
VERSION=$(grep '^version' Cargo.toml | head -1 | sed 's/.*"\(.*\)".*/\1/')
cat > "$DEST_DIR/HelmVersion.swift" <<SWIFT
// Auto-generated by build_rust.sh — do not edit
let helmVersion = "$VERSION"
SWIFT

# Generate xcconfig for Xcode bundle version synchronization
# Strip prerelease tag for MARKETING_VERSION (Apple requires X.Y.Z)
MARKETING_VERSION=$(echo "$VERSION" | sed 's/-.*//')
# Extract build number from prerelease tag (e.g. alpha.1 -> 1); default to 0
BUILD_NUMBER=$(echo "$VERSION" | sed -n 's/.*\.\([0-9][0-9]*\)$/\1/p')
BUILD_NUMBER=${BUILD_NUMBER:-0}
cat > "$DEST_DIR/HelmVersion.xcconfig" <<XCCONFIG
// Auto-generated by build_rust.sh — do not edit
MARKETING_VERSION = $MARKETING_VERSION
CURRENT_PROJECT_VERSION = $BUILD_NUMBER
XCCONFIG

echo "Rust build complete (v$VERSION). Artifacts in $DEST_DIR"
