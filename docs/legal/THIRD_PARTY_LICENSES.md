# Third-Party Dependency Licenses and Release Obligations

This document tracks third-party license obligations for Helm distributions.

It is an engineering compliance reference, not legal advice.

---

## Scope

Helm has three dependency surfaces with different obligations:

1. **macOS app runtime artifact** (DMG/app bundle)
2. **Build-only tooling** (used to produce artifacts, not shipped)
3. **Website build toolchain** (`web/` dependencies)

The Helm project license (`LICENSE`) does not replace third-party license obligations.

---

## Snapshot (Audited 2026-08-11; release context updated 2026-08-15)

Release context:

- source candidate: `v0.19.0-rc.7`
- published stable baseline: `v0.18.2`

### 1) macOS App Runtime Dependencies

#### Rust core/runtime (`helm-ffi` normal dependency graph)

- Third-party package versions in the macOS target tree: **80**
- License families observed include MIT, Apache-2.0, BSD-3-Clause, ISC,
  Zlib, 0BSD, BSL-1.0, CDLA-Permissive-2.0, Unicode-3.0, and Unlicense
  under permissive single- or multi-license expressions.
- Notable updated direct dependencies include `rusqlite` `0.40.1` (MIT),
  `quick-xml` `0.41.0` (MIT), and `ureq` `3.3.0` (MIT OR Apache-2.0).

Implication:
- Current app runtime crates are permissive licenses.
- Preserve required copyright/license text in third-party notices.

#### Swift Package Dependency

- `Sparkle` `2.9.5` (`apps/macos-ui/Helm.xcodeproj/project.pbxproj`)
- Sparkle license is MIT-style and includes an `EXTERNAL LICENSES` section.

Implication:
- When distributing Developer ID builds that include Sparkle, preserve Sparkle license text and external attributions.

### 2) Build-Only Dependencies (Not App Runtime)

- `cbindgen` `0.29.4` (`core/rust/crates/helm-ffi/Cargo.toml` build-dependency)
- License: `MPL-2.0`

Implication:
- Treated as a build tool dependency.
- Reassess obligations only if build tooling artifacts/code are redistributed beyond standard build use.

### 3) Website Toolchain Dependencies (`web/`)

Direct dependencies:

- `astro` `^7.1.6` (MIT)
- `@astrojs/starlight` `^0.41.7` (MIT)
- `sharp` `^0.35.3` (Apache-2.0)

Additional lockfile notes:

- `nanoid` `3.3.18` is MIT-licensed and retains the security fix shipped in the RC2 lineage.
- Platform `@img/sharp-libvips-*` packages appear under `LGPL-3.0-or-later`.
- The lockfile scan found no `UNKNOWN` or `NOASSERTION` package licenses.

Implication:
- Current macOS app release artifacts do not ship `node_modules`.
- If Helm distributes runtime artifacts that include `sharp/libvips` binaries, LGPL obligations must be handled in that distribution path.

---

## Required Release Actions

### For macOS app releases (DMG)

- Keep this document current for dependency/version/license changes.
- Keep third-party attributions available from release materials (repository docs or bundled notices).
- Preserve Sparkle license + external notices for channels that include Sparkle.
- Flag any new strong-copyleft runtime dependencies for explicit owner review before release.

### For website/infrastructure distributions

- If distributing only generated static site output, keep source-level notices in repository docs.
- If distributing containers or binaries that include `sharp/libvips` payloads, include LGPL notices and required corresponding-source instructions for that artifact.

---

## Audit Commands

Use these commands to refresh the snapshot:

```bash
cargo metadata --manifest-path core/rust/Cargo.toml --format-version 1 --locked
cargo tree --manifest-path core/rust/Cargo.toml -e normal -p helm-ffi
cargo tree --manifest-path core/rust/Cargo.toml -e build -p helm-ffi
node -e 'const lock=require("./web/package-lock.json"); console.log(lock.packages["node_modules/astro"].license)'
node -e 'const lock=require("./web/package-lock.json"); for (const [k,v] of Object.entries(lock.packages)) if(k && /GPL|UNKNOWN|NOASSERTION/i.test(v.license||"UNKNOWN")) console.log(k, v.license)'
```

---

## Known Follow-Ups

- Automate generation of a release-ready third-party notices artifact.
- Add explicit packaging step for notices if/when DMG bundling policy requires in-bundle notice files.
