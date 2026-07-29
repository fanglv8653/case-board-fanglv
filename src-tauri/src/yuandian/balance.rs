//! 元典 MCP 官方余额查询、Key 隔离缓存与本机积分账对账。
//!
//! API Key 仅从 Windows Credential Manager 在 Rust 运行时内解析，并只作为
//! 当次 MCP HTTP 连接的 Bearer 头使用。前端、SQLite、settings、日志和命令行
//! 均不会取得明文 Key。

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::Local;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use tokio::sync::Mutex;

use crate::chat::mcp_bridge::{McpClient, McpServerConfig, McpTransport};
use crate::credentials::{self, StaticCredential};

const MCP_LAW_URL: &str = "https://open.chineselaw.com/mcp/law/stream";
const BALANCE_TOOL: &str = "yuandian_get_user_balance";
const ERROR_NOT_CONFIGURED: &str = "YUANDIAN_CREDENTIAL_NOT_CONFIGURED";
const ERROR_SECURE_STORE: &str = "YUANDIAN_CREDENTIAL_UNAVAILABLE";
const ERROR_AUTH: &str = "YUANDIAN_BALANCE_AUTH_FAILED";
const ERROR_NETWORK: &str = "YUANDIAN_BALANCE_NETWORK_FAILED";
const ERROR_RESPONSE: &str = "YUANDIAN_BALANCE_RESPONSE_INVALID";
const ERROR_DATABASE: &str = "YUANDIAN_BALANCE_DATABASE_FAILED";

static REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn refresh_lock() -> &'static Mutex<()> {
    REFRESH_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, FromRow)]
struct BalanceSnapshot {
    id: i64,
    key_fingerprint: String,
    point_balance: i64,
    count_balance: i64,
    local_credits_total: i64,
    local_api_calls_total: i64,
    fetched_at: String,
}

/// 设置页使用的官方余额视图。`difference` 为“官方余额减少 - 本机记账”。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct YuandianBalanceView {
    pub point_balance: i64,
    pub count_balance: i64,
    pub fetched_at: String,
    pub cached: bool,
    pub previous_point_balance: Option<i64>,
    pub previous_fetched_at: Option<String>,
    pub official_spent_since_previous: Option<i64>,
    pub local_recorded_since_previous: Option<i64>,
    pub local_api_calls_since_previous: Option<i64>,
    pub difference: Option<i64>,
    pub balance_increased_since_previous: Option<i64>,
    pub comparison_status: String,
    pub refresh_error_code: Option<String>,
    pub refresh_error: Option<String>,
}

impl YuandianBalanceView {
    fn with_refresh_error(mut self, error: BalanceError) -> Self {
        self.cached = true;
        self.refresh_error_code = Some(error.code().to_string());
        self.refresh_error = Some(error.user_message().to_string());
        self
    }
}

#[derive(Debug)]
enum BalanceError {
    NotConfigured,
    SecureStore,
    Authentication,
    Network,
    InvalidResponse,
    Database,
}

impl BalanceError {
    const fn code(&self) -> &'static str {
        match self {
            Self::NotConfigured => ERROR_NOT_CONFIGURED,
            Self::SecureStore => ERROR_SECURE_STORE,
            Self::Authentication => ERROR_AUTH,
            Self::Network => ERROR_NETWORK,
            Self::InvalidResponse => ERROR_RESPONSE,
            Self::Database => ERROR_DATABASE,
        }
    }

    const fn user_message(&self) -> &'static str {
        match self {
            Self::NotConfigured => "元典凭据尚未安全保存",
            Self::SecureStore => "无法从系统安全凭据存储读取元典凭据",
            Self::Authentication => "元典凭据无效或已失效",
            Self::Network => "暂时无法连接元典官方余额服务",
            Self::InvalidResponse => "元典官方余额响应格式暂时无法识别",
            Self::Database => "元典官方余额缓存读写失败",
        }
    }
}

fn classify_remote_error(error: &str) -> BalanceError {
    if error.contains("401") || error.contains("403") {
        BalanceError::Authentication
    } else {
        BalanceError::Network
    }
}

