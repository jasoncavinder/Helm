# SQLite Migration Safety

This document defines Helm's durable SQLite migration invariants and required
verification. These rules apply to app, service, CLI, FFI, and development
builds because all of those surfaces can initialize persistent state.

## Immutable Identity

Every migration is identified by all of the following:

- contiguous integer version
- stable name
- SHA-256 digest of its version, name, up SQL, and down SQL

The committed manifest is
`core/rust/crates/helm-core/resources/sqlite_migration_manifest.json`.
Once a manifest entry reaches a shared branch, it is append-only. Existing
versions, names, SQL, ordering, and checksums must never be changed. Corrections
use a new migration version or a narrowly targeted reconciliation for an exact
known historical identity.

The required Policy Gate compares the PR manifest with its base revision. The
base entries must remain an exact prefix of the proposed manifest. Runtime also
validates compiled definitions against the manifest before opening a migration
transaction.

## Runtime Ledger

Migration 20 adds `definition_checksum` to `helm_schema_migrations`. Existing
ledgers are backfilled transactionally. New records persist the immutable
definition checksum when they are inserted.

Before schema mutation, startup verifies:

- the ledger is contiguous from version 1 through its maximum version
- every recorded version exists in the compiled migration registry
- every recorded name matches the immutable manifest
- every populated checksum matches the immutable manifest

Missing, unknown, reordered, renamed, or checksum-mismatched entries fail
closed. Helm does not replay historical DDL on an already-current database.

Known historical discrepancies require exact identity matching and one atomic
transaction. Unknown discrepancies must not be guessed, relabeled, or repaired
generically.

## Pre-Migration Backup

Before any forward migration or known reconciliation, Helm creates an online
SQLite backup next to the database and verifies it with `PRAGMA integrity_check`.
On Unix, backup permissions are restricted to `0600`.

Backup names use this form:

```text
helm.db.pre-migration-v<source>-<timestamp>.backup
```

At most three completed backups are retained. Failed partial backups are
removed. Intentional downgrade/reset does not create a backup, and successful
Reset Local Data removes retained migration backups so reset does not leave an
undisclosed copy of user data.

## Environment Isolation

Release builds use:

```text
~/Library/Application Support/Helm/helm.db
```

Debug app/service and CLI builds use:

```text
~/Library/Application Support/Helm-Development/helm.db
```

Tests and explicit recovery work may override the path with `HELM_DB_PATH`.
Development tooling must not point at the stable database unless the operator
has intentionally created a backup and explicitly supplied that path.

## Required Compatibility Gate

Run:

```bash
scripts/ci/check_sqlite_migration_compatibility.sh
```

The gate validates the immutable manifest and exercises:

- fresh database initialization and repeated startup
- clean v0.17.12 upgrade with representative data preservation
- the frozen affected-v0.18.0 schema
- exact legacy reconciliation and unknown-identity rejection
- ledger name, checksum, and gap tampering
- migration and reconciliation rollback
- reset and reinitialization
- real CLI initialization
- isolated-process FFI initialization and Refresh acceptance

The gate runs in normal CI, the macOS release canary, and direct GUI and CLI
release workflows before artifacts are built.

## Adding A Migration

1. Append one new `SqliteMigration`; never edit an existing definition.
2. Generate the candidate manifest with
   `python3 scripts/ci/check_sqlite_migration_manifest.py --emit`.
3. Append only the new generated identity to the committed manifest.
4. Add a frozen origin fixture when the migration changes persisted data.
5. Add preservation, idempotency, rollback, reset, and boundary tests as applicable.
6. Run the compatibility gate and full repository quality gate.
7. Obtain independent review for migration-engine, manifest, and recovery changes.

Release canaries must exercise a stateful upgrade from the prior public stable
release. Signing/notarization success alone is not evidence of database upgrade
compatibility.
