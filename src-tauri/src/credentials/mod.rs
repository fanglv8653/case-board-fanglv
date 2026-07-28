//! Secure credential storage shared by CaseBoard integrations.
//!
//! Secret values are intentionally neither serializable nor displayable. The
//! production backend is Windows Credential Manager; other platforms fail
//! closed until an equivalent secure backend is implemented.

use std::fmt;

use serde::Serialize;
use zeroize::Zeroize;

#[cfg(target_os = "windows")]
mod windows;

pub const SERVICE: &str = "com.fanglv.caseboard.credentials.v1";
pub const MAX_SECRET_BYTES: usize = 5 * 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CredentialError {
    #[error("当前平台不支持安全凭据存储")]
    UnsupportedSecureStore,
    #[error("凭据定位符无效")]
    InvalidLocator,
    #[error("凭据值无效")]
    InvalidSecret,
    #[error("安全凭据存储不可用")]
    SecureStore,
    #[error("凭据写入校验失败")]
    VerificationFailed,
    #[error("凭据更新失败且回滚未完成")]
    RollbackFailed,
}

impl CredentialError {
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedSecureStore => "UNSUPPORTED_SECURE_STORE",
            Self::InvalidLocator => "INVALID_CREDENTIAL_LOCATOR",
            Self::InvalidSecret => "INVALID_CREDENTIAL_SECRET",
            Self::SecureStore => "SECURE_STORE_UNAVAILABLE",
            Self::VerificationFailed => "CREDENTIAL_VERIFY_FAILED",
            Self::RollbackFailed => "CREDENTIAL_ROLLBACK_FAILED",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CredentialLocator(String);

impl CredentialLocator {
    pub fn new(scope: &str, owner: &str, slot: &str) -> Result<Self, CredentialError> {
        let value = format!("{scope}/{owner}/{slot}");
        let valid = !value.is_empty()
            && value.len() <= 240
            && [scope, owner, slot].iter().all(|segment| {
                !segment.is_empty()
                    && segment.bytes().all(|byte| {
                        byte.is_ascii_lowercase()
                            || byte.is_ascii_digit()
                            || matches!(byte, b'-' | b'_')
                    })
            });
        if valid {
            Ok(Self(value))
        } else {
            Err(CredentialError::InvalidLocator)
        }
    }

    pub fn id(&self) -> &str {
        &self.0
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn target_name(&self) -> String {
        format!("{SERVICE}/{}", self.0)
    }
}

impl fmt::Debug for CredentialLocator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("CredentialLocator").field(&self.0).finish()
    }
}

pub struct SecretValue(String);

impl SecretValue {
    pub fn new(value: String) -> Result<Self, CredentialError> {
        if value.trim().is_empty() || value.len() > MAX_SECRET_BYTES {
            return Err(CredentialError::InvalidSecret);
        }
        Ok(Self(value))
    }

    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    pub fn into_string(mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

impl Clone for SecretValue {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretValue([REDACTED])")
    }
}

impl Drop for SecretValue {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

pub trait CredentialBackend {
    fn set(
        &mut self,
        locator: &CredentialLocator,
        secret: &SecretValue,
    ) -> Result<(), CredentialError>;
    fn get(&mut self, locator: &CredentialLocator) -> Result<Option<SecretValue>, CredentialError>;
    fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError>;
}

pub struct SystemCredentialBackend;

impl CredentialBackend for SystemCredentialBackend {
    fn set(
        &mut self,
        locator: &CredentialLocator,
        secret: &SecretValue,
    ) -> Result<(), CredentialError> {
        #[cfg(target_os = "windows")]
        {
            windows::set(locator, secret)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = (locator, secret);
            Err(CredentialError::UnsupportedSecureStore)
        }
    }

    fn get(&mut self, locator: &CredentialLocator) -> Result<Option<SecretValue>, CredentialError> {
        #[cfg(target_os = "windows")]
        {
            windows::get(locator)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = locator;
            Err(CredentialError::UnsupportedSecureStore)
        }
    }

    fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
        #[cfg(target_os = "windows")]
        {
            windows::delete(locator)
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = locator;
            Err(CredentialError::UnsupportedSecureStore)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StaticCredential {
    Mineru,
    PaddleVl,
    Deepseek,
    Minimax,
    Glm,
    Mimo,
    Custom,
    Yuandian,
    KuaidiCustomer,
    KuaidiKey,
    Embedding,
    CourtFilingAccount,
    CourtFilingPassword,
}

impl StaticCredential {
    pub const ALL: [Self; 13] = [
        Self::Mineru,
        Self::PaddleVl,
        Self::Deepseek,
        Self::Minimax,
        Self::Glm,
        Self::Mimo,
        Self::Custom,
        Self::Yuandian,
        Self::KuaidiCustomer,
        Self::KuaidiKey,
        Self::Embedding,
        Self::CourtFilingAccount,
        Self::CourtFilingPassword,
    ];

    pub const fn parts(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Mineru => ("provider", "mineru", "api-key"),
            Self::PaddleVl => ("provider", "paddle-vl", "api-key"),
            Self::Deepseek => ("provider", "deepseek", "api-key"),
            Self::Minimax => ("provider", "minimax", "api-key"),
            Self::Glm => ("provider", "glm", "api-key"),
            Self::Mimo => ("provider", "mimo", "api-key"),
            Self::Custom => ("provider", "custom", "api-key"),
            Self::Yuandian => ("provider", "yuandian", "api-key"),
            Self::KuaidiCustomer => ("provider", "kuaidi100", "customer"),
            Self::KuaidiKey => ("provider", "kuaidi100", "key"),
            Self::Embedding => ("provider", "embedding", "api-key"),
            Self::CourtFilingAccount => ("court-filing", "zxfw", "account"),
            Self::CourtFilingPassword => ("court-filing", "zxfw", "password"),
        }
    }

    pub fn locator(self) -> CredentialLocator {
        let (scope, owner, slot) = self.parts();
        CredentialLocator::new(scope, owner, slot).expect("static locator is valid")
    }

    pub fn from_locator_id(id: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|slot| slot.locator().id() == id)
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CredentialStatus {
    pub locator: String,
    pub configured: bool,
    pub backend: &'static str,
    pub error_code: Option<&'static str>,
}

pub fn status_with<B: CredentialBackend>(
    backend: &mut B,
    locator: &CredentialLocator,
) -> CredentialStatus {
    match backend.get(locator) {
        Ok(value) => CredentialStatus {
            locator: locator.id().to_string(),
            configured: value.is_some(),
            backend: "windows_credential_manager",
            error_code: None,
        },
        Err(error) => CredentialStatus {
            locator: locator.id().to_string(),
            configured: false,
            backend: "windows_credential_manager",
            error_code: Some(error.code()),
        },
    }
}

pub fn static_statuses() -> Vec<CredentialStatus> {
    let mut backend = SystemCredentialBackend;
    StaticCredential::ALL
        .iter()
        .map(|slot| status_with(&mut backend, &slot.locator()))
        .collect()
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let max = left.len().max(right.len());
    let mut diff = left.len() ^ right.len();
    for index in 0..max {
        diff |= usize::from(
            left.get(index).copied().unwrap_or(0) ^ right.get(index).copied().unwrap_or(0),
        );
    }
    diff == 0
}

pub fn replace_verified_with<B: CredentialBackend>(
    backend: &mut B,
    locator: &CredentialLocator,
    secret: &SecretValue,
) -> Result<(), CredentialError> {
    let previous = backend.get(locator)?;
    let result = (|| {
        backend.set(locator, secret)?;
        let readback = backend
            .get(locator)?
            .ok_or(CredentialError::VerificationFailed)?;
        if !constant_time_eq(readback.expose().as_bytes(), secret.expose().as_bytes()) {
            return Err(CredentialError::VerificationFailed);
        }
        Ok(())
    })();
    if let Err(original) = result {
        let rollback = match previous {
            Some(ref value) => backend.set(locator, value),
            None => backend.delete(locator),
        };
        return if rollback.is_ok() {
            Err(original)
        } else {
            Err(CredentialError::RollbackFailed)
        };
    }
    Ok(())
}

pub fn replace_verified(
    locator: &CredentialLocator,
    secret: &SecretValue,
) -> Result<(), CredentialError> {
    replace_verified_with(&mut SystemCredentialBackend, locator, secret)
}

pub fn delete_verified_with<B: CredentialBackend>(
    backend: &mut B,
    locator: &CredentialLocator,
) -> Result<(), CredentialError> {
    let previous = backend.get(locator)?;
    backend.delete(locator)?;
    match backend.get(locator) {
        Ok(None) => Ok(()),
        Ok(Some(_)) | Err(_) => {
            let restored = match previous {
                Some(ref value) => backend.set(locator, value),
                None => backend.delete(locator),
            };
            if restored.is_ok() {
                Err(CredentialError::VerificationFailed)
            } else {
                Err(CredentialError::RollbackFailed)
            }
        }
    }
}

pub fn delete_verified(locator: &CredentialLocator) -> Result<(), CredentialError> {
    delete_verified_with(&mut SystemCredentialBackend, locator)
}

pub fn resolve(locator: &CredentialLocator) -> Result<Option<SecretValue>, CredentialError> {
    SystemCredentialBackend.get(locator)
}

pub fn resolve_static(slot: StaticCredential) -> Result<Option<SecretValue>, CredentialError> {
    resolve(&slot.locator())
}

pub(crate) fn resolve_static_string(
    slot: StaticCredential,
) -> Result<Option<String>, CredentialError> {
    resolve_static(slot).map(|value| value.map(SecretValue::into_string))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryBackend {
        values: HashMap<String, String>,
        corrupt_readback: bool,
        fail_rollback: bool,
        writes: usize,
    }

    impl CredentialBackend for MemoryBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            self.writes += 1;
            if self.fail_rollback && self.writes > 1 {
                return Err(CredentialError::SecureStore);
            }
            self.values
                .insert(locator.id().to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.values
                .get(locator.id())
                .cloned()
                .map(|mut value| {
                    if self.corrupt_readback && self.writes > 0 {
                        value.push('x');
                    }
                    SecretValue::new(value)
                })
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.values.remove(locator.id());
            Ok(())
        }
    }

    #[test]
    fn static_targets_are_stable_and_distinct() {
        let mut ids = StaticCredential::ALL
            .iter()
            .map(|slot| slot.locator().id().to_string())
            .collect::<Vec<_>>();
        let count = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), count);
        assert!(ids.contains(&"provider/mineru/api-key".to_string()));
        assert!(ids.contains(&"provider/kuaidi100/customer".to_string()));
    }

