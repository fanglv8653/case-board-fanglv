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
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::TempDir;

const UNKNOWN_APPLIED_VERSION: i64 = 9_999;
const SYNTHETIC_DIVERGENT_SQL: &[u8] =
    b"CREATE TABLE synthetic_same_version_different_sql (id TEXT PRIMARY KEY);";
const M63_EXPORT_DRAFT_INDEXES_SQL: &str = r#"
CREATE INDEX idx_device_sync_export_drafts_state
ON device_sync_export_drafts(group_id, local_device_id, state, sequence);
CREATE UNIQUE INDEX idx_device_sync_export_drafts_one_prepared
ON device_sync_export_drafts(group_id)
WHERE state='prepared';
"#;
const RC_MIGRATION_CHILD_DATABASE_ENV: &str = "CASEBOARD_RC_MIGRATION_CHILD_DATABASE";

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

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct LegacyQuarantineRow {
    id: String,
    group_id: Option<String>,
    source_path: Option<String>,
    source_device_id: String,
    source_sequence: i64,
    reason_code: String,
    details_json: String,
    status: String,
    first_seen_at: String,
    last_seen_at: String,
    retry_count: i64,
    resolved_at: Option<String>,
    last_error_code: String,
    created_at: String,
}

fn synthetic_divergent_checksum() -> Vec<u8> {
    Sha384::digest(SYNTHETIC_DIVERGENT_SQL).to_vec()
}

async fn migrated_fixture(label: &str) -> (TempDir, PathBuf) {
    let directory = tempfile::Builder::new()
        .prefix(&format!("caseboard-v083-{label}-"))
        .tempdir()
        .expect("create isolated fixture directory");
    let staging_database = directory.path().join("staging.db");
    let database = directory.path().join("caseboard.db");
    let pool = init_pool(staging_database.to_str().expect("UTF-8 fixture path"))
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
    sqlx::query("VACUUM INTO ?1")
        .bind(database.to_str().expect("UTF-8 fixture path"))
        .execute(&pool)
        .await
        .expect("freeze a checkpointed main-only migration fixture");
    pool.close().await;
    assert_sidecars_absent(&database);
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

    let fingerprint = database_fingerprint_from_pool(&pool).await;
    pool.close().await;
    fingerprint
}

