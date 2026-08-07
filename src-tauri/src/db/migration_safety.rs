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

const M63_QUARANTINE_TABLE_SQL: &str = r#"
CREATE TABLE device_sync_quarantine (
    id TEXT PRIMARY KEY NOT NULL,
    group_id TEXT,
    source_path TEXT,
    source_device_id TEXT NOT NULL,
    source_sequence INTEGER NOT NULL,
    reason_code TEXT NOT NULL,
    details_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'active'
        CHECK(status IN ('active','resolved','manual_review')),
    first_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    last_seen_at TEXT NOT NULL DEFAULT(datetime('now')),
    retry_count INTEGER NOT NULL DEFAULT 1 CHECK(retry_count >= 1),
    resolved_at TEXT,
    last_error_code TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE SET NULL
)
"#;

const M63_ACTIVE_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_quarantine_active_key
ON device_sync_quarantine(
    COALESCE(group_id,''), source_device_id, source_sequence, reason_code
)
WHERE status='active'
"#;

const M63_GROUP_STATUS_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_quarantine_group_status
ON device_sync_quarantine(group_id, status, last_seen_at DESC)
"#;

const M63_OUTBOX_CAPTURE_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_outbox_capture_sequence
ON device_sync_outbox(group_id, capture_sequence)
"#;

const M63_OUTBOX_PENDING_CAPTURE_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_outbox_pending_capture
ON device_sync_outbox(group_id, state, capture_sequence)
"#;

const M63_EXPORT_DRAFTS_TABLE_SQL: &str = r#"
CREATE TABLE device_sync_export_drafts (
    group_id TEXT NOT NULL,
    local_device_id TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK(sequence >= 1),
    key_epoch INTEGER NOT NULL CHECK(key_epoch >= 1),
    previous_manifest_hash TEXT,
    event_envelope_bytes BLOB NOT NULL,
    manifest_envelope_bytes BLOB NOT NULL,
    event_ciphertext_sha256 TEXT NOT NULL,
    manifest_ciphertext_sha256 TEXT NOT NULL,
    operation_ids_json TEXT NOT NULL,
    operation_fingerprint TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'prepared'
        CHECK(state IN ('prepared','finalized')),
    created_at TEXT NOT NULL DEFAULT(datetime('now')),
    updated_at TEXT NOT NULL DEFAULT(datetime('now')),
    finalized_at TEXT,
    PRIMARY KEY(group_id, local_device_id, sequence),
    FOREIGN KEY(group_id) REFERENCES device_sync_groups(id) ON DELETE CASCADE
)
"#;

const M63_EXPORT_DRAFTS_STATE_INDEX_SQL: &str = r#"
CREATE INDEX idx_device_sync_export_drafts_state
ON device_sync_export_drafts(group_id, local_device_id, state, sequence)
"#;

