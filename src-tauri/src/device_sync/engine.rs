use std::path::PathBuf;
use std::sync::OnceLock;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use super::capture::{capture_dirty_entities, ensure_initial_baseline};
use super::crypto::{open, seal, EnvelopeHeader, PROTOCOL_VERSION};
use super::identity::{load_group_key, load_signing_secret};
use super::manifest::SyncManifest;
use super::nas_folder::MountedFolder;
use super::operations::{apply_incoming, ApplyOutcome, OperationAction, SyncOperation};
use super::{SyncError, SyncStatus};

const MAX_OPERATIONS_PER_EVENT: usize = 500;
static SYNC_RUN_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRunResult {
    pub exported_operations: usize,
    pub imported_operations: usize,
    pub conflicts_created: usize,
    pub duplicate_operations: usize,
    pub quarantined_packages: usize,
}

#[derive(Debug, FromRow)]
struct GroupRow {
    id: String,
    connector_root: String,
    local_device_id: String,
    key_epoch: i64,
    next_sequence: i64,
    paused: i64,
    last_manifest_hash: Option<String>,
}

#[derive(Debug, FromRow)]
struct OutboxRow {
    operation_id: String,
    entity_type: String,
    entity_id: String,
    case_id: Option<String>,
    action: String,
    base_revision: i64,
    changed_fields_json: String,
    base_field_hashes_json: String,
    atomic_group: Option<String>,
    author_device_id: String,
    logical_time: i64,
    schema_version: i64,
}

#[derive(Debug, FromRow)]
struct MemberRow {
    device_id: String,
    signing_public_key: String,
    status: String,
    last_seen_sequence: i64,
    last_manifest_hash: Option<String>,
}

pub async fn sync_once(pool: &SqlitePool, group_id: &str) -> Result<SyncRunResult, SyncError> {
    let lock = SYNC_RUN_LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.try_lock().map_err(|_| SyncError::Busy)?;
    sync_once_inner(pool, group_id).await
}

async fn sync_once_inner(pool: &SqlitePool, group_id: &str) -> Result<SyncRunResult, SyncError> {
    let mut group = load_group(pool, group_id).await?;
    if group.paused != 0 {
        return Err(SyncError::Paused);
    }
    let folder = MountedFolder::connect(PathBuf::from(&group.connector_root))?;
    folder.initialize_group(group_id)?;
    if super::pairing::accept_pending_key_rotation(pool, group_id)
        .await?
        .is_some()
    {
        group = load_group(pool, group_id).await?;
    }

    ensure_initial_baseline(pool, group_id).await?;
    capture_dirty_entities(pool, group_id).await?;
    let exported_operations = export_pending(pool, &folder, &group).await?;
    let mut imported_operations = 0;
    let mut conflicts_created = 0;
    let mut duplicate_operations = 0;
    let mut quarantined_packages = 0;

    let members: Vec<MemberRow> = sqlx::query_as(
        "SELECT device_id, signing_public_key, status, last_seen_sequence,
                last_manifest_hash
         FROM device_sync_members
         WHERE group_id=?1 AND device_id<>?2",
    )
    .bind(group_id)
    .bind(&group.local_device_id)
    .fetch_all(pool)
    .await?;
    for mut member in members {
        if member.status != "trusted" {
            continue;
        }
        let events = folder.list_events_after(
            group_id,
            &member.device_id,
            member.last_seen_sequence as u64,
        )?;
        let mut expected = member.last_seen_sequence as u64 + 1;
        for (sequence, path) in events {
            if sequence != expected {
                quarantine(
                    pool,
                    group_id,
                    Some(path.to_string_lossy().as_ref()),
                    "SEQUENCE_GAP",
                    serde_json::json!({"expected": expected, "actual": sequence}),
                )
                .await?;
                quarantined_packages += 1;
                break;
            }
            match import_event(pool, &folder, &group, &member, sequence, &path).await {
                Ok((outcomes, manifest_hash)) => {
                    imported_operations += outcomes.len();
                    for outcome in outcomes {
                        conflicts_created += outcome.conflict_fields.len();
                        duplicate_operations += usize::from(outcome.duplicate);
                    }
                    member.last_manifest_hash = Some(manifest_hash);
                    expected += 1;
                }
                Err(error) => {
                    quarantine(
                        pool,
                        group_id,
                        Some(path.to_string_lossy().as_ref()),
                        error.code(),
                        serde_json::json!({"error": error.to_string()}),
                    )
                    .await?;
                    quarantined_packages += 1;
                    break;
                }
            }
        }
    }
    sqlx::query(
        "UPDATE device_sync_groups SET last_synced_at=datetime('now'), updated_at=datetime('now')
         WHERE id=?1",
    )
    .bind(group_id)
    .execute(pool)
    .await?;
    let daily_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM device_sync_snapshots
             WHERE group_id=?1 AND snapshot_kind='daily'
               AND date(created_at)=date('now')
         )",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    if daily_exists == 0 {
        super::snapshot::create_encrypted_snapshot(pool, group_id, "daily").await?;
    }
    let monthly_exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM device_sync_snapshots
             WHERE group_id=?1 AND snapshot_kind='monthly'
               AND strftime('%Y-%m',created_at)=strftime('%Y-%m','now')
         )",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    if monthly_exists == 0 {
        super::snapshot::create_encrypted_snapshot(pool, group_id, "monthly").await?;
    }
    audit(
        pool,
        Some(group_id),
        Some(&group.local_device_id),
        "sync_once",
        "succeeded",
        serde_json::json!({
            "exported": exported_operations,
            "imported": imported_operations,
            "conflicts": conflicts_created,
            "duplicates": duplicate_operations,
            "quarantined": quarantined_packages
        }),
    )
    .await?;
    Ok(SyncRunResult {
        exported_operations,
        imported_operations,
        conflicts_created,
        duplicate_operations,
        quarantined_packages,
    })
}

