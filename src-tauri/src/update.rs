//! 版本检测 —— 方律个人版默认关闭原版远程更新检查,避免被公开版覆盖。
//!
//! 2026-05-25 V0.1.8 加。
//!
//! 设计:
//!   - 数据源:方律私有发布通道。当前个人版默认关闭远程更新检查,后续发布策略确定后再启用。
//!   - 当前版本:`env!("CARGO_PKG_VERSION")`,跟 Cargo.toml 一致
//!   - 比对:语义化版本(major.minor.patch),远程严格大于本地才算落后
//!   - 超时:8s。失败不报错,返回 `has_update=false` + error 字段给前端日志用
//!
//! 作者明确要求(2026-05-25):**不强制更新**,只提示。用户可点「取消」。

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

const VERSION_JSON_URL: &str =
    "https://raw.githubusercontent.com/fanglv8653/case-board-fanglv/main/release/version.json";
const FETCH_TIMEOUT_SEC: u64 = 8;
const INVALID_REMOTE_VERSION_ERROR: &str = "invalid_remote_version";
const INVALID_CURRENT_VERSION_ERROR: &str = "invalid_current_version";
const UNTRUSTED_DOWNLOAD_URL_ERROR: &str = "untrusted_download_url";
const OFFICIAL_RELEASE_PATH: &str = "/fanglv8653/case-board-fanglv/releases";

/// 远程 version.json 反序列化结构
#[derive(Debug, Clone, Deserialize)]
struct RemoteVersion {
    version: String,
    #[serde(default)]
    released_at: Option<String>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    download_url: Option<String>,
}

/// 给前端的检测结果(序列化为 JSON)
#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    /// 当前本机版本(Cargo.toml)
    pub current: String,
    /// 远程最新版本(失败时 None)
    pub latest: Option<String>,
    /// 是否落后(latest > current 才 true)
    pub has_update: bool,
    /// 发布日期(YYYY-MM-DD)
    pub released_at: Option<String>,
    /// 更新说明(Markdown / 纯文本均可,前端按纯文本渲染避免 XSS)
    pub notes: Option<String>,
    /// 下载页 URL(用户点「去下载」开浏览器去这里)
    pub download_url: Option<String>,
    /// 检测失败时的错误描述(成功为 None)。前端只在调试时显示。
    pub error: Option<String>,
}

impl UpdateInfo {
    fn fail(current: &str, msg: impl Into<String>) -> Self {
        Self {
            current: current.to_string(),
            latest: None,
            has_update: false,
            released_at: None,
            notes: None,
            download_url: None,
            error: Some(msg.into()),
        }
    }
}

/// 检测远程最新版本。
pub async fn check_for_update() -> UpdateInfo {
    let current = env!("CARGO_PKG_VERSION").to_string();

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(FETCH_TIMEOUT_SEC))
        .build()
    {
        Ok(c) => c,
        Err(e) => return UpdateInfo::fail(&current, format!("HTTP 客户端创建失败: {}", e)),
    };

    let resp = match client
        .get(VERSION_JSON_URL)
        .header("Accept", "application/json")
        .header("User-Agent", format!("FanglvCaseBoard/{}", current))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return UpdateInfo::fail(&current, format!("拉取 version.json 失败: {}", e)),
    };

    if !resp.status().is_success() {
        return UpdateInfo::fail(&current, format!("HTTP {}", resp.status().as_u16()));
    }

    let remote: RemoteVersion = match resp.json().await {
        Ok(v) => v,
        Err(e) => return UpdateInfo::fail(&current, format!("解析 version.json 失败: {}", e)),
    };

    update_info_from_remote(&current, remote)
}

