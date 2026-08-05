# Doctor and Repair Knowledge Contract

This document defines Helm's local doctor/repair finding identity, knowledge, persistence, and execution boundaries.

The contract applies to health problems involving package managers, toolchain managers, and the local environment Helm uses to manage them. It is separate from the package-vulnerability advisory system described in `SECURITY_ADVISORY_SYSTEM.md`.

## Scope

Doctor may diagnose:

- package-manager and toolchain-manager installation health
- provenance, executable, metadata, and post-install setup inconsistencies
- locks, permissions, and local environment failures that prevent manager operations
- recoverable Helm configuration state when that state causes a manager operation to fail

Doctor is not a general Helm self-diagnostics or self-repair framework. Helm application defects, crashes, and release/update failures remain in their existing diagnostics and release-recovery systems unless a failure is specifically caused by recoverable manager-facing environment or configuration state.

## Finding Identity

Each finding has two distinct forms of identity:

- `fingerprint`: a deterministic, versioned identity for the normalized problem
- local occurrence identity: database identity and lifecycle metadata for observations on one installation

Equivalent normalized findings must produce the same fingerprint across users and installations. Repeated observations of the same problem must update one local finding lifecycle rather than create unrelated copies.

Fingerprint inputs may include only normalized fields that materially affect repair selection, such as:

- fingerprint algorithm version
- finding and issue code
- manager and source-manager identity
- normalized package or tool identity
- normalized requirement or failure class

Fingerprint inputs must not include incidental or sensitive evidence such as:

- absolute user paths
- usernames or home-directory names
- timestamps
- task IDs
- raw command output
- environment-specific ordering

Those values remain local evidence attached to the finding. Fingerprint serialization must use a canonical field order, normalization rules, and a versioned digest format. Changes to fingerprint inputs or normalization require a new fingerprint version and fixture-backed compatibility tests.

### Fingerprint v2 Encoding

The cross-installation contract begins at fingerprint version `v2`; existing `v1:` and `failure-v1-` values predate this contract and are legacy aliases, not valid `v2` knowledge keys.

The `v2` digest input is an ordered sequence with this fixed prefix and field order:

1. namespace: `helm.doctor.finding`
2. fingerprint version: `v2`
3. finding code
4. issue code
5. manager ID
6. source-manager ID or empty value
7. subject kind
8. canonical subject value

Each value is UTF-8 encoded as a netstring (`<decimal-byte-length>:<bytes>,`) and concatenated without separators beyond the netstring framing. The fingerprint is lowercase hexadecimal SHA-256 over those bytes and is rendered as:

```text
helm-doctor:v2:sha256:<64-lowercase-hex-characters>
```

Normalization rules:

- namespace, version, finding code, issue code, manager ID, source-manager ID, and subject kind are canonical lowercase ASCII identifiers
- package/tool subjects use the detector's manager-owned canonical external identifier normalized to Unicode NFC; the fingerprint layer trims ASCII edge whitespace but does not apply locale-dependent case conversion
- an unordered set of requirement or failure-class identifiers is normalized to lowercase ASCII, deduplicated, sorted by UTF-8 byte order, and encoded as nested netstrings in the subject value
- missing optional values encode as an empty netstring and are not omitted
- evidence order never affects the fingerprint

Initial finding schemas:

| Finding | Subject kind | Canonical subject value |
|---|---|---|
| Homebrew metadata-only manager install | `package` | expected Homebrew formula identity |
| post-install setup required | `requirement_set` | sorted unmet requirement IDs |
| selected executable path stale | `manager_configuration` | fixed identifier `selected_executable_path` (the path remains evidence) |
| Homebrew lock conflict | `package_operation` | nested normalized operation class and package identity; raw error text is excluded |

Homebrew subject normalization:

- formula identity is the lowercase ASCII Homebrew token for core formulae, or lowercase ASCII `owner/tap/token` for non-core taps
- cask identity is the lowercase ASCII cask token, qualified as `owner/tap/token` only for non-core taps
- aliases and paths are resolved to the Homebrew metadata identity before fingerprinting; filesystem paths are never identity inputs
- package operation is one of `install`, `reinstall`, `upgrade`, `uninstall`, `cleanup`, or `other`
- `package_operation` subject bytes are nested netstrings in fixed order: operation, package kind (`formula` or `cask`), canonical package identity
- unknown package identity is an empty nested netstring; it is not omitted or replaced with raw command/error text