pub async fn get_status(pool: &SqlitePool, group_id: &str) -> Result<SyncStatus, SyncError> {
    let group = load_group(pool, group_id).await?;
    let pending_upload: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_outbox WHERE group_id=?1 AND state='pending'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let conflicts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_conflicts WHERE group_id=?1 AND status='pending'",
    )
    .bind(group_id)
    .fetch_one(pool)
    .await?;
    let quarantined: i64 =
        sqlx::query_scalar("SELECT count(*) FROM device_sync_quarantine WHERE group_id=?1")
            .bind(group_id)
            .fetch_one(pool)
            .await?;
    Ok(SyncStatus {
        group_id: group.id,
        connector_root: group.connector_root,
        local_device_id: group.local_device_id,
        key_epoch: group.key_epoch as u32,
        paused: group.paused != 0,
        pending_upload: pending_upload as u64,
        conflicts: conflicts as u64,
        quarantined: quarantined as u64,
    })
}

async fn export_pending(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group: &GroupRow,
) -> Result<usize, SyncError> {
    let rows: Vec<OutboxRow> = sqlx::query_as(
        "SELECT operation_id, entity_type, entity_id, case_id, action, base_revision,
                changed_fields_json, base_field_hashes_json, atomic_group,
                author_device_id, logical_time, schema_version
         FROM device_sync_outbox
         WHERE group_id=?1 AND state='pending'
         ORDER BY logical_time, operation_id
         LIMIT 500",
    )
    .bind(&group.id)
    .fetch_all(pool)
    .await?;
    if rows.is_empty() {
        return Ok(0);
    }
    let operations = rows
        .iter()
        .map(|row| {
            Ok(SyncOperation {
                operation_id: row.operation_id.clone(),
                entity_type: row.entity_type.clone(),
                entity_id: row.entity_id.clone(),
                case_id: row.case_id.clone(),
                action: match row.action.as_str() {
                    "upsert" => OperationAction::Upsert,
                    "tombstone" => OperationAction::Tombstone,
                    other => return Err(SyncError::Protocol(format!("未知操作类型: {other}"))),
                },
                base_revision: row.base_revision,
                changed_fields: serde_json::from_str(&row.changed_fields_json)?,
                base_field_hashes: serde_json::from_str(&row.base_field_hashes_json)?,
                atomic_group: row.atomic_group.clone(),
                author_device_id: row.author_device_id.clone(),
                logical_time: row.logical_time,
                schema_version: row.schema_version as u32,
            })
        })
        .collect::<Result<Vec<_>, SyncError>>()?;
    let plaintext = serde_json::to_vec(&operations)?;
    let group_key = load_group_key(&group.id, &group.local_device_id, group.key_epoch as u32)?;
    let signing_secret = load_signing_secret(&group.id, &group.local_device_id)?;
    let sequence = group.next_sequence as u64;
    let envelope = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group.id.clone(),
            device_id: group.local_device_id.clone(),
            sequence,
            key_epoch: group.key_epoch as u32,
            payload_kind: "operations".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        &plaintext,
        &group_key,
        &signing_secret,
    )?;
    folder.write_event(&group.id, &group.local_device_id, sequence, &envelope)?;
    let manifest = SyncManifest {
        group_id: group.id.clone(),
        device_id: group.local_device_id.clone(),
        sequence,
        event_ciphertext_sha256: envelope.ciphertext_sha256.clone(),
        previous_manifest_hash: group.last_manifest_hash.clone(),
        generated_at: Utc::now().to_rfc3339(),
    };
    let manifest_envelope = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group.id.clone(),
            device_id: group.local_device_id.clone(),
            sequence,
            key_epoch: group.key_epoch as u32,
            payload_kind: "manifest".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        &serde_json::to_vec(&manifest)?,
        &group_key,
        &signing_secret,
    )?;
    folder.write_manifest(
        &group.id,
        &group.local_device_id,
        sequence,
        &manifest_envelope,
    )?;

    let mut tx = pool.begin().await?;
    let advanced = sqlx::query(
        "UPDATE device_sync_groups
         SET next_sequence=next_sequence+1, last_manifest_hash=?3, updated_at=datetime('now')
         WHERE id=?1 AND next_sequence=?2",
    )
    .bind(&group.id)
    .bind(group.next_sequence)
    .bind(&manifest_envelope.ciphertext_sha256)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if advanced != 1 {
        return Err(SyncError::Busy);
    }
    for row in &rows {
        sqlx::query(
            "UPDATE device_sync_outbox
             SET state='exported', exported_sequence=?1, updated_at=datetime('now')
             WHERE operation_id=?2 AND state='pending'",
        )
        .bind(group.next_sequence)
        .bind(&row.operation_id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(rows.len())
}

async fn import_event(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group: &GroupRow,
    member: &MemberRow,
    sequence: u64,
    path: &std::path::Path,
) -> Result<(Vec<ApplyOutcome>, String), SyncError> {
    let envelope = folder.read_envelope(path)?;
    if envelope.header.group_id != group.id
        || envelope.header.device_id != member.device_id
        || envelope.header.sequence != sequence
        || envelope.header.key_epoch == 0
        || envelope.header.key_epoch as i64 > group.key_epoch
        || envelope.header.payload_kind != "operations"
    {
        return Err(SyncError::Integrity(
            "信封头与成员、组、序号或密钥时代不一致".to_string(),
        ));
    }
    let key = load_group_key(&group.id, &group.local_device_id, envelope.header.key_epoch)?;
    let manifest_path = folder.manifest_path(&group.id, &member.device_id, sequence)?;
    let manifest_envelope = folder.read_envelope(&manifest_path)?;
    if manifest_envelope.header.group_id != group.id
        || manifest_envelope.header.device_id != member.device_id
        || manifest_envelope.header.sequence != sequence
        || manifest_envelope.header.key_epoch != envelope.header.key_epoch
        || manifest_envelope.header.payload_kind != "manifest"
    {
        return Err(SyncError::Integrity(
            "加密 manifest 头与事件不一致".to_string(),
        ));
    }
    let manifest_plaintext = open(&manifest_envelope, &key, &member.signing_public_key)?;
    let manifest: SyncManifest = serde_json::from_slice(&manifest_plaintext)?;
    if manifest.group_id != group.id
        || manifest.device_id != member.device_id
        || manifest.sequence != sequence
        || manifest.event_ciphertext_sha256 != envelope.ciphertext_sha256
        || manifest.previous_manifest_hash != member.last_manifest_hash
    {
        return Err(SyncError::Integrity(
            "manifest 链回退、分叉或事件哈希不匹配".to_string(),
        ));
    }
    let plaintext = open(&envelope, &key, &member.signing_public_key)?;
    let operations: Vec<SyncOperation> = serde_json::from_slice(&plaintext)?;
    if operations.len() > MAX_OPERATIONS_PER_EVENT {
        pause_for_fuse(pool, &group.id, "EVENT_OVER_500").await?;
        return Err(SyncError::FuseTriggered(
            "单个事件超过 500 项变化".to_string(),
        ));
    }
    let entity_total: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM device_sync_entity_revisions
         WHERE group_id=?1 AND tombstoned=0",
    )
    .bind(&group.id)
    .fetch_one(pool)
    .await?;
    let tombstones = operations
        .iter()
        .filter(|operation| operation.action == OperationAction::Tombstone)
        .count() as i64;
    if entity_total >= 20 && tombstones * 5 > entity_total {
        pause_for_fuse(pool, &group.id, "TOMBSTONE_OVER_20_PERCENT").await?;
        return Err(SyncError::FuseTriggered(
            "单轮删除超过当前同步实体的 20%".to_string(),
        ));
    }
    if entity_total >= 20 && (operations.len() as i64) * 5 > entity_total {
        pause_for_fuse(pool, &group.id, "CHANGES_OVER_20_PERCENT").await?;
        return Err(SyncError::FuseTriggered(
            "单轮修改超过当前同步实体的 20%".to_string(),
        ));
    }

    let mut tx = pool.begin().await?;
    let mut outcomes = Vec::new();
    for operation in &operations {
        outcomes.push(
            apply_incoming(
                &mut tx,
                &group.id,
                &member.device_id,
                sequence,
                operation,
                &envelope.ciphertext_sha256,
            )
            .await?,
        );
    }
    sqlx::query(
        "UPDATE device_sync_members
         SET last_seen_sequence=?1, last_manifest_hash=?4, updated_at=datetime('now')
         WHERE group_id=?2 AND device_id=?3 AND last_seen_sequence<?1",
    )
    .bind(sequence as i64)
    .bind(&group.id)
    .bind(&member.device_id)
    .bind(&manifest_envelope.ciphertext_sha256)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok((outcomes, manifest_envelope.ciphertext_sha256))
}

