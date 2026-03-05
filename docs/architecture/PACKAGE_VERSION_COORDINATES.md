# Package Version Coordinates

This document defines Helm terminology and storage behavior for complex package version selectors.

## Terminology

- **Package Coordinate**: `<package-name>@<version-selector>`
  - Example: `python@mambaforge-24.11.0-1`
  - Example: `java@zulu-jre-javafx-8.92.0.21`
- **Version Selector**: The full manager-provided selector string after `@`.
- **Selector Atoms**: Hyphen-delimited parts of a selector.
  - Example: `zulu-jre-javafx-8.92.0.21` -> `["zulu", "jre", "javafx", "8.92.0.21"]`
- **Qualifier Atoms**: Selector atoms before the first atom containing a digit.
- **Release Token**: Selector atoms from the first digit-containing atom onward, joined by `-`.

## Data Rules

- Helm stores manager-reported installed versions/selectors as authoritative raw strings.
- Helm does not rewrite selector strings into semver.
- Helm treats selector decomposition as helper metadata for UX and matching, not as canonical storage.

## SQLite Storage

- Installed package snapshots now preserve multiple versions/selectors per `(manager_id, package_name)`.
- Table: `installed_package_versions`
  - primary key: `(manager_id, package_name, installed_version)`
  - `installed_version` persists as a normalized non-null token (`""` for unknown).
- Legacy `installed_packages` remains for compatibility/migration history; active reads/writes use `installed_package_versions`.

## Runtime Behavior

- `list_installed` returns all installed versions/selectors for a package-manager pair.
- Upgrade result persistence updates the matching installed version and preserves sibling installed versions.
- Search/package selector parsing now avoids misclassifying version-selector suffixes as manager suffixes in CLI package selectors.

## Adapter Guidance

- Adapters should pass through selector/version strings exactly as reported by managers.
- Adapters that support multi-version installs should return one `InstalledPackage` per installed selector/version.
- Adapters should avoid collapsing multiple installed versions into one row unless manager output itself only exposes one active version.
