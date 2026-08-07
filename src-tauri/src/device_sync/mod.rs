//! v0.8.1 NAS mounted-folder encrypted backup and bidirectional sync core.
//!
//! Security boundary:
//! - NAS stores only authenticated encrypted envelopes and non-sensitive protocol metadata.
//! - SQLite/WAL/SHM, credentials, source materials, extracted text, chat and memory never enter
//!   the registry.
//! - import is fail-closed on unknown entities/fields, signature, hash, epoch or sequence errors.

pub mod capture;
pub mod commands;
pub mod crypto;
pub mod engine;
pub mod identity;
pub mod manifest;
pub mod nas_folder;
pub mod operations;
pub mod pairing;
pub mod queries;
pub mod recovery;
pub mod registry;
pub mod scheduler;
pub mod snapshot;

#[cfg(test)]
mod v083_failure_tests;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub group_id: String,
    pub connector_root: String,
    pub local_device_id: String,
    pub key_epoch: u32,
    pub paused: bool,
    pub pending_upload: u64,
    pub conflicts: u64,
    pub quarantined: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error("数据库错误: {0}")]
    Database(String),
    #[error("序列化错误: {0}")]
    Serialization(String),
    #[error("加密错误: {0}")]
    Crypto(String),
    #[error("完整性校验失败: {0}")]
    Integrity(String),
    #[error("同步协议错误: {0}")]
    Protocol(String),
    #[error("不允许同步实体: {0}")]
    EntityNotAllowed(String),
    #[error("实体 {entity_type} 的字段不允许同步: {field}")]
    FieldNotAllowed { entity_type: String, field: String },
    #[error("NAS 目录无效: {0}")]
    InvalidNasPath(String),
    #[error("NAS 当前不可用: {0}")]
    NasUnavailable(String),
    #[error("Windows 凭据存储错误: {0}")]
    CredentialStore(String),
    #[error("当前平台不支持设备同步密钥存储")]
    UnsupportedPlatform,
    #[error("同步组已暂停")]
    Paused,
    #[error("同步任务正在运行或状态已变化")]
    Busy,
    #[error("异常变更熔断: {0}")]
    FuseTriggered(String),
    #[error("未找到: {0}")]
    NotFound(String),
}

impl SyncError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Database(_) => "SYNC_DATABASE",
            Self::Serialization(_) => "SYNC_SERIALIZATION",
            Self::Crypto(_) => "SYNC_CRYPTO",
            Self::Integrity(_) => "SYNC_INTEGRITY",
            Self::Protocol(_) => "SYNC_PROTOCOL",
            Self::EntityNotAllowed(_) => "SYNC_ENTITY_NOT_ALLOWED",
            Self::FieldNotAllowed { .. } => "SYNC_FIELD_NOT_ALLOWED",
            Self::InvalidNasPath(_) => "SYNC_NAS_PATH",
            Self::NasUnavailable(_) => "SYNC_NAS_UNAVAILABLE",
            Self::CredentialStore(_) => "SYNC_CREDENTIAL_STORE",
            Self::UnsupportedPlatform => "SYNC_UNSUPPORTED_PLATFORM",
            Self::Paused => "SYNC_PAUSED",
            Self::Busy => "SYNC_BUSY",
            Self::FuseTriggered(_) => "SYNC_FUSE_TRIGGERED",
            Self::NotFound(_) => "SYNC_NOT_FOUND",
        }
    }
}

impl serde::Serialize for SyncError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}: {}", self.code(), self))
    }
}

impl From<sqlx::Error> for SyncError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error.to_string())
    }
}

impl From<serde_json::Error> for SyncError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error.to_string())
    }
}
