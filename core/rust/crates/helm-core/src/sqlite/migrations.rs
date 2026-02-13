#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SqliteMigration {
    pub version: i64,
    pub name: &'static str,
    pub up_sql: &'static str,
    pub down_sql: &'static str,
}

const MIGRATION_0001: SqliteMigration = SqliteMigration {
    version: 1,
    name: "initial_core_schema",
    up_sql: r#"
CREATE TABLE IF NOT EXISTS installed_packages (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    installed_version TEXT,
    pinned INTEGER NOT NULL DEFAULT 0,
    updated_at_unix INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name)
);

CREATE TABLE IF NOT EXISTS outdated_packages (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    installed_version TEXT,
    candidate_version TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    updated_at_unix INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name)
);

CREATE TABLE IF NOT EXISTS pin_records (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    pin_kind TEXT NOT NULL,
    pinned_version TEXT,
    created_at_unix INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name)
);

CREATE TABLE IF NOT EXISTS search_cache (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    version TEXT,
    summary TEXT,
    originating_query TEXT NOT NULL,
    cached_at_unix INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_search_cache_query_time
    ON search_cache (originating_query, cached_at_unix DESC);

CREATE TABLE IF NOT EXISTS task_records (
    task_id INTEGER PRIMARY KEY,
    manager_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL
);
"#,
    down_sql: r#"
DROP TABLE IF EXISTS task_records;
DROP INDEX IF EXISTS idx_search_cache_query_time;
DROP TABLE IF EXISTS search_cache;
DROP TABLE IF EXISTS pin_records;
DROP TABLE IF EXISTS outdated_packages;
DROP TABLE IF EXISTS installed_packages;
"#,
};

const MIGRATION_0002: SqliteMigration = SqliteMigration {
    version: 2,
    name: "add_manager_detection_and_preferences",
    up_sql: r#"
CREATE TABLE IF NOT EXISTS manager_detection (
    manager_id TEXT PRIMARY KEY,
    detected INTEGER NOT NULL DEFAULT 0,
    executable_path TEXT,
    version TEXT,
    detected_at_unix INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS manager_preferences (
    manager_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1
);
"#,
    down_sql: r#"
DROP TABLE IF EXISTS manager_preferences;
DROP TABLE IF EXISTS manager_detection;
"#,
};

const MIGRATIONS: [SqliteMigration; 2] = [MIGRATION_0001, MIGRATION_0002];

pub fn migrations() -> &'static [SqliteMigration] {
    &MIGRATIONS
}

pub fn migration(version: i64) -> Option<&'static SqliteMigration> {
    MIGRATIONS.iter().find(|entry| entry.version == version)
}

pub fn current_schema_version() -> i64 {
    MIGRATIONS.last().map(|entry| entry.version).unwrap_or(0)
}
