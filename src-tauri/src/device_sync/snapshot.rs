use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{ConnectOptions, Row, SqlitePool};

use super::crypto::{open, seal, sha256_hex, EnvelopeHeader, PROTOCOL_VERSION};
use super::identity::{load_group_key, load_signing_secret};
use super::nas_folder::MountedFolder;
use super::registry;
use super::SyncError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogicalSnapshot {
    pub protocol_version: u32,
    pub group_id: String,
    pub snapshot_id: String,
    pub logical_time: i64,
    pub entities: BTreeMap<String, Vec<Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotResult {
    pub snapshot_id: String,
    pub encrypted_path: String,
    pub manifest_hash: String,
    pub entity_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RestorePreview {
    pub snapshot_id: String,
    pub entity_counts: BTreeMap<String, usize>,
    pub new_entities: BTreeMap<String, usize>,
    pub existing_entities: BTreeMap<String, usize>,
    pub plaintext_sha256: String,
    pub formal_database_unchanged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsolatedRestorePreview {
    pub isolated_database_path: String,
    pub preview: RestorePreview,
}

pub async fn create_encrypted_snapshot(
    pool: &SqlitePool,
    group_id: &str,
    snapshot_kind: &str,
) -> Result<SnapshotResult, SyncError> {
    if !matches!(snapshot_kind, "daily" | "monthly" | "manual") {
        return Err(SyncError::Protocol(
            "快照类型必须是 daily/monthly/manual".to_string(),
        ));
    }
    let group: Option<(String, String, i64, i64)> = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch, next_sequence
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (connector_root, device_id, key_epoch, logical_time) =
        group.ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    let folder = MountedFolder::connect(PathBuf::from(connector_root))?;
    folder.initialize_group(group_id)?;
    let snapshot_id = format!(
        "{}-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        uuid::Uuid::new_v4().simple()
    );
    let mut entities = BTreeMap::new();
    let mut counts = BTreeMap::new();
    for policy in registry::all_policies() {
        let fields = policy
            .columns
            .iter()
            .flat_map(|field| [format!("'{field}'"), format!("\"{field}\"")])
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT json_object({fields}) AS payload FROM \"{}\" ORDER BY id",
            policy.table
        );
        let rows = sqlx::query(&sql).fetch_all(pool).await?;
        let values = rows
            .into_iter()
            .map(|row| {
                let raw: String = row.try_get("payload")?;
                serde_json::from_str::<Value>(&raw).map_err(SyncError::from)
            })
            .collect::<Result<Vec<_>, SyncError>>()?;
        counts.insert(policy.entity_type.to_string(), values.len());
        entities.insert(policy.entity_type.to_string(), values);
    }
    let snapshot = LogicalSnapshot {
        protocol_version: PROTOCOL_VERSION,
        group_id: group_id.to_string(),
        snapshot_id: snapshot_id.clone(),
        logical_time,
        entities,
    };
    let plaintext = serde_json::to_vec(&snapshot)?;
    let manifest_hash = sha256_hex(&plaintext);
    let key = load_group_key(group_id, &device_id, key_epoch as u32)?;
    let signing = load_signing_secret(group_id, &device_id)?;
    let envelope = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group_id.to_string(),
            device_id: device_id.clone(),
            sequence: logical_time as u64,
            key_epoch: key_epoch as u32,
            payload_kind: "snapshot".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        &plaintext,
        &key,
        &signing,
    )?;
    let path = folder.write_encrypted_snapshot(group_id, &snapshot_id, &envelope)?;
    sqlx::query(
        "INSERT INTO device_sync_snapshots (
             id, group_id, key_epoch, manifest_hash, encrypted_file_name,
             entity_counts_json, logical_time, snapshot_kind, state
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,'verified')",
    )
    .bind(&snapshot_id)
    .bind(group_id)
    .bind(key_epoch)
    .bind(&manifest_hash)
    .bind(
        path.file_name()
            .and_then(|value| value.to_str())
            .unwrap_or(""),
    )
    .bind(serde_json::to_string(&counts)?)
    .bind(logical_time)
    .bind(snapshot_kind)
    .execute(pool)
    .await?;
    enforce_retention(pool, &folder, group_id).await?;
    Ok(SnapshotResult {
        snapshot_id,
        encrypted_path: path.to_string_lossy().into_owned(),
        manifest_hash,
        entity_counts: counts,
    })
}

