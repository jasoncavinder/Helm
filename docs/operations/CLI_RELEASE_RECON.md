# CLI Release Recon Snapshot

Date: 2026-02-23  
Context: CLI self-update + distribution plumbing pass

## Project Structure Findings

- CLI source:
  - `core/rust/crates/helm-cli`
- macOS app source:
  - `apps/macos-ui/Helm`
  - `apps/macos-ui/HelmService`
- Core runtime:
  - `core/rust/crates/helm-core`
- Docs roots:
  - `docs/`
  - `docs/architecture/`
- GitHub workflows:
  - `.github/workflows/`
- Website content:
  - `web/src/` (site/docs content)
  - `web/public/updates/` (update metadata payloads)

## Variant/Channel Baseline Findings

- App channel profiles already present:
  - `developer_id`, `app_store`, `setapp`, `fleet`
- Sparkle flow is already implemented and release-automated for direct DMG (`developer_id`).
- Pre-pass CLI self-update behavior was Homebrew-formula-oriented only.
- No first-class CLI direct installer script/workflow existed before this pass.
