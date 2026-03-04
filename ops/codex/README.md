# Codex Operations Lane

This directory is the canonical home for Codex-only operational assets.

## Canonical paths

- Skills: `ops/codex/skills/`
- Scripts: `ops/codex/scripts/`
- Docs: `ops/codex/docs/`

## Compatibility layer

Legacy paths are retained to avoid breakage while references are migrated:

- `skills` -> symlink to `ops/codex/skills`
- `scripts/codex/*` -> wrapper scripts that exec `ops/codex/scripts/*`
- `docs/codex/*` -> pointer docs to `ops/codex/docs/*`

Use canonical paths for all new documentation and automation.
