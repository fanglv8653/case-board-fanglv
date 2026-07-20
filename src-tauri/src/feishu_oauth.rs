//! 飞书用户 OAuth 与安全凭据存储。
//!
//! 安全边界：
//! - App Secret、access token、refresh token 只进入进程内存和 Windows Credential Manager；
//! - SQLite、`settings.json`、公开 DTO、错误文本和日志均不得包含上述秘密；
//! - OAuth 回调只监听 `127.0.0.1:3000`，并校验一次性高熵 `state`；
//! - 仅申请案件同步所需的只读权限。

use std::fmt;
use std::time::Duration;

use chrono::Utc;
use reqwest::redirect::Policy;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{timeout, Instant};
use zeroize::Zeroize;

const AUTHORIZE_URL: &str = "https://accounts.feishu.cn/open-apis/authen/v1/authorize";
const TOKEN_URL: &str = "https://open.feishu.cn/open-apis/authen/v2/oauth/token";
pub const REDIRECT_URI: &str = "http://localhost:3000/callback";
pub const READONLY_SCOPES: &str = "offline_access bitable:app:readonly auth:user.id:read";

const CALLBACK_ADDR: &str = "127.0.0.1:3000";
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const TOKEN_REFRESH_MARGIN_SECONDS: i64 = 120;
const MAX_CALLBACK_REQUEST_BYTES: usize = 16 * 1024;
const MAX_CALLBACK_ATTEMPTS: usize = 8;
const CREDENTIAL_SERVICE: &str = "com.fanglv.caseboard.feishu";
#[cfg(target_os = "windows")]
const MAX_CREDENTIAL_BLOB_BYTES: usize = 5 * 512;

/// 可安全返回前端的连接状态；不含任何认证凭据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FeishuOAuthStatus {
    pub connected: bool,
    pub app_id: String,
    pub scopes: Vec<String>,
    pub access_expires_at: Option<i64>,
    pub refresh_expires_at: Option<i64>,
    pub reauthorization_required: bool,
}

/// OAuth 模块对外只返回稳定分类和固定文案，绝不附带 HTTP body、请求头、
/// 回调 code、state、App Secret 或 token。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum FeishuOAuthError {
    #[error("当前系统不支持安全保存飞书凭据")]
    UnsupportedPlatform,
    #[error("飞书应用 ID 格式无效")]
    InvalidAppId,
    #[error("飞书 App Secret 不能为空")]
    MissingAppSecret,
    #[error("本机 3000 端口不可用，无法接收飞书授权回调")]
    CallbackPortUnavailable,
    #[error("等待飞书授权超时")]
    CallbackTimeout,
    #[error("飞书授权回调无效")]
    InvalidCallback,
    #[error("飞书授权状态校验失败")]
    StateMismatch,
    #[error("用户取消了飞书授权")]
    AccessDenied,
    #[error("无法打开飞书授权页面")]
    BrowserOpenFailed,
    #[error("飞书认证服务暂时不可用")]
    Network,
    #[error("飞书认证响应无效")]
    InvalidTokenResponse,
    #[error("飞书认证失败")]
    TokenRejected,
    #[error("飞书只读权限不完整，需要重新授权")]
    MissingReadonlyScope,
    #[error("飞书授权已过期，需要重新授权")]
    ReauthorizationRequired,
    #[error("Windows 凭据库不可用")]
    CredentialStore,
}

