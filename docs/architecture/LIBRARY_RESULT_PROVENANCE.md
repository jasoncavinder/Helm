# Library Result Provenance Contract

This document defines the versioned provenance carried by production Library package results.

## Purpose

Library presentation must distinguish how Helm served a result from how Helm originally discovered it. A result found by a remote manager search and later read from SQLite is a local-cache read with remote-search discovery, not a live remote result.

The Rust core owns this classification. Service/XPC transports the JSON unchanged, and Swift validates the contract before presenting an origin label.

## Version 1 Wire Shape

The package-read FFI payloads retain their existing top-level fields and add an optional-to-consumers `provenance` object. This example is from `helm_search_local`:

```json
{
  "manager": "cargo",
  "name": "ripgrep",
  "version": "14.1.1",
  "summary": "A fast search tool",
  "source_manager": "cargo",
  "provenance": {
    "schema_version": 1,
    "origin": "local_cache",
    "discovery_source": "remote_search",
    "source_manager": "cargo",
    "originating_query": "ripgrep",
    "observed_at_unix": 1800000000
  }
}
```

Fields:

- `schema_version`: Contract version. Version 1 is the only currently supported value.
- `origin`: How this result was delivered to the current read.
- `discovery_source`: How the result entered Helm's result set.
- `source_manager`: Canonical manager ID responsible for the result. It must match the enclosing result's `source_manager`.
- `originating_query`: Trimmed query that caused remote discovery, or absent for snapshot/catalog data.
- `observed_at_unix`: Optional whole-second Unix timestamp for the persisted observation.

## Origin Values

- `local`: Current local manager snapshot, such as installed or outdated package state.
- `local_cache`: Result served from Helm's persisted search cache.
- `remote`: Result delivered directly by an active remote search. Reserved until a production direct-result transport uses it.
- `deferred`: Remote result intentionally pending because its network work cannot run. Reserved for a production deferred-result projection.

`helm_list_installed_packages` and `helm_list_outdated_packages` emit `local` plus `manager_snapshot`. `helm_search_local` always emits `local_cache`; it reads SQLite even when the cache row was populated by remote manager search.

## Discovery Values

- `manager_snapshot`: Local installed/outdated manager state.
- `catalog_sync`: Search-cache data populated without an interactive query.
- `remote_search`: Data discovered for a non-empty manager search query.

## Valid Combinations

Version 1 accepts only:

| Origin | Discovery source | Query |
|---|---|---|
| `local` | `manager_snapshot` | absent |
| `local_cache` | `catalog_sync` | absent |
| `local_cache` | `remote_search` | non-empty |
| `remote` | `remote_search` | non-empty |
| `deferred` | `remote_search` | non-empty |

All other combinations fail closed in the Swift presentation layer.

## Compatibility And Authority

- The nested object is additive. Swift keeps it optional so an older service response remains decodable during rolling development.
- Rust is authoritative for classifying persisted search results. Views do not infer provenance from timing, connectivity, or task state.
- Swift presents provenance only after validating schema version, enum values, field relationships, and source-manager identity.
- Unknown future schema or enum values remain decodable but produce no provenance label until the consumer supports them.
- Semantic changes require a new `schema_version`; existing version 1 meanings must not be repurposed.

## Persistence

No SQLite migration is required. Existing `search_cache.originating_query` and `cached_at_unix` columns contain the source facts needed for version 1. An empty query denotes catalog sync; a non-empty query denotes remote-search discovery.

This contract does not complete the Slice 20.3 native table, remote-search cancellation, or 20,000-row performance work.
