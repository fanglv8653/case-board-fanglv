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
    pub auto_paused: i64,
    pub pause_reason_code: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ManualReviewSummary {
    pub id: String,
    pub group_id: String,
    pub reason_code: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub retry_count: i64,
}

pub async fn list_groups(pool: &SqlitePool) -> Result<Vec<SyncGroupSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id,connector_root,local_device_id,protocol_version,key_epoch,
                paused,auto_paused,pause_reason_code,last_attempt_at,last_success_at,
                last_synced_at,created_at
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
    let affected = if paused {
        sqlx::query(
            "UPDATE device_sync_groups
             SET paused=1,auto_paused=0,pause_reason_code='USER_PAUSED',
                 updated_at=datetime('now')
             WHERE id=?1",
        )
        .bind(group_id)
        .execute(pool)
        .await?
        .rows_affected()
    } else {
        sqlx::query(
            "UPDATE device_sync_groups
             SET paused=0,auto_paused=0,pause_reason_code=NULL,
                 updated_at=datetime('now')
             WHERE id=?1",
        )
        .bind(group_id)
        .execute(pool)
        .await?
        .rows_affected()
    };
    if affected != 1 {
        return Err(SyncError::NotFound(format!("同步组不存在: {group_id}")));
    }
    Ok(())
}

pub async fn list_manual_reviews(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Vec<ManualReviewSummary>, SyncError> {
    Ok(sqlx::query_as(
        "SELECT id,group_id,reason_code,first_seen_at,last_seen_at,retry_count
         FROM device_sync_quarantine
         WHERE group_id=?1 AND status='manual_review'
         ORDER BY first_seen_at,id",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?)
}

pub async fn review_manual_quarantine(
    pool: &SqlitePool,
    group_id: &str,
    review_id: &str,
    action: &str,
) -> Result<(), SyncError> {
    let audit_action = match action {
        "archive" => "manual_review_archived",
        "retain" => "manual_review_retained",
        _ => {
            return Err(SyncError::Protocol(
                "manual review action must be archive or retain".to_string(),
            ))
        }
    };
    let mut tx = pool.begin().await?;
    let exists: i64 = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1 FROM device_sync_quarantine
             WHERE id=?1 AND group_id=?2 AND status='manual_review'
         )",
    )
    .bind(review_id)
    .bind(group_id)
    .fetch_one(&mut *tx)
    .await?;
    if exists == 0 {
        return Err(SyncError::NotFound(
            "manual review record does not exist".to_string(),
        ));
    }
    if action == "archive" {
        sqlx::query(
            "UPDATE device_sync_quarantine
             SET status='resolved',resolved_at=datetime('now'),last_seen_at=datetime('now')
             WHERE id=?1 AND group_id=?2 AND status='manual_review'",
        )
        .bind(review_id)
        .bind(group_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "INSERT INTO device_sync_audits(
             id,group_id,device_id,action,outcome,details_json
         ) VALUES(?1,?2,NULL,?3,'succeeded',?4)",
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(group_id)
    .bind(audit_action)
    .bind(
        serde_json::json!({
            "review_id": review_id,
            "decision": action
        })
        .to_string(),
    )
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}