Normative golden vectors:

| Case | Canonical subject | Fingerprint |
|---|---|---|
| rustup Homebrew metadata-only | `rustup` | `helm-doctor:v2:sha256:4043881e7469e0316b86c5f86c43f56c9b04995a95e393f806ddb76de61e9c14` |
| mise setup requirement | `13:mise_activate,` | `helm-doctor:v2:sha256:7cc56535a887459d3555d1fd7304a9736f3b08cb2311a74f1f9cbf2e33360c58` |
| rustup stale executable selection | `selected_executable_path` | `helm-doctor:v2:sha256:771a5b1c26fdab9e9221521f3227e1536d5f1a80c307ae525338ffd9534ac65e` |
| Homebrew formula `fd` upgrade lock | `7:upgrade,7:formula,2:fd,` | `helm-doctor:v2:sha256:c675dec7de83e1c75d7c496851f4f26f842369ebe679f31da91eda2415d0aa9f` |
| Homebrew cask lock with unknown target | `5:other,4:cask,0:,` | `helm-doctor:v2:sha256:f02fb46e796b94d64f1bbf1a214e8a8ffea54e04577202d41f4d38cec42b9291` |

Complete digest preimages for those vectors, in the same order:

```text
19:helm.doctor.finding,2:v2,30:homebrew_metadata_only_install,21:metadata_only_install,6:rustup,16:homebrew_formula,7:package,6:rustup,
19:helm.doctor.finding,2:v2,27:post_install_setup_required,27:post_install_setup_required,4:mise,4:mise,15:requirement_set,17:13:mise_activate,,
19:helm.doctor.finding,2:v2,30:selected_executable_path_stale,30:selected_executable_path_stale,6:rustup,6:rustup,21:manager_configuration,24:selected_executable_path,
19:helm.doctor.finding,2:v2,29:homebrew_cellar_lock_conflict,29:homebrew_cellar_lock_conflict,16:homebrew_formula,16:homebrew_formula,17:package_operation,25:7:upgrade,7:formula,2:fd,,
19:helm.doctor.finding,2:v2,29:homebrew_cellar_lock_conflict,29:homebrew_cellar_lock_conflict,13:homebrew_cask,13:homebrew_cask,17:package_operation,18:5:other,4:cask,0:,,
```

Every new finding code must define its subject schema and golden vectors before release. Tests must also cover field framing, empty values, set ordering, case rules, non-ASCII package identity preservation, tap-qualified Homebrew identities, and evidence/path exclusion.

During migration, a finding may retain prior `v1:` or `failure-v1-` values as local aliases linked to its `v2` fingerprint. Knowledge lookup uses `v2`; legacy lookup is allowed only through an explicit migrated alias. Legacy aliases must never be recomputed as though they followed `v2` semantics.

## Knowledge Is Data, Not Code

Repair knowledge tells Helm which existing repair capability may address a finding. It never contains executable behavior.

A repair knowledge entry may contain:

- stable `knowledge_entry_id`
- stable surface-facing `option_id`
- matching fingerprint or normalized finding selector
- typed registry `action_id`
- recommendation rank
- safety and confirmation policy
- parameter constraints or references to normalized finding fields
- localized content keys
- knowledge schema/version and source provenance

A repair knowledge entry must never contain:

- executable paths
- command names or argument arrays
- shell fragments or scripts
- dynamic libraries or plugins
- arbitrary code or expressions
- unvalidated environment substitutions

Unknown action IDs are non-executable. Imported knowledge cannot register a new action or weaken the safety policy of an existing action.

`knowledge_entry_id`, `option_id`, and `action_id` have different contracts:

- `knowledge_entry_id` identifies one source-owned knowledge record and revision lineage
- `option_id` is the stable identifier submitted through GUI/CLI/FFI/XPC repair interfaces
- `action_id` identifies the trusted core executor capability

Existing option IDs (`reinstall_manager_via_homebrew`, `remove_stale_package_entry`, `apply_post_install_setup_defaults`, and `clear_selected_executable_override`) remain stable during the database migration. A knowledge revision may change content or add restrictions without changing its public `option_id`; it cannot change the protected `action_id` binding. A protected option binding may change only through an explicit compiled, versioned registry migration shipped with Helm.

