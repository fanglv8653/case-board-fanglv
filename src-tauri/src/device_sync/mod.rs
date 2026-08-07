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
pub(crate) mod feishu_binding_lifecycle;
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
    pub auto_paused: bool,
    pub pause_reason_code: Option<String>,
    pub last_attempt_at: Option<String>,
    pub last_success_at: Option<String>,
    pub pending_upload: u64,
    pub conflicts: u64,
    pub quarantined: u64,
    pub manual_review: u64,
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
    #[error("飞书绑定生命周期正在变更")]
    FeishuLifecycleBusy,
    #[error("异常变更熔断: {0}")]
    FuseTriggered(String),
    #[error("同步包依赖在当前包和接收端均不存在")]
    PackageDependencyMissing,
    #[error("sync package dependency closure exceeds the event capacity")]
    PackageTooLarge,
    #[error("同步包内依赖实体的最终动作与引用关系冲突")]
    PackageDependencyConflict,
    #[error("同步组因确定性同步错误已自动暂停")]
    GroupAutoPaused,
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
            Self::FeishuLifecycleBusy => "SYNC_FEISHU_LIFECYCLE_BUSY",
            Self::FuseTriggered(_) => "SYNC_FUSE_TRIGGERED",
            Self::PackageDependencyMissing => "SYNC_PACKAGE_DEPENDENCY_MISSING",
            Self::PackageTooLarge => "SYNC_PACKAGE_TOO_LARGE",
            Self::PackageDependencyConflict => "SYNC_PACKAGE_DEPENDENCY_CONFLICT",
            Self::GroupAutoPaused => "SYNC_GROUP_AUTO_PAUSED",
            Self::NotFound(_) => "SYNC_NOT_FOUND",
        }
    }

    pub fn public_message(&self) -> &'static str {
        match self {
            Self::Database(_) => "设备同步暂时无法访问本地数据库",
            Self::Serialization(_) => "设备同步数据格式无效",
            Self::Crypto(_) => "设备同步加密处理失败",
            Self::Integrity(_) => "同步数据完整性校验失败",
            Self::Protocol(_) => "设备同步协议数据无效",
            Self::EntityNotAllowed(_) => "该类数据不允许设备同步",
            Self::FieldNotAllowed { .. } => "该字段不允许设备同步",
            Self::InvalidNasPath(_) => "请选择有效的设备同步目录",
            Self::NasUnavailable(_) => "设备同步目录当前不可用",
            Self::CredentialStore(_) => "设备同步密钥暂时不可用",
            Self::UnsupportedPlatform => "当前系统不支持设备同步密钥存储",
            Self::Paused => "设备同步已暂停",
            Self::Busy => "设备同步任务正在运行或状态已变化",
            Self::FeishuLifecycleBusy => "飞书绑定正在变更，请稍后重试设备同步",
            Self::FuseTriggered(_) => "设备同步因异常变更已停止",
            Self::PackageDependencyMissing => "同步包缺少必要的依赖数据",
            Self::PackageTooLarge => "同步包超出容量限制",
            Self::PackageDependencyConflict => "同步包的依赖状态冲突",
            Self::GroupAutoPaused => "同步组因可重现错误已自动暂停",
            Self::NotFound(_) => "未找到所需的设备同步数据",
        }
    }
}

impl serde::Serialize for SyncError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("[{}] {}", self.code(), self.public_message()))
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