async fn enforce_retention(
    pool: &SqlitePool,
    folder: &MountedFolder,
    group_id: &str,
) -> Result<(), SyncError> {
    for (kind, keep) in [("daily", 30_i64), ("monthly", 12_i64)] {
        let stale: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, encrypted_file_name
             FROM device_sync_snapshots
             WHERE group_id=?1 AND snapshot_kind=?2
             ORDER BY created_at DESC, id DESC
             LIMIT -1 OFFSET ?3",
        )
        .bind(group_id)
        .bind(kind)
        .bind(keep)
        .fetch_all(pool)
        .await?;
        for (id, file_name) in stale {
            folder.remove_snapshot(group_id, &file_name)?;
            sqlx::query("DELETE FROM device_sync_snapshots WHERE id=?1 AND group_id=?2")
                .bind(id)
                .bind(group_id)
                .execute(pool)
                .await?;
        }
    }
    Ok(())
}

pub async fn preview_restore(
    pool: &SqlitePool,
    group_id: &str,
    snapshot_path: &Path,
) -> Result<RestorePreview, SyncError> {
    let (snapshot, plaintext) = decrypt_snapshot(pool, group_id, snapshot_path).await?;
    build_restore_preview(pool, snapshot, &plaintext).await
}

pub async fn prepare_isolated_restore(
    pool: &SqlitePool,
    group_id: &str,
    snapshot_path: &Path,
    isolated_database_path: &Path,
) -> Result<IsolatedRestorePreview, SyncError> {
    if !isolated_database_path.is_absolute() || isolated_database_path.exists() {
        return Err(SyncError::InvalidNasPath(
            "隔离恢复库必须是尚不存在的绝对文件路径".to_string(),
        ));
    }
    let parent = isolated_database_path
        .parent()
        .ok_or_else(|| SyncError::InvalidNasPath("隔离恢复库缺少父目录".to_string()))?;
    std::fs::create_dir_all(parent)
        .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let (snapshot, plaintext) = decrypt_snapshot(pool, group_id, snapshot_path).await?;
    let preview = build_restore_preview(pool, snapshot.clone(), &plaintext).await?;
    let options = SqliteConnectOptions::new()
        .filename(isolated_database_path)
        .create_if_missing(true)
        .disable_statement_logging();
    let isolated = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await?;
    let write_result = async {
        sqlx::raw_sql(
            "CREATE TABLE restore_preview_meta (
                 snapshot_id TEXT PRIMARY KEY NOT NULL,
                 group_id TEXT NOT NULL,
                 plaintext_sha256 TEXT NOT NULL,
                 created_at TEXT NOT NULL DEFAULT(datetime('now'))
             );
             CREATE TABLE restore_preview_entities (
                 entity_type TEXT NOT NULL,
                 entity_id TEXT NOT NULL,
                 payload_json TEXT NOT NULL,
                 PRIMARY KEY(entity_type,entity_id)
             );",
        )
        .execute(&isolated)
        .await?;
        let mut tx = isolated.begin().await?;
        sqlx::query(
            "INSERT INTO restore_preview_meta (
                 snapshot_id,group_id,plaintext_sha256
             ) VALUES (?1,?2,?3)",
        )
        .bind(&snapshot.snapshot_id)
        .bind(group_id)
        .bind(&preview.plaintext_sha256)
        .execute(&mut *tx)
        .await?;
        for (entity_type, values) in &snapshot.entities {
            for value in values {
                let id = value
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| sqlx::Error::Protocol("snapshot entity missing id".into()))?;
                sqlx::query(
                    "INSERT INTO restore_preview_entities (
                         entity_type,entity_id,payload_json
                     ) VALUES (?1,?2,?3)",
                )
                .bind(entity_type)
                .bind(id)
                .bind(value.to_string())
                .execute(&mut *tx)
                .await?;
            }
        }
        tx.commit().await?;
        Ok::<(), sqlx::Error>(())
    }
    .await;
    isolated.close().await;
    if let Err(error) = write_result {
        let _ = std::fs::remove_file(isolated_database_path);
        return Err(SyncError::Database(error.to_string()));
    }
    Ok(IsolatedRestorePreview {
        isolated_database_path: isolated_database_path.to_string_lossy().into_owned(),
        preview,
    })
}