async fn database_fingerprint_from_pool(pool: &SqlitePool) -> DatabaseFingerprint {
    let migration_history = sqlx::query_as(
        "SELECT version, description, success, checksum, execution_time \
         FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .expect("fingerprint migration history");
    let schema = sqlx::query_as(
        "SELECT type, name, tbl_name, COALESCE(sql, '') \
         FROM sqlite_master ORDER BY type, name",
    )
    .fetch_all(pool)
    .await
    .expect("fingerprint schema");
    let synthetic_cases = sqlx::query_as("SELECT id, name, source_folder FROM cases ORDER BY id")
        .fetch_all(pool)
        .await
        .expect("fingerprint synthetic business rows");

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

fn run_rc_production_init_child(database: &Path) {
    let output =
        Command::new(std::env::current_exe().expect("resolve current Rust test executable"))
            .arg("db::migration_lineage_tests::rc_local_production_init_child")
            .arg("--exact")
            .arg("--ignored")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(RC_MIGRATION_CHILD_DATABASE_ENV, database)
            .output()
            .expect("run production init in an isolated application-process fixture");
    assert!(
        output.status.success(),
        "production-init child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
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

async fn assert_m63_sentinel_failure_without_write(database: &Path, expected_code: &str) {
    let before_physical = physical_fingerprint(database);
    let before = database_fingerprint(database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("malformed migration 63 schema must fail closed");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_SCHEMA_SENTINEL_MISSING);
    assert_eq!(compatibility.version, Some(63));
    assert_eq!(compatibility.reason, "applied_migration_schema_missing");
    assert!(
        compatibility
            .missing_sentinels
            .iter()
            .any(|code| code == expected_code),
        "expected sentinel {expected_code}, got {:?}",
        compatibility.missing_sentinels
    );
    assert_failure_fingerprints_unchanged(database, before_physical, before).await;
}

async fn replace_export_drafts_table_definition(pool: &SqlitePool, from: &str, to: &str) {
    let original: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_master \
         WHERE type='table' AND name='device_sync_export_drafts'",
    )
    .fetch_one(pool)
    .await
    .expect("read current export-drafts DDL");
    let replacement = original.replacen(from, to, 1);
    assert_ne!(replacement, original, "test DDL transform must take effect");
    sqlx::query("DROP TABLE device_sync_export_drafts")
        .execute(pool)
        .await
        .expect("drop current export-drafts table");
    sqlx::raw_sql(&replacement)
        .execute(pool)
        .await
        .expect("create malformed export-drafts lookalike");
    sqlx::raw_sql(M63_EXPORT_DRAFT_INDEXES_SQL)
        .execute(pool)
        .await
        .expect("restore correctly named export-drafts indexes");
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
    assert_eq!(actual_versions.len(), 62);
    assert_eq!(actual_versions.last(), Some(&63));
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
    assert_eq!(migrated_count, 62);
    migrated_empty.close().await;
}

#[tokio::test]
async fn rc_local_pre_0063_database_upgrades_through_production_init_idempotently() {
    let directory = tempfile::Builder::new()
        .prefix("caseboard-v083-rc-pre-0063-")
        .tempdir()
        .expect("create isolated pre-0063 fixture directory");
    let staging_database = directory.path().join("staging.db");
    let database = directory.path().join("caseboard.db");
    std::fs::File::create(&staging_database).expect("create pre-0063 staging file");

    let embedded = sqlx::migrate!("./migrations");
    let pre_0063 = Migrator {
        migrations: Cow::Owned(
            embedded
                .iter()
                .filter(|migration| migration.version <= 62)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    };
    let fixture = fixture_pool(&staging_database, true).await;
    pre_0063
        .run(&fixture)
        .await
        .expect("apply the real 0001-0062 migration set");
    sqlx::query(
        "INSERT INTO cases (id,name,case_type,source_folder,legal_domain,domain_source)
         VALUES ('rc-pre-0063-marker','RC synthetic marker','诉讼',
                 'synthetic://rc/pre-0063','civil','manual')",
    )
    .execute(&fixture)
    .await
    .expect("insert de-identified pre-0063 marker");
    let pre_state: (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),max(version),
                sum(CASE WHEN version=63 THEN 1 ELSE 0 END)
         FROM _sqlx_migrations WHERE success=1",
    )
    .fetch_one(&fixture)
    .await
    .expect("inspect pre-0063 migration state");
    assert_eq!(pre_state, (61, 62, 0));
    sqlx::query("VACUUM INTO ?1")
        .bind(database.to_str().expect("UTF-8 fixture path"))
        .execute(&fixture)
        .await
        .expect("freeze a main-only real pre-0063 fixture");
    fixture.close().await;
    assert_sidecars_absent(&database);

    run_rc_production_init_child(&database);
    assert_sidecars_absent(&database);
    let first_upgrade_fingerprint = database_fingerprint(&database).await;

    run_rc_production_init_child(&database);
    assert_sidecars_absent(&database);
    assert_eq!(
        database_fingerprint(&database).await,
        first_upgrade_fingerprint
    );
}

#[test]
#[ignore = "invoked by the RC migration parent fixture in a fresh process"]
fn rc_local_production_init_child() {
    let database = std::env::var_os(RC_MIGRATION_CHILD_DATABASE_ENV)
        .map(PathBuf::from)
        .expect("RC migration child database path must be provided");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create RC migration child runtime");
    runtime.block_on(async {
        let pool = init_pool(database.to_str().expect("UTF-8 fixture path"))
            .await
            .expect("production init upgrades or reopens the RC database");
        let state: (i64, i64, i64) = sqlx::query_as(
            "SELECT count(*),max(version),
                    sum(CASE WHEN version=63 THEN 1 ELSE 0 END)
             FROM _sqlx_migrations WHERE success=1",
        )
        .fetch_one(&pool)
        .await
        .expect("inspect production migration state");
        assert_eq!(state, (62, 63, 1));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT name FROM cases WHERE id='rc-pre-0063-marker'",
            )
            .fetch_one(&pool)
            .await
            .expect("read preserved synthetic marker"),
            "RC synthetic marker"
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>("PRAGMA quick_check")
                .fetch_one(&pool)
                .await
                .expect("run production quick_check"),
            "ok"
        );
        assert!(sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .expect("run production foreign_key_check")
            .is_empty());
        pool.close().await;
    });
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
    let after = database_fingerprint_from_pool(&reopened).await;
    assert_eq!(after, before);
    reopened.close().await;
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
async fn migration_63_requires_semantic_active_quarantine_index_before_write() {
    let (_directory, database) = migrated_fixture("missing-m63-sentinel").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DROP INDEX idx_device_sync_quarantine_active_key")
        .execute(&pool)
        .await
        .expect("remove migration 63 active quarantine index");
    sqlx::query(
        "CREATE UNIQUE INDEX idx_device_sync_quarantine_active_key \
         ON device_sync_quarantine( \
             COALESCE(group_id,''), COALESCE(source_path,''), reason_code \
         ) WHERE status='active'",
    )
    .execute(&pool)
    .await
    .expect("replace migration 63 sentinel with the obsolete active-key shape");
    pool.close().await;

    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("migration 63 must fail when active-index semantics are missing");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_SCHEMA_SENTINEL_MISSING);
    assert_eq!(compatibility.version, Some(63));
    assert_eq!(compatibility.reason, "applied_migration_schema_missing");
    assert!(compatibility
        .missing_sentinels
        .iter()
        .any(|code| code == "M63.index.idx_device_sync_quarantine_active_key"));
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn migration_63_requires_exact_group_status_index_order_before_write() {
    let (_directory, database) = migrated_fixture("wrong-m63-group-status-index").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DROP INDEX idx_device_sync_quarantine_group_status")
        .execute(&pool)
        .await
        .expect("remove migration 63 group-status index");
    sqlx::query(
        "CREATE INDEX idx_device_sync_quarantine_group_status \
         ON device_sync_quarantine(status, group_id, last_seen_at DESC)",
    )
    .execute(&pool)
    .await
    .expect("replace migration 63 group-status index with wrong column order");
    pool.close().await;

    let before_physical = physical_fingerprint(&database);
    let before = database_fingerprint(&database).await;
    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("migration 63 must reject a reordered group-status index");
    let compatibility = expect_compatibility(&error, DB_MIGRATION_SCHEMA_SENTINEL_MISSING);
    assert_eq!(compatibility.version, Some(63));
    assert!(compatibility
        .missing_sentinels
        .iter()
        .any(|code| code == "M63.index.idx_device_sync_quarantine_group_status"));
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}

#[tokio::test]
async fn migration_63_requires_exact_outbox_capture_sequence_index_before_write() {
    let (_directory, database) = migrated_fixture("wrong-m63-outbox-capture-index").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DROP INDEX idx_device_sync_outbox_capture_sequence")
        .execute(&pool)
        .await
        .expect("remove migration 63 outbox capture index");
    sqlx::query(
        "CREATE UNIQUE INDEX idx_device_sync_outbox_capture_sequence \
         ON device_sync_outbox(capture_sequence, group_id)",
    )
    .execute(&pool)
    .await
    .expect("replace outbox capture index with wrong column order");
    pool.close().await;

    assert_m63_sentinel_failure_without_write(
        &database,
        "M63.index.idx_device_sync_outbox_capture_sequence",
    )
    .await;
}

#[tokio::test]
async fn migration_63_requires_export_drafts_table_before_write() {
    let (_directory, database) = migrated_fixture("missing-m63-export-drafts").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DROP TABLE device_sync_export_drafts")
        .execute(&pool)
        .await
        .expect("remove migration 63 export-drafts table");
    pool.close().await;

    assert_m63_sentinel_failure_without_write(&database, "M63.table.device_sync_export_drafts")
        .await;
}

#[tokio::test]
async fn migration_63_requires_every_export_draft_column_before_write() {
    let (_directory, database) = migrated_fixture("missing-m63-export-draft-column").await;
    let pool = fixture_pool(&database, true).await;
    replace_export_drafts_table_definition(&pool, "operation_fingerprint TEXT NOT NULL,", "").await;
    pool.close().await;

    assert_m63_sentinel_failure_without_write(
        &database,
        "M63.column.export_drafts.operation_fingerprint",
    )
    .await;
}

#[tokio::test]
async fn migration_63_requires_exact_export_draft_checks_before_write() {
    let (_directory, database) = migrated_fixture("wrong-m63-export-draft-check").await;
    let pool = fixture_pool(&database, true).await;
    replace_export_drafts_table_definition(&pool, "CHECK(sequence >= 1)", "CHECK(sequence >= 0)")
        .await;
    pool.close().await;

    assert_m63_sentinel_failure_without_write(
        &database,
        "M63.table.device_sync_export_drafts.definition",
    )
    .await;
}

#[tokio::test]
async fn migration_63_rejects_export_draft_path_column_before_write() {
    let (_directory, database) = migrated_fixture("m63-export-draft-path-column").await;
    let pool = fixture_pool(&database, true).await;
    replace_export_drafts_table_definition(
        &pool,
        "finalized_at TEXT,",
        "finalized_at TEXT, nas_path TEXT,",
    )
    .await;
    pool.close().await;

    assert_m63_sentinel_failure_without_write(
        &database,
        "M63.table.device_sync_export_drafts.definition",
    )
    .await;
}

#[tokio::test]
async fn migration_63_requires_exact_one_prepared_draft_index_before_write() {
    let (_directory, database) = migrated_fixture("wrong-m63-one-prepared-index").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("DROP INDEX idx_device_sync_export_drafts_one_prepared")
        .execute(&pool)
        .await
        .expect("remove one-prepared export-drafts index");
    sqlx::query(
        "CREATE UNIQUE INDEX idx_device_sync_export_drafts_one_prepared \
         ON device_sync_export_drafts(group_id, local_device_id) \
         WHERE state='prepared'",
    )
    .execute(&pool)
    .await
    .expect("replace one-prepared index with a weaker lookalike");
    pool.close().await;

    assert_m63_sentinel_failure_without_write(
        &database,
        "M63.index.idx_device_sync_export_drafts_one_prepared",
    )
    .await;
}

#[tokio::test]
async fn migration_63_preserves_legacy_rows_as_manual_review_without_inventing_identity() {
    let directory = tempfile::Builder::new()
        .prefix("caseboard-v083-m63-legacy-")
        .tempdir()
        .expect("create migration 63 legacy fixture directory");
    let database = directory.path().join("caseboard.db");
    std::fs::File::create(&database).expect("create migration 63 legacy fixture database");
    let pool = fixture_pool(&database, true).await;
    sqlx::raw_sql(
        r#"
        CREATE TABLE device_sync_groups (
            id TEXT PRIMARY KEY NOT NULL,
            connector_type TEXT NOT NULL DEFAULT 'mounted_folder'
                CHECK(connector_type = 'mounted_folder'),
            connector_root TEXT NOT NULL,
            local_device_id TEXT NOT NULL,
            protocol_version INTEGER NOT NULL DEFAULT 1,
            key_epoch INTEGER NOT NULL DEFAULT 1,
            next_sequence INTEGER NOT NULL DEFAULT 1,
            paused INTEGER NOT NULL DEFAULT 0 CHECK(paused IN (0,1)),
            last_manifest_hash TEXT,
            last_synced_at TEXT,
            created_at TEXT NOT NULL DEFAULT(datetime('now')),
            updated_at TEXT NOT NULL DEFAULT(datetime('now'))
        );
        CREATE TABLE device_sync_outbox (
            operation_id TEXT PRIMARY KEY NOT NULL,
            group_id TEXT NOT NULL,
            entity_type TEXT NOT NULL CHECK(entity_type IN (
                'case','party','contact','work_item','stage_item','agency_contact',
                'criminal_deadline','criminal_workflow','criminal_task','case_todo',
                'calendar_event','income_record','case_payment','feishu_link',
                'feishu_snapshot','feishu_conflict','feishu_inbox',
                'feishu_binding_audit','legal_skill_package','legal_skill_binding',
                'legal_skill_binding_suppression'
            )),
            entity_id TEXT NOT NULL,
            case_id TEXT,
            action TEXT NOT NULL CHECK(action IN ('upsert','tombstone')),
            base_revision INTEGER NOT NULL,
            changed_fields_json TEXT NOT NULL,
            base_field_hashes_json TEXT NOT NULL DEFAULT '{}',
            atomic_group TEXT,
            author_device_id TEXT NOT NULL,
            logical_time INTEGER NOT NULL,
            schema_version INTEGER NOT NULL DEFAULT 1,
            state TEXT NOT NULL DEFAULT 'pending'
                CHECK(state IN ('pending','exported','acknowledged','quarantined')),
            exported_sequence INTEGER,
            created_at TEXT NOT NULL DEFAULT(datetime('now')),
            updated_at TEXT NOT NULL DEFAULT(datetime('now')),
            FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
        );
        CREATE INDEX idx_device_sync_outbox_pending
        ON device_sync_outbox(group_id, state, logical_time);
        CREATE TABLE device_sync_quarantine (
            id TEXT PRIMARY KEY NOT NULL,
            group_id TEXT,
            source_path TEXT,
            reason_code TEXT NOT NULL,
            details_json TEXT NOT NULL DEFAULT '{}',
            created_at TEXT NOT NULL DEFAULT(datetime('now')),
            FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
        );
        "#,
    )
    .execute(&pool)
    .await
    .expect("create synthetic pre-63 schema");
    sqlx::query(
        "INSERT INTO device_sync_groups \
         (id, connector_root, local_device_id, last_synced_at) VALUES (?1, ?2, ?3, ?4)",
    )
    .bind("g-legacy")
    .bind("synthetic-mounted-root")
    .bind("legacy-device")
    .bind("2026-08-07T01:02:03Z")
    .execute(&pool)
    .await
    .expect("insert synthetic pre-63 group");
    sqlx::query(
        "INSERT INTO device_sync_groups \
         (id, connector_root, local_device_id) VALUES ('g-second', 'second-root', 'second-device')",
    )
    .execute(&pool)
    .await
    .expect("insert second group for per-group outbox normalization");
    for (operation_id, group_id, logical_time) in [
        ("op-z", "g-legacy", 100_i64),
        ("op-a", "g-legacy", 100_i64),
        ("op-mid", "g-legacy", 101_i64),
        ("op-g2-z", "g-second", 100_i64),
        ("op-g2-a", "g-second", 100_i64),
    ] {
        sqlx::query(
            "INSERT INTO device_sync_outbox \
             (operation_id, group_id, entity_type, entity_id, action, base_revision, \
              changed_fields_json, author_device_id, logical_time) \
             VALUES (?1, ?2, 'case', ?1, 'upsert', 0, '{}', 'legacy-device', ?3)",
        )
        .bind(operation_id)
        .bind(group_id)
        .bind(logical_time)
        .execute(&pool)
        .await
        .expect("insert pre-63 outbox row in deliberately non-planner order");
    }
    for (id, details, created_at) in [
        (
            "legacy-a",
            r#"{"copy":1,"database_error":"C:\\Sensitive\\Client\\caseboard.db"}"#,
            "2026-08-01T10:00:00Z",
        ),
        (
            "legacy-b",
            r#"{"copy":2,"secret":"client-business-body"}"#,
            "2026-08-02T11:00:00Z",
        ),
    ] {
        sqlx::query(
            "INSERT INTO device_sync_quarantine \
             (id, group_id, source_path, reason_code, details_json, created_at) \
             VALUES (?1, 'g-legacy', ?2, 'LEGACY_ERROR', ?3, ?4)",
        )
        .bind(id)
        .bind(r"C:\Sensitive\Client\package.sync")
        .bind(details)
        .bind(created_at)
        .execute(&pool)
        .await
        .expect("insert synthetic duplicate legacy quarantine row");
    }

    sqlx::raw_sql(include_str!(
        "../../migrations/0063_device_sync_quarantine_lifecycle.sql"
    ))
    .execute(&pool)
    .await
    .expect("apply the actual migration 63 SQL to the legacy fixture");

    let group_state: (Option<String>, Option<String>, i64, Option<String>) = sqlx::query_as(
        "SELECT last_attempt_at, last_success_at, auto_paused, pause_reason_code \
         FROM device_sync_groups WHERE id = 'g-legacy'",
    )
    .fetch_one(&pool)
    .await
    .expect("read migrated group lifecycle fields");
    assert_eq!(
        group_state,
        (
            Some("2026-08-07T01:02:03Z".to_string()),
            Some("2026-08-07T01:02:03Z".to_string()),
            0,
            None,
        )
    );

    let normalized_outbox: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT operation_id, group_id, capture_sequence \
         FROM device_sync_outbox ORDER BY group_id, capture_sequence",
    )
    .fetch_all(&pool)
    .await
    .expect("read normalized legacy outbox order");
    assert_eq!(
        normalized_outbox,
        vec![
            ("op-a".to_string(), "g-legacy".to_string(), 1),
            ("op-z".to_string(), "g-legacy".to_string(), 2),
            ("op-mid".to_string(), "g-legacy".to_string(), 3),
            ("op-g2-a".to_string(), "g-second".to_string(), 1),
            ("op-g2-z".to_string(), "g-second".to_string(), 2),
        ],
        "migration 63 must freeze the exact legacy planner order per group"
    );
    let duplicate_capture_sequence = sqlx::query(
        "INSERT INTO device_sync_outbox \
         (operation_id, group_id, entity_type, entity_id, action, base_revision, \
          changed_fields_json, author_device_id, logical_time, capture_sequence) \
         VALUES ('op-duplicate-sequence', 'g-legacy', 'case', 'case-new', 'upsert', \
                 0, '{}', 'legacy-device', 102, 1)",
    )
    .execute(&pool)
    .await;
    assert!(
        duplicate_capture_sequence.is_err(),
        "capture_sequence must be unique within each group"
    );

    let rows: Vec<LegacyQuarantineRow> = sqlx::query_as(
        "SELECT id, group_id, source_path, source_device_id, source_sequence, \
                reason_code, details_json, status, first_seen_at, last_seen_at, \
                retry_count, resolved_at, last_error_code, created_at \
         FROM device_sync_quarantine ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .expect("read migrated legacy quarantine rows");
    assert_eq!(rows.len(), 2);
    for (row, expected_id, expected_created_at) in [
        (&rows[0], "legacy-a", "2026-08-01T10:00:00Z"),
        (&rows[1], "legacy-b", "2026-08-02T11:00:00Z"),
    ] {
        assert_eq!(row.id, expected_id);
        assert_eq!(row.group_id.as_deref(), Some("g-legacy"));
        assert_eq!(row.source_path, None);
        assert_eq!(row.source_device_id, "__legacy__");
        assert_eq!(row.source_sequence, -1);
        assert_eq!(row.reason_code, "LEGACY_ERROR");
        assert_eq!(
            row.details_json,
            r#"{"legacy_record":true,"identity":"unknown","sensitive_content":"redacted"}"#
        );
        assert!(!row.details_json.contains("Sensitive"));
        assert!(!row.details_json.contains("client-business-body"));
        assert_eq!(row.status, "manual_review");
        assert_eq!(row.first_seen_at, expected_created_at);
        assert_eq!(row.last_seen_at, expected_created_at);
        assert_eq!(row.retry_count, 1);
        assert_eq!(row.resolved_at, None);
        assert_eq!(row.last_error_code, "LEGACY_ERROR");
        assert_eq!(row.created_at, expected_created_at);
    }

    let missing_identity = sqlx::query(
        "INSERT INTO device_sync_quarantine (id, reason_code, last_error_code) \
         VALUES ('missing-identity', 'E', 'E')",
    )
    .execute(&pool)
    .await;
    assert!(
        missing_identity.is_err(),
        "package identity must be NOT NULL"
    );

    let invalid_status = sqlx::query(
        "INSERT INTO device_sync_quarantine \
         (id, source_device_id, source_sequence, reason_code, status, last_error_code) \
         VALUES ('bad-status', 'device-a', 7, 'E', 'unknown', 'E')",
    )
    .execute(&pool)
    .await;
    assert!(
        invalid_status.is_err(),
        "status CHECK must reject unknown values"
    );

    let invalid_retry = sqlx::query(
        "INSERT INTO device_sync_quarantine \
         (id, source_device_id, source_sequence, reason_code, retry_count, last_error_code) \
         VALUES ('bad-retry', 'device-a', 7, 'E', 0, 'E')",
    )
    .execute(&pool)
    .await;
    assert!(invalid_retry.is_err(), "retry CHECK must reject zero");

    sqlx::query(
        "INSERT INTO device_sync_quarantine \
         (id, group_id, source_device_id, source_sequence, reason_code, last_error_code) \
         VALUES ('active-a', 'g-legacy', 'device-a', 7, 'E', 'E')",
    )
    .execute(&pool)
    .await
    .expect("insert first active package identity");
    let duplicate_active = sqlx::query(
        "INSERT INTO device_sync_quarantine \
         (id, group_id, source_device_id, source_sequence, reason_code, last_error_code) \
         VALUES ('active-b', 'g-legacy', 'device-a', 7, 'E', 'E')",
    )
    .execute(&pool)
    .await;
    assert!(
        duplicate_active.is_err(),
        "active package identity must be unique"
    );
    sqlx::query(
        "INSERT INTO device_sync_quarantine \
         (id, group_id, source_device_id, source_sequence, reason_code, status, last_error_code) \
         VALUES ('manual-c', 'g-legacy', 'device-a', 7, 'E', 'manual_review', 'E')",
    )
    .execute(&pool)
    .await
    .expect("manual-review history must not participate in the active unique key");

    let missing_draft_payload = sqlx::query(
        "INSERT INTO device_sync_export_drafts \
         (group_id, local_device_id, sequence, key_epoch) \
         VALUES ('g-legacy', 'legacy-device', 1, 1)",
    )
    .execute(&pool)
    .await;
    assert!(
        missing_draft_payload.is_err(),
        "encrypted draft payload and fingerprints must be NOT NULL"
    );
    for (id, sequence, key_epoch, state) in [
        ("bad-sequence", 0_i64, 1_i64, "prepared"),
        ("bad-key-epoch", 1_i64, 0_i64, "prepared"),
        ("bad-state", 1_i64, 1_i64, "publishing"),
    ] {
        let invalid = sqlx::query(
            "INSERT INTO device_sync_export_drafts \
             (group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
              manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
              operation_ids_json, operation_fingerprint, state) \
             VALUES ('g-legacy', ?1, ?2, ?3, X'01', X'02', 'event-hash', \
                     'manifest-hash', '[]', 'fingerprint', ?4)",
        )
        .bind(id)
        .bind(sequence)
        .bind(key_epoch)
        .bind(state)
        .execute(&pool)
        .await;
        assert!(invalid.is_err(), "draft CHECK constraints must fail closed");
    }
    sqlx::query(
        "INSERT INTO device_sync_export_drafts \
         (group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
          manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
          operation_ids_json, operation_fingerprint) \
         VALUES ('g-legacy', 'legacy-device', 1, 1, X'01', X'02', 'event-hash', \
                 'manifest-hash', '[\"op-a\"]', 'fingerprint')",
    )
    .execute(&pool)
    .await
    .expect("insert valid prepared export draft");
    let draft_defaults: (String, String, String, Option<String>) = sqlx::query_as(
        "SELECT state, created_at, updated_at, finalized_at \
         FROM device_sync_export_drafts \
         WHERE group_id='g-legacy' AND local_device_id='legacy-device' AND sequence=1",
    )
    .fetch_one(&pool)
    .await
    .expect("read export draft defaults");
    assert_eq!(draft_defaults.0, "prepared");
    assert!(!draft_defaults.1.is_empty());
    assert!(!draft_defaults.2.is_empty());
    assert_eq!(draft_defaults.3, None);
    let duplicate_draft_primary_key = sqlx::query(
        "INSERT INTO device_sync_export_drafts \
         (group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
          manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
          operation_ids_json, operation_fingerprint, state) \
         SELECT group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
                manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
                operation_ids_json, operation_fingerprint, 'finalized' \
         FROM device_sync_export_drafts WHERE group_id='g-legacy'",
    )
    .execute(&pool)
    .await;
    assert!(
        duplicate_draft_primary_key.is_err(),
        "group/device/sequence composite primary key must be unique"
    );
    let second_prepared = sqlx::query(
        "INSERT INTO device_sync_export_drafts \
         (group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
          manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
          operation_ids_json, operation_fingerprint) \
         VALUES ('g-legacy', 'other-device', 2, 1, X'03', X'04', 'event-2', \
                 'manifest-2', '[]', 'fingerprint-2')",
    )
    .execute(&pool)
    .await;
    assert!(
        second_prepared.is_err(),
        "only one prepared draft may exist for a group"
    );
    sqlx::query(
        "INSERT INTO device_sync_export_drafts \
         (group_id, local_device_id, sequence, key_epoch, event_envelope_bytes, \
          manifest_envelope_bytes, event_ciphertext_sha256, manifest_ciphertext_sha256, \
          operation_ids_json, operation_fingerprint, state) \
         VALUES ('g-legacy', 'other-device', 2, 1, X'03', X'04', 'event-2', \
                 'manifest-2', '[]', 'fingerprint-2', 'finalized')",
    )
    .execute(&pool)
    .await
    .expect("finalized history does not participate in one-prepared partial index");
    let draft_columns: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM pragma_table_info('device_sync_export_drafts') ORDER BY cid",
    )
    .fetch_all(&pool)
    .await
    .expect("inspect export draft column names");
    assert!(
        draft_columns
            .iter()
            .all(|column| !column.to_ascii_lowercase().contains("path")),
        "durable draft schema must never persist a NAS path"
    );

    sqlx::query("DELETE FROM device_sync_groups WHERE id = 'g-legacy'")
        .execute(&pool)
        .await
        .expect("delete synthetic group to exercise SET NULL");
    let retained_with_group: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_sync_quarantine WHERE group_id IS NOT NULL",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect quarantine foreign-key result");
    assert_eq!(retained_with_group, 0);
    let retained_drafts: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM device_sync_export_drafts WHERE group_id='g-legacy'",
    )
    .fetch_one(&pool)
    .await
    .expect("inspect export-draft cascade result");
    assert_eq!(retained_drafts, 0);
    let foreign_key_violations: Vec<(String, i64, String, i64)> =
        sqlx::query_as("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .expect("check migration 63 foreign keys");
    assert!(foreign_key_violations.is_empty());
    pool.close().await;
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
    sqlx::query("UPDATE _sqlx_migrations SET success = 0 WHERE version = 63")
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
    assert_eq!(compatibility.version, Some(63));
    assert_eq!(compatibility.reason, "failed_history_row");
    assert_failure_fingerprints_unchanged(&database, before_physical, before).await;
}
