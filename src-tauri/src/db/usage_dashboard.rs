//! Read-only local usage dashboards.
//!
//! These queries never call a provider. `extraction_metrics` contains one row
//! per extraction-stage invocation, not a vendor invoice. The Yuandian ledger
//! is likewise a local estimate and must never be presented as official
//! balance or quota data.

use chrono::{Datelike, Local, NaiveDate};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

use super::credits::{self, MonthlyCredits};

const RECOGNITION_SOURCE: &str = "local_extraction_metrics";
const YUANDIAN_SOURCE: &str = "local_estimate";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDashboardError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
}

impl UsageDashboardError {
    fn recognition_query() -> Self {
        Self {
            code: "LOCAL_RECOGNITION_USAGE_QUERY_FAILED",
            message: "无法读取本地识别用量，请稍后重试".to_string(),
            retryable: true,
        }
    }

    fn yuandian_query() -> Self {
        Self {
            code: "YUANDIAN_LOCAL_USAGE_QUERY_FAILED",
            message: "无法刷新元典本地用量估算，请稍后重试".to_string(),
            retryable: true,
        }
    }

    fn invalid_range(message: impl Into<String>) -> Self {
        Self {
            code: "LOCAL_USAGE_INVALID_RANGE",
            message: message.into(),
            retryable: false,
        }
    }
}

