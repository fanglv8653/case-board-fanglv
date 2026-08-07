//! V083-N0 migration-lineage fixtures.
//!
//! These tests intentionally document the v0.8.2 behavior before M1 changes
//! it. Every database is created under a fresh `tempfile::TempDir`; none of the
//! helpers resolve or inspect the application's default data directory.

use super::{init_pool, reconcile_migration_checksums, DbError};
use sha2::{Digest, Sha384};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const UNKNOWN_APPLIED_VERSION: i64 = 9_999;
const SYNTHETIC_DIVERGENT_SQL: &[u8] =
    b"CREATE TABLE synthetic_same_version_different_sql (id TEXT PRIMARY KEY);";

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

async fn object_exists(pool: &SqlitePool, object_type: &str, name: &str) -> bool {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
    )
    .bind(object_type)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("inspect sqlite_master")
        == 1
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
    let query = format!("PRAGMA table_info(\"{table}\")");
    sqlx::query(&query)
        .fetch_all(pool)
        .await
        .expect("inspect table columns")
        .iter()
        .any(|row| row.get::<String, _>("name") == column)
}

async fn foreign_key_exists(
    pool: &SqlitePool,
    table: &str,
    from: &str,
    target_table: &str,
    target_column: &str,
    on_delete: &str,
) -> bool {
    let query = format!("PRAGMA foreign_key_list(\"{table}\")");
    sqlx::query(&query)
        .fetch_all(pool)
        .await
        .expect("inspect foreign keys")
        .iter()
        .any(|row| {
            row.get::<String, _>("from") == from
                && row.get::<String, _>("table") == target_table
                && row.get::<String, _>("to") == target_column
                && row.get::<String, _>("on_delete") == on_delete
        })
}

