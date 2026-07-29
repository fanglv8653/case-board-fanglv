use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Utc;
use pbkdf2::pbkdf2_hmac;
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use sqlx::SqlitePool;
use zeroize::{Zeroize, Zeroizing};

use super::crypto::{open, seal, EncryptedEnvelope, EnvelopeHeader, PROTOCOL_VERSION};
use super::identity::{
    create_sync_group, delete_device_secrets, load_group_key, load_signing_secret,
    LocalDeviceIdentity,
};
use super::pairing::MemberPublic;
use super::SyncError;

const RECOVERY_KDF_ITERATIONS: u32 = 310_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryPayload {
    protocol_version: u32,
    group_id: String,
    created_at: String,
    latest_key_epoch: u32,
    historical_group_keys_b64: BTreeMap<u32, String>,
    members: Vec<MemberPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RecoveryFile {
    format: String,
    kdf: String,
    iterations: u32,
    salt_b64: String,
    signing_public_key: String,
    envelope: EncryptedEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryExport {
    pub path: String,
    pub group_id: String,
    pub key_epochs: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedGroupWithRecovery {
    pub identity: LocalDeviceIdentity,
    pub recovery: RecoveryExport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryPreview {
    pub group_id: String,
    pub latest_key_epoch: u32,
    pub historical_key_epochs: Vec<u32>,
    pub trusted_members: Vec<MemberPublic>,
    pub formal_database_unchanged: bool,
}

pub async fn create_group_with_recovery(
    pool: &SqlitePool,
    connector_root: &Path,
    display_name: &str,
    recovery_destination: &Path,
    recovery_passphrase: &str,
) -> Result<CreatedGroupWithRecovery, SyncError> {
    let identity = create_sync_group(pool, connector_root, display_name).await?;
    match export_recovery_package(
        pool,
        &identity.group_id,
        recovery_destination,
        recovery_passphrase,
    )
    .await
    {
        Ok(recovery) => Ok(CreatedGroupWithRecovery { identity, recovery }),
        Err(error) => {
            let _ = sqlx::query("DELETE FROM device_sync_groups WHERE id=?1")
                .bind(&identity.group_id)
                .execute(pool)
                .await;
            delete_device_secrets(&identity.group_id, &identity.device_id, identity.key_epoch);
            Err(error)
        }
    }
}

pub async fn export_recovery_package(
    pool: &SqlitePool,
    group_id: &str,
    destination: &Path,
    passphrase: &str,
) -> Result<RecoveryExport, SyncError> {
    validate_passphrase(passphrase)?;
    validate_destination(pool, group_id, destination).await?;
    let group: Option<(String, i64)> =
        sqlx::query_as("SELECT local_device_id, key_epoch FROM device_sync_groups WHERE id=?1")
            .bind(group_id)
            .fetch_optional(pool)
            .await?;
    let (device_id, latest_epoch) =
        group.ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    let mut historical = BTreeMap::new();
    for epoch in 1..=latest_epoch as u32 {
        let key = load_group_key(group_id, &device_id, epoch)?;
        historical.insert(epoch, B64.encode(key.as_slice()));
    }
    let members = sqlx::query_as::<_, MemberDbRow>(
        "SELECT device_id, display_name, signing_public_key, exchange_public_key,
                fingerprint, key_epoch, status
         FROM device_sync_members WHERE group_id=?1 ORDER BY created_at",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(MemberPublic::from)
    .collect::<Vec<_>>();
    let payload = RecoveryPayload {
        protocol_version: PROTOCOL_VERSION,
        group_id: group_id.to_string(),
        created_at: Utc::now().to_rfc3339(),
        latest_key_epoch: latest_epoch as u32,
        historical_group_keys_b64: historical,
        members,
    };
    let mut salt = [0_u8; 32];
    OsRng.fill_bytes(&mut salt);
    let wrapping_key = derive_recovery_key(passphrase, &salt, RECOVERY_KDF_ITERATIONS);
    let signing = load_signing_secret(group_id, &device_id)?;
    let signing_public_key: String = sqlx::query_scalar(
        "SELECT signing_public_key FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2",
    )
    .bind(group_id)
    .bind(&device_id)
    .fetch_one(pool)
    .await?;
    let envelope = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group_id.to_string(),
            device_id,
            sequence: 0,
            key_epoch: latest_epoch as u32,
            payload_kind: "offline-recovery".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        &serde_json::to_vec(&payload)?,
        &wrapping_key,
        &signing,
    )?;
    let package = RecoveryFile {
        format: "fanglv-caseboard-recovery-v1".to_string(),
        kdf: "PBKDF2-HMAC-SHA256".to_string(),
        iterations: RECOVERY_KDF_ITERATIONS,
        salt_b64: B64.encode(salt),
        signing_public_key,
        envelope,
    };
    write_new_file_atomic(destination, &serde_json::to_vec_pretty(&package)?)?;
    Ok(RecoveryExport {
        path: destination.to_string_lossy().into_owned(),
        group_id: group_id.to_string(),
        key_epochs: payload.historical_group_keys_b64.keys().copied().collect(),
    })
}

pub fn preview_recovery_package(
    package_path: &Path,
    passphrase: &str,
) -> Result<RecoveryPreview, SyncError> {
    validate_passphrase(passphrase)?;
    let bytes =
        fs::read(package_path).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let package: RecoveryFile = serde_json::from_slice(&bytes)?;
    if package.format != "fanglv-caseboard-recovery-v1"
        || package.kdf != "PBKDF2-HMAC-SHA256"
        || package.iterations < 100_000
        || package.envelope.header.payload_kind != "offline-recovery"
    {
        return Err(SyncError::Protocol("离线恢复包格式不受支持".to_string()));
    }
    let salt = B64
        .decode(&package.salt_b64)
        .map_err(|_| SyncError::Crypto("恢复包 KDF 盐编码损坏".to_string()))?;
    let wrapping = derive_recovery_key(passphrase, &salt, package.iterations);
    let plaintext = open(&package.envelope, &wrapping, &package.signing_public_key)?;
    let payload: RecoveryPayload = serde_json::from_slice(&plaintext)?;
    if payload.protocol_version != PROTOCOL_VERSION
        || payload.group_id != package.envelope.header.group_id
    {
        return Err(SyncError::Integrity("恢复包正文与信封不一致".to_string()));
    }
    Ok(RecoveryPreview {
        group_id: payload.group_id,
        latest_key_epoch: payload.latest_key_epoch,
        historical_key_epochs: payload.historical_group_keys_b64.keys().copied().collect(),
        trusted_members: payload.members,
        formal_database_unchanged: true,
    })
}

fn validate_passphrase(passphrase: &str) -> Result<(), SyncError> {
    if passphrase.chars().count() < 12 {
        return Err(SyncError::Protocol(
            "离线恢复包口令至少需要 12 个字符".to_string(),
        ));
    }
    Ok(())
}

async fn validate_destination(
    pool: &SqlitePool,
    group_id: &str,
    destination: &Path,
) -> Result<(), SyncError> {
    if !destination.is_absolute() || destination.file_name().is_none() {
        return Err(SyncError::InvalidNasPath(
            "恢复包目标必须是绝对文件路径".to_string(),
        ));
    }
    if destination.exists() {
        return Err(SyncError::InvalidNasPath(
            "拒绝覆盖已有离线恢复包".to_string(),
        ));
    }
    let connector_root: String =
        sqlx::query_scalar("SELECT connector_root FROM device_sync_groups WHERE id=?1")
            .bind(group_id)
            .fetch_one(pool)
            .await?;
    let connector = PathBuf::from(connector_root);
    let parent = destination
        .parent()
        .ok_or_else(|| SyncError::InvalidNasPath("恢复包目标缺少父目录".to_string()))?;
    fs::create_dir_all(parent).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let canonical_parent =
        fs::canonicalize(parent).map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    let canonical_connector = fs::canonicalize(&connector)
        .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
    if canonical_parent.starts_with(&canonical_connector) {
        return Err(SyncError::InvalidNasPath(
            "离线恢复包必须保存到 NAS 同步目录之外".to_string(),
        ));
    }
    Ok(())
}

fn derive_recovery_key(passphrase: &str, salt: &[u8], iterations: u32) -> Zeroizing<Vec<u8>> {
    let mut key = vec![0_u8; 32];
    let mut passphrase_bytes = passphrase.as_bytes().to_vec();
    pbkdf2_hmac::<Sha256>(&passphrase_bytes, salt, iterations, &mut key);
    passphrase_bytes.zeroize();
    Zeroizing::new(key)
}

fn write_new_file_atomic(path: &Path, bytes: &[u8]) -> Result<(), SyncError> {
    let parent = path
        .parent()
        .ok_or_else(|| SyncError::InvalidNasPath("目标缺少父目录".to_string()))?;
    let temp = parent.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        file.write_all(bytes)
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        file.sync_all()
            .map_err(|error| SyncError::NasUnavailable(error.to_string()))?;
        fs::rename(&temp, path).map_err(|error| SyncError::NasUnavailable(error.to_string()))
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[derive(sqlx::FromRow)]
struct MemberDbRow {
    device_id: String,
    display_name: String,
    signing_public_key: String,
    exchange_public_key: String,
    fingerprint: String,
    key_epoch: i64,
    status: String,
}

impl From<MemberDbRow> for MemberPublic {
    fn from(row: MemberDbRow) -> Self {
        Self {
            device_id: row.device_id,
            display_name: row.display_name,
            signing_public_key: row.signing_public_key,
            exchange_public_key: row.exchange_public_key,
            fingerprint: row.fingerprint,
            key_epoch: row.key_epoch as u32,
            status: row.status,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_kdf_is_salt_and_passphrase_bound() {
        let a = derive_recovery_key("strong-passphrase", b"salt-a", 100_000);
        let b = derive_recovery_key("strong-passphrase", b"salt-b", 100_000);
        let c = derive_recovery_key("another-passphrase", b"salt-a", 100_000);
        assert_ne!(a.as_slice(), b.as_slice());
        assert_ne!(a.as_slice(), c.as_slice());
    }
}