impl FeishuOAuthError {
    /// 适合写入审计表或遥测的稳定错误码；不含外部响应内容。
    pub const fn code(self) -> &'static str {
        match self {
            Self::UnsupportedPlatform => "FEISHU_OAUTH_UNSUPPORTED_PLATFORM",
            Self::InvalidAppId => "FEISHU_OAUTH_INVALID_APP_ID",
            Self::MissingAppSecret => "FEISHU_OAUTH_MISSING_APP_SECRET",
            Self::CallbackPortUnavailable => "FEISHU_OAUTH_CALLBACK_PORT_UNAVAILABLE",
            Self::CallbackTimeout => "FEISHU_OAUTH_CALLBACK_TIMEOUT",
            Self::InvalidCallback => "FEISHU_OAUTH_INVALID_CALLBACK",
            Self::StateMismatch => "FEISHU_OAUTH_STATE_MISMATCH",
            Self::AccessDenied => "FEISHU_OAUTH_ACCESS_DENIED",
            Self::BrowserOpenFailed => "FEISHU_OAUTH_BROWSER_OPEN_FAILED",
            Self::Network => "FEISHU_OAUTH_NETWORK",
            Self::InvalidTokenResponse => "FEISHU_OAUTH_INVALID_TOKEN_RESPONSE",
            Self::TokenRejected => "FEISHU_OAUTH_TOKEN_REJECTED",
            Self::MissingReadonlyScope => "FEISHU_OAUTH_MISSING_READONLY_SCOPE",
            Self::ReauthorizationRequired => "FEISHU_OAUTH_REAUTHORIZATION_REQUIRED",
            Self::CredentialStore => "FEISHU_OAUTH_CREDENTIAL_STORE",
        }
    }
}

/// access token 的进程内包装。没有 `Debug` / `Display` / `Serialize` 实现，
/// 离开作用域时主动清零；仅供 Rust 后端构造 Authorization header。
pub(crate) struct FeishuAccessToken(String);

impl FeishuAccessToken {
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for FeishuAccessToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

struct SecretValue(String);

impl SecretValue {
    fn new(value: String) -> Self {
        Self(value)
    }

    fn expose(&self) -> &str {
        &self.0
    }

    fn is_empty(&self) -> bool {
        self.0.trim().is_empty()
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

#[derive(Serialize, Deserialize)]
struct StoredTokenBundle {
    access_token: String,
    refresh_token: String,
    access_expires_at: i64,
    refresh_expires_at: i64,
    scopes: Vec<String>,
}

impl fmt::Debug for StoredTokenBundle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredTokenBundle")
            .field("access_token", &"[REDACTED]")
            .field("refresh_token", &"[REDACTED]")
            .field("access_expires_at", &self.access_expires_at)
            .field("refresh_expires_at", &self.refresh_expires_at)
            .field("scopes", &self.scopes)
            .finish()
    }
}

impl Drop for StoredTokenBundle {
    fn drop(&mut self) {
        self.access_token.zeroize();
        self.refresh_token.zeroize();
    }
}

#[derive(Deserialize)]
struct TokenResponse {
    #[serde(default)]
    code: i64,
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    refresh_token_expires_in: Option<i64>,
    scope: Option<String>,
}

impl Drop for TokenResponse {
    fn drop(&mut self) {
        if let Some(value) = self.access_token.as_mut() {
            value.zeroize();
        }
        if let Some(value) = self.refresh_token.as_mut() {
            value.zeroize();
        }
    }
}

#[derive(Serialize)]
struct AuthorizationCodeRequest<'a> {
    grant_type: &'static str,
    client_id: &'a str,
    client_secret: &'a str,
    code: &'a str,
    redirect_uri: &'static str,
}

#[derive(Serialize)]
struct RefreshTokenRequest<'a> {
    grant_type: &'static str,
    client_id: &'a str,
    client_secret: &'a str,
    refresh_token: &'a str,
}

enum CallbackOutcome {
    Code(SecretValue),
    Denied,
    Ignore,
}

/// 完成一次应用内 OAuth 授权。
///
/// 调用方负责用 Tauri opener 打开传入 URL。为避免浏览器错误带出包含 state 的
/// URL，闭包错误被压缩为 `()`，本模块只返回固定错误分类。
pub async fn authorize_readonly<F>(
    app_id: &str,
    app_secret: String,
    open_browser: F,
) -> Result<FeishuOAuthStatus, FeishuOAuthError>
where
    F: FnOnce(&str) -> Result<(), ()>,
{
    validate_app_id(app_id)?;
    let app_secret = SecretValue::new(app_secret);
    if app_secret.is_empty() {
        return Err(FeishuOAuthError::MissingAppSecret);
    }

    // 必须先占住回调端口，再打开浏览器，避免授权完成后回调无人接收。
    let listener = TcpListener::bind(CALLBACK_ADDR)
        .await
        .map_err(|_| FeishuOAuthError::CallbackPortUnavailable)?;
    let state = SecretValue::new(generate_state());
    let authorization_url = build_authorization_url(app_id, state.expose())?;
    open_browser(authorization_url.as_str()).map_err(|_| FeishuOAuthError::BrowserOpenFailed)?;

    let code = wait_for_callback(&listener, state.expose()).await?;
    let response = exchange_authorization_code(app_id, &app_secret, &code).await?;
    let bundle = token_response_to_bundle(response, None)?;

    // 只有飞书已成功签发完整只读 token 后才持久化，失败授权不留下半套凭据。
    store_credentials(app_id, &app_secret, &bundle)?;
    Ok(status_from_bundle(app_id, &bundle))
}

