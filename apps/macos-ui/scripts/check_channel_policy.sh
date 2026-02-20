#!/bin/bash
set -euo pipefail

REPO_ROOT=$(git rev-parse --show-toplevel)
RENDER_SCRIPT="$REPO_ROOT/apps/macos-ui/scripts/render_channel_xcconfig.sh"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

run_render() {
    local output="$1"
    shift
    env "$@" "$RENDER_SCRIPT" "$output"
}

assert_succeeds() {
    local name="$1"
    shift
    if ! "$@"; then
        echo "::error::$name failed unexpectedly"
        exit 1
    fi
}

assert_fails() {
    local name="$1"
    shift
    local output_file
    output_file=$(mktemp)
    if "$@" >"$output_file" 2>&1; then
        echo "::error::$name unexpectedly succeeded"
        cat "$output_file"
        rm -f "$output_file"
        exit 1
    fi
    rm -f "$output_file"
}

DEFAULT_OUT="$TMP_DIR/default.xcconfig"
assert_succeeds "default developer profile should render" \
    run_render "$DEFAULT_OUT" HELM_CHANNEL_PROFILE=developer_id
grep -q '^HELM_DISTRIBUTION_CHANNEL = developer_id$' "$DEFAULT_OUT"
grep -q '^HELM_SPARKLE_ENABLED = NO$' "$DEFAULT_OUT"

APP_STORE_OUT="$TMP_DIR/app_store.xcconfig"
assert_succeeds "app_store profile should render" \
    run_render "$APP_STORE_OUT" HELM_CHANNEL_PROFILE=app_store
grep -q '^HELM_DISTRIBUTION_CHANNEL = app_store$' "$APP_STORE_OUT"
grep -q '^HELM_SPARKLE_ENABLED = NO$' "$APP_STORE_OUT"

assert_fails "unknown channel profile should fail" \
    run_render "$TMP_DIR/invalid_unknown_profile.xcconfig" \
    HELM_CHANNEL_PROFILE=unknown_profile

assert_fails "app_store must reject sparkle enabled override" \
    run_render "$TMP_DIR/invalid_app_store_enabled.xcconfig" \
    HELM_CHANNEL_PROFILE=app_store \
    HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES

assert_fails "setapp must reject sparkle metadata override" \
    run_render "$TMP_DIR/invalid_setapp_feed.xcconfig" \
    HELM_CHANNEL_PROFILE=setapp \
    HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL=https://updates.example.com/appcast.xml

assert_fails "developer_id sparkle requires feed/key" \
    run_render "$TMP_DIR/invalid_developer_missing_feed_key.xcconfig" \
    HELM_CHANNEL_PROFILE=developer_id \
    HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES

assert_fails "developer_id sparkle feed must use https" \
    run_render "$TMP_DIR/invalid_developer_http_feed.xcconfig" \
    HELM_CHANNEL_PROFILE=developer_id \
    HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES \
    HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL=http://updates.example.com/appcast.xml \
    HELM_CHANNEL_OVERRIDE_SPARKLE_PUBLIC_ED_KEY=example-public-ed-key

DEV_SPARKLE_OUT="$TMP_DIR/developer_sparkle.xcconfig"
assert_succeeds "developer_id sparkle override should render when fully configured" \
    run_render "$DEV_SPARKLE_OUT" \
    HELM_CHANNEL_PROFILE=developer_id \
    HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED=YES \
    HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL=https://updates.example.com/appcast.xml \
    HELM_CHANNEL_OVERRIDE_SPARKLE_PUBLIC_ED_KEY=example-public-ed-key
grep -q '^HELM_SPARKLE_ENABLED = YES$' "$DEV_SPARKLE_OUT"
grep -q '^HELM_SPARKLE_FEED_URL = https://updates.example.com/appcast.xml$' "$DEV_SPARKLE_OUT"
grep -q '^HELM_SPARKLE_PUBLIC_ED_KEY = example-public-ed-key$' "$DEV_SPARKLE_OUT"

echo "Channel policy checks passed."
