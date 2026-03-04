# AGENTS.md — web

This file applies to `web/**`.

## Parent Policy

- Read and follow repository root `AGENTS.md` first.
- Root policy wins on conflicts.

## Scope

Use this subtree guidance for:
- Astro/Starlight site code
- content/docs pages under `web/`
- web build and content-id checks

## Local Working Rules

- Preserve existing Helm website visual/system conventions unless a redesign task explicitly says otherwise.
- Keep website changes isolated from app/core/runtime code unless requested.
- Do not reintroduce GitHub Pages deploy assumptions; production host is Cloudflare Pages.

## Fast Verification Commands

Run from `web/`:
- `npm ci`
- `npm run check:content-ids`
- `npm run build`
