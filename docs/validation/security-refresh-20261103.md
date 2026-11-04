# Security Refresh Before PR #650

## Scope and review order

The October 18 checks no longer cover the current advisory set. The November 3
refresh preserves #650's independently reviewed tracking provenance/error
isolation correction (`969bae40`) and adds only dependency, CI and documentation
changes. No application/service source, migration, locale, release version,
license, signing configuration or publication metadata changes.

Review and merge #653 (stable Rust patches), then #652 (website patches and full
lockfile audit), then #650 (development synchronization). #652 includes #653's
exact prerequisite commit `5e751d8a`, without promoting v0.20 features to `main`.
#652 also includes all four dependency updates from #649; the older bot PR is
redundant once #652 is merged. Do not merge an older lockfile over the refreshed
candidate. Existing open alert counts can remain until default-branch merges
and GitHub's next scan; no alerts were dismissed.

## Dependency changes

Both Rust graphs receive rustls-webpki 0.103.16 (RUSTSEC-2026-0382), webpki-roots
1.0.10 (RUSTSEC-2026-0381), and Tokio 1.53.3 (RUSTSEC-2026-0386). Existing audit
exceptions RUSTSEC-2024-0436 and RUSTSEC-2026-0002 are unchanged; no exception was
added. The lockfile diff preserves unrelated dependency resolutions.

The website resolves Astro 7.3.3, Vite 8.1.6, Undici 8.12.2, Sharp 0.35.5 with
matching native/libvips packages, devalue 5.9.4, smol-toml 1.9.1 and Zod 4.6.6.
The fresh npm audit found the Zod advisory before it appeared in the initial
repository alert list. Astro's patch also updates find-proc and adds verkit.
Starlight, Expressive Code and the unrelated compiler/bundler versions remain
unchanged. postcss-selector-parser now uses its compatible 6.1.5 security
backport; the earlier cross-major override is removed. The graph retains
http-cache-semantics 4.3.0 with #651's disputed cache-policy caveat, not a claim
that every reported behavior is fixed.

Dependency Security now runs `npm audit --package-lock-only --audit-level=low`
on PRs and the existing weekly schedule. It includes development dependencies,
does not install dependencies/run scripts, and does not suppress failures.
Three policy regressions cover event coverage, command/failure behavior and
read-only permissions. They run alongside the existing runner/cache contracts.

## Fresh disposable-VM evidence

Evidence root: `/Users/agent/helm-certification/security-refresh-20261103/`.
Runtime work used private HOME/cache/temp paths and an explicit development DB
path. The host's production Helm and stable database were not touched.

- Rust/Cargo 1.98.1, native arm64 macOS. GitHub CI separately tests its pinned
  Rust 1.97.1 and platform matrix.
- RustSec database commit `965bf9ad083b58e6340d65829d85217b18be9290`.
- Both original Rust lockfiles fail with the two TLS vulnerabilities and denied
  Tokio unsoundness warning. Both updated lockfiles pass the same audit command
  and unchanged approved exceptions.
- Development `cargo test --locked --workspace`: 1,734 passed, zero failed,
  five existing opt-in uv lifecycle tests ignored, across 62 result suites.
  Those real-manager lifecycles were not repeated for this lockfile refresh.
- Stable `cargo test --locked --workspace`: 1,311 passed, zero failed/ignored,
  across 44 result suites. Both graphs pass Clippy with `-D warnings`.
- The original website graph fails with seven npm vulnerability groups. Final
  graph: zero vulnerabilities from the full lockfile audit; reproducible
  `npm ci`, all three CSS regressions, guide content-ID validation and static
  production build pass under Node 24.22.2 / npm 11.19.1. The 16-page build
  retains the existing empty-i18n/missing-404 content warnings.
- Final development controls: 20 runner/cache/audit policy tests, 28 security
  backport tests, eight PR-only concurrency tests and actionlint pass. The merge
  explicitly preserves development's PR-only concurrency checks and #650's
  corrected tracker implementation.

Final lockfile SHA-256 values:

| Graph | SHA-256 |
| --- | --- |
| Development Rust | `459948846970329aa89b5d87750376e7271e9850e5fe0df40850c1098471b004` |
| Stable Rust | `8d673a94b174acf5d749f3532c83a2ca01d13ce503ec07ad816e7eb5f737ccda` |
| Website, both branches | `d62147440be9f458b472d15aba190decb90705907664a3b97fd41a9c5c0ef6e5` |

## Remaining gates

Fresh remote head CI and owner review/merge are required; historical green checks
and this VM record are not merge approval for changed code. After #650 merges,
carry its tracker correction to `main` separately as already planned. Website
deployment and any stable app/CLI patch publication remain distinct delivery
steps; nothing was published or released by this validation. v0.20's operational
Sparkle and final-candidate gates are unchanged.