const M63_EXPORT_DRAFTS_ONE_PREPARED_INDEX_SQL: &str = r#"
CREATE UNIQUE INDEX idx_device_sync_export_drafts_one_prepared
ON device_sync_export_drafts(group_id)
WHERE state='prepared'
"#;

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
        (
            63,
            "M63.table.device_sync_export_drafts",
            "device_sync_export_drafts",
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
        (
            63,
            "M63.column.groups.last_attempt_at",
            "device_sync_groups",
            "last_attempt_at",
        ),
        (
            63,
            "M63.column.groups.last_success_at",
            "device_sync_groups",
            "last_success_at",
        ),
        (
            63,
            "M63.column.groups.auto_paused",
            "device_sync_groups",
            "auto_paused",
        ),
        (
            63,
            "M63.column.groups.pause_reason_code",
            "device_sync_groups",
            "pause_reason_code",
        ),
        (
            63,
            "M63.column.outbox.capture_sequence",
            "device_sync_outbox",
            "capture_sequence",
        ),
        (
            63,
            "M63.column.quarantine.source_device_id",
            "device_sync_quarantine",
            "source_device_id",
        ),
        (
            63,
            "M63.column.quarantine.source_sequence",
            "device_sync_quarantine",
            "source_sequence",
        ),
        (
            63,
            "M63.column.quarantine.status",
            "device_sync_quarantine",
            "status",
        ),
        (
            63,
            "M63.column.quarantine.first_seen_at",
            "device_sync_quarantine",
            "first_seen_at",
        ),
        (
            63,
            "M63.column.quarantine.last_seen_at",
            "device_sync_quarantine",
            "last_seen_at",
        ),
        (
            63,
            "M63.column.quarantine.retry_count",
            "device_sync_quarantine",
            "retry_count",
        ),
        (
            63,
            "M63.column.quarantine.resolved_at",
            "device_sync_quarantine",
            "resolved_at",
        ),
        (
            63,
            "M63.column.quarantine.last_error_code",
            "device_sync_quarantine",
            "last_error_code",
        ),
        (
            63,
            "M63.column.export_drafts.group_id",
            "device_sync_export_drafts",
            "group_id",
        ),
        (
            63,
            "M63.column.export_drafts.local_device_id",
            "device_sync_export_drafts",
            "local_device_id",
        ),
        (
            63,
            "M63.column.export_drafts.sequence",
            "device_sync_export_drafts",
            "sequence",
        ),
        (
            63,
            "M63.column.export_drafts.key_epoch",
            "device_sync_export_drafts",
            "key_epoch",
        ),
        (
            63,
            "M63.column.export_drafts.previous_manifest_hash",
            "device_sync_export_drafts",
            "previous_manifest_hash",
        ),
        (
            63,
            "M63.column.export_drafts.event_envelope_bytes",
            "device_sync_export_drafts",
            "event_envelope_bytes",
        ),
        (
            63,
            "M63.column.export_drafts.manifest_envelope_bytes",
            "device_sync_export_drafts",
            "manifest_envelope_bytes",
        ),
        (
            63,
            "M63.column.export_drafts.event_ciphertext_sha256",
            "device_sync_export_drafts",
            "event_ciphertext_sha256",
        ),
        (
            63,
            "M63.column.export_drafts.manifest_ciphertext_sha256",
            "device_sync_export_drafts",
            "manifest_ciphertext_sha256",
        ),
        (
            63,
            "M63.column.export_drafts.operation_ids_json",
            "device_sync_export_drafts",
            "operation_ids_json",
        ),
        (
            63,
            "M63.column.export_drafts.operation_fingerprint",
            "device_sync_export_drafts",
            "operation_fingerprint",
        ),
        (
            63,
            "M63.column.export_drafts.state",
            "device_sync_export_drafts",
            "state",
        ),
        (
            63,
            "M63.column.export_drafts.created_at",
            "device_sync_export_drafts",
            "created_at",
        ),
        (
            63,
            "M63.column.export_drafts.updated_at",
            "device_sync_export_drafts",
            "updated_at",
        ),
        (
            63,
            "M63.column.export_drafts.finalized_at",
            "device_sync_export_drafts",
            "finalized_at",
        ),
    ] {
        if applied_versions.contains(&version) && !column_exists(pool, table, column).await? {
            missing.push(MissingSentinel {
                migration_version: version,
                code,
            });
        }
    }

    if applied_versions.contains(&63) {
        for (code, table, column, expected_type, not_null, default_value) in [
            (
                "M63.column.groups.auto_paused.definition",
                "device_sync_groups",
                "auto_paused",
                "INTEGER",
                true,
                Some("0"),
            ),
            (
                "M63.column.outbox.capture_sequence.definition",
                "device_sync_outbox",
                "capture_sequence",
                "INTEGER",
                true,
                Some("0"),
            ),
            (
                "M63.column.quarantine.source_device_id.definition",
                "device_sync_quarantine",
                "source_device_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.quarantine.source_sequence.definition",
                "device_sync_quarantine",
                "source_sequence",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.quarantine.status.definition",
                "device_sync_quarantine",
                "status",
                "TEXT",
                true,
                Some("'active'"),
            ),
            (
                "M63.column.quarantine.retry_count.definition",
                "device_sync_quarantine",
                "retry_count",
                "INTEGER",
                true,
                Some("1"),
            ),
            (
                "M63.column.quarantine.last_error_code.definition",
                "device_sync_quarantine",
                "last_error_code",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.group_id.definition",
                "device_sync_export_drafts",
                "group_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.local_device_id.definition",
                "device_sync_export_drafts",
                "local_device_id",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.sequence.definition",
                "device_sync_export_drafts",
                "sequence",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.key_epoch.definition",
                "device_sync_export_drafts",
                "key_epoch",
                "INTEGER",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.previous_manifest_hash.definition",
                "device_sync_export_drafts",
                "previous_manifest_hash",
                "TEXT",
                false,
                None,
            ),
            (
                "M63.column.export_drafts.event_envelope_bytes.definition",
                "device_sync_export_drafts",
                "event_envelope_bytes",
                "BLOB",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.manifest_envelope_bytes.definition",
                "device_sync_export_drafts",
                "manifest_envelope_bytes",
                "BLOB",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.event_ciphertext_sha256.definition",
                "device_sync_export_drafts",
                "event_ciphertext_sha256",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.manifest_ciphertext_sha256.definition",
                "device_sync_export_drafts",
                "manifest_ciphertext_sha256",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.operation_ids_json.definition",
                "device_sync_export_drafts",
                "operation_ids_json",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.operation_fingerprint.definition",
                "device_sync_export_drafts",
                "operation_fingerprint",
                "TEXT",
                true,
                None,
            ),
            (
                "M63.column.export_drafts.state.definition",
                "device_sync_export_drafts",
                "state",
                "TEXT",
                true,
                Some("'prepared'"),
            ),
            (
                "M63.column.export_drafts.created_at.definition",
                "device_sync_export_drafts",
                "created_at",
                "TEXT",
                true,
                Some("datetime('now')"),
            ),
            (
                "M63.column.export_drafts.updated_at.definition",
                "device_sync_export_drafts",
                "updated_at",
                "TEXT",
                true,
                Some("datetime('now')"),
            ),
            (
                "M63.column.export_drafts.finalized_at.definition",
                "device_sync_export_drafts",
                "finalized_at",
                "TEXT",
                false,
                None,
            ),
        ] {
            if !column_definition_matches(
                pool,
                table,
                column,
                expected_type,
                not_null,
                default_value,
            )
            .await?
            {
                missing.push(MissingSentinel {
                    migration_version: 63,
                    code,
                });
            }
        }

        if !schema_object_sql_matches(
            pool,
            "table",
            "device_sync_quarantine",
            "device_sync_quarantine",
            M63_QUARANTINE_TABLE_SQL,
        )
        .await?
        {
            missing.push(MissingSentinel {
                migration_version: 63,
                code: "M63.table.device_sync_quarantine.definition",
            });
        }

        if !schema_object_sql_matches(
            pool,
            "table",
            "device_sync_export_drafts",
            "device_sync_export_drafts",
            M63_EXPORT_DRAFTS_TABLE_SQL,
        )
        .await?
        {
            missing.push(MissingSentinel {
                migration_version: 63,
                code: "M63.table.device_sync_export_drafts.definition",
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

    if applied_versions.contains(&63) {
        for (code, table, index, unique, partial, expected_sql) in [
            (
                "M63.index.idx_device_sync_quarantine_active_key",
                "device_sync_quarantine",
                "idx_device_sync_quarantine_active_key",
                true,
                true,
                M63_ACTIVE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_quarantine_group_status",
                "device_sync_quarantine",
                "idx_device_sync_quarantine_group_status",
                false,
                false,
                M63_GROUP_STATUS_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_outbox_capture_sequence",
                "device_sync_outbox",
                "idx_device_sync_outbox_capture_sequence",
                true,
                false,
                M63_OUTBOX_CAPTURE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_outbox_pending_capture",
                "device_sync_outbox",
                "idx_device_sync_outbox_pending_capture",
                false,
                false,
                M63_OUTBOX_PENDING_CAPTURE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_export_drafts_state",
                "device_sync_export_drafts",
                "idx_device_sync_export_drafts_state",
                false,
                false,
                M63_EXPORT_DRAFTS_STATE_INDEX_SQL,
            ),
            (
                "M63.index.idx_device_sync_export_drafts_one_prepared",
                "device_sync_export_drafts",
                "idx_device_sync_export_drafts_one_prepared",
                true,
                true,
                M63_EXPORT_DRAFTS_ONE_PREPARED_INDEX_SQL,
            ),
        ] {
            if !index_definition_matches(pool, table, index, unique, partial, expected_sql).await? {
                missing.push(MissingSentinel {
                    migration_version: 63,
                    code,
                });
            }
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
        (
            63,
            "M63.fk.export_drafts.group_id",
            "device_sync_export_drafts",
            "group_id",
            "device_sync_groups",
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

    if applied_versions.contains(&58) {
        let (version, code) = if applied_versions.contains(&63) {
            (63, "M63.fk.quarantine.group_id")
        } else {
            (58, "M58.fk.quarantine.group_id")
        };
        if !foreign_key_exists(
            pool,
            "device_sync_quarantine",
            "group_id",
            "device_sync_groups",
            "id",
            "SET NULL",
        )
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

async fn column_definition_matches(
    pool: &SqlitePool,
    table: &str,
    column: &str,
    expected_type: &str,
    expected_not_null: bool,
    expected_default: Option<&str>,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA table_info(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    for row in rows {
        let name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if name != column {
            continue;
        }
        let data_type: String = row.try_get("type").map_err(schema_metadata_unreadable)?;
        let not_null: i64 = row.try_get("notnull").map_err(schema_metadata_unreadable)?;
        let default_value: Option<String> = row
            .try_get("dflt_value")
            .map_err(schema_metadata_unreadable)?;
        return Ok(data_type.eq_ignore_ascii_case(expected_type)
            && (not_null == 1) == expected_not_null
            && default_value.as_deref().map(normalize_schema_sql)
                == expected_default.map(normalize_schema_sql));
    }
    Ok(false)
}

async fn index_definition_matches(
    pool: &SqlitePool,
    table: &str,
    index: &str,
    expected_unique: bool,
    expected_partial: bool,
    expected_sql: &str,
) -> Result<bool, DbError> {
    let query = format!("PRAGMA index_list(\"{table}\")");
    let rows = sqlx::query(&query)
        .fetch_all(pool)
        .await
        .map_err(schema_metadata_unreadable)?;
    let mut flags_match = false;
    for row in rows {
        let row_name: String = row.try_get("name").map_err(schema_metadata_unreadable)?;
        if row_name != index {
            continue;
        }
        let unique: i64 = row.try_get("unique").map_err(schema_metadata_unreadable)?;
        let partial: i64 = row.try_get("partial").map_err(schema_metadata_unreadable)?;
        flags_match = (unique == 1) == expected_unique && (partial == 1) == expected_partial;
        break;
    }
    if !flags_match {
        return Ok(false);
    }

    schema_object_sql_matches(pool, "index", index, table, expected_sql).await
}

async fn schema_object_sql_matches(
    pool: &SqlitePool,
    object_type: &str,
    name: &str,
    table: &str,
    expected_sql: &str,
) -> Result<bool, DbError> {
    let sql: Option<String> = sqlx::query_scalar(
        "SELECT COALESCE(sql, '') FROM sqlite_master \
         WHERE type = ?1 AND name = ?2 AND tbl_name = ?3",
    )
    .bind(object_type)
    .bind(name)
    .bind(table)
    .fetch_optional(pool)
    .await
    .map_err(schema_metadata_unreadable)?;
    let Some(sql) = sql else {
        return Ok(false);
    };
    Ok(normalize_schema_sql(&sql) == normalize_schema_sql(expected_sql))
}

fn normalize_schema_sql(sql: &str) -> String {
    sql.chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase()
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
