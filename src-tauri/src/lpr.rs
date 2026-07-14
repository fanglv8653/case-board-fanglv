//! 中国人民银行 LPR 官方公告刷新与本地缓存。
//!
//! 网络失败或页面结构变化时必须 fail closed：保留上次成功缓存，由前端继续
//! 使用修正后的内置基线，绝不写入猜测值。

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{Datelike, FixedOffset, NaiveDate, Timelike, Utc};
use regex::Regex;
use reqwest::redirect::Policy;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use tokio::sync::Mutex;

const LPR_INDEX_URL: &str =
    "https://www.pbc.gov.cn/zhengcehuobisi/125207/125213/125440/3876551/index.html";
const OFFICIAL_HOST: &str = "www.pbc.gov.cn";
const MAX_HTML_BYTES: usize = 2 * 1024 * 1024;

static REFRESH_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn refresh_lock() -> &'static Mutex<()> {
    REFRESH_LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LprPoint {
    pub publication_date: String,
    pub lpr_1y: f64,
    pub lpr_5y: f64,
    pub source_url: String,
    pub fetched_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LprSnapshot {
    pub points: Vec<LprPoint>,
    pub latest_published_date: Option<String>,
    pub latest_1y: Option<f64>,
    pub latest_5y: Option<f64>,
    pub source_url: Option<String>,
    pub last_success_at: Option<String>,
    pub last_attempt_at: Option<String>,
    pub stale: bool,
    pub last_error: Option<String>,
    pub data_origin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct LprRefreshResult {
    pub status: String,
    pub added_count: usize,
    pub snapshot: LprSnapshot,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
struct AnnouncementLink {
    publication_date: NaiveDate,
    url: String,
}

#[derive(Debug, Clone, PartialEq)]
struct ParsedAnnouncement {
    publication_date: NaiveDate,
    lpr_1y: f64,
    lpr_5y: f64,
}

fn cst_now() -> chrono::DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(8 * 3600).expect("valid UTC+8"))
}

fn strip_tags(html: &str) -> String {
    let tags = Regex::new(r"(?s)<[^>]+>").expect("tag regex");
    tags.replace_all(html, " ").replace("&nbsp;", " ")
}

fn parse_date(year: &str, month: &str, day: &str) -> Result<NaiveDate, String> {
    let year = year.parse::<i32>().map_err(|_| "公告年份无效")?;
    let month = month.parse::<u32>().map_err(|_| "公告月份无效")?;
    let day = day.parse::<u32>().map_err(|_| "公告日期无效")?;
    NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| "公告日期无效".to_string())
}

