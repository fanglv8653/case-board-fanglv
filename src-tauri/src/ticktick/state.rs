//! 滴答清单(dida365 / TickTick)双向同步 —— 本地状态。
//!
//! **公开功能**。**不建任何 SQLite migration、不碰 settings.rs**:API 口令 / 同步台账 / cutoff
//! 全落一个本地 JSON 文件 `<app_data_dir>/ticktick_sync.json`(本地运行态,不进 git,
//! 避开 migration checksum 红线;凭证是用户在滴答设置里生成的「API 口令」,符合密钥铁律)。
//!
//! 鉴权走「API 口令」(dida365 设置 → 账户与安全 → API 口令,dp_ 前缀的个人访问令牌),
//! 直接当 Bearer token 打 `/open/v1/` 接口 —— 免注册开发者应用、免 OAuth 授权、免刷新。
//!
//! 「独立滴答镜像」模型:在「独立」tab 里维护一份滴答某清单的镜像列表,完整双向、
//! 带完成状态,**不碰** 案件待办(case_todos)/ 首页日历(calendar_events)。

use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

use crate::credentials::{
    replace_verified_with, CredentialBackend, CredentialError, CredentialLocator, SecretValue,
    SystemCredentialBackend,
};

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickConfig {
    /// API 域:dida365 = https://api.dida365.com,国际版 = https://api.ticktick.com
    #[serde(default = "default_api_base")]
    pub api_base: String,
    /// 同步目标清单(project)。None = 未选。
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub project_name: Option<String>,
    /// 自动同步(每分钟 + 切回 App)。默认开。
    #[serde(default = "default_true")]
    pub auto_sync: bool,
}

fn default_api_base() -> String {
    "https://api.dida365.com".to_string()
}
fn default_true() -> bool {
    true
}

impl Default for TickTickConfig {
    fn default() -> Self {
        Self {
            api_base: default_api_base(),
            project_id: None,
            project_name: None,
            auto_sync: true,
        }
    }
}

/// 鉴权令牌。API 口令模型下只用 `access_token` 存用户粘贴的口令(长期有效、
/// 不过期、无刷新);`refresh_token`/`expires_at_ms` 保留为兼容字段,恒为 None/0。
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickTokens {
    #[serde(default, skip_serializing)]
    pub access_token: Option<String>,
    #[serde(default, skip_serializing)]
    pub refresh_token: Option<String>,
    /// 兼容字段:API 口令不过期,恒为 0。
    #[serde(default)]
    pub expires_at_ms: i64,
}

impl std::fmt::Debug for TickTickTokens {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TickTickTokens")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[REDACTED]"),
            )
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

/// 镜像列表里的一条待办。本地 uuid 为主键,`ticktick_id` 是远端对应任务 id。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MirrorItem {
    pub id: String,
    #[serde(default)]
    pub ticktick_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub done: bool,
    /// ISO 日期或日期时间(可空)。
    #[serde(default)]
    pub due: Option<String>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    /// 本地软删墓碑:下次同步把远端也删掉,删成功后真正移除本行。
    #[serde(default)]
    pub deleted: bool,
    /// 本地有未推送的改动(新建 / 改标题 / 勾完成 / 改日期)。
    #[serde(default)]
    pub dirty: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickTickState {
    #[serde(default)]
    pub config: TickTickConfig,
    #[serde(default)]
    pub tokens: TickTickTokens,
    /// cutoff 展示用:连接成功的时间点(毫秒)。0 = 未连接。
    #[serde(default)]
    pub sync_enabled_at_ms: i64,
    /// 是否已建立基线。首次同步时把当时远端**已有任务 id** 全记入 `baseline_ids`(视为历史积压,
    /// 一律不拉),之后只拉「不在基线、也不在本地台账」的新任务。
    /// 用 id 集合而非时间戳比较 —— 滴答 `/project/{id}/data` 不保证返回 modifiedTime,
    /// 用时间戳会导致「永远拉不进任何新任务」的静默失败。
    #[serde(default)]
    pub baseline_captured: bool,
    #[serde(default)]
    pub baseline_ids: Vec<String>,
    #[serde(default)]
    pub last_sync_ms: i64,
    /// 最近一次连接/同步的错误(供前端展示),成功后清空。
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub items: Vec<MirrorItem>,
}

