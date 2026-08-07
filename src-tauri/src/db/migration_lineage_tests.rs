//! V083-M1 migration-lineage regression fixtures.
//!
//! Every database is synthetic and lives under a fresh `TempDir`. Existing
//! incompatible files must fail during the read-only preflight, before a
//! read-write/WAL pool can mutate migration history, schema or business rows.

use super::{
    init_pool, DbError, DbMigrationCompatibilityError, DB_MIGRATION_APPLIED_VERSION_UNKNOWN,
    DB_MIGRATION_CHECKSUM_UNKNOWN, DB_MIGRATION_LINEAGE_INCOMPATIBLE,
    DB_MIGRATION_SCHEMA_SENTINEL_MISSING,
};
use sha2::{Digest, Sha384};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const UNKNOWN_APPLIED_VERSION: i64 = 9_999;
const SYNTHETIC_DIVERGENT_SQL: &[u8] =
    b"CREATE TABLE synthetic_same_version_different_sql (id TEXT PRIMARY KEY);";

#[derive(Debug, PartialEq, Eq)]
struct DatabaseFingerprint {
    migration_history: Vec<(i64, String, bool, Vec<u8>, i64)>,
    schema: Vec<(String, String, String, String)>,
    synthetic_cases: Vec<(String, String, String)>,
}

#[derive(Debug, PartialEq, Eq)]
struct PhysicalFingerprint {
    database: Vec<u8>,
    wal: Option<Vec<u8>>,
    shm: Option<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq)]
struct ExistingSchemaFingerprint {
    schema: Vec<(String, String, String, String)>,
    business_rows: Vec<(String, String)>,
}

fn synthetic_divergent_checksum() -> Vec<u8> {
    Sha384::digest(SYNTHETIC_DIVERGENT_SQL).to_vec()
}

async fn migrated_fixture(label: &str) -> (TempDir, PathBuf) {
    let directory = tempfile::Builder::new()
        .prefix(&format!("caseboard-v083-{label}-"))
        .tempdir()
        .expect("create isolated fixture directory");
    let database = directory.path().join("caseboard.db");
    let pool = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("apply current migrations to synthetic fixture");
    sqlx::query(
        "INSERT INTO cases (id, name, case_type, source_folder) \
         VALUES (?1, ?2, '诉讼', ?3)",
    )
    .bind(format!("synthetic-case-{label}"))
    .bind(format!("合成迁移夹具-{label}"))
    .bind(format!("synthetic://migration-fixture/{label}"))
    .execute(&pool)
    .await
    .expect("insert synthetic business marker");
    pool.close().await;
    (directory, database)
}

async fn fixture_pool(database: &Path, foreign_keys: bool) -> SqlitePool {
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(false)
        .foreign_keys(foreign_keys);
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open isolated fixture database")
}