/// Frozen M1 schema preflight contract. This is test-only by design: N0 must
/// describe the required read-only checks without changing startup behavior.
async fn missing_schema_sentinels(pool: &SqlitePool) -> Vec<&'static str> {
    let mut missing = Vec::new();

    for (code, table) in [
        ("M49.table.feishu_sync_links", "feishu_sync_links"),
        ("M49.table.feishu_sync_inbox", "feishu_sync_inbox"),
        (
            "M51.table.feishu_sync_binding_audits",
            "feishu_sync_binding_audits",
        ),
        ("M58.table.device_sync_groups", "device_sync_groups"),
        ("M58.table.device_sync_members", "device_sync_members"),
        ("M58.table.device_sync_outbox", "device_sync_outbox"),
        (
            "M58.table.device_sync_dirty_entities",
            "device_sync_dirty_entities",
        ),
        (
            "M58.table.device_sync_applied_operations",
            "device_sync_applied_operations",
        ),
        (
            "M58.table.device_sync_entity_revisions",
            "device_sync_entity_revisions",
        ),
        ("M58.table.device_sync_conflicts", "device_sync_conflicts"),
        ("M58.table.device_sync_receipts", "device_sync_receipts"),
        ("M58.table.device_sync_snapshots", "device_sync_snapshots"),
        ("M58.table.device_sync_quarantine", "device_sync_quarantine"),
        ("M58.table.device_sync_audits", "device_sync_audits"),
        (
            "M59.table.legal_skill_binding_suppressions",
            "legal_skill_binding_suppressions",
        ),
        (
            "M60.table.case_domain_status_migration_audits",
            "case_domain_status_migration_audits",
        ),
        (
            "M61.table.feishu_sync_operation_audits",
            "feishu_sync_operation_audits",
        ),
        (
            "M62.table.feishu_sync_entity_previews",
            "feishu_sync_entity_previews",
        ),
    ] {
        if !object_exists(pool, "table", table).await {
            missing.push(code);
        }
    }

    for (code, table, column) in [
        (
            "M49.column.links.entity_type",
            "feishu_sync_links",
            "entity_type",
        ),
        (
            "M49.column.links.local_entity_id",
            "feishu_sync_links",
            "local_entity_id",
        ),
        ("M49.column.links.status", "feishu_sync_links", "status"),
        ("M49.column.inbox.status", "feishu_sync_inbox", "status"),
        (
            "M49.column.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
        ),
        (
            "M51.column.inbox.auto_bind_suppressed",
            "feishu_sync_inbox",
            "auto_bind_suppressed",
        ),
        (
            "M59.column.suppression.id",
            "legal_skill_binding_suppressions",
            "id",
        ),
        (
            "M59.column.suppression.legal_domain",
            "legal_skill_binding_suppressions",
            "legal_domain",
        ),
        (
            "M59.column.suppression.task_type",
            "legal_skill_binding_suppressions",
            "task_type",
        ),
        (
            "M61.column.field_preview.review_status",
            "feishu_sync_field_previews",
            "review_status",
        ),
        (
            "M61.column.field_preview.resolution_value_json",
            "feishu_sync_field_previews",
            "resolution_value_json",
        ),
        (
            "M61.column.field_preview.resolved_at",
            "feishu_sync_field_previews",
            "resolved_at",
        ),
        (
            "M62.column.entity_preview.review_status",
            "feishu_sync_entity_previews",
            "review_status",
        ),
    ] {
        if !column_exists(pool, table, column).await {
            missing.push(code);
        }
    }

    for (code, index) in [
        (
            "M49.index.idx_feishu_sync_inbox_status",
            "idx_feishu_sync_inbox_status",
        ),
        (
            "M58.index.idx_device_sync_outbox_pending",
            "idx_device_sync_outbox_pending",
        ),
        (
            "M60.index.idx_case_domain_status_migration_audits_case",
            "idx_case_domain_status_migration_audits_case",
        ),
        (
            "M61.index.idx_feishu_sync_operation_audits_preview",
            "idx_feishu_sync_operation_audits_preview",
        ),
        (
            "M62.index.idx_feishu_sync_entity_previews_pending",
            "idx_feishu_sync_entity_previews_pending",
        ),
    ] {
        if !object_exists(pool, "index", index).await {
            missing.push(code);
        }
    }

    for (code, trigger) in [
        (
            "M58.trigger.device_sync_cases_insert",
            "device_sync_cases_insert",
        ),
        (
            "M58.trigger.device_sync_contacts_insert",
            "device_sync_contacts_insert",
        ),
        (
            "M59.trigger.device_sync_skill_binding_suppressions_insert",
            "device_sync_skill_binding_suppressions_insert",
        ),
        (
            "M59.trigger.device_sync_skill_binding_suppressions_update",
            "device_sync_skill_binding_suppressions_update",
        ),
        (
            "M59.trigger.device_sync_skill_binding_suppressions_delete",
            "device_sync_skill_binding_suppressions_delete",
        ),
        (
            "M60.trigger.case_stage_items_domain_guard_insert",
            "case_stage_items_domain_guard_insert",
        ),
        (
            "M60.trigger.case_stage_items_domain_guard_update",
            "case_stage_items_domain_guard_update",
        ),
    ] {
        if !object_exists(pool, "trigger", trigger).await {
            missing.push(code);
        }
    }

    for (code, table, from, target_table, target_column, on_delete) in [
        (
            "M49.fk.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            "M51.fk.binding_audit.inbox_id",
            "feishu_sync_binding_audits",
            "inbox_id",
            "feishu_sync_inbox",
            "id",
            "CASCADE",
        ),
        (
            "M51.fk.binding_audit.previous_case_id",
            "feishu_sync_binding_audits",
            "previous_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            "M58.fk.member.group_id",
            "device_sync_members",
            "group_id",
            "device_sync_groups",
            "id",
            "CASCADE",
        ),
        (
            "M58.fk.quarantine.group_id",
            "device_sync_quarantine",
            "group_id",
            "device_sync_groups",
            "id",
            "SET NULL",
        ),
        (
            "M61.fk.operation_audit.preview_id",
            "feishu_sync_operation_audits",
            "preview_id",
            "feishu_sync_field_previews",
            "id",
            "SET NULL",
        ),
        (
            "M62.fk.entity_preview.case_id",
            "feishu_sync_entity_previews",
            "case_id",
            "cases",
            "id",
            "CASCADE",
        ),
    ] {
        if !foreign_key_exists(pool, table, from, target_table, target_column, on_delete).await {
            missing.push(code);
        }
    }

    missing
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
    assert_eq!(missing_schema_sentinels(&pool).await, Vec::<&str>::new());
    pool.close().await;
}