/// 返回一个尚未过期的 access token；必要时使用 refresh token 自动刷新。
/// 返回值只能在 Rust 后端内使用，不能序列化或返回前端。
pub(crate) async fn valid_access_token(
    app_id: &str,
) -> Result<FeishuAccessToken, FeishuOAuthError> {
    validate_app_id(app_id)?;
    let mut bundle = load_token_bundle(app_id)?.ok_or(FeishuOAuthError::ReauthorizationRequired)?;
    let now = Utc::now().timestamp();
    if bundle.access_expires_at > now + TOKEN_REFRESH_MARGIN_SECONDS {
        return Ok(FeishuAccessToken(bundle.access_token.clone()));
    }
    if bundle.refresh_token.is_empty() || bundle.refresh_expires_at <= now {
        return Err(FeishuOAuthError::ReauthorizationRequired);
    }

    let app_secret = load_app_secret(app_id)?.ok_or(FeishuOAuthError::ReauthorizationRequired)?;
    let response = refresh_token(app_id, &app_secret, &bundle.refresh_token).await?;
    let previous_scopes = std::mem::take(&mut bundle.scopes);
    let refreshed = token_response_to_bundle(response, Some(previous_scopes))?;
    store_token_bundle(app_id, &refreshed)?;
    Ok(FeishuAccessToken(refreshed.access_token.clone()))
}

/// 查询连接状态，不读取或返回具体凭据内容。
pub fn connection_status(app_id: &str) -> Result<FeishuOAuthStatus, FeishuOAuthError> {
    validate_app_id(app_id)?;
    match load_token_bundle(app_id)? {
        Some(bundle) => Ok(status_from_bundle(app_id, &bundle)),
        None => Ok(FeishuOAuthStatus {
            connected: false,
            app_id: app_id.to_string(),
            scopes: Vec::new(),
            access_expires_at: None,
            refresh_expires_at: None,
            reauthorization_required: true,
        }),
    }
}

/// 删除本应用的飞书认证凭据。删除不存在的凭据也视为成功。
pub fn disconnect(app_id: &str) -> Result<(), FeishuOAuthError> {
    validate_app_id(app_id)?;
    credential_delete(&credential_account(app_id, "app-secret"))?;
    credential_delete(&credential_account(app_id, "token-bundle"))?;
    Ok(())
}

fn validate_app_id(app_id: &str) -> Result<(), FeishuOAuthError> {
    let valid = app_id.starts_with("cli_")
        && app_id.len() <= 128
        && app_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if valid {
        Ok(())
    } else {
        Err(FeishuOAuthError::InvalidAppId)
    }
}

fn generate_state() -> String {
    // 两个 UUID v4 提供约 244 bit 随机性，且字符集可直接安全放入 URL。
    format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4())
}

fn build_authorization_url(app_id: &str, state: &str) -> Result<reqwest::Url, FeishuOAuthError> {
    validate_app_id(app_id)?;
    let mut url =
        reqwest::Url::parse(AUTHORIZE_URL).map_err(|_| FeishuOAuthError::InvalidTokenResponse)?;
    url.query_pairs_mut()
        .append_pair("client_id", app_id)
        .append_pair("redirect_uri", REDIRECT_URI)
        .append_pair("state", state)
        .append_pair("scope", READONLY_SCOPES);
    Ok(url)
}