The effective repair plan permits only one action binding for each `(selector, option_id)`. Existing option IDs are protected by their compiled registry binding and cannot be rebound by imported knowledge. An import attempting to bind a protected option ID to another action is rejected. For non-protected IDs, higher locally assigned source authority may establish the binding; equal-authority conflicting bindings fail closed and neither option is executable.

## Trusted Action Registry

Repair execution is implemented by a finite, reviewed registry of typed Helm capabilities in core. For example, knowledge may select `homebrew.reinstall_formula`; trusted core code validates the current finding, derives the formula from normalized finding data, and lowers the action to structured process arguments.

The action registry owns:

- supported action identifiers
- accepted finding classes and typed parameters
- capability, eligibility, provenance, and managed-policy checks
- required confirmation and preview behavior
- structured process construction
- task orchestration, timeout, cancellation, and observability
- post-action verification

Knowledge can select and describe a capability; it cannot define how that capability executes.

Registry safety is an immutable minimum. Imported policy may require additional confirmation, reduce automation, or disable an option, but it cannot remove a registry-required preview/confirmation, increase automation, or bypass any policy gate. Effective policy is the most restrictive combination of registry, managed-environment, and knowledge policy.

Trust is assigned by the importer after local verification; payload claims cannot assign their own trust level. Checksum/signature fields in an envelope are evidence inputs until a configured verifier validates them. Unverified or forged trust metadata never receives verified-source authority.

## Local Persistence

SQLite is the canonical local store for doctor/repair state. The persistence model must cover these concepts:

- findings: normalized identity, fingerprint/version, severity, structured evidence, detector version, first/last seen, and resolution state
- repair knowledge: selectors, typed action IDs, policy metadata, content keys, source, version, and trust metadata
- imports: source, schema version, checksum/signature metadata, import time, and result
- repair history: finding, selected action, task, result, and verification outcome

Each scan records a scan ID, monotonic observation generation allocated at scan start, explicit scope, detector ID/version, and detector completion result. Scope is represented as the exact set of `(detector_id, manager_id)` pairs covered; a global scan contains all manager pairs it actually evaluates rather than using an implicit wildcard.

Doctor scans upsert findings from successful detectors. A previously active finding is marked resolved only when its owning detector completed successfully for the same covered pair, did not emit it, and the scan generation is not older than the finding's latest observation generation. Partial, cancelled, failed, or narrower scans leave findings outside their successfully completed detector scope unchanged. An older scan completing late cannot overwrite or resolve a finding observed by a newer generation.

Stored findings are diagnostic history, not permanent authorization: repair apply must revalidate the finding against current local state before mutation.

Schema changes use explicit tested migrations. Malformed or unsupported knowledge records fail closed and do not prevent doctor scans from reporting findings.

## Import and Export

Knowledge uses a versioned JSON envelope so entries can later be bundled, imported, exported, shared, or synchronized without changing execution semantics. Canonical bytes for checksums/signatures and deterministic comparison use RFC 8785 JSON Canonicalization Scheme (JCS).

### Knowledge Envelope v1

The top-level envelope contains exactly these required fields unless a later schema version adds optional fields:

| Field | Contract |
|---|---|
| `schema_version` | integer `1` |
| `declared_source_id` | artifact-declared source name; informational until bound by the importer |
| `source_revision` | non-negative monotonically increasing integer for this source |
| `generated_at_unix` | informational Unix-seconds timestamp; excluded from entry identity |
| `entries` | array sorted by `knowledge_entry_id`, then entry `revision` |
| `integrity` | object containing checksum algorithm/value and optional signature metadata |

Each entry contains:

| Field | Contract |
|---|---|
| `knowledge_entry_id` | stable source-owned lowercase ASCII identifier |
| `revision` | non-negative monotonically increasing integer within the entry lineage |
| `state` | `active` or `tombstone` |
| `selector` | exact `v2` fingerprint or normalized finding selector fields |
| `option_id` | stable surface-facing repair option identifier |
| `action_id` | typed action registered by the installed Helm version |
| `recommendation_rank` | optional non-negative rank; the lowest available rank is recommended |
| `policy` | restrictions only: confirmation, automation ceiling, and enabled state |
| `parameter_bindings` | allowlisted references to normalized finding fields; no literal executable data |
| `content_keys` | localization keys for title, description, impact, and guidance |

