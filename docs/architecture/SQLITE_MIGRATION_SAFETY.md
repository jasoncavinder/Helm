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

Migration 24 adds per-app external Sparkle adoption/revocation history plus a
random database epoch. Confirmation compares the prior epoch/path revision in
one immediate transaction, consumes its consent ID and commits with FULL/fullfsync
durability. Revocation appends a tombstone even before any grant and remains
available in safe mode. Claim/handoff/verification transactions check the latest
grant, not cached receipts. Reset/downgrade cannot drop either this ledger or
migration 23 while an external-update reservation remains; a permitted reset
forgets grants and creates a new epoch on reinitialization. Existing migration
identities remain unchanged. This is user consent, not proven installation
history or a general-purpose authorization service. See
[the adoption contract](../validation/v0.20-sparkle-adoption.md).

Migration 23 adds external Sparkle update sessions. Operation IDs remain consumed
after terminal results, independently of ordinary task-history deletion. Active
canonical paths and original filesystem identities have unique reservations;
unverified outcomes retain their reservations. Session writes use per-connection
`synchronous=FULL` plus `fullfsync=ON` before committing authorization or handoff;
ordinary cache connections retain the existing NORMAL policy. This requests
stronger durability for external effects, not an absolute hardware/power-loss
guarantee. Reset/downgrade cannot drop this ledger while a reservation remains;
the check and table removal are serialized with new claims in one immediate
transaction. No installer or automatic restart recovery is activated by this
migration. See [the session contract](../validation/v0.20-sparkle-durable-session.md).

Migration 22 adds local first-run preference-repair receipts. The reviewed
preference change and applied/unverified receipt commit atomically; a separate
post-observation transaction finalizes the verification result. Interruption
does not replay the mutation or imply success. This is a narrow action ledger,
not a generalized persisted setup scheduler.

Migration 21 adds a durable task-ID high-water mark, seeded from existing task
records. Queued tasks are reserved in one immediate transaction before adapter
execution. Record insertion advances the mark, and history deletion/pruning does
not reset it. This prevents independent CLI/service runtimes from colliding or
reusing IDs still referenced by another runtime. It does not itself establish
cross-process manager execution leases.

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

The unembedded external Sparkle helper's explicit `--prepare-ledger` command
uses a separate, fixed OS-account-derived private namespace. It does not use
`HELM_DB_PATH` or import the application database. A native filesystem lease is
required around the private in-process initializer; SQLite opens existing files
with no-follow behavior and never recreates missing storage. Existing stores
must already contain migration 24 or later, valid immutable migration identities,
the adoption epoch and session tables. An empty/partial existing store is not
fresh initialization. Ordinary app connection behavior and all historical
migrations are unchanged. See the [private helper ledger contract](../validation/v0.20-sparkle-private-ledger.md).

Authenticated helper consent inspection is a separate existing-only read path,
not the normal migrating store constructor. It requires the exact current schema,
valid migration identities/checksums, integrity, adoption epoch and session tables,
then reads history in one read-only snapshot. No absent file/schema/epoch is
created or repaired. Do not use SQLite `immutable` here: a committed revocation
in WAL must be visible. Normal WAL/SHM bookkeeping is possible; no consent or
schema writes are performed. This diagnostic returns no authority token. See
the [consent-status contract](../validation/v0.20-sparkle-consent-status.md).

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

Store connections use WAL with `synchronous=NORMAL` and `fullfsync=ON`. The latter
requests macOS's stronger flush at existing synchronization boundaries without
adding a sync to every commit. Recent transactions can still roll back after
system/power loss; this policy is not a guarantee that every acknowledged write
survives. The [VM restart investigation](../validation/v0.20-qa-database-restart-triage.md)
records the bounded evidence and remaining platform limits. Corruption must fail
closed, not trigger silent reset or deletion of recovery sidecars.

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
- per-connection WAL/NORMAL/fullfsync policy on initial and subsequent opens
- committed and spilled-uncommitted WAL recovery after child-process termination
- real CLI rejection of a truncated isolated database without replacement/fallback
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
