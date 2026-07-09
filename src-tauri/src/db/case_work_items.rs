//! 案件工作台账(2026-07-03 · case_work_items 表)
//!
//! 本地工作台账只服务案件详情页后续 UI/导入上游,本轮不接首页、团队、聊天、AI、MCP。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

const DEFAULT_WORK_TYPE: &str = "other";
const DEFAULT_SOURCE: &str = "manual";

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
    let sql =
        format!("{RECORD_SELECT} {FILTER_SQL} ORDER BY occurred_at DESC, updated_at DESC");
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
            external_updated_at, raw_payload_json
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
    let work_type =
        normalize_opt(input.work_type).unwrap_or_else(|| DEFAULT_WORK_TYPE.to_string());
    let source = normalize_opt(input.source).unwrap_or_else(|| DEFAULT_SOURCE.to_string());
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