impl TickTickState {
    pub fn connected(&self) -> bool {
        self.tokens.access_token.is_some() && self.sync_enabled_at_ms > 0
    }
}

/// 状态文件绝对路径:`<app_data_dir>/ticktick_sync.json`。
pub fn state_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("取 app_data_dir 失败:{e}"))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("创建数据目录失败:{e}"))?;
    Ok(dir.join("ticktick_sync.json"))
}

fn access_token_locator() -> CredentialLocator {
    CredentialLocator::new("integration", "ticktick", "access-token")
        .expect("static TickTick access-token locator")
}

fn refresh_token_locator() -> CredentialLocator {
    CredentialLocator::new("integration", "ticktick", "refresh-token")
        .expect("static TickTick refresh-token locator")
}

fn restore_token_snapshot<B: CredentialBackend>(
    backend: &mut B,
    snapshot: &[(CredentialLocator, Option<SecretValue>)],
) -> bool {
    let mut complete = true;
    for (locator, value) in snapshot {
        complete &= match value {
            Some(value) => backend.set(locator, value).is_ok(),
            None => backend.delete(locator).is_ok(),
        };
    }
    complete
}

fn atomic_write_state(path: &Path, state: &TickTickState) -> Result<(), String> {
    let parent = path.parent().ok_or("TICKTICK_STATE_PATH_INVALID")?;
    std::fs::create_dir_all(parent).map_err(|_| "TICKTICK_STATE_DIR_FAILED")?;
    let bytes = serde_json::to_vec_pretty(state).map_err(|_| "TICKTICK_STATE_SERIALIZE_FAILED")?;
    let mut temporary =
        tempfile::NamedTempFile::new_in(parent).map_err(|_| "TICKTICK_STATE_TEMP_FAILED")?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.flush())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "TICKTICK_STATE_TEMP_WRITE_FAILED")?;
    temporary
        .persist(path)
        .map_err(|_| "TICKTICK_STATE_REPLACE_FAILED")?;
    Ok(())
}

