use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use super::SyncError;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SyncGroupSummary {
    pub id: String,
    pub connector_root: String,
    pub local_device_id: String,
    pub protocol_version: i64,
    pub key_epoch: i64,
    pub paused: i64,
    pub last_synced_at: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SyncMemberSummary {
    pub device_id: String,
    pub display_name: String,
    pub fingerprint: String,
    pub key_epoch: i64,
    pub status: String,
    pub last_seen_sequence: i64,
    pub revoked_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SyncConflictSummary {
    pub id: String,
    pub operation_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub case_id: Option<String>,
    pub field_key: String,
    pub atomic_group: Option<String>,
    pub local_value_json: Option<String>,
    pub remote_value_json: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SyncSnapshotSummary {
    pub id: String,
    pub key_epoch: i64,
    pub manifest_hash: String,
    pub encrypted_file_name: String,
    pub entity_counts_json: String,
    pub snapshot_kind: String,
    pub state: String,
    pub created_at: String,
}

pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<SyncGroupSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id,connector_root,local_device_id,protocol_version,key_epoch,
                paused,last_synced_at,created_at
         FROM device_sync_groups ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?)
}

pub async fn list_members(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<SyncMemberSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT device_id,display_name,fingerprint,key_epoch,status,
                last_seen_sequence,revoked_at,updated_at
         FROM device_sync_members WHERE group_id=?1
         ORDER BY status,display_name,device_id",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_conflicts(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<SyncConflictSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id,operation_id,entity_type,entity_id,case_id,field_key,
                atomic_group,local_value_json,remote_value_json,status,created_at
         FROM device_sync_conflicts WHERE group_id=?1
         ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END,created_at DESC",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?)
}

pub async fn list_snapshots(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<SyncSnapshotSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id,key_epoch,manifest_hash,encrypted_file_name,entity_counts_json,
                snapshot_kind,state,created_at
         FROM device_sync_snapshots WHERE group_id=?1 ORDER BY created_at DESC",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?)
}

pub async fn set_paused(pool: &SqlitePool, group_id: &str, paused: bool) -> Result<(), SyncError> {
    let affected = sqlx::query(
        "UPDATE device_sync_groups SET paused=?1,updated_at=datetime('now') WHERE id=?2",
    )
    .bind(if paused { 1 } else { 0 })
    .bind(group_id)
    .execute(pool)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(SyncError::NotFound(format!("同步组不存在: {group_id}")));
    }
    Ok(())
}
