//! Read-only migration-lineage preflight. Existing databases must pass this
//! module before a read-write/WAL pool is created.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use super::{
    DbError, DbMigrationCompatibilityError, DB_MIGRATION_APPLIED_VERSION_UNKNOWN,
    DB_MIGRATION_CHECKSUM_UNKNOWN, DB_MIGRATION_LINEAGE_INCOMPATIBLE,
    DB_MIGRATION_SCHEMA_SENTINEL_MISSING,
};

#[derive(Debug)]
struct MissingSentinel {
    migration_version: i64,
    code: &'static str,
}

fn compatibility_error(
    code: &'static str,
    version: Option<i64>,
    reason: &'static str,
    stored_checksum: Option<&[u8]>,
    current_checksum: Option<&[u8]>,
    missing_sentinels: Vec<String>,
) -> DbError {
    let error = DbMigrationCompatibilityError {
        code,
        version,
        reason,
        stored_checksum: stored_checksum.map(checksum_hex),
        current_checksum: current_checksum.map(checksum_hex),
        missing_sentinels,
    };
    crate::dlog!(
        "[db] migration preflight blocked code={} version={:?} reason={} missing_sentinels={:?}",
        error.code,
        error.version,
        error.reason,
        error.missing_sentinels
    );
    DbError::MigrationCompatibility(error)
}

fn checksum_hex(checksum: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(checksum.len() * 2);
    for byte in checksum {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn schema_metadata_unreadable(_error: sqlx::Error) -> DbError {
    compatibility_error(
        DB_MIGRATION_LINEAGE_INCOMPATIBLE,
        None,
        "schema_metadata_unreadable",
        None,
        None,
        Vec::new(),
    )
}

pub(crate) async fn preflight_existing_database(database_path: &Path) -> Result<(), DbError> {
    // SQLite's immutable mode does not reliably expose committed content that
    // exists only in a WAL. Refuse every sidecar shape before the first SQLite
    // connection; recovery/checkpointing must happen on an isolated copy.
    ensure_no_wal_sidecars(database_path)?;

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .read_only(true)
        .immutable(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| DbError::Connect(format!("数据库只读预检连接失败: {error}")))?;

    let result = preflight_pool(&pool).await;
    pool.close().await;

    // Detect a sidecar that appeared during preflight. Sidecar recovery has
    // higher priority than a classification made from the immutable main DB.
    ensure_no_wal_sidecars(database_path)?;
    result
}

fn sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut value = database_path.as_os_str().to_os_string();
    value.push(suffix);
    PathBuf::from(value)
}

pub(crate) fn ensure_no_wal_sidecars(database_path: &Path) -> Result<(), DbError> {
    let sidecar_present_or_unreadable = ["-wal", "-shm"].iter().any(|suffix| {
        sidecar_path(database_path, suffix)
            .try_exists()
            .unwrap_or(true)
    });
    if sidecar_present_or_unreadable {
        return Err(compatibility_error(
            DB_MIGRATION_LINEAGE_INCOMPATIBLE,
            None,
            "wal_sidecar_present_requires_recovery",
            None,
            None,
            Vec::new(),
        ));
    }
    Ok(())
}

async fn preflight_pool(pool: &SqlitePool) -> Result<(), DbError> {
    if !object_exists(pool, "table", "_sqlx_migrations").await? {
        if has_user_schema_objects_other_than_migration_table(pool).await? {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                None,
                "migration_history_missing_for_existing_schema",
                None,
                None,
                Vec::new(),
            ));
        }
        return Ok(());
    }

    let history: Vec<(i64, String, bool, Vec<u8>)> = sqlx::query_as(
        "SELECT version, description, success, checksum \
         FROM _sqlx_migrations ORDER BY version",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| {
        compatibility_error(
            DB_MIGRATION_LINEAGE_INCOMPATIBLE,
            None,
            "migration_history_unreadable",
            None,
            None,
            Vec::new(),
        )
    })?;

    if history.is_empty() {
        if has_user_schema_objects_other_than_migration_table(pool).await? {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                None,
                "migration_history_empty_for_existing_schema",
                None,
                None,
                Vec::new(),
            ));
        }
        return Ok(());
    }

    let embedded_migrator = sqlx::migrate!("./migrations");
    let embedded_by_version: HashMap<i64, _> = embedded_migrator
        .iter()
        .map(|migration| (migration.version, migration))
        .collect();

    for (version, _description, success, checksum) in &history {
        if !success {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                Some(*version),
                "failed_history_row",
                Some(checksum),
                embedded_by_version
                    .get(version)
                    .map(|migration| migration.checksum.as_ref()),
                Vec::new(),
            ));
        }
        if !embedded_by_version.contains_key(version) {
            return Err(compatibility_error(
                DB_MIGRATION_APPLIED_VERSION_UNKNOWN,
                Some(*version),
                "applied_version_not_embedded",
                Some(checksum),
                None,
                Vec::new(),
            ));
        }
    }

    let applied_versions: HashSet<i64> = history.iter().map(|row| row.0).collect();
    if let Some(max_applied_version) = applied_versions.iter().max().copied() {
        if let Some(missing) = embedded_migrator.iter().find(|migration| {
            migration.version <= max_applied_version
                && !applied_versions.contains(&migration.version)
        }) {
            return Err(compatibility_error(
                DB_MIGRATION_LINEAGE_INCOMPATIBLE,
                Some(missing.version),
                "applied_history_gap",
                None,
                Some(missing.checksum.as_ref()),
                Vec::new(),
            ));
        }
    }
    let missing_sentinels = collect_missing_sentinels(pool, &applied_versions).await?;

    // Freeze combination priority: a proven schema defect is more actionable
    // than a checksum mismatch and must not be hidden by it.
    if !missing_sentinels.is_empty() {
        let version = missing_sentinels
            .iter()
            .map(|missing| missing.migration_version)
            .min();
        return Err(compatibility_error(
            DB_MIGRATION_SCHEMA_SENTINEL_MISSING,
            version,
            "applied_migration_schema_missing",
            None,
            None,
            missing_sentinels
                .into_iter()
                .map(|missing| missing.code.to_string())
                .collect(),
        ));
    }

    for (version, _description, _success, stored_checksum) in &history {
        let embedded = embedded_by_version
            .get(version)
            .expect("unknown versions were rejected above");
        let current_checksum = embedded.checksum.as_ref();
        if stored_checksum.as_slice() == current_checksum {
            continue;
        }

        return Err(compatibility_error(
            DB_MIGRATION_CHECKSUM_UNKNOWN,
            Some(*version),
            "checksum_not_allowlisted",
            Some(stored_checksum),
            Some(current_checksum),
            Vec::new(),
        ));
    }

    Ok(())
}

