# Helm Website

This is the source code for the [Helm product website](https://helmapp.dev/).

## Built With

- [Astro](https://astro.build)
- [Starlight](https://starlight.astro.build)

## Development

1. Install dependencies: `npm install`
2. Start dev server: `npm run dev`
3. Build for production: `npm run build`

## Dependency Security

The `postcss-nested@6.2.0` dependency used by Expressive Code has a scoped
`postcss-selector-parser@7.1.6` override for
[GHSA-rj75-hqrm-r3gf](https://github.com/advisories/GHSA-rj75-hqrm-r3gf).
The parent still requires parser 6.x, so a lockfile-only update cannot select the
patched parser. Keep this override until the parent dependency uses a patched
parser itself; review it when upgrading Expressive Code or `postcss-nested`.

Run `npm run test:css-dependencies` and `npm run build` when changing this chain.
The tests cover nested selector expansion, escaping, functional selectors, media
rules, and the parser version actually resolved by the consumer. Use `npm audit`
to check the complete dependency tree; a green dependency-review check alone
does not mean existing alerts have been resolved. Changes merged to `main` must
also be synchronized to `dev` and successfully deployed before delivery is
considered complete.

The separate `http-cache-semantics` advisory
[GHSA-ch52-4w7c-c8xp](https://github.com/advisories/GHSA-ch52-4w7c-c8xp) is
[disputed upstream](https://github.com/github/advisory-database/issues/10139).
Version 4.3.0 is outside the current affected range, but this is not proof that
every reported cache-policy edge case is fixed. Helm currently generates a
static site; Astro uses this dependency in its build-time remote-image cache,
not a shared server-side cache serving visitors. Reassess exposure before adding
SSR, a shared request cache, or visitor-controlled remote image URLs.
