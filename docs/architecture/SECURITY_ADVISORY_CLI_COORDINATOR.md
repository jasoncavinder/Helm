# Security Advisory CLI/Coordinator Contract

## Purpose

This document specifies the shared GUI/CLI coordinator contract for security advisory operations. It defines how advisory data flows through the system, how deterministic execution is enforced, how cancellation is handled, and what cache-first behavior means in practice.

This module is the v0.18 groundwork. It defines types, contracts, and tests. It does not include network access, provider implementations, or user-facing features.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         Coordinator                             │
│  (GUI and CLI share the same advisory coordinator instance)     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────────┐  │
│  │ Refresh      │    │ Evaluate     │    │ Cache Store      │  │
│  │ Task Hook    │    │ Task Hook    │    │ (SQLite, m18)    │  │
│  └──────┬───────┘    └──────┬───────┘    └──────────────────┘  │
│         │                   │                                   │
│         ▼                   ▼                                   │
│  ┌──────────────┐    ┌──────────────┐                          │
│  │ Advisory     │    │ Advisory     │                          │
│  │ Refresh      │    │ Evaluation   │                          │
│  │ Request/     │    │ Request/     │                          │
│  │ Result       │    │ Result       │                          │
│  └──────────────┘    └──────────────┘                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
         ▲                           ▲
         │                           │
  ┌──────┴──────┐           ┌───────┴────────┐
  │  CLI        │           │  GUI (SwiftUI) │
  └─────────────┘           └────────────────┘
```

Both GUI and CLI entry points route through the same coordinator. This ensures:
- A single source of truth for advisory data
- Deterministic execution regardless of entry point
- Shared cache and deduplication logic

## Deterministic Execution

### Ordering

Advisory records are sorted by:
1. Severity (descending: critical > high > medium > low)
2. Advisory ID (ascending, byte-wise)

The `advisory_sort_key()` function produces a `(u8, String)` tuple suitable for Rust's standard sort. This ordering is locale-independent and reproducible across runs.

### Deduplication

Records are deduplicated by cache key (`advisory:{source}:{advisory_id}`). When duplicates exist, the first occurrence is retained. The deduplication step runs after sorting, so the highest-severity variant wins if the same advisory appears with different severity from different sources.

### Reproducibility

Given the same input set and the same clock time, the coordinator produces identical output:
- Normalization is deterministic (lowercase ecosystem and source, trim package names)
- Sorting uses a total ordering
- Deduplication preserves first-occurrence order
- Cache keys are constructed from normalized components

## Cancellation

Advisory operations may be cancelled during execution. The cancellation model follows the existing task cancellation contract:

1. **Refresh tasks**: A cancellation signal stops network fetches. Partial results are discarded. The coordinator marks the task as `cancelled` and does not update the cache.

2. **Evaluation tasks**: Evaluation reads from cache only and is fast. Cancellation is not expected but supported. A cancelled evaluation returns an empty result set.

3. **Grace period**: The coordinator respects the existing grace period model. After a cancellation signal, a grace period allows in-progress operations to complete cleanly.

The `AdvisoryTaskHook` enum encodes the operation type for the task scheduler. Task labels are deterministic strings like `advisory_refresh:osv` or `advisory_evaluate:42_packages`.

## Cache-First Behavior

### Freshness Model

The advisory cache uses a TTL-based freshness model with three states:

| State    | Meaning                                              | Offline-safe |
|----------|------------------------------------------------------|--------------|
| Fresh    | Cache data is within TTL (default: 24 hours)         | Yes          |
| Stale    | Cache data has exceeded TTL but is still available   | Yes          |
| Missing  | No cache data available                              | No           |

### Cache Flow

1. **Evaluate**: Always reads from cache first. If cache is `fresh` or `stale`, returns cached results. If `missing` and `allow_stale` is false, returns empty results.

2. **Refresh**: A future provider coordinator fetches new data and transactionally upserts it. The v0.18 store does not invalidate existing cache during a failed batch and rejects stale writes over newer records.

3. **Prune**: Expired records (`expires_at_epoch_ms <= now`) are candidates for removal. The v0.18 store exposes deterministic pruning; periodic scheduling remains deferred.

### Offline Safety

The system is designed to work without network access:
- Stale cache data is used for evaluation when `allow_stale` is true
- Missing cache data results in empty matches, not errors
- Refresh failures do not degrade existing cache

## Domain Model Separation

### Advisory Records vs Doctor Findings

Advisory records and doctor findings are separate domain models with separate stores:

| Aspect              | Advisory Records           | Doctor Findings         |
|---------------------|----------------------------|-------------------------|
| Purpose             | Package vulnerability data | System health issues    |
| Source              | External feeds (OSV, NVD) | Local system inspection |
| Store               | `security_advisories` (m18) | doctor persistence tables (m17) |
| TTL                 | 24-hour default            | No TTL                  |
| Schema Version      | Yes (`ADVISORY_SCHEMA_V`)  | No                      |
| Cache Key           | `advisory:{src}:{id}`      | N/A                     |
| Severity            | AdvisorySeverity enum      | FindingSeverity enum    |

The coordinator may reference advisory data when generating doctor findings, but the data models remain distinct. An advisory finding in the doctor report contains a reference to the advisory ID, not the advisory record itself.

## Integration Points

### SQLite Migration 18

The `AdvisoryCacheStore` trait defines the storage interface. Migration 18 and `SqliteStore` implement the transactional local backend:

```sql
CREATE TABLE security_advisories (
    cache_key TEXT PRIMARY KEY,
    advisory_id TEXT NOT NULL,
    ecosystem TEXT NOT NULL,
    scope TEXT,
    package_name TEXT NOT NULL,
    affected_range_json TEXT NOT NULL,
    severity TEXT NOT NULL,
    cvss_score REAL,
    summary TEXT NOT NULL,
    description TEXT,
    fixed_version TEXT,
    source_provider TEXT NOT NULL,
    source_feed TEXT,
    fetched_at_epoch_ms INTEGER NOT NULL,
    expires_at_epoch_ms INTEGER NOT NULL
);

