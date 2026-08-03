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

#[test]
fn advisory_cache_rejects_adversarial_records_transactionally() {
    let store = store("adversarial-validation");
    let valid = cache_record("OSV-1", 1_000, 2_000);

    let mut bad_key = cache_record("OSV-2", 1_000, 2_000);
    bad_key.cache_key = "advisory:osv:cve-other".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), bad_key]).is_err());
    assert_eq!(store.count().unwrap(), 0);

    let mut bad_eco = cache_record("OSV-3", 1_000, 2_000);
    bad_eco.ecosystem = "  cargo  ".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), bad_eco]).is_err());
    assert_eq!(store.count().unwrap(), 0);

    let mut bad_prov = cache_record("OSV-4", 1_000, 2_000);
    bad_prov.source_provider = "OSV".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), bad_prov]).is_err());
    assert_eq!(store.count().unwrap(), 0);

    let mut bad_ctrl = cache_record("OSV-5", 1_000, 2_000);
    bad_ctrl.summary = "bad\u{001b}summary".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), bad_ctrl]).is_err());
    assert_eq!(store.count().unwrap(), 0);

    let mut bad_id = cache_record("OSV-6", 1_000, 2_000);
    bad_id.advisory_id = "OSV-6\nforged".to_string();
    assert!(store.upsert_advisories(&[valid.clone(), bad_id]).is_err());

    let mut bad_scope = cache_record("OSV-7", 1_000, 2_000);
    bad_scope.scope = Some("scope\tname".to_string());
    assert!(
        store
            .upsert_advisories(&[valid.clone(), bad_scope])
            .is_err()
    );

    let mut bad_feed = cache_record("OSV-8", 1_000, 2_000);
    bad_feed.source_feed = Some("feed\nname".to_string());
    assert!(store.upsert_advisories(&[valid.clone(), bad_feed]).is_err());

    let mut bad_range = cache_record("OSV-9", 1_000, 2_000);
    bad_range.affected_range_json =
        serde_json::to_string(&AffectedRange::exact("1.0\nforged")).unwrap();
    assert!(
        store
            .upsert_advisories(&[valid.clone(), bad_range])
            .is_err()
    );

    let mut bad_fixed = cache_record("OSV-10", 1_000, 2_000);
    bad_fixed.fixed_version = Some("2.0\tforged".to_string());
    assert!(
        store
            .upsert_advisories(&[valid.clone(), bad_fixed])
            .is_err()
    );
    assert_eq!(store.count().unwrap(), 0);

    let mut formatted_prose = valid;
    formatted_prose.summary = "First line\nSecond line".to_string();
    formatted_prose.description = Some("Details:\n\titem".to_string());
    assert_eq!(store.upsert_advisories(&[formatted_prose]).unwrap(), 1);
}

#[test]
fn advisory_cache_normalized_queries() {
    let store = store("normalized-queries");

    let mut record = cache_record("OSV-1", 1_000, 2_000);
    record.ecosystem = "cargo".to_string();
    record.package_name = "café".to_string();
    store.upsert_advisories(&[record]).unwrap();

    let records = store
        .get_advisories_for_package("  Cargo  ", "  café  ")
        .unwrap();
    assert_eq!(records.len(), 1);

    let nfd_name = "cafe\u{0301}";
    let records_nfd = store.get_advisories_for_package("cargo", nfd_name).unwrap();
    assert_eq!(records_nfd.len(), 1);

    let src_records = store.get_advisories_by_source("  osV ").unwrap();
    assert_eq!(src_records.len(), 1);
}
