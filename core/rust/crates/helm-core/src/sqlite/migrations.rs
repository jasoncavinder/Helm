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

const MIGRATION_0003: SqliteMigration = SqliteMigration {
    version: 3,
    name: "add_restart_required_to_outdated",
    up_sql: r#"
ALTER TABLE outdated_packages ADD COLUMN restart_required INTEGER NOT NULL DEFAULT 0;
"#,
    down_sql: r#"
CREATE TABLE outdated_packages_backup (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    installed_version TEXT,
    candidate_version TEXT NOT NULL,
    pinned INTEGER NOT NULL DEFAULT 0,
    updated_at_unix INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name)
);
INSERT INTO outdated_packages_backup
    SELECT manager_id, package_name, installed_version, candidate_version, pinned, updated_at_unix
    FROM outdated_packages;
DROP TABLE outdated_packages;
ALTER TABLE outdated_packages_backup RENAME TO outdated_packages;
"#,
};

const MIGRATION_0004: SqliteMigration = SqliteMigration {
    version: 4,
    name: "add_app_settings",
    up_sql: r#"
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#,
    down_sql: r#"
DROP TABLE IF EXISTS app_settings;
"#,
};

const MIGRATION_0005: SqliteMigration = SqliteMigration {
    version: 5,
    name: "add_homebrew_keg_policy_overrides",
    up_sql: r#"
CREATE TABLE IF NOT EXISTS package_keg_policies (
    manager_id TEXT NOT NULL,
    package_name TEXT NOT NULL,
    policy TEXT NOT NULL,
    updated_at_unix INTEGER NOT NULL,
    PRIMARY KEY (manager_id, package_name)
);
"#,
    down_sql: r#"
DROP TABLE IF EXISTS package_keg_policies;
"#,
};

const MIGRATION_0006: SqliteMigration = SqliteMigration {
    version: 6,
    name: "add_task_log_records",
    up_sql: r#"
CREATE TABLE IF NOT EXISTS task_log_records (
    log_id INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id INTEGER NOT NULL,
    manager_id TEXT NOT NULL,
    task_type TEXT NOT NULL,
    status TEXT,
    level TEXT NOT NULL,
    message TEXT NOT NULL,
    created_at_unix INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_task_log_records_task_time
    ON task_log_records (task_id, created_at_unix DESC, log_id DESC);

CREATE INDEX IF NOT EXISTS idx_task_log_records_created_at
    ON task_log_records (created_at_unix);
"#,
    down_sql: r#"
DROP INDEX IF EXISTS idx_task_log_records_created_at;
DROP INDEX IF EXISTS idx_task_log_records_task_time;
DROP TABLE IF EXISTS task_log_records;
"#,
};

const MIGRATION_0007: SqliteMigration = SqliteMigration {
    version: 7,
    name: "add_manager_selection_preferences",
    up_sql: r#"
ALTER TABLE manager_preferences ADD COLUMN selected_executable_path TEXT;
ALTER TABLE manager_preferences ADD COLUMN selected_install_method TEXT;
"#,
    down_sql: r#"
CREATE TABLE manager_preferences_backup (
    manager_id TEXT PRIMARY KEY,
    enabled INTEGER NOT NULL DEFAULT 1
);
INSERT INTO manager_preferences_backup
    SELECT manager_id, enabled
    FROM manager_preferences;
DROP TABLE manager_preferences;
ALTER TABLE manager_preferences_backup RENAME TO manager_preferences;
"#,
};

const MIGRATIONS: [SqliteMigration; 7] = [
    MIGRATION_0001,
    MIGRATION_0002,
    MIGRATION_0003,
    MIGRATION_0004,
    MIGRATION_0005,
    MIGRATION_0006,
    MIGRATION_0007,
];

pub fn migrations() -> &'static [SqliteMigration] {
    &MIGRATIONS
}

pub fn migration(version: i64) -> Option<&'static SqliteMigration> {
    MIGRATIONS.iter().find(|entry| entry.version == version)
}

pub fn current_schema_version() -> i64 {
    MIGRATIONS.last().map(|entry| entry.version).unwrap_or(0)
}