fn update_info_from_remote(current: &str, remote: RemoteVersion) -> UpdateInfo {
    let remote_version = match parse_semver(&remote.version) {
        Ok(version) => version,
        Err(()) => return UpdateInfo::fail(current, INVALID_REMOTE_VERSION_ERROR),
    };
    let current_version = match parse_semver(current) {
        Ok(version) => version,
        Err(()) => return UpdateInfo::fail(current, INVALID_CURRENT_VERSION_ERROR),
    };
    let download_url = match official_download_url(&remote.version, remote.download_url.as_deref())
    {
        Ok(url) => url,
        Err(error) => return UpdateInfo::fail(current, error),
    };

    UpdateInfo {
        current: current.to_string(),
        latest: Some(remote.version),
        has_update: remote_version.precedence_cmp(&current_version) == Ordering::Greater,
        released_at: remote.released_at,
        notes: remote.notes,
        download_url: Some(download_url),
        error: None,
    }
}

fn official_download_url(version: &str, candidate: Option<&str>) -> Result<String, &'static str> {
    let fallback = format!("https://github.com{OFFICIAL_RELEASE_PATH}/tag/v{version}-fanglv");
    let value = candidate.unwrap_or(&fallback);
    let parsed = reqwest::Url::parse(value).map_err(|_| UNTRUSTED_DOWNLOAD_URL_ERROR)?;
    if parsed.scheme() != "https"
        || parsed.host_str() != Some("github.com")
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(UNTRUSTED_DOWNLOAD_URL_ERROR);
    }

    let tag_path = format!("{OFFICIAL_RELEASE_PATH}/tag/v{version}-fanglv");
    let download_prefix = format!("{OFFICIAL_RELEASE_PATH}/download/v{version}-fanglv/");
    let path = parsed.path();
    let valid_download_asset = path
        .strip_prefix(&download_prefix)
        .is_some_and(|asset| !asset.is_empty() && !asset.contains('/'));
    if path != tag_path && !valid_download_asset {
        return Err(UNTRUSTED_DOWNLOAD_URL_ERROR);
    }
    Ok(parsed.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticVersion {
    major: u64,
    minor: u64,
    patch: u64,
    prerelease: Option<Vec<PrereleaseIdentifier>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PrereleaseIdentifier {
    Numeric(u64),
    Text(String),
}

impl SemanticVersion {
    fn precedence_cmp(&self, other: &Self) -> Ordering {
        let core =
            (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch));
        if core != Ordering::Equal {
            return core;
        }
        match (&self.prerelease, &other.prerelease) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(left), Some(right)) => {
                for (left_id, right_id) in left.iter().zip(right.iter()) {
                    let ordering = match (left_id, right_id) {
                        (
                            PrereleaseIdentifier::Numeric(left),
                            PrereleaseIdentifier::Numeric(right),
                        ) => left.cmp(right),
                        (PrereleaseIdentifier::Numeric(_), PrereleaseIdentifier::Text(_)) => {
                            Ordering::Less
                        }
                        (PrereleaseIdentifier::Text(_), PrereleaseIdentifier::Numeric(_)) => {
                            Ordering::Greater
                        }
                        (PrereleaseIdentifier::Text(left), PrereleaseIdentifier::Text(right)) => {
                            left.cmp(right)
                        }
                    };
                    if ordering != Ordering::Equal {
                        return ordering;
                    }
                }
                left.len().cmp(&right.len())
            }
        }
    }
}

fn parse_semver(value: &str) -> Result<SemanticVersion, ()> {
    if value.is_empty() || value.trim() != value {
        return Err(());
    }
    let mut build_parts = value.split('+');
    let core_and_prerelease = build_parts.next().ok_or(())?;
    let build = build_parts.next();
    if build_parts.next().is_some() {
        return Err(());
    }
    if let Some(build) = build {
        validate_identifiers(build, false)?;
    }

    let (core, prerelease) = match core_and_prerelease.split_once('-') {
        Some((core, prerelease)) => (core, Some(parse_prerelease(prerelease)?)),
        None => (core_and_prerelease, None),
    };
    let mut core_parts = core.split('.');
    let major = parse_core_number(core_parts.next().ok_or(())?)?;
    let minor = parse_core_number(core_parts.next().ok_or(())?)?;
    let patch = parse_core_number(core_parts.next().ok_or(())?)?;
    if core_parts.next().is_some() {
        return Err(());
    }
    Ok(SemanticVersion {
        major,
        minor,
        patch,
        prerelease,
    })
}

