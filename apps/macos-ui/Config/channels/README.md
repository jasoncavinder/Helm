# Channel Profiles

These xcconfig profiles define channel-specific update metadata consumed by
`apps/macos-ui/scripts/build_rust.sh`, which emits
`apps/macos-ui/Generated/HelmChannel.xcconfig` during build.

Supported `HELM_CHANNEL_PROFILE` values:

- `developer_id`
- `app_store`
- `setapp`
- `fleet`

Environment overrides (optional):

- `HELM_CHANNEL_OVERRIDE_DISTRIBUTION`
- `HELM_CHANNEL_OVERRIDE_SPARKLE_ENABLED`
- `HELM_CHANNEL_OVERRIDE_SPARKLE_FEED_URL`
- `HELM_CHANNEL_OVERRIDE_SPARKLE_PUBLIC_ED_KEY`
