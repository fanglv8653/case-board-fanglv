use std::path::Path;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use zeroize::{Zeroize, Zeroizing};

use super::crypto::{generate_device_keys, generate_group_key, DeviceKeyMaterial};
use super::nas_folder::MountedFolder;
use super::SyncError;

#[cfg(target_os = "windows")]
const CREDENTIAL_PREFIX: &str = "FanglvCaseBoard/device-sync";
#[cfg(target_os = "windows")]
const MAX_CREDENTIAL_BLOB_BYTES: usize = 2560;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalDeviceIdentity {
    pub group_id: String,
    pub device_id: String,
    pub display_name: String,
    pub signing_public_key: String,
    pub exchange_public_key: String,
    pub fingerprint: String,
    pub key_epoch: u32,
}

pub async fn create_sync_group(
    pool: &SqlitePool,
    connector_root: &Path,
    display_name: &str,
) -> Result<LocalDeviceIdentity, SyncError> {
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 80 {
        return Err(SyncError::Protocol(
            "设备名称不能为空且不能超过 80 个字符".to_string(),
        ));
    }
    let folder = MountedFolder::connect(connector_root)?;
    let group_id = uuid::Uuid::new_v4().simple().to_string();
    let device_id = uuid::Uuid::new_v4().simple().to_string();
    folder.initialize_group(&group_id)?;

    let device_keys = generate_device_keys();
    let group_key = generate_group_key();
    let secrets = [
        (
            credential_account(&group_id, &device_id, "signing"),
            B64.encode(device_keys.signing_secret.as_slice()),
        ),
        (
            credential_account(&group_id, &device_id, "exchange"),
            B64.encode(device_keys.exchange_secret.as_slice()),
        ),
        (
            credential_account(&group_id, &device_id, "group-key-1"),
            B64.encode(group_key.as_slice()),
        ),
    ];
    let mut stored = Vec::new();
    for (account, value) in &secrets {
        if let Err(error) = credential_set(account, value) {
            for account in stored {
                let _ = credential_delete(account);
            }
            return Err(error);
        }
        stored.push(account.as_str());
    }

    let mut tx = pool.begin().await?;
    let db_result = async {
        sqlx::query(
            "INSERT INTO device_sync_groups (
                 id, connector_root, local_device_id, protocol_version, key_epoch
             ) VALUES (?1, ?2, ?3, 1, 1)",
        )
        .bind(&group_id)
        .bind(connector_root.to_string_lossy().as_ref())
        .bind(&device_id)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO device_sync_members (
                 group_id, device_id, display_name, signing_public_key,
                 exchange_public_key, fingerprint, key_epoch, status
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 1, 'trusted')",
        )
        .bind(&group_id)
        .bind(&device_id)
        .bind(display_name)
        .bind(&device_keys.signing_public_b64)
        .bind(&device_keys.exchange_public_b64)
        .bind(&device_keys.fingerprint)
        .execute(&mut *tx)
        .await?;
        Ok::<(), sqlx::Error>(())
    }
    .await;
    if let Err(error) = db_result {
        for account in stored {
            let _ = credential_delete(account);
        }
        return Err(SyncError::Database(error.to_string()));
    }
    tx.commit().await?;

    Ok(LocalDeviceIdentity {
        group_id,
        device_id,
        display_name: display_name.to_string(),
        signing_public_key: device_keys.signing_public_b64,
        exchange_public_key: device_keys.exchange_public_b64,
        fingerprint: device_keys.fingerprint,
        key_epoch: 1,
    })
}

pub fn load_signing_secret(
    group_id: &str,
    device_id: &str,
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    load_secret(&credential_account(group_id, device_id, "signing"))
}

pub fn load_group_key(
    group_id: &str,
    device_id: &str,
    epoch: u32,
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    load_secret(&credential_account(
        group_id,
        device_id,
        &format!("group-key-{epoch}"),
    ))
}

pub(crate) fn persist_device_keys(
    group_id: &str,
    device_id: &str,
    keys: &DeviceKeyMaterial,
) -> Result<(), SyncError> {
    credential_set(
        &credential_account(group_id, device_id, "signing"),
        &B64.encode(keys.signing_secret.as_slice()),
    )?;
    if let Err(error) = credential_set(
        &credential_account(group_id, device_id, "exchange"),
        &B64.encode(keys.exchange_secret.as_slice()),
    ) {
        let _ = credential_delete(&credential_account(group_id, device_id, "signing"));
        return Err(error);
    }
    Ok(())
}

pub(crate) fn load_exchange_secret(
    group_id: &str,
    device_id: &str,
) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    load_secret(&credential_account(group_id, device_id, "exchange"))
}

pub(crate) fn store_group_key(
    group_id: &str,
    device_id: &str,
    epoch: u32,
    key: &[u8],
) -> Result<(), SyncError> {
    credential_set(
        &credential_account(group_id, device_id, &format!("group-key-{epoch}")),
        &B64.encode(key),
    )
}

