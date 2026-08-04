CREATE TABLE advisory_cache (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    affected_versions TEXT NOT NULL,
    severity TEXT NOT NULL,
    summary TEXT NOT NULL,
    fixed_version TEXT,
    source TEXT NOT NULL,
    fetched_at_epoch_ms INTEGER NOT NULL,
    expires_at_epoch_ms INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name, source)
);

INSERT INTO helm_schema_migrations (version, name, applied_at_unix)
VALUES (17, 'add_advisory_cache', 1785514652);

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

CREATE INDEX idx_security_advisories_package
    ON security_advisories (ecosystem, package_name, cache_key);
CREATE INDEX idx_security_advisories_source
    ON security_advisories (source_provider, cache_key);
CREATE INDEX idx_security_advisories_expiry
    ON security_advisories (expires_at_epoch_ms);

INSERT INTO helm_schema_migrations (version, name, applied_at_unix)
VALUES (18, 'add_security_advisory_cache', 1785823873);