async fn database_fingerprint(database: &Path) -> DatabaseFingerprint {
    assert_sidecars_absent(database);
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(false)
        .read_only(true)
        .immutable(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open fixture read-only for fingerprint");

    let migration_history = sqlx::query_as(
        "SELECT version, description, success, checksum, execution_time \
         FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(&pool)
    .await
    .expect("fingerprint migration history");
    let schema = sqlx::query_as(
        "SELECT type, name, tbl_name, COALESCE(sql, '') \
         FROM sqlite_master ORDER BY type, name",
    )
    .fetch_all(&pool)
    .await
    .expect("fingerprint schema");
    let synthetic_cases = sqlx::query_as("SELECT id, name, source_folder FROM cases ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("fingerprint synthetic business rows");
    pool.close().await;

    DatabaseFingerprint {
        migration_history,
        schema,
        synthetic_cases,
    }
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut value = database.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

fn file_bytes(path: &Path) -> Option<Vec<u8>> {
    std::fs::read(path).ok()
}

fn assert_sidecars_absent(database: &Path) {
    assert!(!sidecar_path(database, "-wal").exists());
    assert!(!sidecar_path(database, "-shm").exists());
}

fn physical_fingerprint(database: &Path) -> PhysicalFingerprint {
    PhysicalFingerprint {
        database: std::fs::read(database).expect("read synthetic database file"),
        wal: file_bytes(&sidecar_path(database, "-wal")),
        shm: file_bytes(&sidecar_path(database, "-shm")),
    }
}

async fn existing_schema_fingerprint(database: &Path) -> ExistingSchemaFingerprint {
    assert_sidecars_absent(database);
    let options = SqliteConnectOptions::new()
        .filename(database)
        .create_if_missing(false)
        .read_only(true)
        .immutable(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open unknown existing schema read-only for fingerprint");
    let schema = sqlx::query_as(
        "SELECT type, name, tbl_name, COALESCE(sql, '') \
         FROM sqlite_master ORDER BY type, name",
    )
    .fetch_all(&pool)
    .await
    .expect("fingerprint unknown existing schema");
    let business_rows = sqlx::query_as("SELECT id, payload FROM legacy_cases ORDER BY id")
        .fetch_all(&pool)
        .await
        .expect("fingerprint unknown existing business rows");
    pool.close().await;
    ExistingSchemaFingerprint {
        schema,
        business_rows,
    }
}

async fn frozen_wal_fixture(
    label: &str,
    include_wal: bool,
    include_shm: bool,
) -> (TempDir, PathBuf) {
    let directory = tempfile::Builder::new()
        .prefix(&format!("caseboard-v083-wal-{label}-"))
        .tempdir()
        .expect("create WAL sidecar fixture directory");
    let live_database = directory.path().join("live.db");
    let frozen_database = directory.path().join("caseboard.db");
    let options = SqliteConnectOptions::new()
        .filename(&live_database)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("open live WAL fixture");
    sqlx::query("PRAGMA wal_autocheckpoint = 0")
        .execute(&pool)
        .await
        .expect("disable WAL autocheckpoint in synthetic fixture");
    sqlx::query("CREATE TABLE wal_only_marker (id TEXT PRIMARY KEY, payload TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create WAL-only synthetic schema");
    sqlx::query("INSERT INTO wal_only_marker VALUES ('wal-1', 'committed-only-in-wal')")
        .execute(&pool)
        .await
        .expect("commit synthetic WAL-only business row");

    let live_wal = sidecar_path(&live_database, "-wal");
    let live_shm = sidecar_path(&live_database, "-shm");
    assert!(live_wal.exists(), "WAL fixture must contain a WAL file");
    assert!(live_shm.exists(), "WAL fixture must contain an SHM file");
    std::fs::copy(&live_database, &frozen_database).expect("freeze synthetic main database");
    if include_wal {
        std::fs::copy(&live_wal, sidecar_path(&frozen_database, "-wal"))
            .expect("freeze synthetic WAL sidecar");
    }
    if include_shm {
        std::fs::copy(&live_shm, sidecar_path(&frozen_database, "-shm"))
            .expect("freeze synthetic SHM sidecar");
    }
    pool.close().await;
    (directory, frozen_database)
}

fn expect_compatibility<'a>(
    error: &'a DbError,
    expected_code: &str,
) -> &'a DbMigrationCompatibilityError {
    let compatibility = error
        .migration_compatibility()
        .expect("expected structured migration compatibility error");
    assert_eq!(compatibility.code, expected_code);
    compatibility
}

async fn assert_fingerprint_unchanged(
    database: &Path,
    before: DatabaseFingerprint,
) -> DatabaseFingerprint {
    let after = database_fingerprint(database).await;
    assert_eq!(after, before);
    after
}

async fn assert_failure_fingerprints_unchanged(
    database: &Path,
    before_physical: PhysicalFingerprint,
    before: DatabaseFingerprint,
) {
    let after_physical = physical_fingerprint(database);
    assert_eq!(after_physical, before_physical);
    assert_fingerprint_unchanged(database, before).await;
}

async fn assert_sidecar_shape_is_blocked(label: &str, include_wal: bool, include_shm: bool) {
    let (_directory, database) = frozen_wal_fixture(label, include_wal, include_shm).await;

    // This is deliberately the first operation against the frozen target.
    // No SQLite helper may normalize or rebuild its sidecars before baseline.
    let before_physical = physical_fingerprint(&database);
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("every WAL/SHM sidecar shape must fail before SQLite opens");
    // Likewise, sample bytes before any post-failure SQLite helper.
    let after_physical = physical_fingerprint(&database);
    assert_eq!(after_physical, before_physical);

    let compatibility = expect_compatibility(&error, DB_MIGRATION_LINEAGE_INCOMPATIBLE);
    assert_eq!(
        compatibility.reason,
        "wal_sidecar_present_requires_recovery"
    );
    let message = error
        .startup_recovery_message(database.to_str().expect("UTF-8 fixture path"))
        .expect("sidecar recovery refusal has a native recovery message");
    assert!(message.contains("不要删除"));
    assert!(message.contains("WAL/SHM"));
    assert!(message.contains("隔离副本"));
}

#[tokio::test]
async fn fresh_database_reaches_current_lineage_and_all_frozen_sentinels() {
    let (_directory, database) = migrated_fixture("fresh").await;
    let pool = fixture_pool(&database, true).await;
    let actual_versions: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("inspect applied migration versions");
    let embedded_versions: Vec<i64> = sqlx::migrate!("./migrations")
        .iter()
        .map(|migration| migration.version)
        .collect();
    let failed: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE success = 0")
        .fetch_one(&pool)
        .await
        .expect("inspect failed migration count");

    assert_eq!(actual_versions, embedded_versions);
    assert_eq!(actual_versions.len(), 61);
    assert_eq!(actual_versions.last(), Some(&62));
    assert!(!actual_versions.contains(&36), "version 36 is a legal gap");
    assert_eq!(failed, 0);
    pool.close().await;

    // Reopening is what exercises every production sentinel against the
    // newly-created current schema.
    let reopened = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("all frozen schema sentinels must pass");
    reopened.close().await;

    // A pre-existing empty SQLite file also has no migration table and must be
    // allowed through the read-only preflight into normal migration.
    let empty_directory = tempfile::Builder::new()
        .prefix("caseboard-v083-existing-empty-")
        .tempdir()
        .expect("create pre-existing empty fixture directory");
    let empty_database = empty_directory.path().join("caseboard.db");
    std::fs::File::create(&empty_database).expect("create pre-existing empty database file");
    let migrated_empty = init_pool(empty_database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("existing database without migration history must migrate");
    let migrated_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations")
        .fetch_one(&migrated_empty)
        .await
        .expect("inspect migrated empty database");
    assert_eq!(migrated_count, 61);
    migrated_empty.close().await;
}

#[tokio::test]
async fn existing_user_schema_without_migration_history_fails_closed_before_any_write() {
    let directory = tempfile::Builder::new()
        .prefix("caseboard-v083-existing-schema-no-history-")
        .tempdir()
        .expect("create unknown existing schema fixture directory");
    let database = directory.path().join("caseboard.db");
    std::fs::File::create(&database).expect("create unknown existing database file");
    let pool = fixture_pool(&database, true).await;
    sqlx::query("CREATE TABLE legacy_cases (id TEXT PRIMARY KEY, payload TEXT NOT NULL)")
        .execute(&pool)
        .await
        .expect("create synthetic existing user schema");
    sqlx::query("INSERT INTO legacy_cases (id, payload) VALUES ('legacy-1', 'preserve-me')")
        .execute(&pool)
        .await
        .expect("insert synthetic existing business row");
    pool.close().await;

    let before_physical = physical_fingerprint(&database);
    let before = existing_schema_fingerprint(&database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("existing user schema without migration history must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_LINEAGE_INCOMPATIBLE);
    assert_eq!(
        compatibility.reason,
        "migration_history_missing_for_existing_schema"
    );
    assert_eq!(compatibility.version, None);

    let after_physical = physical_fingerprint(&database);
    assert_eq!(after_physical, before_physical);
    assert_eq!(existing_schema_fingerprint(&database).await, before);
}

#[tokio::test]
async fn empty_migration_history_with_existing_schema_fails_closed_before_any_write() {
    let (_directory, database) = migrated_fixture("empty-history-existing-schema").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DELETE FROM _sqlx_migrations")
        .execute(&pool)
        .await
        .expect("empty synthetic migration history");
    pool.close().await;

    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("empty history with existing schema must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_LINEAGE_INCOMPATIBLE);
    assert_eq!(
        compatibility.reason,
        "migration_history_empty_for_existing_schema"
    );
    assert_eq!(compatibility.version, None);
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn complete_wal_and_shm_are_blocked_before_sqlite_connection() {
    assert_sidecar_shape_is_blocked("complete", true, true).await;
}

#[tokio::test]
async fn wal_without_shm_is_blocked_before_sqlite_connection() {
    assert_sidecar_shape_is_blocked("missing-shm", true, false).await;
}

#[tokio::test]
async fn shm_without_wal_is_blocked_before_sqlite_connection() {
    assert_sidecar_shape_is_blocked("only-shm", false, true).await;
}

#[tokio::test]
async fn current_database_reopen_keeps_all_fingerprints_unchanged() {
    let (_directory, database) = migrated_fixture("current-reopen").await;
    let before = database_fingerprint(&database).await;

    let reopened = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("current lineage must reopen");
    reopened.close().await;

    assert_fingerprint_unchanged(&database, before).await;
}

#[tokio::test]
async fn unknown_checksum_fails_closed_before_any_database_write() {
    let (_directory, database) = migrated_fixture("unknown-checksum").await;
    let pool = fixture_pool(&database, true).await;
    let unknown_checksum = synthetic_divergent_checksum();
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 49")
        .bind(&unknown_checksum)
        .execute(&pool)
        .await
        .expect("install synthetic unknown checksum");
    pool.close().await;
    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("unknown checksum must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_CHECKSUM_UNKNOWN);
    assert_eq!(compatibility.version, Some(49));
    assert_eq!(compatibility.reason, "checksum_not_allowlisted");
    assert_eq!(
        compatibility.stored_checksum.as_deref().map(str::len),
        Some(96)
    );
    assert_eq!(
        compatibility.current_checksum.as_deref().map(str::len),
        Some(96)
    );
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;

    let serialized =
        serde_json::to_value(&error).expect("serialize structured compatibility error");
    assert_eq!(serialized["code"], DB_MIGRATION_CHECKSUM_UNKNOWN);
    assert_eq!(serialized["version"], 49);
    assert!(serialized.get("sql").is_none());
    assert!(serialized.get("business_data").is_none());

    let message = error
        .startup_recovery_message("X:\\synthetic\\caseboard.db")
        .expect("compatibility errors have a native recovery message");
    assert!(message.contains(DB_MIGRATION_CHECKSUM_UNKNOWN));
    assert!(message.contains("X:\\synthetic\\caseboard.db"));
    assert!(message.contains("备份"));
    assert!(message.contains("退出"));
}

#[tokio::test]
async fn migration_49_success_without_inbox_fails_on_schema_sentinel_before_write() {
    let (_directory, database) = migrated_fixture("missing-m49-sentinel").await;
    let pool = fixture_pool(&database, false).await;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version >= 51")
        .execute(&pool)
        .await
        .expect("roll migration history back to version 50");
    sqlx::query("DROP TABLE feishu_sync_inbox")
        .execute(&pool)
        .await
        .expect("remove the migration 49 sentinel from synthetic fixture");
    pool.close().await;
    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("missing migration 49 sentinel must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_SCHEMA_SENTINEL_MISSING);
    assert_eq!(compatibility.version, Some(49));
    assert_eq!(compatibility.reason, "applied_migration_schema_missing");
    assert!(compatibility
        .missing_sentinels
        .iter()
        .any(|code| code == "M49.table.feishu_sync_inbox"));
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn missing_sentinel_takes_priority_over_unknown_checksum() {
    let (_directory, database) = migrated_fixture("sentinel-before-checksum").await;
    let pool = fixture_pool(&database, false).await;
    sqlx::query("DELETE FROM _sqlx_migrations WHERE version >= 51")
        .execute(&pool)
        .await
        .expect("roll combination fixture history back to version 50");
    sqlx::query("DROP TABLE feishu_sync_inbox")
        .execute(&pool)
        .await
        .expect("remove combination fixture sentinel");
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 49")
        .bind(synthetic_divergent_checksum())
        .execute(&pool)
        .await
        .expect("install combination fixture unknown checksum");
    pool.close().await;

    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("schema sentinel must take priority over checksum mismatch");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_SCHEMA_SENTINEL_MISSING);
    assert_eq!(compatibility.version, Some(49));
    assert_eq!(compatibility.reason, "applied_migration_schema_missing");
    assert!(compatibility
        .missing_sentinels
        .iter()
        .any(|code| code == "M49.table.feishu_sync_inbox"));
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn unknown_applied_version_fails_closed_before_any_database_write() {
    let (_directory, database) = migrated_fixture("unknown-version").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query(
        "INSERT INTO _sqlx_migrations \
         (version, description, success, checksum, execution_time) \
         VALUES (?1, 'synthetic_unknown_applied', 1, ?2, 0)",
    )
    .bind(UNKNOWN_APPLIED_VERSION)
    .bind(synthetic_divergent_checksum())
    .execute(&pool)
    .await
    .expect("insert synthetic unknown applied migration");
    pool.close().await;
    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("unknown applied version must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_APPLIED_VERSION_UNKNOWN);
    assert_eq!(compatibility.version, Some(UNKNOWN_APPLIED_VERSION));
    assert_eq!(compatibility.reason, "applied_version_not_embedded");
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn failed_migration_row_fails_closed_before_any_database_write() {
    let (_directory, database) = migrated_fixture("failed-row").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("UPDATE _sqlx_migrations SET success = 0 WHERE version = 62")
        .execute(&pool)
        .await
        .expect("mark synthetic migration row failed");
    pool.close().await;
    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("failed migration history must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_LINEAGE_INCOMPATIBLE);
    assert_eq!(compatibility.version, Some(62));
    assert_eq!(compatibility.reason, "failed_history_row");
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}