async fn wait_for_callback(
    listener: &TcpListener,
    expected_state: &str,
) -> Result<SecretValue, FeishuOAuthError> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    for _ in 0..MAX_CALLBACK_ATTEMPTS {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(FeishuOAuthError::CallbackTimeout);
        }
        let (mut stream, _) = timeout(remaining, listener.accept())
            .await
            .map_err(|_| FeishuOAuthError::CallbackTimeout)?
            .map_err(|_| FeishuOAuthError::InvalidCallback)?;

        let mut buffer = vec![0_u8; MAX_CALLBACK_REQUEST_BYTES];
        let read = timeout(Duration::from_secs(5), stream.read(&mut buffer))
            .await
            .map_err(|_| FeishuOAuthError::InvalidCallback)?
            .map_err(|_| FeishuOAuthError::InvalidCallback)?;
        let outcome = parse_callback_request(&buffer[..read], expected_state);
        match outcome {
            Ok(CallbackOutcome::Code(code)) => {
                write_callback_response(&mut stream, true).await;
                return Ok(code);
            }
            Ok(CallbackOutcome::Denied) => {
                write_callback_response(&mut stream, false).await;
                return Err(FeishuOAuthError::AccessDenied);
            }
            Ok(CallbackOutcome::Ignore) => {
                write_callback_response(&mut stream, false).await;
            }
            Err(FeishuOAuthError::StateMismatch) => {
                write_callback_response(&mut stream, false).await;
                return Err(FeishuOAuthError::StateMismatch);
            }
            Err(_) => {
                write_callback_response(&mut stream, false).await;
            }
        }
    }
    Err(FeishuOAuthError::InvalidCallback)
}

fn parse_callback_request(
    request: &[u8],
    expected_state: &str,
) -> Result<CallbackOutcome, FeishuOAuthError> {
    let request = std::str::from_utf8(request).map_err(|_| FeishuOAuthError::InvalidCallback)?;
    let first_line = request
        .lines()
        .next()
        .ok_or(FeishuOAuthError::InvalidCallback)?;
    let mut parts = first_line.split_whitespace();
    if parts.next() != Some("GET") {
        return Ok(CallbackOutcome::Ignore);
    }
    let target = parts.next().ok_or(FeishuOAuthError::InvalidCallback)?;
    let url = reqwest::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|_| FeishuOAuthError::InvalidCallback)?;
    if url.path() != "/callback" {
        return Ok(CallbackOutcome::Ignore);
    }

    let mut code = None;
    let mut state = None;
    let mut denied = false;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.into_owned()),
            "state" => state = Some(value.into_owned()),
            "error" if value == "access_denied" => denied = true,
            _ => {}
        }
    }
    let mut state = state.ok_or(FeishuOAuthError::InvalidCallback)?;
    let state_matches = constant_time_eq(state.as_bytes(), expected_state.as_bytes());
    state.zeroize();
    if !state_matches {
        if let Some(value) = code.as_mut() {
            value.zeroize();
        }
        return Err(FeishuOAuthError::StateMismatch);
    }
    if denied {
        if let Some(value) = code.as_mut() {
            value.zeroize();
        }
        return Ok(CallbackOutcome::Denied);
    }
    let code = code
        .filter(|value| !value.trim().is_empty())
        .ok_or(FeishuOAuthError::InvalidCallback)?;
    Ok(CallbackOutcome::Code(SecretValue::new(code)))
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