fn parse_latest_announcement_link(index_html: &str) -> Result<AnnouncementLink, String> {
    let anchor_re = Regex::new(r#"(?s)<a[^>]+href=[\"']([^\"']+)[\"'][^>]*>(.*?)</a>"#)
        .map_err(|e| e.to_string())?;
    let date_re = Regex::new(r"(\d{4})年(\d{1,2})月(\d{1,2})日").map_err(|e| e.to_string())?;
    let base = reqwest::Url::parse(LPR_INDEX_URL).map_err(|e| e.to_string())?;
    let mut candidates = Vec::new();

    for captures in anchor_re.captures_iter(index_html) {
        let title = strip_tags(captures.get(2).map(|m| m.as_str()).unwrap_or_default());
        if !title.contains("受权公布贷款市场报价利率") || !title.contains("LPR") {
            continue;
        }
        let Some(date_cap) = date_re.captures(&title) else {
            continue;
        };
        let date = parse_date(&date_cap[1], &date_cap[2], &date_cap[3])?;
        let url = base
            .join(captures.get(1).map(|m| m.as_str()).unwrap_or_default())
            .map_err(|_| "LPR公告链接无效".to_string())?;
        if url.scheme() != "https" || url.host_str() != Some(OFFICIAL_HOST) {
            return Err("LPR公告链接不是人民银行HTTPS地址".into());
        }
        candidates.push(AnnouncementLink {
            publication_date: date,
            url: url.to_string(),
        });
    }

    candidates
        .into_iter()
        .max_by_key(|item| item.publication_date)
        .ok_or_else(|| "人民银行栏目中未找到LPR公告".to_string())
}

fn parse_announcement(html: &str, expected_date: NaiveDate) -> Result<ParsedAnnouncement, String> {
    let text = strip_tags(html);
    if !text.contains("下一次发布LPR之前有效") {
        return Err("公告正文缺少有效期校验语句".into());
    }
    let date_re = Regex::new(r"(\d{4})年(\d{1,2})月(\d{1,2})日").map_err(|e| e.to_string())?;
    let date_cap = date_re
        .captures(&text)
        .ok_or_else(|| "公告正文缺少发布日期".to_string())?;
    let publication_date = parse_date(&date_cap[1], &date_cap[2], &date_cap[3])?;
    if publication_date != expected_date {
        return Err("LPR列表日期与公告正文日期不一致".into());
    }
    if publication_date > cst_now().date_naive() {
        return Err("拒绝写入未来发布日期".into());
    }

    let one_re =
        Regex::new(r"1年期LPR为[：:]?\s*([0-9]+(?:\.[0-9]+)?)%").map_err(|e| e.to_string())?;
    let five_re =
        Regex::new(r"5年期以上LPR为[：:]?\s*([0-9]+(?:\.[0-9]+)?)%").map_err(|e| e.to_string())?;
    let lpr_1y = one_re
        .captures(&text)
        .and_then(|c| c.get(1))
        .ok_or_else(|| "公告正文缺少1年期LPR".to_string())?
        .as_str()
        .parse::<f64>()
        .map_err(|_| "1年期LPR格式无效".to_string())?;
    let lpr_5y = five_re
        .captures(&text)
        .and_then(|c| c.get(1))
        .ok_or_else(|| "公告正文缺少5年期以上LPR".to_string())?
        .as_str()
        .parse::<f64>()
        .map_err(|_| "5年期以上LPR格式无效".to_string())?;
    if !(0.0 < lpr_1y && lpr_1y < 20.0 && 0.0 < lpr_5y && lpr_5y < 20.0) {
        return Err("公告LPR超出合理范围".into());
    }
    Ok(ParsedAnnouncement {
        publication_date,
        lpr_1y,
        lpr_5y,
    })
}

fn should_auto_refresh_at(
    now: chrono::DateTime<FixedOffset>,
    latest_published_date: Option<NaiveDate>,
    last_attempt_cst_date: Option<NaiveDate>,
) -> bool {
    if now.day() < 20 || (now.day() == 20 && now.hour() < 10) {
        return false;
    }
    if latest_published_date
        .is_some_and(|date| date.year() == now.year() && date.month() == now.month())
    {
        return false;
    }
    last_attempt_cst_date != Some(now.date_naive())
}

async fn read_html(client: &reqwest::Client, url: &str) -> Result<String, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if parsed.scheme() != "https" || parsed.host_str() != Some(OFFICIAL_HOST) {
        return Err("只允许访问人民银行HTTPS地址".into());
    }
    let response = client.get(parsed).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!("人民银行返回HTTP {}", response.status()));
    }
    if response.url().scheme() != "https" || response.url().host_str() != Some(OFFICIAL_HOST) {
        return Err("人民银行请求被重定向到非官方地址".into());
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| !value.to_ascii_lowercase().contains("text/html"))
    {
        return Err("人民银行响应不是HTML".into());
    }
    if response
        .content_length()
        .is_some_and(|size| size as usize > MAX_HTML_BYTES)
    {
        return Err("人民银行响应体过大".into());
    }
    let bytes = response.bytes().await.map_err(|e| e.to_string())?;
    if bytes.len() > MAX_HTML_BYTES {
        return Err("人民银行响应体过大".into());
    }
    String::from_utf8(bytes.to_vec()).map_err(|_| "人民银行响应不是UTF-8".into())
}