#[tokio::test]
async fn current_database_reopen_keeps_migration_history_unchanged() {
    let (_directory, database) = migrated_fixture("current-reopen").await;
    let pool = fixture_pool(&database, true).await;
    let before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .expect("snapshot current migration history");
    pool.close().await;

    let reopened = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("current lineage must reopen");
    let after: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&reopened)
            .await
            .expect("snapshot reopened migration history");
    assert_eq!(after, before);
    reopened.close().await;
}

#[tokio::test]
async fn unknown_checksum_fixture_documents_unconditional_preflight_write() {
    let (_directory, database) = migrated_fixture("unknown-checksum").await;
    let pool = fixture_pool(&database, true).await;
    let current: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = 49")
            .fetch_one(&pool)
            .await
            .expect("read current synthetic checksum");
    let unknown_checksum = synthetic_divergent_checksum();
    assert_ne!(current, unknown_checksum);
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 49")
        .bind(&unknown_checksum)
        .execute(&pool)
        .await
        .expect("install synthetic unknown checksum");

    reconcile_migration_checksums(&pool)
        .await
        .expect("v0.8.2 reconciliation accepts any checksum");

    let after: Vec<u8> =
        sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = 49")
            .fetch_one(&pool)
            .await
            .expect("read reconciled checksum");
    assert_eq!(after, current, "v0.8.2 overwrites an untrusted checksum");
    assert_ne!(after, unknown_checksum);
    pool.close().await;
}

#[tokio::test]
async fn migration_49_success_without_inbox_reaches_migration_51_failure() {
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

    let missing = missing_schema_sentinels(&pool).await;
    assert!(missing.contains(&"M49.table.feishu_sync_inbox"));
    assert!(missing.contains(&"M51.column.inbox.auto_bind_suppressed"));
    pool.close().await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("v0.8.2 must reproduce the notebook migration failure");
    match error {
        DbError::Migrate(message) => {
            assert!(
                message.contains("51"),
                "unexpected migration error: {message}"
            );
            assert!(
                message.contains("feishu_sync_inbox"),
                "unexpected migration error: {message}"
            );
        }
        other => panic!("unexpected error kind: {other}"),
    }
}

#[tokio::test]
async fn unknown_applied_version_fixture_documents_ignore_missing_behavior() {
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

    let reopened = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect("v0.8.2 set_ignore_missing(true) accepts unknown applied versions");
    let retained: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM _sqlx_migrations WHERE version = ?1 AND success = 1",
    )
    .bind(UNKNOWN_APPLIED_VERSION)
    .fetch_one(&reopened)
    .await
    .expect("inspect unknown migration row");
    assert_eq!(retained, 1);
    reopened.close().await;
}

#[tokio::test]
async fn failed_migration_row_fixture_is_rejected_by_sqlx() {
    let (_directory, database) = migrated_fixture("failed-row").await;
    let pool = fixture_pool(&database, true).await;
    sqlx::query("UPDATE _sqlx_migrations SET success = 0 WHERE version = 62")
        .execute(&pool)
        .await
        .expect("mark synthetic migration row failed");
    pool.close().await;

    let error = init_pool(database.to_str().expect("UTF-8 fixture path"))
        .await
        .expect_err("a failed migration row must not be accepted");
    match error {
        DbError::Migrate(message) => assert!(
            message.contains("62") || message.to_ascii_lowercase().contains("failed"),
            "unexpected migration error: {message}"
        ),
        other => panic!("unexpected error kind: {other}"),
    }
}