async fn collect_missing_sentinels(
    pool: &SqlitePool,
    applied_versions: &HashSet<i64>,
) -> Result<Vec<MissingSentinel>, DbError> {
    let mut missing = Vec::new();

    for (version, code, table) in [
        (49, "M49.table.feishu_sync_links", "feishu_sync_links"),
        (49, "M49.table.feishu_sync_inbox", "feishu_sync_inbox"),
        (
            51,
            "M51.table.feishu_sync_binding_audits",
            "feishu_sync_binding_audits",
        ),
        (58, "M58.table.device_sync_groups", "device_sync_groups"),
        (58, "M58.table.device_sync_members", "device_sync_members"),
        (58, "M58.table.device_sync_outbox", "device_sync_outbox"),
        (
            58,
            "M58.table.device_sync_dirty_entities",
            "device_sync_dirty_entities",
        ),
        (
            58,
            "M58.table.device_sync_applied_operations",
            "device_sync_applied_operations",
        ),
        (
            58,
            "M58.table.device_sync_entity_revisions",
            "device_sync_entity_revisions",
        ),
        (
            58,
            "M58.table.device_sync_conflicts",
            "device_sync_conflicts",
        ),
        (58, "M58.table.device_sync_receipts", "device_sync_receipts"),
        (
            58,
            "M58.table.device_sync_snapshots",
            "device_sync_snapshots",
        ),
        (
            58,
            "M58.table.device_sync_quarantine",
            "device_sync_quarantine",
        ),
        (58, "M58.table.device_sync_audits", "device_sync_audits"),
        (
            59,
            "M59.table.legal_skill_binding_suppressions",
            "legal_skill_binding_suppressions",
        ),
        (
            60,
            "M60.table.case_domain_status_migration_audits",
            "case_domain_status_migration_audits",
        ),
        (
            61,
            "M61.table.feishu_sync_operation_audits",
            "feishu_sync_operation_audits",
        ),
        (
            62,
            "M62.table.feishu_sync_entity_previews",
            "feishu_sync_entity_previews",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "table", table).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, table, column) in [
        (
            49,
            "M49.column.links.entity_type",
            "feishu_sync_links",
            "entity_type",
        ),
        (
            49,
            "M49.column.links.local_entity_id",
            "feishu_sync_links",
            "local_entity_id",
        ),
        (49, "M49.column.links.status", "feishu_sync_links", "status"),
        (49, "M49.column.inbox.status", "feishu_sync_inbox", "status"),
        (
            49,
            "M49.column.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
        ),
        (
            51,
            "M51.column.inbox.auto_bind_suppressed",
            "feishu_sync_inbox",
            "auto_bind_suppressed",
        ),
        (
            59,
            "M59.column.suppression.id",
            "legal_skill_binding_suppressions",
            "id",
        ),
        (
            59,
            "M59.column.suppression.legal_domain",
            "legal_skill_binding_suppressions",
            "legal_domain",
        ),
        (
            59,
            "M59.column.suppression.task_type",
            "legal_skill_binding_suppressions",
            "task_type",
        ),
        (
            61,
            "M61.column.field_preview.review_status",
            "feishu_sync_field_previews",
            "review_status",
        ),
        (
            61,
            "M61.column.field_preview.resolution_value_json",
            "feishu_sync_field_previews",
            "resolution_value_json",
        ),
        (
            61,
            "M61.column.field_preview.resolved_at",
            "feishu_sync_field_previews",
            "resolved_at",
        ),
        (
            62,
            "M62.column.entity_preview.review_status",
            "feishu_sync_entity_previews",
            "review_status",
        ),
    ] {
        if applied_versions.contains(&version) && !column_exists(pool, table, column).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, index) in [
        (
            49,
            "M49.index.idx_feishu_sync_inbox_status",
            "idx_feishu_sync_inbox_status",
        ),
        (
            58,
            "M58.index.idx_device_sync_outbox_pending",
            "idx_device_sync_outbox_pending",
        ),
        (
            60,
            "M60.index.idx_case_domain_status_migration_audits_case",
            "idx_case_domain_status_migration_audits_case",
        ),
        (
            61,
            "M61.index.idx_feishu_sync_operation_audits_preview",
            "idx_feishu_sync_operation_audits_preview",
        ),
        (
            62,
            "M62.index.idx_feishu_sync_entity_previews_pending",
            "idx_feishu_sync_entity_previews_pending",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "index", index).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, trigger) in [
        (
            58,
            "M58.trigger.device_sync_cases_insert",
            "device_sync_cases_insert",
        ),
        (
            58,
            "M58.trigger.device_sync_contacts_insert",
            "device_sync_contacts_insert",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_insert",
            "device_sync_skill_binding_suppressions_insert",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_update",
            "device_sync_skill_binding_suppressions_update",
        ),
        (
            59,
            "M59.trigger.device_sync_skill_binding_suppressions_delete",
            "device_sync_skill_binding_suppressions_delete",
        ),
        (
            60,
            "M60.trigger.case_stage_items_domain_guard_insert",
            "case_stage_items_domain_guard_insert",
        ),
        (
            60,
            "M60.trigger.case_stage_items_domain_guard_update",
            "case_stage_items_domain_guard_update",
        ),
    ] {
        if applied_versions.contains(&version) && !object_exists(pool, "trigger", trigger).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    for (version, code, table, from, target_table, target_column, on_delete) in [
        (
            49,
            "M49.fk.inbox.bound_case_id",
            "feishu_sync_inbox",
            "bound_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            51,
            "M51.fk.binding_audit.inbox_id",
            "feishu_sync_binding_audits",
            "inbox_id",
            "feishu_sync_inbox",
            "id",
            "CASCADE",
        ),
        (
            51,
            "M51.fk.binding_audit.previous_case_id",
            "feishu_sync_binding_audits",
            "previous_case_id",
            "cases",
            "id",
            "SET NULL",
        ),
        (
            58,
            "M58.fk.member.group_id",
            "device_sync_members",
            "group_id",
            "device_sync_groups",
            "id",
            "CASCADE",
        ),
        (
            58,
            "M58.fk.quarantine.group_id",
            "device_sync_quarantine",
            "group_id",
            "device_sync_groups",
            "id",
            "SET NULL",
        ),
        (
            61,
            "M61.fk.operation_audit.preview_id",
            "feishu_sync_operation_audits",
            "preview_id",
            "feishu_sync_field_previews",
            "id",
            "SET NULL",
        ),
        (
            62,
            "M62.fk.entity_preview.case_id",
            "feishu_sync_entity_previews",
            "case_id",
            "cases",
            "id",
            "CASCADE",
        ),
    ] {
        if applied_versions.contains(&version)
            && !foreign_key_exists(pool, table, from, target_table, target_column, on_delete)
                .await?
        {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    Ok(missing)
}

async fn object_exists(pool: &SqlitePool, object_type: &str, name: &str) -> Result<bool, DbError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = ?1 AND name = ?2)",
    )
    .bind(object_type)
    .bind(name)
    .fetch_one(pool)
    .await
    .map(|exists| exists == 1)
    .map_err(schema_metadata_unreadable)
}

async fn has_user_schema_objects_other_than_migration_table(
    pool: &SqlitePool,
) -> Result<bool, DbError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(\
            SELECT 1 FROM sqlite_master \
            WHERE type IN ('table', 'view', 'trigger', 'index') \
              AND name NOT GLOB 'sqlite_*' \
              AND name <> '_sqlx_migrations'\
        )",
    )
    .fetch_one(pool)
    .await
    .map(|exists| exists == 1)
    .map_err(schema_metadata_unreadable)
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, DbError> {
    let query = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

async fn foreign_key_exists(
    pool: &SqlitePool,
    table: &str,
    from: &str,
    target_table: &str,
    target_column: &str,
    on_delete: &str,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA foreign_key_list(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let row_from: String = row.try_get("from").map_err(schema_metadata_unreadable)?;
        let row_table: String = row.try_get("table").map_err(schema_metadata_unreadable)?;
        let row_to: String = row.try_get("to").map_err(schema_metadata_unreadable)?;
        let row_on_delete: String = row
            .try_get("on_delete")
            .map_err(schema_metadata_unreadable)?;
        if row_from == from
            && row_table == target_table
            && row_to == target_column
            && row_on_delete == on_delete
        {
            return Ok(true);
        }
    }
    Ok(false)
}