fn http_client() -> Result<reqwest::Client, String> {
    let user_agent = concat!("CaseBoard-LPR-Reference/", env!("CARGO_PKG_VERSION"));
    reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(Policy::limited(3))
        .user_agent(user_agent)
        .build()
        .map_err(|e| e.to_string())
}

async fn load_snapshot(pool: &SqlitePool) -> Result<LprSnapshot, String> {
    let rows = sqlx::query(
        "SELECT publication_date, lpr_1y, lpr_5y, source_url, fetched_at \
         FROM lpr_rate_cache ORDER BY publication_date ASC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let points: Vec<LprPoint> = rows
        .into_iter()
        .map(|row| LprPoint {
            publication_date: row.get("publication_date"),
            lpr_1y: row.get("lpr_1y"),
            lpr_5y: row.get("lpr_5y"),
            source_url: row.get("source_url"),
            fetched_at: row.get("fetched_at"),
        })
        .collect();
    let state = sqlx::query(
        "SELECT last_attempt_at, last_success_at, latest_published_date, last_error \
         FROM lpr_refresh_state WHERE id = 1",
    )
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;
    let (last_attempt_at, last_success_at, state_latest, last_error) = state
        .map(|row| {
            (
                row.get::<Option<String>, _>("last_attempt_at"),
                row.get::<Option<String>, _>("last_success_at"),
                row.get::<Option<String>, _>("latest_published_date"),
                row.get::<Option<String>, _>("last_error"),
            )
        })
        .unwrap_or_default();
    let latest = points.last().cloned();
    let latest_published_date = latest
        .as_ref()
        .map(|point| point.publication_date.clone())
        .or(state_latest);
    let now = cst_now();
    let latest_date = latest_published_date
        .as_deref()
        .and_then(|date| NaiveDate::parse_from_str(date, "%Y-%m-%d").ok());
    // 上月公告在本月新公告发布前仍然有效；仅到本月应检查时点后仍无本月点才标 stale。
    let stale = should_auto_refresh_at(now, latest_date, None);
    Ok(LprSnapshot {
        latest_1y: latest.as_ref().map(|point| point.lpr_1y),
        latest_5y: latest.as_ref().map(|point| point.lpr_5y),
        source_url: latest.as_ref().map(|point| point.source_url.clone()),
        points,
        latest_published_date,
        last_success_at,
        last_attempt_at,
        stale,
        last_error,
        data_origin: if latest.is_some() {
            "official_cache".into()
        } else {
            "builtin".into()
        },
    })
}

async fn mark_attempt(pool: &SqlitePool, error: Option<&str>) -> Result<(), String> {
    let now = cst_now();
    sqlx::query(
        "UPDATE lpr_refresh_state SET last_attempt_at = ?1, last_attempt_cst_date = ?2, \
         last_error = ?3 WHERE id = 1",
    )
    .bind(now.to_rfc3339())
    .bind(now.date_naive().format("%Y-%m-%d").to_string())
    .bind(error)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(())
}

async fn persist_announcement(
    pool: &SqlitePool,
    parsed: &ParsedAnnouncement,
    source_url: &str,
) -> Result<usize, String> {
    let date = parsed.publication_date.format("%Y-%m-%d").to_string();
    if let Some(row) =
        sqlx::query("SELECT lpr_1y, lpr_5y FROM lpr_rate_cache WHERE publication_date = ?1")
            .bind(&date)
            .fetch_optional(pool)
            .await
            .map_err(|e| e.to_string())?
    {
        let existing_1y: f64 = row.get("lpr_1y");
        let existing_5y: f64 = row.get("lpr_5y");
        if (existing_1y - parsed.lpr_1y).abs() > 1e-9 || (existing_5y - parsed.lpr_5y).abs() > 1e-9
        {
            return Err("官方缓存同一发布日期存在冲突值，拒绝覆盖".into());
        }
    }

    let now = cst_now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let inserted = sqlx::query(
        "INSERT OR IGNORE INTO lpr_rate_cache \
         (publication_date, lpr_1y, lpr_5y, source_url, fetched_at) VALUES (?1, ?2, ?3, ?4, ?5)",
    )
    .bind(&date)
    .bind(parsed.lpr_1y)
    .bind(parsed.lpr_5y)
    .bind(source_url)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected() as usize;
    sqlx::query(
        "UPDATE lpr_refresh_state SET last_attempt_at = ?1, last_attempt_cst_date = ?2, \
         last_success_at = ?1, latest_published_date = ?3, last_error = NULL WHERE id = 1",
    )
    .bind(&now)
    .bind(cst_now().date_naive().format("%Y-%m-%d").to_string())
    .bind(&date)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(inserted)
}

async fn refresh_inner(pool: &SqlitePool) -> Result<LprRefreshResult, String> {
    let guard = match refresh_lock().try_lock() {
        Ok(guard) => guard,
        Err(_) => {
            return Ok(LprRefreshResult {
                status: "in_progress".into(),
                added_count: 0,
                snapshot: load_snapshot(pool).await?,
                warning: Some("LPR官方数据正在刷新，请稍后查看".into()),
            });
        }
    };
    let client = http_client()?;
    let result = async {
        let index_html = read_html(&client, LPR_INDEX_URL).await?;
        let announcement = parse_latest_announcement_link(&index_html)?;
        let body = read_html(&client, &announcement.url).await?;
        let parsed = parse_announcement(&body, announcement.publication_date)?;
        let added_count = persist_announcement(pool, &parsed, &announcement.url).await?;
        let current = cst_now();
        let status = if added_count > 0 {
            "updated"
        } else if parsed.publication_date.year() == current.year()
            && parsed.publication_date.month() == current.month()
        {
            "up_to_date"
        } else {
            "not_published"
        };
        Ok::<_, String>(LprRefreshResult {
            status: status.into(),
            added_count,
            snapshot: load_snapshot(pool).await?,
            warning: (status == "not_published")
                .then(|| "人民银行尚未发布本月LPR，继续使用本地已核验数据".into()),
        })
    }
    .await;
    drop(guard);

    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let _ = mark_attempt(pool, Some(&error)).await;
            Ok(LprRefreshResult {
                status: "fallback".into(),
                added_count: 0,
                snapshot: load_snapshot(pool).await?,
                warning: Some(format!("LPR更新失败，继续使用本地数据：{error}")),
            })
        }
    }
}