fn save_with_backend<B: CredentialBackend>(
    path: &Path,
    state: &TickTickState,
    backend: &mut B,
) -> Result<(), String> {
    let updates = [
        (access_token_locator(), state.tokens.access_token.as_ref()),
        (refresh_token_locator(), state.tokens.refresh_token.as_ref()),
    ]
    .into_iter()
    .filter_map(|(locator, value)| {
        value
            .filter(|value| !value.trim().is_empty())
            .map(|value| (locator, value))
    })
    .collect::<Vec<_>>();
    let mut snapshot = Vec::with_capacity(updates.len());
    for (locator, _) in &updates {
        snapshot.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for (locator, value) in &updates {
        let secret =
            SecretValue::new((*value).clone()).map_err(|error| error.code().to_string())?;
        if let Err(error) = replace_verified_with(backend, locator, &secret) {
            let restored = restore_token_snapshot(backend, &snapshot);
            return Err(if restored {
                error.code().to_string()
            } else {
                CredentialError::RollbackFailed.code().to_string()
            });
        }
    }
    let mut sanitized = state.clone();
    sanitized.tokens = TickTickTokens {
        access_token: None,
        refresh_token: None,
        expires_at_ms: state.tokens.expires_at_ms,
    };
    if let Err(error) = atomic_write_state(path, &sanitized) {
        if !restore_token_snapshot(backend, &snapshot) {
            return Err(CredentialError::RollbackFailed.code().to_string());
        }
        return Err(error);
    }
    Ok(())
}

fn load_with_backend<B: CredentialBackend>(
    path: &Path,
    backend: &mut B,
) -> Result<TickTickState, String> {
    if !path.exists() {
        return Ok(TickTickState::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|_| "TICKTICK_STATE_READ_FAILED")?;
    if raw.trim().is_empty() {
        return Ok(TickTickState::default());
    }
    let mut state: TickTickState =
        serde_json::from_str(&raw).map_err(|_| "TICKTICK_STATE_PARSE_FAILED")?;
    let had_legacy_tokens =
        state.tokens.access_token.is_some() || state.tokens.refresh_token.is_some();
    if had_legacy_tokens {
        save_with_backend(path, &state, backend)?;
    }
    state.tokens.access_token = backend
        .get(&access_token_locator())
        .map_err(|error| error.code().to_string())?
        .map(SecretValue::into_string);
    state.tokens.refresh_token = backend
        .get(&refresh_token_locator())
        .map_err(|error| error.code().to_string())?
        .map(SecretValue::into_string);
    Ok(state)
}

pub fn load(app: &AppHandle) -> Result<TickTickState, String> {
    let p = state_path(app)?;
    load_with_backend(&p, &mut SystemCredentialBackend)
}

pub fn save(app: &AppHandle, st: &TickTickState) -> Result<(), String> {
    let p = state_path(app)?;
    save_with_backend(&p, st, &mut SystemCredentialBackend)
}

pub fn delete_tokens() -> Result<(), String> {
    let mut backend = SystemCredentialBackend;
    delete_tokens_with_backend(&mut backend)
}

/// Commit disconnect as one recoverable operation: credential deletion is
/// verified first, then the sanitized state is atomically persisted. If state
/// persistence fails, both token locators are restored from the snapshot.
pub fn disconnect(app: &AppHandle, state: &TickTickState) -> Result<(), String> {
    let path = state_path(app)?;
    disconnect_with_backend_and_writer(state, &mut SystemCredentialBackend, |sanitized| {
        atomic_write_state(&path, sanitized)
    })
}

fn disconnect_with_backend_and_writer<B, F>(
    state: &TickTickState,
    backend: &mut B,
    write: F,
) -> Result<(), String>
where
    B: CredentialBackend,
    F: FnOnce(&TickTickState) -> Result<(), String>,
{
    let locators = [access_token_locator(), refresh_token_locator()];
    let mut snapshot = Vec::with_capacity(locators.len());
    for locator in &locators {
        snapshot.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for locator in &locators {
        if let Err(error) = crate::credentials::delete_verified_with(backend, locator) {
            let restored = restore_token_snapshot(backend, &snapshot);
            return Err(if restored {
                error.code().to_string()
            } else {
                CredentialError::RollbackFailed.code().to_string()
            });
        }
    }
    let mut sanitized = state.clone();
    sanitized.tokens.access_token = None;
    sanitized.tokens.refresh_token = None;
    if let Err(error) = write(&sanitized) {
        if !restore_token_snapshot(backend, &snapshot) {
            return Err(CredentialError::RollbackFailed.code().to_string());
        }
        return Err(error);
    }
    Ok(())
}

fn delete_tokens_with_backend<B: CredentialBackend>(backend: &mut B) -> Result<(), String> {
    let locators = [access_token_locator(), refresh_token_locator()];
    let mut snapshot = Vec::with_capacity(locators.len());
    for locator in &locators {
        snapshot.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for locator in &locators {
        if let Err(error) = crate::credentials::delete_verified_with(backend, locator) {
            let restored = restore_token_snapshot(backend, &snapshot);
            return Err(if restored {
                error.code().to_string()
            } else {
                CredentialError::RollbackFailed.code().to_string()
            });
        }
    }
    Ok(())
}

/// 解析滴答返回的时间(如 `2026-06-15T10:00:00.000+0000`)→ 毫秒。
pub fn parse_iso_ms(s: &str) -> Option<i64> {
    use chrono::DateTime;
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    for fmt in ["%Y-%m-%dT%H:%M:%S%.3f%z", "%Y-%m-%dT%H:%M:%S%z"] {
        if let Ok(dt) = DateTime::parse_from_str(s, fmt) {
            return Some(dt.timestamp_millis());
        }
    }
    None
}

#[cfg(test)]
mod credential_tests {
    use std::collections::HashMap;

    use super::*;

    #[derive(Default)]
    struct MemoryBackend {
        values: HashMap<String, String>,
        corrupt_readback: bool,
        has_written: bool,
    }

    impl CredentialBackend for MemoryBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            self.values
                .insert(locator.id().to_string(), secret.expose().to_string());
            self.has_written = true;
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.values
                .get(locator.id())
                .cloned()
                .map(|value| {
                    if self.corrupt_readback && self.has_written {
                        SecretValue::new(format!("{value}-corrupt"))
                    } else {
                        SecretValue::new(value)
                    }
                })
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.values.remove(locator.id());
            Ok(())
        }
    }

    #[test]
    fn legacy_tokens_migrate_and_are_removed_from_json() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("ticktick_sync.json");
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "tokens": {
                    "accessToken": "legacy-access-marker",
                    "refreshToken": "legacy-refresh-marker",
                    "expiresAtMs": 42
                },
                "syncEnabledAtMs": 1
            }))
            .expect("json"),
        )
        .expect("write");
        let mut backend = MemoryBackend::default();

        let state = load_with_backend(&path, &mut backend).expect("migrate");

        assert_eq!(
            state.tokens.access_token.as_deref(),
            Some("legacy-access-marker")
        );
        assert_eq!(
            state.tokens.refresh_token.as_deref(),
            Some("legacy-refresh-marker")
        );
        assert!(state.connected());
        let disk = std::fs::read_to_string(path).expect("state");
        assert!(!disk.contains("legacy-access-marker"));
        assert!(!disk.contains("legacy-refresh-marker"));
        assert!(!disk.contains("accessToken"));
        assert!(!disk.contains("refreshToken"));
    }

    #[test]
    fn credential_verification_failure_keeps_existing_value_and_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("ticktick_sync.json");
        std::fs::write(&path, "{\"syncEnabledAtMs\":7}").expect("write");
        let original = std::fs::read(&path).expect("original");
        let mut backend = MemoryBackend::default();
        backend.values.insert(
            access_token_locator().id().to_string(),
            "old-access".to_string(),
        );
        backend.corrupt_readback = true;
        let state = TickTickState {
            tokens: TickTickTokens {
                access_token: Some("new-access".to_string()),
                refresh_token: None,
                expires_at_ms: 0,
            },
            ..TickTickState::default()
        };

        assert_eq!(
            save_with_backend(&path, &state, &mut backend),
            Err(CredentialError::VerificationFailed.code().to_string())
        );
        assert_eq!(std::fs::read(path).expect("file"), original);
        assert_eq!(
            backend
                .values
                .get(access_token_locator().id())
                .map(String::as_str),
            Some("old-access")
        );
    }

    #[test]
    fn serialized_state_never_contains_tokens() {
        let state = TickTickState {
            tokens: TickTickTokens {
                access_token: Some("access-marker".to_string()),
                refresh_token: Some("refresh-marker".to_string()),
                expires_at_ms: 12,
            },
            ..TickTickState::default()
        };

        let json = serde_json::to_string(&state).expect("serialize");

        assert!(!json.contains("access-marker"));
        assert!(!json.contains("refresh-marker"));
        assert!(!json.contains("accessToken"));
        assert!(!json.contains("refreshToken"));
        let debug = format!("{state:?}");
        assert!(!debug.contains("access-marker"));
        assert!(!debug.contains("refresh-marker"));
    }

    #[test]
    fn disconnect_deletes_access_and_refresh_credentials() {
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &access_token_locator(),
                &SecretValue::new("access".into()).expect("secret"),
            )
            .expect("seed");
        backend
            .set(
                &refresh_token_locator(),
                &SecretValue::new("refresh".into()).expect("secret"),
            )
            .expect("seed");

        delete_tokens_with_backend(&mut backend).expect("delete");

        assert!(backend.values.is_empty());
    }

    #[test]
    fn disconnect_state_failure_restores_both_token_snapshots() {
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &access_token_locator(),
                &SecretValue::new("access-snapshot".into()).expect("secret"),
            )
            .expect("seed");
        backend
            .set(
                &refresh_token_locator(),
                &SecretValue::new("refresh-snapshot".into()).expect("secret"),
            )
            .expect("seed");
        let before = backend.values.clone();
        let state = TickTickState {
            tokens: TickTickTokens {
                access_token: Some("access-snapshot".to_string()),
                refresh_token: Some("refresh-snapshot".to_string()),
                expires_at_ms: 7,
            },
            sync_enabled_at_ms: 1,
            ..TickTickState::default()
        };

        let result = disconnect_with_backend_and_writer(&state, &mut backend, |sanitized| {
            assert!(sanitized.tokens.access_token.is_none());
            assert!(sanitized.tokens.refresh_token.is_none());
            Err("TICKTICK_STATE_REPLACE_FAILED".to_string())
        });

        assert_eq!(result, Err("TICKTICK_STATE_REPLACE_FAILED".to_string()));
        assert_eq!(backend.values, before);
    }
}
