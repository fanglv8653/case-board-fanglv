use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use super::nas_folder::MountedFolder;
use super::{engine, operations, pairing, recovery, snapshot, SyncError, SyncStatus};

fn command_error(error: SyncError) -> String {
    format!("[{}] {error}", error.code())
}

#[derive(Debug, Deserialize)]
pub struct CreateGroupInput {
    pub connector_root: String,
    pub display_name: String,
    pub recovery_destination: String,
    pub recovery_passphrase: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateJoinRequestInput {
    pub connector_root: String,
    pub pairing_code: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct CompleteJoinInput {
    pub connector_root: String,
    pub request_path: String,
    pub completion_path: String,
    pub pairing_code: String,
}

#[derive(Debug, Serialize)]
pub struct NasValidation {
    pub connector_root: String,
    pub writable: bool,
}

#[derive(Debug, Serialize)]
pub struct JoinApproval {
    pub completion_path: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct MemberView {
    pub device_id: String,
    pub display_name: String,
    pub signing_public_key: String,
    pub exchange_public_key: String,
    pub fingerprint: String,
    pub key_epoch: i64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct ConflictView {
    pub id: String,
    pub operation_id: String,
    pub group_id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub case_id: Option<String>,
    pub field_key: String,
    pub local_value_json: Option<String>,
    pub remote_value_json: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, FromRow)]
struct SnapshotRow {
    id: String,
    manifest_hash: String,
    encrypted_file_name: String,
    entity_counts_json: String,
}

#[tauri::command]
pub async fn get_device_sync_status(
    pool: tauri::State<'_, SqlitePool>,
) -> Result<Option<SyncStatus>, String> {
    let group_id: Option<String> =
        sqlx::query_scalar("SELECT id FROM device_sync_groups ORDER BY created_at LIMIT 1")
            .fetch_optional(pool.inner())
            .await
            .map_err(|error| command_error(error.into()))?;
    match group_id {
        Some(group_id) => engine::get_status(pool.inner(), &group_id)
            .await
            .map(Some)
            .map_err(command_error),
        None => Ok(None),
    }
}

#[tauri::command]
pub fn validate_device_sync_nas_path(connector_root: String) -> Result<NasValidation, String> {
    let root = PathBuf::from(&connector_root);
    MountedFolder::connect(&root).map_err(command_error)?;
    let canonical = fs::canonicalize(&root)
        .map_err(|error| command_error(SyncError::NasUnavailable(error.to_string())))?;
    Ok(NasValidation {
        connector_root: canonical.to_string_lossy().into_owned(),
        writable: true,
    })
}

#[tauri::command]
pub async fn create_device_sync_group(
    pool: tauri::State<'_, SqlitePool>,
    input: CreateGroupInput,
) -> Result<recovery::CreatedGroupWithRecovery, String> {
    recovery::create_group_with_recovery(
        pool.inner(),
        Path::new(&input.connector_root),
        &input.display_name,
        Path::new(&input.recovery_destination),
        &input.recovery_passphrase,
    )
    .await
    .map_err(command_error)
}

#[tauri::command]
pub async fn set_device_sync_paused(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
    paused: bool,
) -> Result<SyncStatus, String> {
    super::queries::set_paused(pool.inner(), &group_id, paused)
        .await
        .map_err(command_error)?;
    engine::get_status(pool.inner(), &group_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn run_device_sync(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
) -> Result<engine::SyncRunResult, String> {
    engine::sync_once(pool.inner(), &group_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn create_device_sync_invite(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
) -> Result<pairing::CreatedInvite, String> {
    pairing::create_pairing_invite(pool.inner(), &group_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub fn create_device_sync_join_request(
    input: CreateJoinRequestInput,
) -> Result<pairing::JoinRequest, String> {
    let (group_id, invite_id) =
        find_single_active_invite(Path::new(&input.connector_root)).map_err(command_error)?;
    pairing::create_join_request(
        Path::new(&input.connector_root),
        &group_id,
        &invite_id,
        &input.pairing_code,
        &input.display_name,
    )
    .map_err(command_error)
}

#[tauri::command]
pub async fn approve_device_sync_join(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
    request_path: String,
    expected_fingerprint: String,
) -> Result<JoinApproval, String> {
    let connector_root: String =
        sqlx::query_scalar("SELECT connector_root FROM device_sync_groups WHERE id=?1")
            .bind(&group_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|error| command_error(error.into()))?
            .ok_or_else(|| command_error(SyncError::NotFound("同步组不存在".to_string())))?;
    let folder = MountedFolder::connect(&connector_root).map_err(command_error)?;
    let request: pairing::JoinRequest = serde_json::from_slice(
        &folder
            .read_group_file(Path::new(&request_path))
            .map_err(command_error)?,
    )
    .map_err(|error| command_error(error.into()))?;
    if request.group_id != group_id {
        return Err(command_error(SyncError::Integrity(
            "加入申请与当前同步组不一致".to_string(),
        )));
    }
    let member = pairing::approve_join(
        pool.inner(),
        &group_id,
        &request.invite_id,
        expected_fingerprint.trim(),
    )
    .await
    .map_err(command_error)?;
    let key_epoch: i64 = sqlx::query_scalar("SELECT key_epoch FROM device_sync_groups WHERE id=?1")
        .bind(&group_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|error| command_error(error.into()))?;
    let completion_path = folder
        .member_envelope_path(&group_id, &member.device_id, key_epoch as u32)
        .map_err(command_error)?;
    Ok(JoinApproval {
        completion_path: completion_path.to_string_lossy().into_owned(),
    })
}

#[tauri::command]
pub async fn complete_device_sync_join(
    pool: tauri::State<'_, SqlitePool>,
    input: CompleteJoinInput,
) -> Result<pairing::JoinCompletion, String> {
    if input.pairing_code.trim().len() < 20 {
        return Err(command_error(SyncError::Protocol(
            "一次性配对码长度不足".to_string(),
        )));
    }
    let folder = MountedFolder::connect(&input.connector_root).map_err(command_error)?;
    let request: pairing::JoinRequest = serde_json::from_slice(
        &folder
            .read_group_file(Path::new(&input.request_path))
            .map_err(command_error)?,
    )
    .map_err(|error| command_error(error.into()))?;
    let completion = fs::canonicalize(&input.completion_path)
        .map_err(|error| command_error(SyncError::NasUnavailable(error.to_string())))?;
    let expected = folder
        .member_envelope_path(
            &request.group_id,
            &request.device_id,
            read_envelope_epoch(&folder, &completion)?,
        )
        .map_err(command_error)?;
    let expected = fs::canonicalize(expected)
        .map_err(|error| command_error(SyncError::NasUnavailable(error.to_string())))?;
    if completion != expected {
        return Err(command_error(SyncError::Integrity(
            "完成包路径与加入申请不匹配".to_string(),
        )));
    }
    pairing::complete_join(
        pool.inner(),
        Path::new(&input.connector_root),
        &request.invite_id,
        &request,
    )
    .await
    .map_err(command_error)
}

#[tauri::command]
pub async fn list_device_sync_members(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
) -> Result<Vec<MemberView>, String> {
    sqlx::query_as(
        "SELECT device_id,display_name,signing_public_key,exchange_public_key,
                fingerprint,key_epoch,status
         FROM device_sync_members WHERE group_id=?1
         ORDER BY CASE status WHEN 'trusted' THEN 0 ELSE 1 END,display_name,device_id",
    )
    .bind(group_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|error| command_error(error.into()))
}

#[tauri::command]
pub async fn revoke_device_sync_member(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
    device_id: String,
) -> Result<SyncStatus, String> {
    let fingerprint: String = sqlx::query_scalar(
        "SELECT fingerprint FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(&group_id)
    .bind(&device_id)
    .fetch_optional(pool.inner())
    .await
    .map_err(|error| command_error(error.into()))?
    .ok_or_else(|| command_error(SyncError::NotFound("受信设备不存在".to_string())))?;
    pairing::revoke_device(pool.inner(), &group_id, &device_id, &fingerprint)
        .await
        .map_err(command_error)?;
    engine::get_status(pool.inner(), &group_id)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn list_device_sync_conflicts(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
) -> Result<Vec<ConflictView>, String> {
    sqlx::query_as(
        "SELECT id,operation_id,group_id,entity_type,entity_id,case_id,field_key,
                local_value_json,remote_value_json,status,created_at
         FROM device_sync_conflicts WHERE group_id=?1
         ORDER BY CASE status WHEN 'pending' THEN 0 ELSE 1 END,created_at DESC",
    )
    .bind(group_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|error| command_error(error.into()))
}

#[tauri::command]
pub async fn resolve_device_sync_conflict(
    pool: tauri::State<'_, SqlitePool>,
    operation_id: String,
    resolution: String,
) -> Result<usize, String> {
    let resolution = match resolution.as_str() {
        "keep_local" => operations::ConflictResolution::KeepLocal,
        "keep_remote" => operations::ConflictResolution::KeepRemote,
        _ => {
            return Err(command_error(SyncError::Protocol(
                "冲突处理方式不受支持".to_string(),
            )))
        }
    };
    operations::resolve_operation_conflicts(pool.inner(), &operation_id, resolution, None)
        .await
        .map_err(command_error)
}

#[tauri::command]
pub async fn create_device_sync_snapshot(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
    snapshot_kind: Option<String>,
) -> Result<snapshot::SnapshotResult, String> {
    snapshot::create_encrypted_snapshot(
        pool.inner(),
        &group_id,
        snapshot_kind.as_deref().unwrap_or("manual"),
    )
    .await
    .map_err(command_error)
}

#[tauri::command]
pub async fn list_device_sync_snapshots(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
) -> Result<Vec<snapshot::SnapshotResult>, String> {
    let connector_root: String =
        sqlx::query_scalar("SELECT connector_root FROM device_sync_groups WHERE id=?1")
            .bind(&group_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|error| command_error(error.into()))?
            .ok_or_else(|| command_error(SyncError::NotFound("同步组不存在".to_string())))?;
    let rows: Vec<SnapshotRow> = sqlx::query_as(
        "SELECT id,manifest_hash,encrypted_file_name,entity_counts_json
         FROM device_sync_snapshots WHERE group_id=?1 ORDER BY created_at DESC",
    )
    .bind(&group_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|error| command_error(error.into()))?;
    rows.into_iter()
        .map(|row| {
            let counts: BTreeMap<String, usize> = serde_json::from_str(&row.entity_counts_json)
                .map_err(|error| command_error(error.into()))?;
            Ok(snapshot::SnapshotResult {
                snapshot_id: row.id,
                encrypted_path: Path::new(&connector_root)
                    .join("fanglv-caseboard-sync")
                    .join("groups")
                    .join(&group_id)
                    .join("snapshots")
                    .join(row.encrypted_file_name)
                    .to_string_lossy()
                    .into_owned(),
                manifest_hash: row.manifest_hash,
                entity_counts: counts,
            })
        })
        .collect()
}

#[tauri::command]
pub async fn preview_device_sync_restore(
    pool: tauri::State<'_, SqlitePool>,
    group_id: String,
    snapshot_path: String,
) -> Result<snapshot::RestorePreview, String> {
    snapshot::preview_restore(pool.inner(), &group_id, Path::new(&snapshot_path))
        .await
        .map_err(command_error)
}

#[tauri::command]
pub fn preview_device_sync_recovery(
    package_path: String,
    passphrase: String,
) -> Result<recovery::RecoveryPreview, String> {
    recovery::preview_recovery_package(Path::new(&package_path), &passphrase).map_err(command_error)
}

fn read_envelope_epoch(folder: &MountedFolder, path: &Path) -> Result<u32, String> {
    folder
        .read_envelope(path)
        .map(|envelope| envelope.header.key_epoch)
        .map_err(command_error)
}

fn find_single_active_invite(root: &Path) -> Result<(String, String), SyncError> {
    MountedFolder::connect(root)?;
    let groups_root = root.join("fanglv-caseboard-sync").join("groups");
    let canonical_groups = fs::canonicalize(&groups_root)
        .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let mut active = Vec::new();
    for group_entry in fs::read_dir(&canonical_groups)
        .map_err(|error| SyncError::NasUnavailable(error.to_string()))?
    {
        let group_entry =
            group_entry.map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        if !group_entry
            .file_type()
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?
            .is_dir()
        {
            continue;
        }
        let invites = group_entry.path().join("invites");
        if !invites.is_dir() {
            continue;
        }
        for entry in
            fs::read_dir(invites).map_err(|error| SyncError::NasUnavailable(error.to_string()))?
        {
            let entry = entry.map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
            let path = entry.path();
            if !path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".invite.json"))
            {
                continue;
            }
            let canonical = fs::canonicalize(&path)
                .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
            if !canonical.starts_with(&canonical_groups) {
                return Err(SyncError::InvalidNasPath(
                    "邀请文件越出同步目录".to_string(),
                ));
            }
            let invite: pairing::PairingInvite = serde_json::from_slice(
                &fs::read(canonical)
                    .map_err(|error| SyncError::NasUnavailable(error.to_string()))?,
            )?;
            let expires = DateTime::parse_from_rfc3339(&invite.expires_at)
                .map_err(|_| SyncError::Protocol("邀请有效期格式错误".to_string()))?
                .with_timezone(&Utc);
            if expires > Utc::now() {
                active.push((invite.group_id, invite.invite_id));
            }
        }
    }
    active.sort();
    active.dedup();
    match active.len() {
        0 => Err(SyncError::NotFound(
            "NAS 中没有尚未过期的配对邀请".to_string(),
        )),
        1 => Ok(active.remove(0)),
        _ => Err(SyncError::Protocol(
            "NAS 中存在多个有效邀请，请先撤销多余邀请".to_string(),
        )),
    }
}
