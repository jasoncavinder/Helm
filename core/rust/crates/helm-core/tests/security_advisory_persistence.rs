use helm_core::persistence::MigrationStore;
use helm_core::security_advisory::{
    AdvisoryCacheRecord, AdvisoryCacheStore, AdvisoryRecord, AdvisorySeverity, AdvisorySource,
    AffectedRange, PackageCoordinates,
};
use helm_core::sqlite::{SqliteStore, current_schema_version};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn store(name: &str) -> SqliteStore {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path: PathBuf = std::env::temp_dir().join(format!("helm-advisory-{name}-{nanos}.sqlite3"));
    let store = SqliteStore::new(path);
    store.apply_migration(current_schema_version()).unwrap();
    store
}

fn cache_record(id: &str, fetched_at: i64, expires_at: i64) -> AdvisoryCacheRecord {
    AdvisoryCacheRecord::from_advisory(&AdvisoryRecord::new(
        id,
        PackageCoordinates::new("cargo", "serde"),
        AffectedRange::All,
        AdvisorySeverity::High,
        format!("summary-{id}"),
        AdvisorySource::new("osv"),
        fetched_at,
        expires_at,
    ))
}

#[test]
fn advisory_cache_roundtrips_in_deterministic_order_and_rejects_stale_writes() {
    let store = store("roundtrip");
    let second = cache_record("OSV-2", 2_000, 4_000);
    let first = cache_record("OSV-1", 2_000, 4_000);
    assert_eq!(
        store
            .upsert_advisories(&[second.clone(), first.clone()])
            .unwrap(),
        2
    );

    let records = store.get_advisories_for_package("cargo", "serde").unwrap();
    assert_eq!(records.len(), 2);
    assert!(records[0].cache_key < records[1].cache_key);
    assert_eq!(store.get_advisories_by_source("osv").unwrap(), records);

    let mut stale = first.clone();
    stale.fetched_at_epoch_ms = 1_000;
    stale.expires_at_epoch_ms = 3_000;
    stale.summary = "stale overwrite".to_string();
    assert_eq!(store.upsert_advisories(&[stale]).unwrap(), 0);
    assert_eq!(
        store
            .get_advisories_for_package("cargo", "serde")
            .unwrap()
            .into_iter()
            .find(|record| record.cache_key == first.cache_key)
            .unwrap()
            .summary,
        first.summary
    );
}

#[test]
fn advisory_cache_batch_validation_is_transactional_and_pruning_is_inclusive() {
    let store = store("validation-prune");
    let valid = cache_record("OSV-1", 1_000, 2_000);
    let mut invalid = cache_record("OSV-2", 1_000, 2_000);
    invalid.affected_range_json = "not-json".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), invalid]).is_err());
    assert_eq!(store.count().unwrap(), 0);

    store.upsert_advisories(&[valid]).unwrap();
    assert_eq!(store.prune_expired(1_999).unwrap(), 0);
    assert_eq!(store.prune_expired(2_000).unwrap(), 1);
    assert_eq!(store.count().unwrap(), 0);
}

#[test]
fn advisory_cache_clear_all_is_idempotent() {
    let store = store("clear");
    store
        .upsert_advisories(&[cache_record("OSV-1", 1_000, 2_000)])
        .unwrap();
    store.clear_all().unwrap();
    store.clear_all().unwrap();
    assert_eq!(store.count().unwrap(), 0);
}