CREATE INDEX idx_security_advisories_package ON security_advisories(ecosystem, package_name, cache_key);
CREATE INDEX idx_security_advisories_source ON security_advisories(source_provider, cache_key);
CREATE INDEX idx_security_advisories_expiry ON security_advisories(expires_at_epoch_ms);
```

### Orchestration Hooks

The `AdvisoryTaskHook` enum integrates with the task scheduler. The coordinator creates task hooks, the scheduler executes them, and results are persisted through the cache store.

### Provider Implementations

Provider-specific fetch logic (OSV, GitHub Advisory Database, RustSec) lives outside this module. The coordinator accepts `AdvisoryRecord` instances from any provider and normalizes them through the same pipeline.

## v0.18 Non-Goals

The following are explicitly out of scope for v0.18:

- **Network access**: No HTTP clients, no provider implementations
- **User-facing features**: No CLI commands, no SwiftUI views
- **Automatic actions**: No auto-upgrade, no patching
- **Telemetry**: No analytics, no upload
- **Pro entitlements**: No gating, no licensing
- **Doctor integration**: Advisory data is not yet connected to doctor findings
- **Orchestration wiring**: Task hooks are defined but not connected to the scheduler
- **Central backend**: No cloud service, no aggregation server

## Current Deliverables (v0.18)

- `security_advisory` module in `helm-core` with domain models
- Normalization, validation, deduplication, and ordering functions
- Deterministic cache key generation
- TTL-based freshness evaluation with offline-safe semantics
- Storage-facing `AdvisoryCacheStore` trait
- Migration 18 and transactional `SqliteStore` cache implementation with deterministic query order, stale-write protection, pruning, and clear/count operations
- Future refresh/evaluation request/result/task-hook contracts
- 50+ focused unit tests covering normalization, serialization, cache identity, ordering, dedup, TTL boundaries, invalid data, and deterministic output
- Architecture documentation (this file)

## Next Steps (post-v0.18)

1. **Provider lane**: OSV and GitHub Advisory Database fetchers
2. **Orchestration lane**: Task hook integration with scheduler
3. **CLI lane**: `helm advisory check` command
4. **GUI lane**: Security panel in SwiftUI
5. **Doctor lane**: Advisory findings in doctor reports