impl std::fmt::Display for UsageDashboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for UsageDashboardError {}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionUsageQuery {
    /// `day` or `month`.
    pub granularity: String,
    /// Inclusive bucket, `YYYY-MM-DD` for day or `YYYY-MM` for month.
    pub from: Option<String>,
    /// Inclusive bucket, `YYYY-MM-DD` for day or `YYYY-MM` for month.
    pub to: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionUsageBucket {
    pub bucket: String,
    pub stage: String,
    /// Existing metrics call this `backend`. It may include a model label.
    pub provider_model: String,
    /// Number of stage invocations represented by metric rows.
    pub task_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub skipped_count: i64,
    pub average_elapsed_ms: Option<f64>,
    /// Identified from the already-redacted `error_short` value.
    pub rate_limit_429_count: i64,
    /// Unavailable: existing schema stores only the actual successful backend,
    /// not requested-backend -> actual-backend pairs.
    pub fallback_count: Option<i64>,
    /// Unavailable until a future write path records a provider-sourced page
    /// count. File count or image count is not silently substituted.
    pub page_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionUsageCapabilities {
    pub fallback_count_available: bool,
    pub fallback_count_reason: String,
    pub page_count_available: bool,
    pub page_count_reason: String,
    pub rate_limit_source: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RecognitionUsageOverview {
    pub data_source: String,
    pub is_vendor_reported: bool,
    pub generated_at: String,
    pub granularity: String,
    pub from: Option<String>,
    pub to: Option<String>,
    pub buckets: Vec<RecognitionUsageBucket>,
    pub capabilities: RecognitionUsageCapabilities,
}

fn validate_recognition_query(
    query: &RecognitionUsageQuery,
) -> Result<(usize, Option<String>, Option<String>), UsageDashboardError> {
    let bucket_len = match query.granularity.as_str() {
        "day" => 10,
        "month" => 7,
        _ => {
            return Err(UsageDashboardError::invalid_range(
                "granularity 只能是 day 或 month",
            ))
        }
    };
    let validate = |value: &str| -> bool {
        if bucket_len == 10 {
            NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        } else {
            NaiveDate::parse_from_str(&format!("{value}-01"), "%Y-%m-%d").is_ok()
        }
    };
    if query.from.as_deref().is_some_and(|value| !validate(value))
        || query.to.as_deref().is_some_and(|value| !validate(value))
    {
        return Err(UsageDashboardError::invalid_range(
            "日期范围格式与 granularity 不匹配",
        ));
    }
    if matches!((&query.from, &query.to), (Some(from), Some(to)) if from > to) {
        return Err(UsageDashboardError::invalid_range("from 不能晚于 to"));
    }
    Ok((bucket_len, query.from.clone(), query.to.clone()))
}

pub async fn query_recognition_usage(
    pool: &SqlitePool,
    query: &RecognitionUsageQuery,
) -> Result<RecognitionUsageOverview, UsageDashboardError> {
    let (bucket_len, from, to) = validate_recognition_query(query)?;
    let rows: Vec<RecognitionUsageBucket> = sqlx::query_as(
        "SELECT substr(created_at,1,?1) AS bucket, stage, backend AS provider_model, \
                COUNT(*) AS task_count, \
                SUM(CASE WHEN outcome='ok' THEN 1 ELSE 0 END) AS success_count, \
                SUM(CASE WHEN outcome IN ('failed','partial') THEN 1 ELSE 0 END) AS failure_count, \
                SUM(CASE WHEN outcome='skipped' THEN 1 ELSE 0 END) AS skipped_count, \
                AVG(CASE WHEN elapsed_ms >= 0 THEN CAST(elapsed_ms AS REAL) END) \
                    AS average_elapsed_ms, \
                SUM(CASE WHEN error_short IS NOT NULL AND ( \
                    lower(error_short) LIKE '%429%' OR \
                    lower(error_short) LIKE '%rate limit%' OR \
                    error_short LIKE '%限流%' OR error_short LIKE '%配额%' \
                ) THEN 1 ELSE 0 END) AS rate_limit_429_count, \
                NULL AS fallback_count, NULL AS page_count \
         FROM extraction_metrics \
         WHERE (?2 IS NULL OR substr(created_at,1,?1) >= ?2) \
           AND (?3 IS NULL OR substr(created_at,1,?1) <= ?3) \
         GROUP BY bucket,stage,backend \
         ORDER BY bucket DESC,stage,backend",
    )
    .bind(bucket_len as i64)
    .bind(&from)
    .bind(&to)
    .fetch_all(pool)
    .await
    .map_err(|_| UsageDashboardError::recognition_query())?;

    Ok(RecognitionUsageOverview {
        data_source: RECOGNITION_SOURCE.to_string(),
        is_vendor_reported: false,
        generated_at: Local::now().to_rfc3339(),
        granularity: query.granularity.clone(),
        from,
        to,
        buckets: rows,
        capabilities: RecognitionUsageCapabilities {
            fallback_count_available: false,
            fallback_count_reason: "历史记录没有请求后端与实际后端的成对字段，不能可靠反推降级次数"
                .to_string(),
            page_count_available: false,
            page_count_reason: "历史记录没有可靠页数字段，不以文件数或图片数替代".to_string(),
            rate_limit_source: "redacted_error_short_pattern".to_string(),
        },
    })
}

#[tauri::command]
pub async fn get_local_recognition_usage(
    pool: tauri::State<'_, SqlitePool>,
    query: RecognitionUsageQuery,
) -> Result<RecognitionUsageOverview, UsageDashboardError> {
    query_recognition_usage(pool.inner(), &query).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct YuandianLocalUsageOverview {
    pub data_source: String,
    pub is_official_balance: bool,
    pub official_balance: Option<i64>,
    pub estimate_basis: String,
    pub current: MonthlyCredits,
    pub previous_recorded_month: Option<MonthlyCredits>,
    pub total_estimated_credits: i64,
    pub total_recorded_api_calls: i64,
    pub total_recorded_kb_hits: i64,
    pub has_any_record: bool,
    pub last_recorded_at: Option<String>,
    /// Time this local SQLite view was read; this is not a vendor refresh time.
    pub refreshed_at: String,
}

pub async fn query_yuandian_local_usage(
    pool: &SqlitePool,
) -> Result<YuandianLocalUsageOverview, UsageDashboardError> {
    let now = Local::now();
    let year_month = format!("{:04}-{:02}", now.year(), now.month());
    let overview = credits::get_overview(pool, &year_month)
        .await
        .map_err(|_| UsageDashboardError::yuandian_query())?;
    let last_recorded_at: Option<String> =
        sqlx::query_scalar("SELECT MAX(updated_at) FROM yuandian_credits_monthly")
            .fetch_one(pool)
            .await
            .map_err(|_| UsageDashboardError::yuandian_query())?;
    let has_any_record = last_recorded_at.is_some();

    Ok(YuandianLocalUsageOverview {
        data_source: YUANDIAN_SOURCE.to_string(),
        is_official_balance: false,
        official_balance: None,
        estimate_basis: "应用内已记录的成功元典调用按端点估算积分；本地知识库命中单独计数"
            .to_string(),
        current: overview.current,
        previous_recorded_month: overview.prev_month,
        total_estimated_credits: overview.total_credits,
        total_recorded_api_calls: overview.total_api_calls,
        total_recorded_kb_hits: overview.total_kb_hits,
        has_any_record,
        last_recorded_at,
        refreshed_at: now.to_rfc3339(),
    })
}

/// "Refresh" means re-read the local estimate table. It deliberately performs
/// no network call and never claims to refresh an official vendor balance.
#[tauri::command]
pub async fn refresh_yuandian_local_usage(
    pool: tauri::State<'_, SqlitePool>,
) -> Result<YuandianLocalUsageOverview, UsageDashboardError> {
    query_yuandian_local_usage(pool.inner()).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[tokio::test]
    async fn recognition_usage_groups_and_keeps_unknown_capabilities_null() {
        let pool = db::init_pool(":memory:").await.unwrap();
        for (backend, outcome, elapsed, error, created) in [
            ("paddle-vl", "ok", 100, None, "2026-07-28 10:00:00"),
            (
                "paddle-vl",
                "failed",
                300,
                Some("HTTP 429 rate limited"),
                "2026-07-28 11:00:00",
            ),
            ("deepseek:model-a", "ok", 200, None, "2026-07-28 12:00:00"),
            ("paddle-vl", "ok", 500, None, "2026-07-27 10:00:00"),
        ] {
            sqlx::query(
                "INSERT INTO extraction_metrics \
                 (filename,ext,file_size_bytes,stage,backend,outcome,elapsed_ms,error_short,created_at) \
                 VALUES ('safe.pdf','pdf',1,'ocr',?1,?2,?3,?4,?5)",
            )
            .bind(backend)
            .bind(outcome)
            .bind(elapsed)
            .bind(error)
            .bind(created)
            .execute(&pool)
            .await
            .unwrap();
        }
        let overview = query_recognition_usage(
            &pool,
            &RecognitionUsageQuery {
                granularity: "day".into(),
                from: Some("2026-07-28".into()),
                to: Some("2026-07-28".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(overview.data_source, RECOGNITION_SOURCE);
        assert!(!overview.is_vendor_reported);
        assert_eq!(overview.buckets.len(), 2);
        let paddle = overview
            .buckets
            .iter()
            .find(|row| row.provider_model == "paddle-vl")
            .unwrap();
        assert_eq!(paddle.task_count, 2);
        assert_eq!(paddle.success_count, 1);
        assert_eq!(paddle.failure_count, 1);
        assert_eq!(paddle.rate_limit_429_count, 1);
        assert_eq!(paddle.average_elapsed_ms, Some(200.0));
        assert_eq!(paddle.fallback_count, None);
        assert_eq!(paddle.page_count, None);
        assert!(!overview.capabilities.fallback_count_available);
        assert!(!overview.capabilities.page_count_available);

        let monthly = query_recognition_usage(
            &pool,
            &RecognitionUsageQuery {
                granularity: "month".into(),
                from: Some("2026-07".into()),
                to: Some("2026-07".into()),
            },
        )
        .await
        .unwrap();
        let monthly_paddle = monthly
            .buckets
            .iter()
            .find(|row| row.provider_model == "paddle-vl")
            .unwrap();
        assert_eq!(monthly_paddle.bucket, "2026-07");
        assert_eq!(monthly_paddle.task_count, 3);
    }

    #[tokio::test]
    async fn invalid_ranges_return_stable_error_without_querying() {
        let pool = db::init_pool(":memory:").await.unwrap();
        let error = query_recognition_usage(
            &pool,
            &RecognitionUsageQuery {
                granularity: "day".into(),
                from: Some("2026-07-30".into()),
                to: Some("2026-07-01".into()),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "LOCAL_USAGE_INVALID_RANGE");
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn yuandian_empty_and_recorded_views_never_claim_official_balance() {
        let pool = db::init_pool(":memory:").await.unwrap();
        let empty = query_yuandian_local_usage(&pool).await.unwrap();
        assert_eq!(empty.data_source, YUANDIAN_SOURCE);
        assert!(!empty.is_official_balance);
        assert_eq!(empty.official_balance, None);
        assert!(!empty.has_any_record);
        assert_eq!(empty.last_recorded_at, None);

        sqlx::query(
            "INSERT INTO yuandian_credits_monthly \
             (year_month,credits_used,api_calls,kb_hits,updated_at) \
             VALUES (?1,12,3,2,'2026-07-28T10:00:00+08:00')",
        )
        .bind(credits::current_year_month())
        .execute(&pool)
        .await
        .unwrap();
        let recorded = query_yuandian_local_usage(&pool).await.unwrap();
        assert_eq!(recorded.total_estimated_credits, 12);
        assert_eq!(recorded.total_recorded_api_calls, 3);
        assert_eq!(recorded.total_recorded_kb_hits, 2);
        assert!(recorded.has_any_record);
        assert_eq!(
            recorded.last_recorded_at.as_deref(),
            Some("2026-07-28T10:00:00+08:00")
        );
        assert!(!recorded.is_official_balance);
        assert_eq!(recorded.official_balance, None);
    }

    #[tokio::test]
    async fn missing_tables_return_stable_non_sensitive_error_codes() {
        let recognition_pool = db::init_pool(":memory:").await.unwrap();
        sqlx::query("DROP TABLE extraction_metrics")
            .execute(&recognition_pool)
            .await
            .unwrap();
        let recognition_error = query_recognition_usage(
            &recognition_pool,
            &RecognitionUsageQuery {
                granularity: "day".into(),
                from: None,
                to: None,
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            recognition_error.code,
            "LOCAL_RECOGNITION_USAGE_QUERY_FAILED"
        );
        assert!(!recognition_error.message.contains("extraction_metrics"));

        let yuandian_pool = db::init_pool(":memory:").await.unwrap();
        sqlx::query("DROP TABLE yuandian_credits_monthly")
            .execute(&yuandian_pool)
            .await
            .unwrap();
        let yuandian_error = query_yuandian_local_usage(&yuandian_pool)
            .await
            .unwrap_err();
        assert_eq!(yuandian_error.code, "YUANDIAN_LOCAL_USAGE_QUERY_FAILED");
        assert!(!yuandian_error.message.contains("yuandian_credits_monthly"));
    }
}