async fn load_group(pool: &SqlitePool, group_id: &str) -> Result<GroupRow, SyncError> {
    sqlx::query_as(
        "SELECT id, connector_root, local_device_id, key_epoch, next_sequence, paused,
                last_manifest_hash
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))
}

async fn pause_for_fuse(pool: &SqlitePool, group_id: &str, reason: &str) -> Result<(), SyncError> {
    sqlx::query("UPDATE device_sync_groups SET paused=1, updated_at=datetime('now') WHERE id=?1")
        .bind(group_id)
        .execute(pool)
        .await?;
    audit(
        pool,
        Some(group_id),
        None,
        "fuse",
        "paused",
        serde_json::json!({"reason": reason}),
    )
    .await
}

async fn quarantine(
    pool: &SqlitePool,
    group_id: &str,
    source_path: Option<&str>,
    reason_code: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    sqlx::query(
        "INSERT INTO device_sync_quarantine (
             id, group_id, source_path, reason_code, details_json
         ) VALUES (?1,?2,?3,?4,?5)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(source_path)
    .bind(reason_code)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn audit(
    pool: &SqlitePool,
    group_id: Option<&str>,
    device_id: Option<&str>,
    action: &str,
    outcome: &str,
    details: serde_json::Value,
) -> Result<(), SyncError> {
    sqlx::query(
        "INSERT INTO device_sync_audits (
             id, group_id, device_id, action, outcome, details_json
         ) VALUES (?1,?2,?3,?4,?5,?6)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(device_id)
    .bind(action)
    .bind(outcome)
    .bind(details.to_string())
    .execute(pool)
    .await?;
    Ok(())
}