async fn decrypt_snapshot(
    pool: &SqlitePool,
    group_id: &str,
    snapshot_path: &Path,
) -> Result<(LogicalSnapshot, Vec<u8>), SyncError> {
    let group: Option<(String, String)> = sqlx::query_as(
        "SELECT connector_root, local_device_id FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (connector_root, local_device_id) =
        group.ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    let folder = MountedFolder::connect(PathBuf::from(connector_root))?;
    let envelope = folder.read_envelope(snapshot_path)?;
    if envelope.header.group_id != group_id || envelope.header.payload_kind != "snapshot" {
        return Err(SyncError::Integrity(
            "所选文件不是本同步组的加密快照".to_string(),
        ));
    }
    let public_key: Option<String> = sqlx::query_scalar(
        "SELECT signing_public_key FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(group_id)
    .bind(&envelope.header.device_id)
    .fetch_optional(pool)
    .await?;
    let public_key =
        public_key.ok_or_else(|| SyncError::Integrity("快照签名设备不受信".to_string()))?;
    let key = load_group_key(group_id, &local_device_id, envelope.header.key_epoch)?;
    let plaintext = open(&envelope, &key, &public_key)?;
    let snapshot: LogicalSnapshot = serde_json::from_slice(&plaintext)?;
    if snapshot.group_id != group_id || snapshot.protocol_version != PROTOCOL_VERSION {
        return Err(SyncError::Integrity("快照正文组或协议不匹配".to_string()));
    }
    Ok((snapshot, plaintext))
}

async fn build_restore_preview(
    pool: &SqlitePool,
    snapshot: LogicalSnapshot,
    plaintext: &[u8],
) -> Result<RestorePreview, SyncError> {
    let mut counts = BTreeMap::new();
    let mut new_entities = BTreeMap::new();
    let mut existing_entities = BTreeMap::new();
    for (entity_type, values) in &snapshot.entities {
        let policy = registry::policy(entity_type)?;
        let mut new_count = 0;
        let mut existing_count = 0;
        for value in values {
            let id = value
                .get("id")
                .and_then(Value::as_str)
                .ok_or_else(|| SyncError::Serialization("快照实体缺少 id".to_string()))?;
            let exists: i64 = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM \"{}\" WHERE id=?1)",
                policy.table
            ))
            .bind(id)
            .fetch_one(pool)
            .await?;
            if exists == 0 {
                new_count += 1;
            } else {
                existing_count += 1;
            }
        }
        counts.insert(entity_type.clone(), values.len());
        new_entities.insert(entity_type.clone(), new_count);
        existing_entities.insert(entity_type.clone(), existing_count);
    }
    Ok(RestorePreview {
        snapshot_id: snapshot.snapshot_id,
        entity_counts: counts,
        new_entities,
        existing_entities,
        plaintext_sha256: sha256_hex(plaintext),
        // This API is intentionally preview-only: it never starts a write transaction.
        formal_database_unchanged: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_sync::crypto::{
        generate_device_keys, generate_group_key, seal, EnvelopeHeader,
    };

    #[tokio::test]
    async fn retention_keeps_30_daily_and_12_monthly_snapshots() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::raw_sql(
            "CREATE TABLE device_sync_snapshots (
                 id TEXT PRIMARY KEY,
                 group_id TEXT NOT NULL,
                 key_epoch INTEGER NOT NULL,
                 manifest_hash TEXT NOT NULL,
                 encrypted_file_name TEXT NOT NULL,
                 entity_counts_json TEXT NOT NULL,
                 logical_time INTEGER NOT NULL,
                 snapshot_kind TEXT NOT NULL,
                 state TEXT NOT NULL,
                 created_at TEXT NOT NULL
             );",
        )
        .execute(&pool)
        .await
        .unwrap();
        let temp = tempfile::tempdir().unwrap();
        let folder = MountedFolder::connect(temp.path()).unwrap();
        folder.initialize_group("g1").unwrap();
        let device = generate_device_keys();
        let key = generate_group_key();
        for (kind, count) in [("daily", 31), ("monthly", 13)] {
            for index in 0..count {
                let id = format!("{kind}-{index:03}");
                let envelope = seal(
                    EnvelopeHeader {
                        protocol_version: PROTOCOL_VERSION,
                        group_id: "g1".into(),
                        device_id: "d1".into(),
                        sequence: index,
                        key_epoch: 1,
                        payload_kind: "snapshot".into(),
                        created_at: "2026-07-29T00:00:00Z".into(),
                    },
                    id.as_bytes(),
                    &key,
                    &device.signing_secret,
                )
                .unwrap();
                let path = folder
                    .write_encrypted_snapshot("g1", &id, &envelope)
                    .unwrap();
                sqlx::query(
                    "INSERT INTO device_sync_snapshots VALUES (
                         ?1,'g1',1,'hash',?2,'{}',?3,?4,'verified',?5
                     )",
                )
                .bind(&id)
                .bind(path.file_name().unwrap().to_string_lossy().as_ref())
                .bind(index as i64)
                .bind(kind)
                .bind(format!("2026-01-{:02}T00:00:00Z", (index % 28) + 1))
                .execute(&pool)
                .await
                .unwrap();
            }
        }
        enforce_retention(&pool, &folder, "g1").await.unwrap();
        let daily: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_sync_snapshots WHERE snapshot_kind='daily'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let monthly: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM device_sync_snapshots WHERE snapshot_kind='monthly'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(daily, 30);
        assert_eq!(monthly, 12);
    }
}
