//! 案件工作台账(2026-07-03 · case_work_items 表)
//!
//! 本地工作台账只服务案件详情页后续 UI/导入上游,本轮不接首页、团队、聊天、AI、MCP。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const DEFAULT_WORK_TYPE: &str = "other";
const DEFAULT_SOURCE: &str = "manual";
const DEFAULT_CONFIRMATION_STATUS: &str = "confirmed";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseWorkItem {
    pub id: String,
    pub case_id: Option<String>,
    pub occurred_at: String,
    pub work_type: String,
    pub title: String,
    pub content: String,
    pub result: Option<String>,
    pub next_action: Option<String>,
    pub duration_minutes: Option<i64>,
    pub source: String,
    pub external_source: Option<String>,
    pub external_record_id: Option<String>,
    pub external_updated_at: Option<String>,
    pub raw_payload_json: Option<String>,
    pub confirmation_status: String,
    pub source_document_id: Option<String>,
    pub source_filename: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaseWorkItemFilter {
    pub case_id: Option<String>,
    pub occurred_from: Option<String>,
    pub occurred_to: Option<String>,
    pub work_type: Option<String>,
    pub source: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCaseWorkItemInput {
    pub id: Option<String>,
    pub case_id: Option<String>,
    pub occurred_at: String,
    pub work_type: Option<String>,
    pub title: String,
    pub content: String,
    pub result: Option<String>,
    pub next_action: Option<String>,
    pub duration_minutes: Option<i64>,
    pub source: Option<String>,
    pub external_source: Option<String>,
    pub external_record_id: Option<String>,
    pub external_updated_at: Option<String>,
    pub raw_payload_json: Option<String>,
    pub confirmation_status: Option<String>,
    pub source_document_id: Option<String>,
    pub source_filename: Option<String>,
}

struct ComputedInput {
    id: String,
    case_id: Option<String>,
    occurred_at: String,
    work_type: String,
    title: String,
    content: String,
    result: Option<String>,
    next_action: Option<String>,
    duration_minutes: Option<i64>,
    source: String,
    external_source: Option<String>,
    external_record_id: Option<String>,
    external_updated_at: Option<String>,
    raw_payload_json: Option<String>,
    confirmation_status: String,
    source_document_id: Option<String>,
    source_filename: Option<String>,
}

struct PreparedFilter {
    case_id: Option<String>,
    occurred_from: Option<String>,
    occurred_to: Option<String>,
    work_type: Option<String>,
    source: Option<String>,
    query_term: Option<String>,
    query_like: Option<String>,
}

const RECORD_SELECT: &str = r#"
SELECT
    id,
    case_id,
    occurred_at,
    work_type,
    title,
    content,
    result,
    next_action,
    duration_minutes,
    source,
    external_source,
    external_record_id,
    external_updated_at,
    raw_payload_json,
    confirmation_status,
    source_document_id,
    source_filename,
    created_at,
    updated_at,
    deleted_at
FROM case_work_items
"#;

const FILTER_SQL: &str = r#"
WHERE
    deleted_at IS NULL
    AND (?1 IS NULL OR case_id = ?1)
    AND (?2 IS NULL OR occurred_at >= ?2)
    AND (?3 IS NULL OR occurred_at <= ?3)
    AND (?4 IS NULL OR work_type = ?4)
    AND (?5 IS NULL OR source = ?5)
    AND (
        ?6 IS NULL
        OR title LIKE ?7
        OR content LIKE ?7
        OR COALESCE(result, '') LIKE ?7
        OR COALESCE(next_action, '') LIKE ?7
    )
"#;

pub async fn list(
    pool: &SqlitePool,
    filter: CaseWorkItemFilter,
) -> Result<Vec<CaseWorkItem>, String> {
    let prepared = prepare_filter(filter);
    let sql = format!("{RECORD_SELECT} {FILTER_SQL} ORDER BY occurred_at DESC, updated_at DESC");
    sqlx::query_as::<_, CaseWorkItem>(&sql)
        .bind(prepared.case_id)
        .bind(prepared.occurred_from)
        .bind(prepared.occurred_to)
        .bind(prepared.work_type)
        .bind(prepared.source)
        .bind(prepared.query_term)
        .bind(prepared.query_like)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<CaseWorkItem>, String> {
    let sql = format!("{RECORD_SELECT} WHERE id = ? AND deleted_at IS NULL");
    sqlx::query_as::<_, CaseWorkItem>(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn upsert(
    pool: &SqlitePool,
    input: UpsertCaseWorkItemInput,
) -> Result<CaseWorkItem, String> {
    let computed = compute_input(input)?;
    sqlx::query(
        "INSERT INTO case_work_items (
            id, case_id, occurred_at, work_type, title, content, result, next_action,
            duration_minutes, source, external_source, external_record_id,
            external_updated_at, raw_payload_json, confirmation_status,
            source_document_id, source_filename
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            case_id = excluded.case_id,
            occurred_at = excluded.occurred_at,
            work_type = excluded.work_type,
            title = excluded.title,
            content = excluded.content,
            result = excluded.result,
            next_action = excluded.next_action,
            duration_minutes = excluded.duration_minutes,
            source = excluded.source,
            external_source = excluded.external_source,
            external_record_id = excluded.external_record_id,
            external_updated_at = excluded.external_updated_at,
            raw_payload_json = excluded.raw_payload_json,
            confirmation_status = excluded.confirmation_status,
            source_document_id = excluded.source_document_id,
            source_filename = excluded.source_filename,
            deleted_at = NULL,
            updated_at = datetime('now')",
    )
    .bind(&computed.id)
    .bind(&computed.case_id)
    .bind(&computed.occurred_at)
    .bind(&computed.work_type)
    .bind(&computed.title)
    .bind(&computed.content)
    .bind(&computed.result)
    .bind(&computed.next_action)
    .bind(computed.duration_minutes)
    .bind(&computed.source)
    .bind(&computed.external_source)
    .bind(&computed.external_record_id)
    .bind(&computed.external_updated_at)
    .bind(&computed.raw_payload_json)
    .bind(&computed.confirmation_status)
    .bind(&computed.source_document_id)
    .bind(&computed.source_filename)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;

    get(pool, &computed.id)
        .await?
        .ok_or_else(|| "工作台账写入后读取失败".to_string())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, String> {
    let result = sqlx::query(
        "UPDATE case_work_items
         SET deleted_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(result.rows_affected())
}

/// Persist only whitelisted document-extract candidates.  The fingerprint is kept in the
/// existing external_record_id column, so a rescan updates the same pending candidate.
#[allow(clippy::too_many_arguments)]
pub async fn upsert_document_candidate(
    pool: &SqlitePool,
    case_id: &str,
    document_id: &str,
    filename: &str,
    occurred_at: &str,
    work_type: Option<String>,
    content: String,
    duration_minutes: Option<i64>,
) -> Result<CaseWorkItem, String> {
    if !crate::ingest::reliability::is_work_record_filename(filename, None) {
        return Err("非工作日志/会见/阅卷/庭审/沟通材料，不生成工作记录候选".to_string());
    }
    let fingerprint = format!(
        "document_extract:{}:{:x}",
        document_id,
        stable_hash(&content)
    );
    if let Some(id) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM case_work_items WHERE case_id = ? AND external_record_id = ? AND deleted_at IS NULL",
    ).bind(case_id).bind(&fingerprint).fetch_optional(pool).await.map_err(|e| e.to_string())? {
        return upsert(pool, UpsertCaseWorkItemInput { id: Some(id), case_id: Some(case_id.into()), occurred_at: occurred_at.into(), work_type, title: format!("材料候选：{}", filename), content, result: None, next_action: None, duration_minutes, source: Some("document_extract".into()), external_source: None, external_record_id: Some(fingerprint), external_updated_at: None, raw_payload_json: None, confirmation_status: Some("pending".into()), source_document_id: Some(document_id.into()), source_filename: Some(filename.into()) }).await;
    }
    upsert(
        pool,
        UpsertCaseWorkItemInput {
            id: None,
            case_id: Some(case_id.into()),
            occurred_at: occurred_at.into(),
            work_type,
            title: format!("材料候选：{}", filename),
            content,
            result: None,
            next_action: None,
            duration_minutes,
            source: Some("document_extract".into()),
            external_source: None,
            external_record_id: Some(fingerprint),
            external_updated_at: None,
            raw_payload_json: None,
            confirmation_status: Some("pending".into()),
            source_document_id: Some(document_id.into()),
            source_filename: Some(filename.into()),
        },
    )
    .await
}

fn stable_hash(value: &str) -> u64 {
    value
        .bytes()
        .fold(5381u64, |h, b| h.wrapping_mul(33) ^ b as u64)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseWorkDurationSummary {
    pub confirmed_minutes: i64,
}

pub async fn confirmed_minutes(pool: &SqlitePool, case_id: &str) -> Result<i64, String> {
    sqlx::query_scalar(
        "SELECT COALESCE(SUM(duration_minutes), 0)
         FROM case_work_items
         WHERE case_id = ? AND deleted_at IS NULL AND confirmation_status = ?",
    )
    .bind(case_id)
    .bind("confirmed")
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn summarize_confirmed_duration(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<CaseWorkDurationSummary, String> {
    Ok(CaseWorkDurationSummary {
        confirmed_minutes: confirmed_minutes(pool, case_id).await?,
    })
}

fn prepare_filter(filter: CaseWorkItemFilter) -> PreparedFilter {
    let case_id = normalize_opt(filter.case_id);
    let occurred_from = normalize_opt(filter.occurred_from);
    let occurred_to = normalize_opt(filter.occurred_to);
    let work_type = normalize_opt(filter.work_type);
    let source = normalize_opt(filter.source);
    let query_term = normalize_opt(filter.query);
    let query_like = query_term.as_ref().map(|s| format!("%{s}%"));
    PreparedFilter {
        case_id,
        occurred_from,
        occurred_to,
        work_type,
        source,
        query_term,
        query_like,
    }
}

fn compute_input(input: UpsertCaseWorkItemInput) -> Result<ComputedInput, String> {
    let id = input.id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let occurred_at = required_text(input.occurred_at, "occurred_at")?;
    let title = required_text(input.title, "title")?;
    let content = required_text(input.content, "content")?;
    let work_type = normalize_opt(input.work_type).unwrap_or_else(|| DEFAULT_WORK_TYPE.to_string());
    let source = normalize_opt(input.source).unwrap_or_else(|| DEFAULT_SOURCE.to_string());
    let confirmation_status = normalize_opt(input.confirmation_status)
        .unwrap_or_else(|| DEFAULT_CONFIRMATION_STATUS.to_string());
    if !matches!(confirmation_status.as_str(), "pending" | "confirmed") {
        return Err("confirmation_status 必须为 pending 或 confirmed".to_string());
    }
    let duration_minutes = match input.duration_minutes {
        Some(v) if v < 0 => return Err("duration_minutes 不能为负数".to_string()),
        other => other,
    };

    Ok(ComputedInput {
        id,
        case_id: normalize_opt(input.case_id),
        occurred_at,
        work_type,
        title,
        content,
        result: normalize_opt(input.result),
        next_action: normalize_opt(input.next_action),
        duration_minutes,
        source,
        external_source: normalize_opt(input.external_source),
        external_record_id: normalize_opt(input.external_record_id),
        external_updated_at: normalize_opt(input.external_updated_at),
        raw_payload_json: normalize_opt(input.raw_payload_json),
        confirmation_status,
        source_document_id: normalize_opt(input.source_document_id),
        source_filename: normalize_opt(input.source_filename),
    })
}

fn normalize_opt(value: Option<String>) -> Option<String> {
    value.and_then(|v| {
        let trimmed = v.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn required_text(value: String, field: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(format!("{field} 不能为空"))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(status: &str, duration_minutes: i64) -> UpsertCaseWorkItemInput {
        UpsertCaseWorkItemInput {
            occurred_at: "2026-07-13T10:00".to_string(),
            title: "会见".to_string(),
            content: "会见并记录意见".to_string(),
            duration_minutes: Some(duration_minutes),
            confirmation_status: Some(status.to_string()),
            source: Some("document_extract".to_string()),
            source_document_id: Some("doc-opt-n2".to_string()),
            source_filename: Some("会见笔录.pdf".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn validates_status_duration_and_source_fields() {
        let computed = compute_input(input("pending", 35)).expect("pending item should be valid");
        assert_eq!(computed.confirmation_status, "pending");
        assert_eq!(computed.source_document_id.as_deref(), Some("doc-opt-n2"));
        assert_eq!(computed.source_filename.as_deref(), Some("会见笔录.pdf"));
        assert!(compute_input(input("unverified", 10)).is_err());
        assert!(compute_input(input("confirmed", -1)).is_err());
    }

    #[tokio::test]
    async fn pending_and_deleted_items_do_not_count_toward_duration() {
        let pool = SqlitePool::connect(":memory:").await.expect("open sqlite");
        sqlx::query(
            "CREATE TABLE case_work_items (
                case_id TEXT,
                duration_minutes INTEGER,
                confirmation_status TEXT,
                deleted_at TEXT
            )",
        )
        .execute(&pool)
        .await
        .expect("create test table");
        sqlx::query(
            "INSERT INTO case_work_items VALUES
             ('case-1', 85, 'confirmed', NULL),
             ('case-1', 35, 'pending', NULL),
             ('case-1', 20, 'confirmed', '2026-07-13')",
        )
        .execute(&pool)
        .await
        .expect("insert test rows");

        assert_eq!(confirmed_minutes(&pool, "case-1").await.unwrap(), 85);
    }

    #[tokio::test]
    async fn document_candidate_is_idempotent_by_document_and_content() {
        let pool = crate::db::init_pool(":memory:")
            .await
            .expect("migrate database");
        let case = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "测试刑事案件".into(),
                case_type: "criminal".into(),
                source_folder: "D:/test/opt-n3b".into(),
            },
        )
        .await
        .expect("create case");

        let first = upsert_document_candidate(
            &pool,
            &case.id,
            "doc-1",
            "会见笔录.pdf",
            "2026-07-13",
            Some("investigation".into()),
            "会见犯罪嫌疑人并记录意见".into(),
            Some(60),
        )
        .await
        .expect("first candidate");
        let second = upsert_document_candidate(
            &pool,
            &case.id,
            "doc-1",
            "会见笔录.pdf",
            "2026-07-13",
            Some("investigation".into()),
            "会见犯罪嫌疑人并记录意见".into(),
            Some(60),
        )
        .await
        .expect("repeat candidate");

        assert_eq!(first.id, second.id);
        assert_eq!(second.confirmation_status, "pending");
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM case_work_items WHERE case_id = ?")
                .bind(&case.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 1);
    }
}