async fn write_callback_response(stream: &mut tokio::net::TcpStream, success: bool) {
    let (status, title, message) = if success {
        ("200 OK", "飞书授权完成", "可以关闭此页面并返回案件看板。")
    } else {
        ("400 Bad Request", "飞书授权未完成", "请返回案件看板重试。")
    };
    let body = format!(
        "<!doctype html><html lang=\"zh-CN\"><meta charset=\"utf-8\"><title>{title}</title>\
         <body><h1>{title}</h1><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes()).await;
    let _ = stream.shutdown().await;
}

fn oauth_client() -> Result<reqwest::Client, FeishuOAuthError> {
    reqwest::Client::builder()
        .redirect(Policy::none())
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(|_| FeishuOAuthError::Network)
}

async fn exchange_authorization_code(
    app_id: &str,
    app_secret: &SecretValue,
    code: &SecretValue,
) -> Result<TokenResponse, FeishuOAuthError> {
    let request = AuthorizationCodeRequest {
        grant_type: "authorization_code",
        client_id: app_id,
        client_secret: app_secret.expose(),
        code: code.expose(),
        redirect_uri: REDIRECT_URI,
    };
    send_token_request(&request).await
}

async fn refresh_token(
    app_id: &str,
    app_secret: &SecretValue,
    refresh_token: &str,
) -> Result<TokenResponse, FeishuOAuthError> {
    let request = RefreshTokenRequest {
        grant_type: "refresh_token",
        client_id: app_id,
        client_secret: app_secret.expose(),
        refresh_token,
    };
    send_token_request(&request).await
}

async fn send_token_request<T: Serialize + ?Sized>(
    request: &T,
) -> Result<TokenResponse, FeishuOAuthError> {
    let response = oauth_client()?
        .post(TOKEN_URL)
        .json(request)
        .send()
        .await
        .map_err(|_| FeishuOAuthError::Network)?;
    if !response.status().is_success() {
        return Err(FeishuOAuthError::TokenRejected);
    }
    // OAuth token 响应体很小；拒绝异常大响应，避免无界内存占用。
    if response
        .content_length()
        .is_some_and(|length| length > 64 * 1024)
    {
        return Err(FeishuOAuthError::InvalidTokenResponse);
    }
    let token = response
        .json::<TokenResponse>()
        .await
        .map_err(|_| FeishuOAuthError::InvalidTokenResponse)?;
    if token.code != 0 {
        return Err(FeishuOAuthError::TokenRejected);
    }
    Ok(token)
}

fn token_response_to_bundle(
    mut response: TokenResponse,
    previous_scopes: Option<Vec<String>>,
) -> Result<StoredTokenBundle, FeishuOAuthError> {
    let access_token = response
        .access_token
        .take()
        .filter(|value| !value.is_empty())
        .ok_or(FeishuOAuthError::InvalidTokenResponse)?;
    let refresh_token = response
        .refresh_token
        .take()
        .filter(|value| !value.is_empty())
        .ok_or(FeishuOAuthError::ReauthorizationRequired)?;
    let expires_in = response
        .expires_in
        .filter(|value| *value > 0)
        .ok_or(FeishuOAuthError::InvalidTokenResponse)?;
    let refresh_expires_in = response
        .refresh_token_expires_in
        .filter(|value| *value > 0)
        .ok_or(FeishuOAuthError::InvalidTokenResponse)?;
    let scopes = response
        .scope
        .take()
        .map(|value| split_scopes(&value))
        .filter(|value| !value.is_empty())
        .or(previous_scopes)
        .ok_or(FeishuOAuthError::MissingReadonlyScope)?;
    ensure_readonly_scopes(&scopes)?;

    let now = Utc::now().timestamp();
    Ok(StoredTokenBundle {
        access_token,
        refresh_token,
        access_expires_at: now.saturating_add(expires_in),
        refresh_expires_at: now.saturating_add(refresh_expires_in),
        scopes,
    })
}

fn split_scopes(value: &str) -> Vec<String> {
    value
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|scope| !scope.is_empty())
        .map(str::to_string)
        .collect()
}

fn ensure_readonly_scopes(scopes: &[String]) -> Result<(), FeishuOAuthError> {
    let required = [
        "offline_access",
        "bitable:app:readonly",
        "auth:user.id:read",
    ];
    if required
        .iter()
        .all(|required| scopes.iter().any(|scope| scope == required))
    {
        Ok(())
    } else {
        Err(FeishuOAuthError::MissingReadonlyScope)
    }
}

fn status_from_bundle(app_id: &str, bundle: &StoredTokenBundle) -> FeishuOAuthStatus {
    let now = Utc::now().timestamp();
    FeishuOAuthStatus {
        connected: bundle.refresh_expires_at > now,
        app_id: app_id.to_string(),
        scopes: bundle.scopes.clone(),
        access_expires_at: Some(bundle.access_expires_at),
        refresh_expires_at: Some(bundle.refresh_expires_at),
        reauthorization_required: bundle.refresh_expires_at <= now,
    }
}

fn credential_account(app_id: &str, kind: &str) -> String {
    format!("{app_id}:{kind}")
}