fn key_fingerprint(api_key: &str) -> String {
    let digest = Sha256::digest(api_key.as_bytes());
    format!("{digest:x}")[..16].to_string()
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_u64().and_then(|number| i64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
}

fn balance_from_data(data: &Value) -> Option<(i64, i64)> {
    let point_balance = data.get("pointBalance").and_then(value_as_i64)?;
    let count_balance = data.get("countBalance").and_then(value_as_i64).unwrap_or(0);
    Some((point_balance, count_balance))
}

/// 解析 MCP 文本块中的余额 JSON。兼容当前 dataPreview 以及旧版 data 包装。
fn parse_balance_text(text: &str) -> Result<(i64, i64), BalanceError> {
    if text.starts_with("[MCP 工具报错]") {
        return Err(BalanceError::InvalidResponse);
    }
    let value: Value =
        serde_json::from_str(text.trim()).map_err(|_| BalanceError::InvalidResponse)?;
    [
        "/dataPreview/data",
        "/data/data",
        "/data",
        "/structuredContent/data/data",
        "/structuredContent/data",
    ]
    .iter()
    .find_map(|pointer| value.pointer(pointer).and_then(balance_from_data))
    .or_else(|| balance_from_data(&value))
    .ok_or(BalanceError::InvalidResponse)
}

async fn fetch_mcp_balance(api_key: &str) -> Result<(i64, i64), BalanceError> {
    let mut headers = BTreeMap::new();
    headers.insert(
        "Authorization".to_string(),
        format!("Bearer {}", api_key.trim()),
    );
    let config = McpServerConfig {
        name: "yuandian-balance".to_string(),
        transport: McpTransport::Http {
            url: MCP_LAW_URL.to_string(),
            headers,
        },
        enabled: true,
    };
    let client = McpClient::connect(&config)
        .await
        .map_err(|error| classify_remote_error(&error))?;
    let text = client
        .call_tool(BALANCE_TOOL, &json!({}))
        .await
        .map_err(|error| classify_remote_error(&error))?;
    parse_balance_text(&text)
}

/// 使用元典 MCP 的免费余额工具验证凭据，不调用计费的企业搜索或 hall_detect。
pub async fn verify_api_key(api_key: &str) -> Result<(i64, i64), String> {
    fetch_mcp_balance(api_key)
        .await
        .map_err(|error| error.code().to_string())
}

fn resolve_api_key() -> Result<credentials::SecretValue, BalanceError> {
    credentials::resolve_static(StaticCredential::Yuandian)
        .map_err(|_| BalanceError::SecureStore)?
        .ok_or(BalanceError::NotConfigured)
}

async fn local_totals(pool: &SqlitePool) -> Result<(i64, i64), BalanceError> {
    sqlx::query_as(
        "SELECT COALESCE(SUM(credits_used), 0), COALESCE(SUM(api_calls), 0) \
         FROM yuandian_credits_monthly",
    )
    .fetch_one(pool)
    .await
    .map_err(|_| BalanceError::Database)
}

async fn latest_for_key(
    pool: &SqlitePool,
    fingerprint: &str,
) -> Result<Option<BalanceSnapshot>, BalanceError> {
    sqlx::query_as(
        "SELECT id, key_fingerprint, point_balance, count_balance, local_credits_total, \
                local_api_calls_total, fetched_at \
         FROM yuandian_balance_snapshots \
         WHERE key_fingerprint = ? ORDER BY id DESC LIMIT 1",
    )
    .bind(fingerprint)
    .fetch_optional(pool)
    .await
    .map_err(|_| BalanceError::Database)
}

async fn previous_for_key(
    pool: &SqlitePool,
    fingerprint: &str,
    before_id: i64,
) -> Result<Option<BalanceSnapshot>, BalanceError> {
    sqlx::query_as(
        "SELECT id, key_fingerprint, point_balance, count_balance, local_credits_total, \
                local_api_calls_total, fetched_at \
         FROM yuandian_balance_snapshots \
         WHERE key_fingerprint = ? AND id < ? ORDER BY id DESC LIMIT 1",
    )
    .bind(fingerprint)
    .bind(before_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| BalanceError::Database)
}

fn to_view(
    current: BalanceSnapshot,
    previous: Option<BalanceSnapshot>,
    cached: bool,
) -> YuandianBalanceView {
    debug_assert!(previous
        .as_ref()
        .is_none_or(|item| item.key_fingerprint == current.key_fingerprint));
    let mut view = YuandianBalanceView {
        point_balance: current.point_balance,
        count_balance: current.count_balance,
        fetched_at: current.fetched_at,
        cached,
        previous_point_balance: previous.as_ref().map(|item| item.point_balance),
        previous_fetched_at: previous.as_ref().map(|item| item.fetched_at.clone()),
        official_spent_since_previous: None,
        local_recorded_since_previous: None,
        local_api_calls_since_previous: None,
        difference: None,
        balance_increased_since_previous: None,
        comparison_status: "baseline".to_string(),
        refresh_error_code: None,
        refresh_error: None,
    };

    let Some(previous) = previous else {
        return view;
    };
    let official_delta = previous.point_balance - current.point_balance;
    if official_delta < 0 {
        view.balance_increased_since_previous = Some(-official_delta);
        view.comparison_status = "recharged".to_string();
        return view;
    }

    let local_delta = current.local_credits_total - previous.local_credits_total;
    if local_delta < 0 {
        view.official_spent_since_previous = Some(official_delta);
        view.comparison_status = "local_reset".to_string();
        return view;
    }
    view.official_spent_since_previous = Some(official_delta);
    view.local_recorded_since_previous = Some(local_delta);
    view.local_api_calls_since_previous =
        Some(current.local_api_calls_total - previous.local_api_calls_total)
            .filter(|delta| *delta >= 0);
    view.difference = Some(official_delta - local_delta);
    view.comparison_status = if official_delta == local_delta {
        "matched".to_string()
    } else {
        "difference".to_string()
    };
    view
}

async fn persist_snapshot(
    pool: &SqlitePool,
    fingerprint: &str,
    point_balance: i64,
    count_balance: i64,
) -> Result<YuandianBalanceView, BalanceError> {
    let previous = latest_for_key(pool, fingerprint).await?;
    let (local_credits_total, local_api_calls_total) = local_totals(pool).await?;
    let fetched_at = Local::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT INTO yuandian_balance_snapshots \
         (key_fingerprint, point_balance, count_balance, local_credits_total, \
          local_api_calls_total, fetched_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(fingerprint)
    .bind(point_balance)
    .bind(count_balance)
    .bind(local_credits_total)
    .bind(local_api_calls_total)
    .bind(&fetched_at)
    .execute(pool)
    .await
    .map_err(|_| BalanceError::Database)?;

    Ok(to_view(
        BalanceSnapshot {
            id: result.last_insert_rowid(),
            key_fingerprint: fingerprint.to_string(),
            point_balance,
            count_balance,
            local_credits_total,
            local_api_calls_total,
            fetched_at,
        },
        previous,
        false,
    ))
}

async fn cached_for_fingerprint(
    pool: &SqlitePool,
    fingerprint: &str,
) -> Result<Option<YuandianBalanceView>, BalanceError> {
    let Some(current) = latest_for_key(pool, fingerprint).await? else {
        return Ok(None);
    };
    let previous = previous_for_key(pool, fingerprint, current.id).await?;
    Ok(Some(to_view(current, previous, true)))
}

async fn cached_after_refresh_failure(
    pool: &SqlitePool,
    fingerprint: &str,
    error: BalanceError,
) -> Result<Option<YuandianBalanceView>, BalanceError> {
    Ok(cached_for_fingerprint(pool, fingerprint)
        .await?
        .map(|cached| cached.with_refresh_error(error)))
}

/// 仅读取当前安全凭据对应的缓存；换 Key 后不会串用旧账户快照。
pub async fn cached_balance(pool: &SqlitePool) -> Result<Option<YuandianBalanceView>, String> {
    let secret = resolve_api_key().map_err(|error| error.code().to_string())?;
    let fingerprint = key_fingerprint(secret.expose());
    cached_for_fingerprint(pool, &fingerprint)
        .await
        .map_err(|error| error.code().to_string())
}

/// 刷新官方余额。失败时若当前 Key 有旧快照，则返回缓存并附稳定错误码。
pub async fn refresh_balance(pool: &SqlitePool) -> Result<YuandianBalanceView, String> {
    let _guard = refresh_lock().lock().await;
    let secret = resolve_api_key().map_err(|error| error.code().to_string())?;
    let fingerprint = key_fingerprint(secret.expose());
    match fetch_mcp_balance(secret.expose()).await {
        Ok((point_balance, count_balance)) => {
            persist_snapshot(pool, &fingerprint, point_balance, count_balance)
                .await
                .map_err(|error| error.code().to_string())
        }
        Err(error) => {
            let error_code = error.code().to_string();
            match cached_after_refresh_failure(pool, &fingerprint, error)
                .await
                .map_err(|cache_error| cache_error.code().to_string())?
            {
                Some(cached) => Ok(cached),
                None => Err(error_code),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    async fn test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect");
        sqlx::query(
            "CREATE TABLE yuandian_credits_monthly (
                year_month TEXT PRIMARY KEY NOT NULL,
                credits_used INTEGER NOT NULL DEFAULT 0,
                api_calls INTEGER NOT NULL DEFAULT 0,
                kb_hits INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("credits schema");
        sqlx::query(
            "CREATE TABLE yuandian_balance_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                key_fingerprint TEXT NOT NULL,
                point_balance INTEGER NOT NULL,
                count_balance INTEGER NOT NULL DEFAULT 0,
                local_credits_total INTEGER NOT NULL DEFAULT 0,
                local_api_calls_total INTEGER NOT NULL DEFAULT 0,
                fetched_at TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .expect("balance schema");
        pool
    }

    #[test]
    fn parses_current_and_legacy_balance_wrappers() {
        let current = r#"{"dataPreview":{"data":{"pointBalance":"3490","countBalance":7}}}"#;
        let legacy = r#"{"data":{"pointBalance":120,"countBalance":"3"}}"#;
        assert_eq!(parse_balance_text(current).expect("current"), (3490, 7));
        assert_eq!(parse_balance_text(legacy).expect("legacy"), (120, 3));
        assert!(parse_balance_text(r#"{"data":{}}"#).is_err());
    }

    #[test]
    fn fingerprint_is_stable_distinct_and_does_not_contain_secret() {
        let first = key_fingerprint("sk_first-secret");
        let same = key_fingerprint("sk_first-secret");
        let second = key_fingerprint("sk_second-secret");
        assert_eq!(first, same);
        assert_ne!(first, second);
        assert_eq!(first.len(), 16);
        assert!(!first.contains("first-secret"));
    }

    #[tokio::test]
    async fn persists_and_reconciles_official_and_local_deltas() {
        let pool = test_pool().await;
        sqlx::query(
            "INSERT INTO yuandian_credits_monthly
             (year_month, credits_used, api_calls, kb_hits, updated_at)
             VALUES ('2026-07', 10, 1, 0, '2026-07-29T00:00:00Z')",
        )
        .execute(&pool)
        .await
        .expect("seed");
        let baseline = persist_snapshot(&pool, "fingerprint-a", 1000, 0)
            .await
            .expect("baseline");
        assert_eq!(baseline.comparison_status, "baseline");

        sqlx::query(
            "UPDATE yuandian_credits_monthly
             SET credits_used = 25, api_calls = 3, updated_at = '2026-07-29T01:00:00Z'",
        )
        .execute(&pool)
        .await
        .expect("update");
        let next = persist_snapshot(&pool, "fingerprint-a", 980, 0)
            .await
            .expect("next");
        assert_eq!(next.official_spent_since_previous, Some(20));
        assert_eq!(next.local_recorded_since_previous, Some(15));
        assert_eq!(next.local_api_calls_since_previous, Some(2));
        assert_eq!(next.difference, Some(5));
        assert_eq!(next.comparison_status, "difference");
    }

    #[tokio::test]
    async fn key_scoped_cache_never_uses_another_keys_snapshot() {
        let pool = test_pool().await;
        persist_snapshot(&pool, "fingerprint-a", 1000, 4)
            .await
            .expect("a");
        assert!(cached_for_fingerprint(&pool, "fingerprint-b")
            .await
            .expect("b cache")
            .is_none());
        let a = cached_for_fingerprint(&pool, "fingerprint-a")
            .await
            .expect("a cache")
            .expect("a value");
        assert_eq!(a.point_balance, 1000);
        assert!(a.cached);
    }

    #[tokio::test]
    async fn refresh_failure_returns_current_keys_cache_with_safe_error() {
        let pool = test_pool().await;
        persist_snapshot(&pool, "fingerprint-a", 760, 2)
            .await
            .expect("seed");

        let fallback =
            cached_after_refresh_failure(&pool, "fingerprint-a", BalanceError::Network)
                .await
                .expect("fallback query")
                .expect("cached fallback");
        assert_eq!(fallback.point_balance, 760);
        assert!(fallback.cached);
        assert_eq!(
            fallback.refresh_error_code.as_deref(),
            Some(ERROR_NETWORK)
        );
        assert_eq!(
            fallback.refresh_error.as_deref(),
            Some("暂时无法连接元典官方余额服务")
        );

        assert!(
            cached_after_refresh_failure(&pool, "fingerprint-b", BalanceError::Network)
                .await
                .expect("other key")
                .is_none()
        );
    }

    #[test]
    fn remote_errors_are_reduced_to_safe_stable_classes() {
        assert!(matches!(
            classify_remote_error("HTTP 401: secret echo must not escape"),
            BalanceError::Authentication
        ));
        assert!(matches!(
            classify_remote_error("HTTP request failed with arbitrary body"),
            BalanceError::Network
        ));
    }
}