    #[test]
    fn secret_debug_is_redacted_and_size_is_checked_in_bytes() {
        let marker = "secret-marker-value";
        let secret = SecretValue::new(marker.to_string()).expect("secret");
        assert!(!format!("{secret:?}").contains(marker));
        assert_eq!(
            SecretValue::new("密".repeat(MAX_SECRET_BYTES)).expect_err("too many bytes"),
            CredentialError::InvalidSecret
        );
    }

    #[test]
    fn replace_verifies_and_rolls_back_previous_value() {
        let locator = StaticCredential::Mineru.locator();
        let old = SecretValue::new("old-secret".to_string()).expect("old");
        let new = SecretValue::new("new-secret".to_string()).expect("new");
        let mut backend = MemoryBackend::default();
        backend.set(&locator, &old).expect("seed");
        backend.writes = 0;
        backend.corrupt_readback = true;
        assert_eq!(
            replace_verified_with(&mut backend, &locator, &new),
            Err(CredentialError::VerificationFailed)
        );
        backend.corrupt_readback = false;
        assert_eq!(
            backend
                .get(&locator)
                .expect("get")
                .expect("present")
                .expose(),
            "old-secret"
        );
    }

    #[test]
    fn delete_verifies_secret_is_absent() {
        let locator = StaticCredential::Yuandian.locator();
        let secret = SecretValue::new("temporary-secret".to_string()).expect("secret");
        let mut backend = MemoryBackend::default();
        backend.set(&locator, &secret).expect("seed");

        delete_verified_with(&mut backend, &locator).expect("delete");

        assert!(backend.get(&locator).expect("get").is_none());
    }
}