For checksum calculation, `integrity` is omitted and the remaining envelope is serialized with JCS. `integrity.algorithm` is `sha256` for schema v1 and `integrity.value` is lowercase hexadecimal SHA-256. When present, signature metadata identifies a verifier/key and signs the checksum bytes; trust is assigned only after that verifier succeeds locally.

Unknown top-level or entry fields are rejected in schema v1. A tombstone contains identity, revision, state, and selector fields but no executable option content. Selectors and parameter bindings use a closed schema published with the installed Helm version; unknown selector fields or binding names reject the whole envelope.

Duplicate `(knowledge_entry_id, revision)` tuples within one envelope are rejected. Once imported, an entry revision is immutable: replaying the same `(source_key, knowledge_entry_id, revision)` with identical canonical entry bytes is idempotent, while different canonical bytes are equivocation and reject the envelope even when `source_revision` is higher.

Import requirements:

- transactional application
- deterministic conflict resolution
- source and trust provenance retention
- schema and action-ID validation
- checksum/signature metadata support
- no automatic execution after import
- whole-envelope rollback when any entry is malformed, unless a future schema explicitly defines a separate quarantine mode

Export requirements:

- deterministic ordering
- schema and source metadata
- repair knowledge only; local finding/observation history is not part of the knowledge export format
- no implicit upload or network transfer

Entry/source rules:

- the importer assigns an internal `source_key`; entries and revision/tombstone authority are keyed by `(source_key, knowledge_entry_id, revision)`, never by the payload's declared name alone
- bundled source keys bind to the shipped artifact identity; verified source keys bind to the verified signing-key identity plus `declared_source_id`; unverified imports receive a new user-import source key
- protected Helm source namespaces can be bound only by the bundled artifact or an explicitly configured verified Helm signing key; unauthorized namespace claims reject the entire import
- a higher revision supersedes a lower revision only within the same source; downgrades are rejected unless the user explicitly requests a local rollback
- tombstones can disable only entries owned by the same source
- entries from different sources coexist; equivalent selector/option/action tuples merge to the most restrictive effective policy
- source authority (`bundled`, `verified_signed`, `user_imported`) is assigned by the importer, not read from an untrusted payload
- after policy merging, conflicting content/recommendation metadata is ordered by locally assigned source authority (`verified_signed` before `bundled` before `user_imported`), revision descending, then source/entry ID UTF-8 byte order
- local disable/override state is stored separately and is never overwritten by a source update
- replaying the same `(source_key, source_revision)` with the same checksum is idempotent; the same identity/revision with a different checksum is equivocation and is rejected with a diagnostic record

Finding observation/history export, if offered, is a separate explicitly requested diagnostics format with its own redaction policy. It is not a repair knowledge export.

The `0.18.x` implementation remains local-only. A versioned bundled knowledge data file may seed SQLite, but the data must remain inspectable, replaceable, and processed through the same validation/transaction contract as later sources. This trusted bootstrap import is the only non-user-initiated import in the local-only phase. Automatic online lookup is deferred.

## Planning and Apply Flow

1. Doctor scans current manager and environment state.
2. Core normalizes findings and computes deterministic fingerprints.
3. Findings are upserted into SQLite.
4. Repair planning queries local knowledge and resolves known typed action IDs.
5. The surface presents plan, evidence, impact, and required confirmation.
6. Apply re-scans or otherwise revalidates the exact active finding.
7. Core validates the typed action through normal safety and policy gates.
8. Execution runs through the existing task orchestration boundary.
9. Doctor verifies the environment after completion.
10. Repair history records the task and verified outcome.

## Security and Privacy Invariants

- Repair knowledge is never an executable payload.
- No imported record can bypass confirmation, safe mode, eligibility, provenance, or managed-policy checks.
- All process execution uses structured program/argument construction owned by trusted code.
- Fingerprints exclude sensitive and incidental local evidence.
- Import/export is user-initiated during the local-only phase except for the explicit validated bundled-bootstrap import.
- No finding, fingerprint, evidence, or inventory data is uploaded automatically.
- Repair failure is isolated to the affected action and remains visible through normal task diagnostics.

## Relationship to Security Advisories and Shared Brain

Doctor findings describe operational health; security advisories describe package vulnerability data. They may eventually share serialization, provenance, trust, and import infrastructure, but they retain separate domain models, stores, and evaluation semantics.

Future Shared Brain services may distribute or enrich repair knowledge. They do not change the local execution boundary: remote data can select only action IDs already implemented and allowed by the installed Helm version.