pub(crate) fn store_invite_code(
    group_id: &str,
    device_id: &str,
    invite_id: &str,
    code: &str,
) -> Result<(), SyncError> {
    credential_set(
        &credential_account(group_id, device_id, &format!("invite-{invite_id}")),
        code,
    )
}

pub(crate) fn load_invite_code(
    group_id: &str,
    device_id: &str,
    invite_id: &str,
) -> Result<Zeroizing<String>, SyncError> {
    credential_get(&credential_account(
        group_id,
        device_id,
        &format!("invite-{invite_id}"),
    ))?
    .map(Zeroizing::new)
    .ok_or_else(|| SyncError::CredentialStore("一次性配对码不存在".to_string()))
}

pub(crate) fn delete_invite_code(
    group_id: &str,
    device_id: &str,
    invite_id: &str,
) -> Result<(), SyncError> {
    credential_delete(&credential_account(
        group_id,
        device_id,
        &format!("invite-{invite_id}"),
    ))
}

pub(crate) fn delete_device_secrets(group_id: &str, device_id: &str, through_epoch: u32) {
    let _ = credential_delete(&credential_account(group_id, device_id, "signing"));
    let _ = credential_delete(&credential_account(group_id, device_id, "exchange"));
    for epoch in 1..=through_epoch {
        let _ = credential_delete(&credential_account(
            group_id,
            device_id,
            &format!("group-key-{epoch}"),
        ));
    }
}

fn load_secret(account: &str) -> Result<Zeroizing<Vec<u8>>, SyncError> {
    let mut encoded = credential_get(account)?
        .ok_or_else(|| SyncError::CredentialStore("同步密钥不存在".to_string()))?;
    let decoded = B64
        .decode(encoded.as_bytes())
        .map_err(|_| SyncError::CredentialStore("同步密钥编码损坏".to_string()));
    encoded.zeroize();
    decoded.map(Zeroizing::new)
}

fn credential_account(group_id: &str, device_id: &str, kind: &str) -> String {
    format!("{group_id}/{device_id}/{kind}")
}

#[cfg(target_os = "windows")]
fn credential_target(account: &str) -> String {
    format!("{CREDENTIAL_PREFIX}/{account}")
}

#[cfg(target_os = "windows")]
fn credential_set(account: &str, value: &str) -> Result<(), SyncError> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    if value.len() > MAX_CREDENTIAL_BLOB_BYTES {
        return Err(SyncError::CredentialStore(
            "同步密钥超过 Windows 凭据容量".to_string(),
        ));
    }
    let mut target = wide_null(&credential_target(account));
    let mut username = wide_null("FanglvCaseBoard");
    let mut blob = value.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target.as_mut_ptr()),
        CredentialBlobSize: blob.len() as u32,
        CredentialBlob: blob.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        UserName: PWSTR(username.as_mut_ptr()),
        ..Default::default()
    };
    let result = unsafe { CredWriteW(&credential, 0) }
        .map_err(|_| SyncError::CredentialStore("写入 Windows 凭据失败".to_string()));
    blob.zeroize();
    result
}

#[cfg(not(target_os = "windows"))]
fn credential_set(_account: &str, _value: &str) -> Result<(), SyncError> {
    Err(SyncError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn credential_get(account: &str) -> Result<Option<String>, SyncError> {
    use std::ptr::null_mut;
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = wide_null(&credential_target(account));
    let mut raw: *mut CREDENTIALW = null_mut();
    let read = unsafe { CredReadW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None, &mut raw) };
    if let Err(error) = read {
        if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) {
            return Ok(None);
        }
        return Err(SyncError::CredentialStore(
            "读取 Windows 凭据失败".to_string(),
        ));
    }
    if raw.is_null() {
        return Err(SyncError::CredentialStore(
            "Windows 凭据返回空指针".to_string(),
        ));
    }
    let credential = unsafe { &*raw };
    let blob = if credential.CredentialBlobSize == 0 {
        &[][..]
    } else if credential.CredentialBlob.is_null()
        || credential.CredentialBlobSize as usize > MAX_CREDENTIAL_BLOB_BYTES
    {
        unsafe { CredFree(raw.cast()) };
        return Err(SyncError::CredentialStore(
            "Windows 凭据内容无效".to_string(),
        ));
    } else {
        unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        }
    };
    let value = String::from_utf8(blob.to_vec())
        .map_err(|_| SyncError::CredentialStore("Windows 凭据不是 UTF-8".to_string()));
    unsafe { CredFree(raw.cast()) };
    value.map(Some)
}

#[cfg(not(target_os = "windows"))]
fn credential_get(_account: &str) -> Result<Option<String>, SyncError> {
    Err(SyncError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn credential_delete(account: &str) -> Result<(), SyncError> {
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide_null(&credential_target(account));
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
        Err(_) => Err(SyncError::CredentialStore(
            "删除 Windows 凭据失败".to_string(),
        )),
    }
}

#[cfg(not(target_os = "windows"))]
fn credential_delete(_account: &str) -> Result<(), SyncError> {
    Err(SyncError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