#[tauri::command]
pub async fn get_lpr_snapshot(pool: tauri::State<'_, SqlitePool>) -> Result<LprSnapshot, String> {
    load_snapshot(pool.inner()).await
}

#[tauri::command]
pub async fn refresh_lpr_data(
    pool: tauri::State<'_, SqlitePool>,
) -> Result<LprRefreshResult, String> {
    refresh_inner(pool.inner()).await
}

pub fn spawn_startup_refresh(pool: SqlitePool) {
    tauri::async_runtime::spawn(async move {
        let state = sqlx::query(
            "SELECT latest_published_date, last_attempt_cst_date FROM lpr_refresh_state WHERE id = 1",
        )
        .fetch_optional(&pool)
        .await;
        let Ok(state) = state else {
            return;
        };
        let latest = state.as_ref().and_then(|row| {
            row.get::<Option<String>, _>("latest_published_date")
                .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
        });
        let attempted = state.as_ref().and_then(|row| {
            row.get::<Option<String>, _>("last_attempt_cst_date")
                .and_then(|date| NaiveDate::parse_from_str(&date, "%Y-%m-%d").ok())
        });
        if should_auto_refresh_at(cst_now(), latest, attempted) {
            let _ = refresh_inner(&pool).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn cst(y: i32, m: u32, d: u32, h: u32, min: u32) -> chrono::DateTime<FixedOffset> {
        FixedOffset::east_opt(8 * 3600)
            .unwrap()
            .with_ymd_and_hms(y, m, d, h, min, 0)
            .unwrap()
    }

    #[test]
    fn parses_official_list_and_excludes_unrelated_links() {
        let html = r#"
          <a href="/wrong.html">贷款市场报价利率报价行名单</a>
          <a href="./202407/a.html">2024年7月22日全国银行间同业拆借中心受权公布贷款市场报价利率（LPR）及调整发布时间公告</a>
          <a href="./202606/latest.html">2026年6月22日全国银行间同业拆借中心受权公布贷款市场报价利率（LPR）公告</a>
        "#;
        let parsed = parse_latest_announcement_link(html).unwrap();
        assert_eq!(
            parsed.publication_date,
            NaiveDate::from_ymd_opt(2026, 6, 22).unwrap()
        );
        assert!(parsed.url.starts_with("https://www.pbc.gov.cn/"));
    }

    #[test]
    fn parses_announcement_and_rejects_mismatch_or_invalid_rates() {
        let date = NaiveDate::from_ymd_opt(2025, 10, 20).unwrap();
        let html = "2025年10月20日公告 中国人民银行授权全国银行间同业拆借中心公布，2025年10月20日贷款市场报价利率（LPR）为：1年期LPR为3.0%，5年期以上LPR为3.5%。以上LPR在下一次发布LPR之前有效。";
        let parsed = parse_announcement(html, date).unwrap();
        assert_eq!(parsed.lpr_1y, 3.0);
        assert_eq!(parsed.lpr_5y, 3.5);
        assert!(parse_announcement(html, NaiveDate::from_ymd_opt(2025, 10, 21).unwrap()).is_err());
        assert!(parse_announcement(&html.replace("3.0%", "30.0%"), date).is_err());
        assert!(
            parse_announcement(&html.replace("下一次发布LPR之前有效", "长期有效"), date).is_err()
        );
    }

    #[test]
    fn due_policy_uses_beijing_time_and_retries_next_day() {
        assert!(!should_auto_refresh_at(cst(2026, 7, 19, 12, 0), None, None));
        assert!(!should_auto_refresh_at(cst(2026, 7, 20, 9, 59), None, None));
        assert!(should_auto_refresh_at(cst(2026, 7, 20, 10, 0), None, None));
        assert!(!should_auto_refresh_at(
            cst(2026, 7, 20, 11, 0),
            None,
            Some(NaiveDate::from_ymd_opt(2026, 7, 20).unwrap()),
        ));
        assert!(should_auto_refresh_at(
            cst(2026, 7, 21, 10, 0),
            None,
            Some(NaiveDate::from_ymd_opt(2026, 7, 20).unwrap()),
        ));
        assert!(!should_auto_refresh_at(
            cst(2026, 7, 21, 10, 0),
            Some(NaiveDate::from_ymd_opt(2026, 7, 20).unwrap()),
            None,
        ));
    }

    #[tokio::test]
    async fn cache_is_idempotent_and_rejects_conflicting_same_day_values() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let parsed = ParsedAnnouncement {
            publication_date: NaiveDate::from_ymd_opt(2025, 10, 20).unwrap(),
            lpr_1y: 3.0,
            lpr_5y: 3.5,
        };
        assert_eq!(
            persist_announcement(&pool, &parsed, "https://www.pbc.gov.cn/lpr.html")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            persist_announcement(&pool, &parsed, "https://www.pbc.gov.cn/lpr.html")
                .await
                .unwrap(),
            0
        );

        let conflicting = ParsedAnnouncement {
            lpr_1y: 3.1,
            ..parsed
        };
        assert!(
            persist_announcement(&pool, &conflicting, "https://www.pbc.gov.cn/lpr.html")
                .await
                .is_err()
        );

        let snapshot = load_snapshot(&pool).await.unwrap();
        assert_eq!(snapshot.points.len(), 1);
        assert_eq!(snapshot.latest_1y, Some(3.0));
        assert_eq!(snapshot.latest_5y, Some(3.5));
        assert_eq!(snapshot.data_origin, "official_cache");
    }
}