fn store_credentials(
    app_id: &str,
    app_secret: &SecretValue,
    bundle: &StoredTokenBundle,
) -> Result<(), FeishuOAuthError> {
    credential_set(
        &credential_account(app_id, "app-secret"),
        app_secret.expose(),
    )?;
    if let Err(error) = store_token_bundle(app_id, bundle) {
        // 避免 token bundle 写失败时留下只有 App Secret 的半套连接。
        let _ = credential_delete(&credential_account(app_id, "app-secret"));
        return Err(error);
    }
    Ok(())
}

fn store_token_bundle(app_id: &str, bundle: &StoredTokenBundle) -> Result<(), FeishuOAuthError> {
    let mut serialized =
        serde_json::to_string(bundle).map_err(|_| FeishuOAuthError::CredentialStore)?;
    let result = credential_set(&credential_account(app_id, "token-bundle"), &serialized);
    serialized.zeroize();
    result
}

fn load_app_secret(app_id: &str) -> Result<Option<SecretValue>, FeishuOAuthError> {
    credential_get(&credential_account(app_id, "app-secret"))
        .map(|value| value.map(SecretValue::new))
}

fn load_token_bundle(app_id: &str) -> Result<Option<StoredTokenBundle>, FeishuOAuthError> {
    let Some(mut serialized) = credential_get(&credential_account(app_id, "token-bundle"))? else {
        return Ok(None);
    };
    let parsed = serde_json::from_str::<StoredTokenBundle>(&serialized)
        .map_err(|_| FeishuOAuthError::CredentialStore);
    serialized.zeroize();
    parsed.map(Some)
}

#[cfg(target_os = "windows")]
fn credential_set(account: &str, value: &str) -> Result<(), FeishuOAuthError> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    if value.len() > MAX_CREDENTIAL_BLOB_BYTES {
        return Err(FeishuOAuthError::CredentialStore);
    }
    let mut target = wide_null(&credential_target(account));
    let mut username = wide_null("CaseBoard");
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
    let result =
        unsafe { CredWriteW(&credential, 0) }.map_err(|_| FeishuOAuthError::CredentialStore);
    blob.zeroize();
    result
}