fn parse_core_number(value: &str) -> Result<u64, ()> {
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(());
    }
    value.parse().map_err(|_| ())
}

fn validate_identifiers(value: &str, reject_numeric_leading_zero: bool) -> Result<(), ()> {
    for identifier in value.split('.') {
        if identifier.is_empty()
            || !identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || (reject_numeric_leading_zero
                && identifier.len() > 1
                && identifier.starts_with('0')
                && identifier.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(());
        }
    }
    Ok(())
}

fn parse_prerelease(value: &str) -> Result<Vec<PrereleaseIdentifier>, ()> {
    validate_identifiers(value, true)?;
    value
        .split('.')
        .map(|identifier| {
            if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
                identifier
                    .parse()
                    .map(PrereleaseIdentifier::Numeric)
                    .map_err(|_| ())
            } else {
                Ok(PrereleaseIdentifier::Text(identifier.to_string()))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_semver_accepts_current_and_orders_prereleases() {
        let stable = parse_semver("0.8.3").expect("current stable version");
        let rc = parse_semver("0.8.3-rc.1").expect("legal prerelease");
        let next_rc = parse_semver("0.8.3-rc.2+build.7").expect("legal build metadata");
        assert_eq!(stable.precedence_cmp(&rc), Ordering::Greater);
        assert_eq!(next_rc.precedence_cmp(&rc), Ordering::Greater);
    }

    #[test]
    fn strict_semver_rejects_malformed_extra_and_illegal_prerelease() {
        for invalid in [
            "0.8",
            "0.8.3.1",
            "00.8.3",
            "v0.8.3",
            "0.8.3-",
            "0.8.3-alpha..1",
            "0.8.3-01",
            "0.8.3-alpha_1",
            " 0.8.3",
        ] {
            assert!(parse_semver(invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn official_release_urls_are_narrowly_allowlisted() {
        for allowed in [
            "https://github.com/fanglv8653/case-board-fanglv/releases/tag/v0.8.3-fanglv",
            "https://github.com/fanglv8653/case-board-fanglv/releases/download/v0.8.3-fanglv/FanglvCaseBoard_0.8.3_x64-setup.exe",
        ] {
            assert_eq!(
                official_download_url("0.8.3", Some(allowed)).as_deref(),
                Ok(allowed)
            );
        }
        for rejected in [
            "https://evil.example/releases/tag/v0.8.3-fanglv",
            "http://github.com/fanglv8653/case-board-fanglv/releases/tag/v0.8.3-fanglv",
            "https://github.com.evil.example/fanglv8653/case-board-fanglv/releases/tag/v0.8.3-fanglv",
            "https://github.com/fanglv8653/case-board-fanglv/releases/tag/v0.8.4-fanglv",
            "https://github.com/fanglv8653/case-board-fanglv/releases/download/v0.8.3-fanglv/dir/setup.exe",
        ] {
            assert_eq!(
                official_download_url("0.8.3", Some(rejected)),
                Err(UNTRUSTED_DOWNLOAD_URL_ERROR)
            );
        }
    }

    #[test]
    fn malicious_marker_fails_without_exposing_download_url() {
        let result = update_info_from_remote(
            "0.8.2",
            RemoteVersion {
                version: "0.8.3".to_string(),
                released_at: None,
                notes: Some("malicious marker".to_string()),
                download_url: Some("https://evil.example/setup.exe".to_string()),
            },
        );
        assert!(!result.has_update);
        assert_eq!(result.error.as_deref(), Some(UNTRUSTED_DOWNLOAD_URL_ERROR));
        assert!(result.latest.is_none());
        assert!(result.notes.is_none());
        assert!(result.download_url.is_none());
    }
}
