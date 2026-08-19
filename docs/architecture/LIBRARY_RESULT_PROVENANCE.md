# Library Result Provenance Contract

This document defines the versioned provenance carried by production Library package results.

## Purpose

Library presentation must distinguish the logical source class backing a result from how Helm originally discovered it. A search result found by a manager search and later read from SQLite is a local-cache result with manager-search discovery, not a live remote result. Installed/outdated records remain local manager snapshots when served from their persisted service representation; `local_cache` is reserved for the search cache. A manager search may use network, local, or generated/static manager data; discovery source does not independently claim network activity.

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
    "discovery_source": "manager_search",
    "source_manager": "cargo",
    "originating_query": "ripgrep"
  }
}
```

Fields:

- `schema_version`: Contract version. Version 1 is the only currently supported value.
- `origin`: Logical source class backing the result.
- `discovery_source`: How the result entered Helm's result set.
- `source_manager`: Canonical manager ID responsible for the result. It must match the enclosing search result's `source_manager`, or `package.manager` for installed/outdated snapshots.
- `originating_query`: Trimmed query that caused manager-search discovery, or absent for snapshot/catalog data.

## Origin Values

- `local`: Installed/outdated manager-state snapshot derived from the local system, including the service's persisted representation of that snapshot.
- `local_cache`: Result served from Helm's persisted search cache.
- `remote`: Result delivered directly by an active remote search. Reserved until a production direct-result transport uses it.
- `deferred`: Remote result intentionally pending because its network work cannot run. Reserved for a production deferred-result projection.

`helm_list_installed_packages` and `helm_list_outdated_packages` emit `local` plus `manager_snapshot`. `helm_search_local` always emits `local_cache`; it reads SQLite even when the cache row was populated by a manager search.

## Discovery Values

- `manager_snapshot`: Local installed/outdated manager state.
- `catalog_sync`: Search-cache data populated without an interactive query.
- `manager_search`: Data discovered for a non-empty manager search query, regardless of whether that manager used network, local, or generated/static data.

## Valid Combinations

Version 1 accepts only:

| Origin | Discovery source | Query |
|---|---|---|
| `local` | `manager_snapshot` | absent |
| `local_cache` | `catalog_sync` | absent |
| `local_cache` | `manager_search` | non-empty |
| `remote` | `manager_search` | non-empty |
| `deferred` | `manager_search` | non-empty |

All other combinations fail closed in the Swift presentation layer.

## Compatibility And Authority

- The nested object is additive. Swift keeps it optional so an older service response remains decodable during rolling development.
- Rust is authoritative for classifying persisted search results. Views do not infer provenance from timing, connectivity, or task state.
- Swift decodes the nested object lossily so malformed or future provenance cannot invalidate its enclosing package result.
- Swift presents provenance only after validating schema version, enum values, exact canonical manager/query values, endpoint-specific origin/discovery rules, and source-manager identity.
- Unknown future schema, enum, or structurally incompatible values remain decodable but produce no provenance label until the consumer supports them.
- Semantic changes require a new `schema_version`; existing version 1 meanings must not be repurposed.

## Persistence

No SQLite migration is required. Existing `search_cache.originating_query` contains the source fact needed for version 1. An empty query denotes catalog sync; a non-empty query denotes manager-search discovery. The existing cache timestamp is intentionally not exposed because adapters do not yet produce one consistent observation or persistence-time semantic.

This provenance contract is consumed by the Slice 20.3 native table, but it does not complete remote-search cancellation or the 20,000-row performance work.
