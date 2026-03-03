# AGENTS.md — apps/macos-ui

This file applies to `apps/macos-ui/**`.

## Parent Policy

- Read and follow repository root `AGENTS.md` first.
- Root policy wins on conflicts.

## Scope

Use this subtree guidance for:
- SwiftUI presentation
- XPC service integration points in app layer
- build scripts and channel config under `apps/macos-ui/scripts` and `apps/macos-ui/Config`

## Local Working Rules

- Keep UI presentation-only; push business logic to Rust core/service boundaries.
- Keep user-facing text localized via keys.
- Preserve locale mirror parity between:
  - `locales/`
  - `apps/macos-ui/Helm/Resources/locales/`
- Treat `apps/macos-ui/Generated/HelmVersion.xcconfig` as generated output unless task explicitly targets build metadata.

## Fast Verification Commands

- `xcodebuild -project apps/macos-ui/Helm.xcodeproj -scheme Helm -destination 'platform=macOS' -configuration Debug CODE_SIGN_IDENTITY=- CODE_SIGNING_REQUIRED=NO CODE_SIGNING_ALLOWED=NO test`
- `apps/macos-ui/scripts/check_channel_policy.sh`
- `apps/macos-ui/scripts/check_locale_integrity.sh`
- `apps/macos-ui/scripts/check_locale_lengths.sh`

If sandboxed `xcodebuild` output looks unreliable, repeat verification outside sandbox per root policy.