#[cfg(not(target_os = "windows"))]
fn credential_set(_account: &str, _value: &str) -> Result<(), FeishuOAuthError> {
    Err(FeishuOAuthError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn credential_get(account: &str) -> Result<Option<String>, FeishuOAuthError> {
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
        return Err(FeishuOAuthError::CredentialStore);
    }
    if raw.is_null() {
        return Err(FeishuOAuthError::CredentialStore);
    }
    let credential = unsafe { &*raw };
    let blob = if credential.CredentialBlobSize == 0 {
        &[][..]
    } else if credential.CredentialBlob.is_null()
        || credential.CredentialBlobSize as usize > MAX_CREDENTIAL_BLOB_BYTES
    {
        unsafe { CredFree(raw.cast()) };
        return Err(FeishuOAuthError::CredentialStore);
    } else {
        unsafe {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
        }
    };
    let value = String::from_utf8(blob.to_vec()).map_err(|_| FeishuOAuthError::CredentialStore);
    unsafe { CredFree(raw.cast()) };
    value.map(Some)
}

#[cfg(not(target_os = "windows"))]
fn credential_get(_account: &str) -> Result<Option<String>, FeishuOAuthError> {
    Err(FeishuOAuthError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn credential_delete(account: &str) -> Result<(), FeishuOAuthError> {
    use windows::core::{HRESULT, PCWSTR};
    use windows::Win32::Foundation::ERROR_NOT_FOUND;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide_null(&credential_target(account));
    match unsafe { CredDeleteW(PCWSTR(target.as_ptr()), CRED_TYPE_GENERIC, None) } {
        Ok(()) => Ok(()),
        Err(error) if error.code() == HRESULT::from_win32(ERROR_NOT_FOUND.0) => Ok(()),
        Err(_) => Err(FeishuOAuthError::CredentialStore),
    }
}

#[cfg(not(target_os = "windows"))]
fn credential_delete(_account: &str) -> Result<(), FeishuOAuthError> {
    Err(FeishuOAuthError::UnsupportedPlatform)
}

#[cfg(target_os = "windows")]
fn credential_target(account: &str) -> String {
    format!("{CREDENTIAL_SERVICE}/{account}")
}

#[cfg(target_os = "windows")]
fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authorization_url_has_exact_readonly_scope_and_loopback_redirect() {
        let state = "state-value";
        let url = build_authorization_url("cli_test123", state).expect("authorization URL");
        let query = url
            .query_pairs()
            .collect::<std::collections::HashMap<_, _>>();
        assert_eq!(
            query.get("client_id").map(|v| v.as_ref()),
            Some("cli_test123")
        );
        assert_eq!(
            query.get("redirect_uri").map(|v| v.as_ref()),
            Some(REDIRECT_URI)
        );
        assert_eq!(query.get("state").map(|v| v.as_ref()), Some(state));
        assert_eq!(
            query.get("scope").map(|v| v.as_ref()),
            Some(READONLY_SCOPES)
        );
        assert!(!url.as_str().contains("app_secret"));
        assert!(!url.as_str().contains("access_token"));
    }

    #[test]
    fn callback_requires_matching_state_and_hides_code_from_errors() {
        let request =
            b"GET /callback?code=one-time-code&state=correct HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n";
        match parse_callback_request(request, "correct").expect("valid callback") {
            CallbackOutcome::Code(code) => assert_eq!(code.expose(), "one-time-code"),
            _ => panic!("expected authorization code"),
        }

        let error = match parse_callback_request(request, "wrong") {
            Err(error) => error,
            Ok(_) => panic!("expected state mismatch"),
        };
        assert_eq!(error, FeishuOAuthError::StateMismatch);
        let public = format!("{} {}", error.code(), error);
        assert!(!public.contains("one-time-code"));
        assert!(!public.contains("correct"));
        assert!(!public.contains("wrong"));
    }

    #[test]
    fn callback_handles_denial_without_exposing_query() {
        let request = b"GET /callback?error=access_denied&state=correct HTTP/1.1\r\n\r\n";
        assert!(matches!(
            parse_callback_request(request, "correct").expect("denial callback"),
            CallbackOutcome::Denied
        ));
    }

    #[test]
    fn token_bundle_debug_redacts_all_credentials() {
        let bundle = StoredTokenBundle {
            access_token: "access-secret-marker".to_string(),
            refresh_token: "refresh-secret-marker".to_string(),
            access_expires_at: 10,
            refresh_expires_at: 20,
            scopes: split_scopes(READONLY_SCOPES),
        };
        let debug = format!("{bundle:?}");
        assert!(!debug.contains("access-secret-marker"));
        assert!(!debug.contains("refresh-secret-marker"));
        assert!(debug.contains("[REDACTED]"));
    }

    #[test]
    fn token_response_requires_refresh_and_all_readonly_scopes() {
        let valid = TokenResponse {
            code: 0,
            access_token: Some("access".to_string()),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(7200),
            refresh_token_expires_in: Some(604800),
            scope: Some(READONLY_SCOPES.to_string()),
        };
        let bundle = token_response_to_bundle(valid, None).expect("complete token response");
        assert!(bundle.scopes.iter().any(|scope| scope == "offline_access"));

        let incomplete = TokenResponse {
            code: 0,
            access_token: Some("access".to_string()),
            refresh_token: Some("refresh".to_string()),
            expires_in: Some(7200),
            refresh_token_expires_in: Some(604800),
            scope: Some("bitable:app:readonly auth:user.id:read".to_string()),
        };
        assert_eq!(
            token_response_to_bundle(incomplete, None).expect_err("offline access required"),
            FeishuOAuthError::MissingReadonlyScope
        );
    }

    #[test]
    fn invalid_app_ids_are_rejected_before_credential_lookup() {
        for invalid in ["", "app_123", "cli_bad/value", " cli_test"] {
            assert_eq!(
                validate_app_id(invalid),
                Err(FeishuOAuthError::InvalidAppId)
            );
        }
        assert_eq!(validate_app_id("cli_aad702aed6789cbb"), Ok(()));
    }

    #[test]
    fn state_is_high_entropy_and_url_safe() {
        let first = generate_state();
        let second = generate_state();
        assert_ne!(first, second);
        assert!(first.len() >= 64);
        assert!(first
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-'));
    }
}
