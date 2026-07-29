use std::path::{Path, PathBuf};

use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD};
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use hmac::{Hmac, Mac};
use rand::rngs::OsRng;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use zeroize::Zeroize;

use super::crypto::{
    derive_exchange_key, generate_device_keys, generate_group_key, open, seal, sign_detached,
    verify_detached, EnvelopeHeader, PROTOCOL_VERSION,
};
use super::identity::{
    delete_invite_code, load_exchange_secret, load_group_key, load_invite_code,
    load_signing_secret, persist_device_keys, store_group_key, store_invite_code,
};
use super::nas_folder::MountedFolder;
use super::SyncError;

type HmacSha256 = Hmac<Sha256>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairingInvite {
    pub invite_id: String,
    pub group_id: String,
    pub inviter_device_id: String,
    pub inviter_signing_public_key: String,
    pub inviter_exchange_public_key: String,
    pub key_epoch: u32,
    pub expires_at: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatedInvite {
    pub group_id: String,
    pub invite_id: String,
    pub pairing_code: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinRequest {
    pub request_id: String,
    pub invite_id: String,
    pub group_id: String,
    pub device_id: String,
    pub display_name: String,
    pub signing_public_key: String,
    pub exchange_public_key: String,
    pub fingerprint: String,
    pub expires_at: String,
    pub proof_hash: String,
    pub request_signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberPublic {
    pub device_id: String,
    pub display_name: String,
    pub signing_public_key: String,
    pub exchange_public_key: String,
    pub fingerprint: String,
    pub key_epoch: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeyBundle {
    group_id: String,
    recipient_device_id: String,
    key_epoch: u32,
    group_keys_b64: std::collections::BTreeMap<u32, String>,
    members: Vec<MemberPublic>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JoinCompletion {
    pub group_id: String,
    pub device_id: String,
    pub key_epoch: u32,
    pub trusted_member_count: usize,
}

#[derive(Debug, FromRow)]
struct GroupInviteRow {
    connector_root: String,
    local_device_id: String,
    key_epoch: i64,
}

pub async fn create_pairing_invite(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<CreatedInvite, SyncError> {
    let group: GroupInviteRow = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch
         FROM device_sync_groups WHERE id=?1 AND paused=0",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SyncError::NotFound(format!("同步组不存在或已暂停: {group_id}")))?;
    let inviter: Option<(String, String)> = sqlx::query_as(
        "SELECT signing_public_key, exchange_public_key
         FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(group_id)
    .bind(&group.local_device_id)
    .fetch_optional(pool)
    .await?;
    let (signing_public, exchange_public) =
        inviter.ok_or_else(|| SyncError::Integrity("本机不是受信同步成员".to_string()))?;
    let invite_id = uuid::Uuid::new_v4().simple().to_string();
    let mut code_bytes = [0_u8; 20];
    OsRng.fill_bytes(&mut code_bytes);
    let pairing_code = URL_SAFE_NO_PAD.encode(code_bytes);
    let expires_at = Utc::now() + Duration::minutes(30);
    let mut invite = PairingInvite {
        invite_id: invite_id.clone(),
        group_id: group_id.to_string(),
        inviter_device_id: group.local_device_id.clone(),
        inviter_signing_public_key: signing_public,
        inviter_exchange_public_key: exchange_public,
        key_epoch: group.key_epoch as u32,
        expires_at: expires_at.to_rfc3339(),
        signature: String::new(),
    };
    let signing_secret = load_signing_secret(group_id, &group.local_device_id)?;
    invite.signature = sign_detached(&signing_secret, &invite_signing_bytes(&invite)?)?;
    store_invite_code(group_id, &group.local_device_id, &invite_id, &pairing_code)?;
    let code_hash = hash_code(&pairing_code);
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO device_sync_invites (
             id, group_id, inviter_device_id, code_hash, expires_at
         ) VALUES (?1,?2,?3,?4,?5)",
    )
    .bind(&invite_id)
    .bind(group_id)
    .bind(&group.local_device_id)
    .bind(code_hash)
    .bind(expires_at.to_rfc3339())
    .execute(&mut *tx)
    .await?;
    let folder = MountedFolder::connect(PathBuf::from(&group.connector_root))?;
    folder.initialize_group(group_id)?;
    if let Err(error) = folder.write_invite_json(
        group_id,
        &invite_id,
        "invite",
        &serde_json::to_vec(&invite)?,
    ) {
        tx.rollback().await?;
        let _ = delete_invite_code(group_id, &group.local_device_id, &invite_id);
        return Err(error);
    }
    tx.commit().await?;
    Ok(CreatedInvite {
        group_id: group_id.to_string(),
        invite_id,
        pairing_code,
        expires_at: expires_at.to_rfc3339(),
    })
}

pub async fn revoke_pairing_invite(
    pool: &SqlitePool,
    group_id: &str,
    invite_id: &str,
) -> Result<(), SyncError> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT inviter_device_id FROM device_sync_invites
         WHERE id=?1 AND group_id=?2 AND status='active'",
    )
    .bind(invite_id)
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (inviter_device_id,) =
        row.ok_or_else(|| SyncError::NotFound("有效配对邀请不存在".to_string()))?;
    sqlx::query(
        "UPDATE device_sync_invites SET status='revoked',updated_at=datetime('now')
         WHERE id=?1 AND group_id=?2 AND status='active'",
    )
    .bind(invite_id)
    .bind(group_id)
    .execute(pool)
    .await?;
    let _ = delete_invite_code(group_id, &inviter_device_id, invite_id);
    Ok(())
}

pub async fn expire_pairing_invites(pool: &SqlitePool) -> Result<usize, SyncError> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT id,group_id,inviter_device_id,expires_at
         FROM device_sync_invites WHERE status='active'",
    )
    .fetch_all(pool)
    .await?;
    let mut expired = 0;
    for (invite_id, group_id, inviter_device_id, expires_at) in rows {
        if require_not_expired(&expires_at).is_ok() {
            continue;
        }
        let affected = sqlx::query(
            "UPDATE device_sync_invites SET status='expired',updated_at=datetime('now')
             WHERE id=?1 AND status='active'",
        )
        .bind(&invite_id)
        .execute(pool)
        .await?
        .rows_affected();
        if affected == 1 {
            let _ = delete_invite_code(&group_id, &inviter_device_id, &invite_id);
            expired += 1;
        }
    }
    Ok(expired)
}

pub fn create_join_request(
    connector_root: &Path,
    group_id: &str,
    invite_id: &str,
    pairing_code: &str,
    display_name: &str,
) -> Result<JoinRequest, SyncError> {
    let folder = MountedFolder::connect(connector_root)?;
    let invite_path = folder.invite_path(group_id, invite_id, "invite")?;
    let invite: PairingInvite = serde_json::from_slice(&folder.read_group_file(&invite_path)?)?;
    validate_invite(&invite, group_id, invite_id)?;
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 80 {
        return Err(SyncError::Protocol(
            "设备名称不能为空且不能超过 80 个字符".to_string(),
        ));
    }
    if pairing_code.len() < 20 {
        return Err(SyncError::Protocol("一次性配对码长度不足".to_string()));
    }
    let keys = generate_device_keys();
    let device_id = uuid::Uuid::new_v4().simple().to_string();
    persist_device_keys(group_id, &device_id, &keys)?;
    let mut request = JoinRequest {
        request_id: uuid::Uuid::new_v4().simple().to_string(),
        invite_id: invite_id.to_string(),
        group_id: group_id.to_string(),
        device_id,
        display_name: display_name.to_string(),
        signing_public_key: keys.signing_public_b64,
        exchange_public_key: keys.exchange_public_b64,
        fingerprint: keys.fingerprint,
        expires_at: invite.expires_at.clone(),
        proof_hash: String::new(),
        request_signature: String::new(),
    };
    request.proof_hash = join_proof(pairing_code, &join_proof_bytes(&request)?)?;
    request.request_signature =
        sign_detached(&keys.signing_secret, &join_signing_bytes(&request)?)?;
    folder.write_invite_json(
        group_id,
        invite_id,
        "join-request",
        &serde_json::to_vec(&request)?,
    )?;
    Ok(request)
}

pub async fn approve_join(
    pool: &SqlitePool,
    group_id: &str,
    invite_id: &str,
    expected_fingerprint: &str,
) -> Result<MemberPublic, SyncError> {
    let group: GroupInviteRow = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch
         FROM device_sync_groups WHERE id=?1 AND paused=0",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SyncError::NotFound(format!("同步组不存在或已暂停: {group_id}")))?;
    let invite_row: Option<(String, String, String, String)> = sqlx::query_as(
        "SELECT inviter_device_id, expires_at, status, code_hash
         FROM device_sync_invites WHERE id=?1 AND group_id=?2",
    )
    .bind(invite_id)
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (inviter_device_id, expires_at, status, stored_code_hash) =
        invite_row.ok_or_else(|| SyncError::NotFound("配对邀请不存在".to_string()))?;
    if inviter_device_id != group.local_device_id || status != "active" {
        return Err(SyncError::Protocol("配对邀请已核销或已撤销".to_string()));
    }
    require_not_expired(&expires_at)?;
    let folder = MountedFolder::connect(PathBuf::from(&group.connector_root))?;
    let request_path = folder.invite_path(group_id, invite_id, "join-request")?;
    let request: JoinRequest = serde_json::from_slice(&folder.read_group_file(&request_path)?)?;
    if request.group_id != group_id || request.invite_id != invite_id {
        return Err(SyncError::Integrity("加入请求与邀请不匹配".to_string()));
    }
    require_not_expired(&request.expires_at)?;
    verify_detached(
        &request.signing_public_key,
        &join_signing_bytes(&request)?,
        &request.request_signature,
    )?;
    if request.fingerprint != expected_fingerprint {
        return Err(SyncError::Integrity(
            "设备指纹与人工确认值不一致".to_string(),
        ));
    }
    let mut code = load_invite_code(group_id, &group.local_device_id, invite_id)?;
    if !constant_time_eq(hash_code(&code).as_bytes(), stored_code_hash.as_bytes()) {
        code.zeroize();
        return Err(SyncError::Integrity(
            "本地一次性配对码与邀请登记不一致".to_string(),
        ));
    }
    let expected_proof = join_proof(&code, &join_proof_bytes(&request)?)?;
    if !constant_time_eq(expected_proof.as_bytes(), request.proof_hash.as_bytes()) {
        return Err(SyncError::Integrity("一次性配对码证明错误".to_string()));
    }
    code.zeroize();

    let exchange_secret = load_exchange_secret(group_id, &group.local_device_id)?;
    let wrapping_key = derive_exchange_key(
        &exchange_secret,
        &request.exchange_public_key,
        invite_id.as_bytes(),
    )?;
    let mut group_keys_b64 = std::collections::BTreeMap::new();
    for epoch in 1..=group.key_epoch as u32 {
        let group_key = load_group_key(group_id, &group.local_device_id, epoch)?;
        group_keys_b64.insert(epoch, B64.encode(group_key.as_slice()));
    }
    let mut members: Vec<MemberPublic> = sqlx::query_as::<_, MemberDbRow>(
        "SELECT device_id, display_name, signing_public_key, exchange_public_key,
                fingerprint, key_epoch, status
         FROM device_sync_members
         WHERE group_id=?1 AND status='trusted'",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(MemberPublic::from)
    .collect();
    let new_member = MemberPublic {
        device_id: request.device_id.clone(),
        display_name: request.display_name.clone(),
        signing_public_key: request.signing_public_key.clone(),
        exchange_public_key: request.exchange_public_key.clone(),
        fingerprint: request.fingerprint.clone(),
        key_epoch: group.key_epoch as u32,
        status: "trusted".to_string(),
    };
    members.push(new_member.clone());
    let bundle = KeyBundle {
        group_id: group_id.to_string(),
        recipient_device_id: request.device_id.clone(),
        key_epoch: group.key_epoch as u32,
        group_keys_b64,
        members,
    };
    let signing_secret = load_signing_secret(group_id, &group.local_device_id)?;
    let envelope = seal(
        EnvelopeHeader {
            protocol_version: PROTOCOL_VERSION,
            group_id: group_id.to_string(),
            device_id: group.local_device_id.clone(),
            sequence: 0,
            key_epoch: group.key_epoch as u32,
            payload_kind: "member-key-envelope".to_string(),
            created_at: Utc::now().to_rfc3339(),
        },
        &serde_json::to_vec(&bundle)?,
        &wrapping_key,
        &signing_secret,
    )?;
    folder.write_member_envelope(
        group_id,
        &request.device_id,
        group.key_epoch as u32,
        &envelope,
    )?;

    let mut tx = pool.begin().await?;
    let consumed = sqlx::query(
        "UPDATE device_sync_invites
         SET status='consumed', consumed_by_device_id=?1, updated_at=datetime('now')
         WHERE id=?2 AND group_id=?3 AND status='active'",
    )
    .bind(&request.device_id)
    .bind(invite_id)
    .bind(group_id)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if consumed != 1 {
        return Err(SyncError::Protocol("配对邀请已过期或已被使用".to_string()));
    }
    sqlx::query(
        "INSERT INTO device_sync_members (
             group_id, device_id, display_name, signing_public_key,
             exchange_public_key, fingerprint, key_epoch, status
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,'trusted')",
    )
    .bind(group_id)
    .bind(&request.device_id)
    .bind(&request.display_name)
    .bind(&request.signing_public_key)
    .bind(&request.exchange_public_key)
    .bind(&request.fingerprint)
    .bind(group.key_epoch)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO device_sync_join_requests (
             id, invite_id, group_id, device_id, display_name, signing_public_key,
             exchange_public_key, fingerprint, proof_hash, request_signature,
             status, expires_at, approved_at
         ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'approved',?11,datetime('now'))",
    )
    .bind(&request.request_id)
    .bind(invite_id)
    .bind(group_id)
    .bind(&request.device_id)
    .bind(&request.display_name)
    .bind(&request.signing_public_key)
    .bind(&request.exchange_public_key)
    .bind(&request.fingerprint)
    .bind(&request.proof_hash)
    .bind(&request.request_signature)
    .bind(&request.expires_at)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    let _ = delete_invite_code(group_id, &group.local_device_id, invite_id);
    Ok(new_member)
}

pub async fn complete_join(
    pool: &SqlitePool,
    connector_root: &Path,
    invite_id: &str,
    request: &JoinRequest,
) -> Result<JoinCompletion, SyncError> {
    require_not_expired(&request.expires_at)?;
    verify_detached(
        &request.signing_public_key,
        &join_signing_bytes(request)?,
        &request.request_signature,
    )?;
    let folder = MountedFolder::connect(connector_root)?;
    let invite_path = folder.invite_path(&request.group_id, invite_id, "invite")?;
    let invite: PairingInvite = serde_json::from_slice(&folder.read_group_file(&invite_path)?)?;
    validate_invite(&invite, &request.group_id, invite_id)?;
    let envelope_path =
        folder.member_envelope_path(&request.group_id, &request.device_id, invite.key_epoch)?;
    let envelope = folder.read_envelope(&envelope_path)?;
    if envelope.header.group_id != request.group_id
        || envelope.header.device_id != invite.inviter_device_id
        || envelope.header.payload_kind != "member-key-envelope"
    {
        return Err(SyncError::Integrity("成员密钥信封头不匹配".to_string()));
    }
    let exchange_secret = load_exchange_secret(&request.group_id, &request.device_id)?;
    let wrapping_key = derive_exchange_key(
        &exchange_secret,
        &invite.inviter_exchange_public_key,
        invite_id.as_bytes(),
    )?;
    let plaintext = open(&envelope, &wrapping_key, &invite.inviter_signing_public_key)?;
    let bundle: KeyBundle = serde_json::from_slice(&plaintext)?;
    if bundle.group_id != request.group_id
        || bundle.recipient_device_id != request.device_id
        || bundle.key_epoch != envelope.header.key_epoch
    {
        return Err(SyncError::Integrity("成员密钥包正文不匹配".to_string()));
    }
    if !bundle.group_keys_b64.contains_key(&bundle.key_epoch) {
        return Err(SyncError::Integrity(
            "成员密钥包缺少当前时代密钥".to_string(),
        ));
    }
    for (epoch, encoded) in &bundle.group_keys_b64 {
        if *epoch == 0 || *epoch > bundle.key_epoch {
            return Err(SyncError::Integrity("成员密钥包时代非法".to_string()));
        }
        let mut group_key = B64
            .decode(encoded)
            .map_err(|_| SyncError::Crypto("组密钥包编码损坏".to_string()))?;
        if group_key.len() != 32 {
            group_key.zeroize();
            return Err(SyncError::Crypto("组密钥长度错误".to_string()));
        }
        store_group_key(&request.group_id, &request.device_id, *epoch, &group_key)?;
        group_key.zeroize();
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "INSERT INTO device_sync_groups (
             id, connector_root, local_device_id, protocol_version, key_epoch
         ) VALUES (?1,?2,?3,1,?4)",
    )
    .bind(&request.group_id)
    .bind(connector_root.to_string_lossy().as_ref())
    .bind(&request.device_id)
    .bind(bundle.key_epoch as i64)
    .execute(&mut *tx)
    .await?;
    for member in &bundle.members {
        sqlx::query(
            "INSERT INTO device_sync_members (
                 group_id, device_id, display_name, signing_public_key,
                 exchange_public_key, fingerprint, key_epoch, status
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        )
        .bind(&request.group_id)
        .bind(&member.device_id)
        .bind(&member.display_name)
        .bind(&member.signing_public_key)
        .bind(&member.exchange_public_key)
        .bind(&member.fingerprint)
        .bind(member.key_epoch as i64)
        .bind(&member.status)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(JoinCompletion {
        group_id: request.group_id.clone(),
        device_id: request.device_id.clone(),
        key_epoch: bundle.key_epoch,
        trusted_member_count: bundle.members.len(),
    })
}

pub async fn revoke_device(
    pool: &SqlitePool,
    group_id: &str,
    target_device_id: &str,
    expected_fingerprint: &str,
) -> Result<u32, SyncError> {
    let group: GroupInviteRow = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    if target_device_id == group.local_device_id {
        return Err(SyncError::Protocol(
            "不能在本机直接撤销当前设备".to_string(),
        ));
    }
    let actual: Option<String> = sqlx::query_scalar(
        "SELECT fingerprint FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(group_id)
    .bind(target_device_id)
    .fetch_optional(pool)
    .await?;
    if actual.as_deref() != Some(expected_fingerprint) {
        return Err(SyncError::Integrity("撤销设备指纹不匹配".to_string()));
    }
    let next_epoch = group.key_epoch as u32 + 1;
    let new_key = generate_group_key();
    store_group_key(group_id, &group.local_device_id, next_epoch, &new_key)?;
    let signing = load_signing_secret(group_id, &group.local_device_id)?;
    let exchange = load_exchange_secret(group_id, &group.local_device_id)?;
    let members: Vec<MemberPublic> = sqlx::query_as::<_, MemberDbRow>(
        "SELECT device_id, display_name, signing_public_key, exchange_public_key,
                fingerprint, key_epoch, status
         FROM device_sync_members
         WHERE group_id=?1 AND status='trusted' AND device_id<>?2",
    )
    .bind(group_id)
    .bind(target_device_id)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(MemberPublic::from)
    .collect();
    let folder = MountedFolder::connect(PathBuf::from(&group.connector_root))?;
    for member in &members {
        if member.device_id == group.local_device_id {
            continue;
        }
        let path = folder.member_envelope_path(group_id, &member.device_id, next_epoch)?;
        if path.exists() {
            continue;
        }
        let wrapping =
            derive_exchange_key(&exchange, &member.exchange_public_key, b"key-rotation")?;
        let bundle = KeyBundle {
            group_id: group_id.to_string(),
            recipient_device_id: member.device_id.clone(),
            key_epoch: next_epoch,
            group_keys_b64: std::collections::BTreeMap::from([(
                next_epoch,
                B64.encode(new_key.as_slice()),
            )]),
            members: members
                .iter()
                .cloned()
                .map(|mut item| {
                    item.key_epoch = next_epoch;
                    item
                })
                .collect(),
        };
        let envelope = seal(
            EnvelopeHeader {
                protocol_version: PROTOCOL_VERSION,
                group_id: group_id.to_string(),
                device_id: group.local_device_id.clone(),
                sequence: 0,
                key_epoch: next_epoch,
                payload_kind: "member-key-envelope".to_string(),
                created_at: Utc::now().to_rfc3339(),
            },
            &serde_json::to_vec(&bundle)?,
            &wrapping,
            &signing,
        )?;
        folder.write_member_envelope(group_id, &member.device_id, next_epoch, &envelope)?;
    }
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE device_sync_members
         SET status='revoked', revoked_at=datetime('now'), updated_at=datetime('now')
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(group_id)
    .bind(target_device_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE device_sync_members
         SET key_epoch=?1, updated_at=datetime('now')
         WHERE group_id=?2 AND status='trusted'",
    )
    .bind(next_epoch as i64)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE device_sync_groups
         SET key_epoch=?1, updated_at=datetime('now') WHERE id=?2",
    )
    .bind(next_epoch as i64)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(next_epoch)
}

pub async fn accept_key_rotation(
    pool: &SqlitePool,
    group_id: &str,
    author_device_id: &str,
    new_epoch: u32,
) -> Result<u32, SyncError> {
    let group: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (connector_root, local_device_id, current_epoch) =
        group.ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    if new_epoch <= current_epoch as u32 {
        return Ok(current_epoch as u32);
    }
    if new_epoch != current_epoch as u32 + 1 {
        return Err(SyncError::Integrity(
            "密钥时代必须逐代安装，拒绝跳跃".to_string(),
        ));
    }
    let author: Option<(String, String)> = sqlx::query_as(
        "SELECT signing_public_key, exchange_public_key
         FROM device_sync_members
         WHERE group_id=?1 AND device_id=?2 AND status='trusted'",
    )
    .bind(group_id)
    .bind(author_device_id)
    .fetch_optional(pool)
    .await?;
    let (author_signing_public, author_exchange_public) =
        author.ok_or_else(|| SyncError::Integrity("轮换发起设备不受信".to_string()))?;
    let folder = MountedFolder::connect(PathBuf::from(connector_root))?;
    let path = folder.member_envelope_path(group_id, &local_device_id, new_epoch)?;
    let envelope = folder.read_envelope(&path)?;
    if envelope.header.device_id != author_device_id
        || envelope.header.group_id != group_id
        || envelope.header.key_epoch != new_epoch
        || envelope.header.payload_kind != "member-key-envelope"
    {
        return Err(SyncError::Integrity("密钥轮换信封头不匹配".to_string()));
    }
    let local_exchange = load_exchange_secret(group_id, &local_device_id)?;
    let wrapping = derive_exchange_key(&local_exchange, &author_exchange_public, b"key-rotation")?;
    let plaintext = open(&envelope, &wrapping, &author_signing_public)?;
    let bundle: KeyBundle = serde_json::from_slice(&plaintext)?;
    if bundle.group_id != group_id
        || bundle.recipient_device_id != local_device_id
        || bundle.key_epoch != new_epoch
        || !bundle
            .members
            .iter()
            .any(|member| member.device_id == local_device_id && member.status == "trusted")
    {
        return Err(SyncError::Integrity("密钥轮换正文未授权本机".to_string()));
    }
    let encoded_key = bundle
        .group_keys_b64
        .get(&new_epoch)
        .ok_or_else(|| SyncError::Integrity("轮换包缺少目标时代密钥".to_string()))?;
    let mut group_key = B64
        .decode(encoded_key)
        .map_err(|_| SyncError::Crypto("轮换组密钥编码损坏".to_string()))?;
    if group_key.len() != 32 {
        group_key.zeroize();
        return Err(SyncError::Crypto("轮换组密钥长度错误".to_string()));
    }
    store_group_key(group_id, &local_device_id, new_epoch, &group_key)?;
    group_key.zeroize();
    let active_ids = bundle
        .members
        .iter()
        .map(|member| member.device_id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let mut tx = pool.begin().await?;
    let existing: Vec<String> = sqlx::query_scalar(
        "SELECT device_id FROM device_sync_members
         WHERE group_id=?1 AND status='trusted'",
    )
    .bind(group_id)
    .fetch_all(&mut *tx)
    .await?;
    for device_id in existing {
        if !active_ids.contains(&device_id) {
            sqlx::query(
                "UPDATE device_sync_members
                 SET status='revoked', revoked_at=datetime('now'), updated_at=datetime('now')
                 WHERE group_id=?1 AND device_id=?2",
            )
            .bind(group_id)
            .bind(device_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    for member in &bundle.members {
        sqlx::query(
            "INSERT INTO device_sync_members (
                 group_id,device_id,display_name,signing_public_key,
                 exchange_public_key,fingerprint,key_epoch,status
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,'trusted')
             ON CONFLICT(group_id,device_id) DO UPDATE SET
                 display_name=excluded.display_name,
                 signing_public_key=excluded.signing_public_key,
                 exchange_public_key=excluded.exchange_public_key,
                 fingerprint=excluded.fingerprint,
                 key_epoch=excluded.key_epoch,
                 status='trusted',
                 revoked_at=NULL,
                 updated_at=datetime('now')",
        )
        .bind(group_id)
        .bind(&member.device_id)
        .bind(&member.display_name)
        .bind(&member.signing_public_key)
        .bind(&member.exchange_public_key)
        .bind(&member.fingerprint)
        .bind(new_epoch as i64)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query(
        "UPDATE device_sync_groups SET key_epoch=?1,updated_at=datetime('now') WHERE id=?2",
    )
    .bind(new_epoch as i64)
    .bind(group_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(new_epoch)
}

pub async fn accept_pending_key_rotation(
    pool: &SqlitePool,
    group_id: &str,
) -> Result<Option<u32>, SyncError> {
    let group: Option<(String, String, i64)> = sqlx::query_as(
        "SELECT connector_root, local_device_id, key_epoch
         FROM device_sync_groups WHERE id=?1",
    )
    .bind(group_id)
    .fetch_optional(pool)
    .await?;
    let (connector_root, local_device_id, current_epoch) =
        group.ok_or_else(|| SyncError::NotFound(format!("同步组不存在: {group_id}")))?;
    let next_epoch = current_epoch as u32 + 1;
    let folder = MountedFolder::connect(PathBuf::from(connector_root))?;
    let path = folder.member_envelope_path(group_id, &local_device_id, next_epoch)?;
    if !path.exists() {
        return Ok(None);
    }
    let envelope = folder.read_envelope(&path)?;
    let installed =
        accept_key_rotation(pool, group_id, &envelope.header.device_id, next_epoch).await?;
    Ok(Some(installed))
}

#[derive(Debug, FromRow)]
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

fn validate_invite(
    invite: &PairingInvite,
    group_id: &str,
    invite_id: &str,
) -> Result<(), SyncError> {
    if invite.group_id != group_id || invite.invite_id != invite_id {
        return Err(SyncError::Integrity("邀请标识不匹配".to_string()));
    }
    require_not_expired(&invite.expires_at)?;
    verify_detached(
        &invite.inviter_signing_public_key,
        &invite_signing_bytes(invite)?,
        &invite.signature,
    )
}

fn require_not_expired(value: &str) -> Result<(), SyncError> {
    let expires = DateTime::parse_from_rfc3339(value)
        .map_err(|_| SyncError::Protocol("配对过期时间无效".to_string()))?
        .with_timezone(&Utc);
    if expires <= Utc::now() {
        return Err(SyncError::Protocol("配对邀请已过期".to_string()));
    }
    Ok(())
}

fn invite_signing_bytes(invite: &PairingInvite) -> Result<Vec<u8>, SyncError> {
    serde_json::to_vec(&(
        &invite.invite_id,
        &invite.group_id,
        &invite.inviter_device_id,
        &invite.inviter_signing_public_key,
        &invite.inviter_exchange_public_key,
        invite.key_epoch,
        &invite.expires_at,
    ))
    .map_err(SyncError::from)
}

fn join_proof_bytes(request: &JoinRequest) -> Result<Vec<u8>, SyncError> {
    serde_json::to_vec(&(
        &request.request_id,
        &request.invite_id,
        &request.group_id,
        &request.device_id,
        &request.display_name,
        &request.signing_public_key,
        &request.exchange_public_key,
        &request.fingerprint,
        &request.expires_at,
    ))
    .map_err(SyncError::from)
}

fn join_signing_bytes(request: &JoinRequest) -> Result<Vec<u8>, SyncError> {
    let mut bytes = join_proof_bytes(request)?;
    bytes.extend_from_slice(request.proof_hash.as_bytes());
    Ok(bytes)
}

fn join_proof(code: &str, message: &[u8]) -> Result<String, SyncError> {
    let mut mac = HmacSha256::new_from_slice(code.as_bytes())
        .map_err(|_| SyncError::Crypto("无法初始化配对证明".to_string()))?;
    mac.update(b"fanglv-caseboard-join-proof-v1");
    mac.update(message);
    Ok(B64.encode(mac.finalize().into_bytes()))
}

fn hash_code(code: &str) -> String {
    Sha256::digest(code.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_proof_is_code_and_request_bound() {
        let request = JoinRequest {
            request_id: "r".into(),
            invite_id: "i".into(),
            group_id: "g".into(),
            device_id: "d".into(),
            display_name: "PC".into(),
            signing_public_key: "s".into(),
            exchange_public_key: "x".into(),
            fingerprint: "f".into(),
            expires_at: "2099-01-01T00:00:00Z".into(),
            proof_hash: String::new(),
            request_signature: String::new(),
        };
        let bytes = join_proof_bytes(&request).unwrap();
        assert_ne!(
            join_proof("code-one", &bytes).unwrap(),
            join_proof("code-two", &bytes).unwrap()
        );
    }

    #[test]
    fn expired_invite_is_rejected_before_any_key_exchange() {
        assert!(require_not_expired("2020-01-01T00:00:00Z").is_err());
        assert!(require_not_expired("not-a-date").is_err());
    }
}
