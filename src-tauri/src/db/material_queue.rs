//! Persistent material selection and extraction queue.
//!
//! The queue deliberately does not store document text, provider responses or
//! credentials. Callers may persist only a bounded, redacted error summary.

use regex::Regex;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

const QUEUED: &str = "queued";
const RUNNING: &str = "running";
const PAUSED: &str = "paused";
const CANCELLED: &str = "cancelled";
const COMPLETED: &str = "completed";
const FAILED: &str = "failed";
const RECOVERY_REQUIRED: &str = "recovery_required";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSourceDecision {
    pub case_id: String,
    pub source_path: String,
    pub disposition: String,
    pub document_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialDecisionInput {
    pub source_path: String,
    pub disposition: String,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaterialProcessingBatch {
    pub id: String,
    pub case_id: String,
    pub status: String,
    pub error_category: Option<String>,
    pub error_summary: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaterialProcessingItem {
    pub id: String,
    pub batch_id: String,
    pub case_id: String,
    pub source_path: String,
    pub document_id: Option<String>,
    pub ordinal: i64,
    pub status: String,
    pub claim_token: Option<String>,
    pub claimed_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_category: Option<String>,
    pub error_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialQueueItemInput {
    pub source_path: String,
    pub document_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MaterialProcessingEvent {
    pub id: String,
    pub batch_id: String,
    pub item_id: Option<String>,
    pub event_type: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub actor: String,
    pub error_category: Option<String>,
    pub error_summary: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialBatchDetail {
    pub batch: MaterialProcessingBatch,
    pub items: Vec<MaterialProcessingItem>,
    pub events: Vec<MaterialProcessingEvent>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResult {
    pub batches: u64,
    pub items: u64,
}

fn validate_disposition(value: &str) -> Result<(), String> {
    match value {
        "recognize" | "index_only" | "excluded" => Ok(()),
        _ => Err(format!("不支持的材料纳入状态: {value}")),
    }
}

fn validate_error_category(value: Option<&str>) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 64
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err("错误类别只能使用不超过64位的字母、数字、点、横线或下划线".to_string());
    }
    Ok(Some(value.to_string()))
}

/// Remove common credential and URL-query shapes, flatten whitespace and cap
/// the result. This function must receive an operator-facing summary, never a
/// provider response body or extracted document text.
pub fn sanitize_error_summary(value: Option<&str>) -> Option<String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty())?;
    let bearer = Regex::new(r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+").expect("valid regex");
    let credential =
        Regex::new(r"(?i)\b(api[_-]?key|token|secret|authorization|password)\s*[:=]\s*[^\s,;]+")
            .expect("valid regex");
    let query = Regex::new(r"(https?://[^\s?]+)\?[^\s]+").expect("valid regex");
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let redacted = bearer.replace_all(&compact, "Bearer [REDACTED]");
    let redacted = credential.replace_all(&redacted, "$1=[REDACTED]");
    let redacted = query.replace_all(&redacted, "$1?[REDACTED]");
    let mut chars = redacted.chars();
    let mut bounded: String = chars.by_ref().take(500).collect();
    if chars.next().is_some() {
        bounded.push('…');
    }
    Some(bounded)
}

// Keeping every audit column explicit at call sites makes omissions visible in
// review and prevents a generic metadata object from becoming a channel for
// document text or credentials.
#[allow(clippy::too_many_arguments)]
async fn insert_event(
    tx: &mut Transaction<'_, Sqlite>,
    batch_id: &str,
    item_id: Option<&str>,
    event_type: &str,
    from_status: Option<&str>,
    to_status: Option<&str>,
    actor: &str,
    error_category: Option<&str>,
    error_summary: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO material_processing_events \
         (id,batch_id,item_id,event_type,from_status,to_status,actor,error_category,error_summary) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(batch_id)
    .bind(item_id)
    .bind(event_type)
    .bind(from_status)
    .bind(to_status)
    .bind(actor)
    .bind(error_category)
    .bind(error_summary)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[tauri::command]
pub async fn list_material_source_decisions(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<Vec<MaterialSourceDecision>, String> {
    list_decisions(pool.inner(), &case_id).await
}

pub async fn list_decisions(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<MaterialSourceDecision>, String> {
    sqlx::query_as(
        "SELECT case_id,source_path,disposition,document_id,created_at,updated_at \
         FROM material_source_decisions WHERE case_id=?1 ORDER BY source_path",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_material_source_decisions(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    decisions: Vec<MaterialDecisionInput>,
) -> Result<Vec<MaterialSourceDecision>, String> {
    save_decisions(pool.inner(), &case_id, &decisions).await
}

pub async fn save_decisions(
    pool: &SqlitePool,
    case_id: &str,
    decisions: &[MaterialDecisionInput],
) -> Result<Vec<MaterialSourceDecision>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    for decision in decisions {
        if decision.source_path.trim().is_empty() {
            return Err("材料路径不能为空".to_string());
        }
        validate_disposition(&decision.disposition)?;
        sqlx::query(
            "INSERT INTO material_source_decisions \
             (case_id,source_path,disposition,document_id) VALUES (?1,?2,?3,?4) \
             ON CONFLICT(case_id,source_path) DO UPDATE SET \
             disposition=excluded.disposition, document_id=excluded.document_id, \
             updated_at=datetime('now')",
        )
        .bind(case_id)
        .bind(&decision.source_path)
        .bind(&decision.disposition)
        .bind(&decision.document_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    sqlx::query_as(
        "SELECT case_id,source_path,disposition,document_id,created_at,updated_at \
         FROM material_source_decisions WHERE case_id=?1 ORDER BY source_path",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn create_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    items: Vec<MaterialQueueItemInput>,
) -> Result<MaterialBatchDetail, String> {
    create_batch(pool.inner(), &case_id, &items).await
}

pub async fn create_batch(
    pool: &SqlitePool,
    case_id: &str,
    items: &[MaterialQueueItemInput],
) -> Result<MaterialBatchDetail, String> {
    if items.is_empty() {
        return Err("识别批次至少需要一份材料".to_string());
    }
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let batch_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO material_processing_batches(id,case_id,status) VALUES (?1,?2,'queued')",
    )
    .bind(&batch_id)
    .bind(case_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    for (ordinal, item) in items.iter().enumerate() {
        if item.source_path.trim().is_empty() {
            return Err("材料路径不能为空".to_string());
        }
        let disposition: Option<String> = sqlx::query_scalar(
            "SELECT disposition FROM material_source_decisions \
             WHERE case_id=?1 AND source_path=?2",
        )
        .bind(case_id)
        .bind(&item.source_path)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if disposition.as_deref() != Some("recognize") {
            return Err(format!(
                "只有 recognize 材料可以排入识别队列: {}",
                item.source_path
            ));
        }
        let item_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO material_processing_items \
             (id,batch_id,case_id,source_path,document_id,ordinal,status) \
             VALUES (?1,?2,?3,?4,?5,?6,'queued')",
        )
        .bind(&item_id)
        .bind(&batch_id)
        .bind(case_id)
        .bind(&item.source_path)
        .bind(&item.document_id)
        .bind(ordinal as i64)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        insert_event(
            &mut tx,
            &batch_id,
            Some(&item_id),
            "item_created",
            None,
            Some(QUEUED),
            "user",
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    insert_event(
        &mut tx,
        &batch_id,
        None,
        "batch_created",
        None,
        Some(QUEUED),
        "user",
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, &batch_id).await
}

#[tauri::command]
pub async fn get_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    get_batch_detail(pool.inner(), &batch_id).await
}

#[tauri::command]
pub async fn list_material_processing_batches(
    pool: tauri::State<'_, SqlitePool>,
    case_id: Option<String>,
    status: Option<String>,
) -> Result<Vec<MaterialProcessingBatch>, String> {
    if let Some(status) = status.as_deref() {
        if !matches!(
            status,
            QUEUED | RUNNING | PAUSED | CANCELLED | COMPLETED | FAILED | RECOVERY_REQUIRED
        ) {
            return Err(format!("不支持的批次状态: {status}"));
        }
    }
    sqlx::query_as(
        "SELECT id,case_id,status,error_category,error_summary,created_at,started_at,\
         finished_at,updated_at FROM material_processing_batches \
         WHERE (?1 IS NULL OR case_id=?1) AND (?2 IS NULL OR status=?2) \
         ORDER BY created_at DESC,id DESC",
    )
    .bind(case_id)
    .bind(status)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| e.to_string())
}

pub async fn get_batch_detail(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<MaterialBatchDetail, String> {
    let batch = sqlx::query_as(
        "SELECT id,case_id,status,error_category,error_summary,created_at,started_at,\
         finished_at,updated_at FROM material_processing_batches WHERE id=?1",
    )
    .bind(batch_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "识别批次不存在".to_string())?;
    let items = sqlx::query_as(
        "SELECT id,batch_id,case_id,source_path,document_id,ordinal,status,claim_token,\
         claimed_at,completed_at,error_category,error_summary,created_at,updated_at \
         FROM material_processing_items WHERE batch_id=?1 ORDER BY ordinal,id",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let events = sqlx::query_as(
        "SELECT id,batch_id,item_id,event_type,from_status,to_status,actor,error_category,\
         error_summary,created_at FROM material_processing_events \
         WHERE batch_id=?1 ORDER BY created_at,id",
    )
    .bind(batch_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(MaterialBatchDetail {
        batch,
        items,
        events,
    })
}

async fn transition_batch(
    pool: &SqlitePool,
    batch_id: &str,
    allowed_from: &[&str],
    to_status: &str,
    event_type: &str,
) -> Result<MaterialBatchDetail, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let from: String =
        sqlx::query_scalar("SELECT status FROM material_processing_batches WHERE id=?1")
            .bind(batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "识别批次不存在".to_string())?;
    if !allowed_from.contains(&from.as_str()) {
        return Err(format!("批次状态不能从 {from} 转为 {to_status}"));
    }
    sqlx::query(
        "UPDATE material_processing_batches SET status=?1, \
         started_at=CASE WHEN ?1='running' THEN COALESCE(started_at,datetime('now')) \
                         ELSE started_at END, \
         finished_at=CASE WHEN ?1 IN ('cancelled','completed','failed') THEN datetime('now') \
                          ELSE NULL END, updated_at=datetime('now') WHERE id=?2",
    )
    .bind(to_status)
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    insert_event(
        &mut tx,
        batch_id,
        None,
        event_type,
        Some(&from),
        Some(to_status),
        "user",
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, batch_id).await
}

#[tauri::command]
pub async fn start_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    start_batch(pool.inner(), &batch_id).await
}

pub async fn start_batch(pool: &SqlitePool, batch_id: &str) -> Result<MaterialBatchDetail, String> {
    transition_batch(pool, batch_id, &[QUEUED], RUNNING, "batch_started").await
}

#[tauri::command]
pub async fn pause_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    pause_batch(pool.inner(), &batch_id).await
}

pub async fn pause_batch(pool: &SqlitePool, batch_id: &str) -> Result<MaterialBatchDetail, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let from: String =
        sqlx::query_scalar("SELECT status FROM material_processing_batches WHERE id=?1")
            .bind(batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "识别批次不存在".to_string())?;
    if !matches!(from.as_str(), QUEUED | RUNNING) {
        return Err(format!("批次状态不能从 {from} 转为 paused"));
    }
    let affected: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,status FROM material_processing_items WHERE batch_id=?1 \
         AND status IN ('queued','running') ORDER BY ordinal,id",
    )
    .bind(batch_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE material_processing_batches SET status='paused',updated_at=datetime('now') \
         WHERE id=?1",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE material_processing_items SET status='paused',claim_token=NULL,\
         claimed_at=NULL,updated_at=datetime('now') WHERE batch_id=?1 \
         AND status IN ('queued','running')",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    for (item_id, item_from) in affected {
        insert_event(
            &mut tx,
            batch_id,
            Some(&item_id),
            "item_paused",
            Some(&item_from),
            Some(PAUSED),
            "user",
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    insert_event(
        &mut tx,
        batch_id,
        None,
        "batch_paused",
        Some(&from),
        Some(PAUSED),
        "user",
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, batch_id).await
}

#[tauri::command]
pub async fn resume_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    resume_batch(pool.inner(), &batch_id).await
}

pub async fn resume_batch(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<MaterialBatchDetail, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let from: String =
        sqlx::query_scalar("SELECT status FROM material_processing_batches WHERE id=?1")
            .bind(batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "识别批次不存在".to_string())?;
    if !matches!(from.as_str(), PAUSED | RECOVERY_REQUIRED) {
        return Err(format!("批次状态不能从 {from} 恢复为 queued"));
    }

    if from == RECOVERY_REQUIRED {
        let recovered_items: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM material_processing_items \
             WHERE batch_id=?1 AND status='recovery_required' ORDER BY ordinal,id",
        )
        .bind(batch_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE material_processing_items SET status='queued',claim_token=NULL,\
             claimed_at=NULL,updated_at=datetime('now') \
             WHERE batch_id=?1 AND status='recovery_required'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for item_id in recovered_items {
            insert_event(
                &mut tx,
                batch_id,
                Some(&item_id),
                "item_requeued_by_user",
                Some(RECOVERY_REQUIRED),
                Some(QUEUED),
                "user",
                None,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    } else {
        let paused_items: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM material_processing_items \
             WHERE batch_id=?1 AND status='paused' ORDER BY ordinal,id",
        )
        .bind(batch_id)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        sqlx::query(
            "UPDATE material_processing_items SET status='queued',claim_token=NULL,\
             claimed_at=NULL,updated_at=datetime('now') \
             WHERE batch_id=?1 AND status='paused'",
        )
        .bind(batch_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for item_id in paused_items {
            insert_event(
                &mut tx,
                batch_id,
                Some(&item_id),
                "item_requeued_by_user",
                Some(PAUSED),
                Some(QUEUED),
                "user",
                None,
                None,
            )
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    sqlx::query(
        "UPDATE material_processing_batches SET status='queued',finished_at=NULL,\
         updated_at=datetime('now') WHERE id=?1",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    insert_event(
        &mut tx,
        batch_id,
        None,
        "batch_resumed_to_queue",
        Some(&from),
        Some(QUEUED),
        "user",
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, batch_id).await
}

#[tauri::command]
pub async fn cancel_material_processing_batch(
    pool: tauri::State<'_, SqlitePool>,
    batch_id: String,
) -> Result<MaterialBatchDetail, String> {
    cancel_batch(pool.inner(), &batch_id).await
}

pub async fn cancel_batch(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<MaterialBatchDetail, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let from: String =
        sqlx::query_scalar("SELECT status FROM material_processing_batches WHERE id=?1")
            .bind(batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "识别批次不存在".to_string())?;
    if matches!(from.as_str(), CANCELLED | COMPLETED | FAILED) {
        return Err(format!("终态批次不能取消: {from}"));
    }
    let affected: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,status FROM material_processing_items WHERE batch_id=?1 \
         AND status NOT IN ('completed','failed','cancelled')",
    )
    .bind(batch_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    sqlx::query(
        "UPDATE material_processing_items SET status='cancelled', claim_token=NULL, \
         updated_at=datetime('now') WHERE batch_id=?1 \
         AND status NOT IN ('completed','failed','cancelled')",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    for (item_id, item_from) in affected {
        insert_event(
            &mut tx,
            batch_id,
            Some(&item_id),
            "item_cancelled",
            Some(&item_from),
            Some(CANCELLED),
            "user",
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    sqlx::query(
        "UPDATE material_processing_batches SET status='cancelled', \
         finished_at=datetime('now'),updated_at=datetime('now') WHERE id=?1",
    )
    .bind(batch_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    insert_event(
        &mut tx,
        batch_id,
        None,
        "batch_cancelled",
        Some(&from),
        Some(CANCELLED),
        "user",
        None,
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, batch_id).await
}

/// Atomically claim the first queued item. The UPDATE predicate repeats the
/// batch and item gates, so concurrent workers cannot claim the same item and
/// a pause/cancel committed first prevents all later claims.
pub async fn claim_next(
    pool: &SqlitePool,
    batch_id: &str,
) -> Result<Option<MaterialProcessingItem>, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let token = Uuid::new_v4().to_string();
    let claimed: Option<MaterialProcessingItem> = sqlx::query_as(
        "UPDATE material_processing_items SET status='running',claim_token=?1,\
         claimed_at=datetime('now'),updated_at=datetime('now') \
         WHERE id=(SELECT i.id FROM material_processing_items i \
                   JOIN material_processing_batches b ON b.id=i.batch_id \
                   WHERE i.batch_id=?2 AND i.status='queued' AND b.status='running' \
                   ORDER BY i.ordinal,i.id LIMIT 1) \
           AND status='queued' \
           AND EXISTS(SELECT 1 FROM material_processing_batches b \
                      WHERE b.id=?2 AND b.status='running') \
         RETURNING id,batch_id,case_id,source_path,document_id,ordinal,status,claim_token,\
         claimed_at,completed_at,error_category,error_summary,created_at,updated_at",
    )
    .bind(&token)
    .bind(batch_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(item) = &claimed {
        insert_event(
            &mut tx,
            batch_id,
            Some(&item.id),
            "item_claimed",
            Some(QUEUED),
            Some(RUNNING),
            "worker",
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(claimed)
}

/// Hard execution boundary. Workers must call this immediately before every
/// external OCR/LLM request, not merely once after claiming.
pub async fn execution_allowed(
    pool: &SqlitePool,
    item_id: &str,
    claim_token: &str,
) -> Result<bool, String> {
    sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM material_processing_items i \
         JOIN material_processing_batches b ON b.id=i.batch_id \
         WHERE i.id=?1 AND i.claim_token=?2 AND i.status='running' AND b.status='running')",
    )
    .bind(item_id)
    .bind(claim_token)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn finish_item(
    pool: &SqlitePool,
    item_id: &str,
    claim_token: &str,
) -> Result<MaterialProcessingItem, String> {
    settle_item(pool, item_id, claim_token, COMPLETED, None, None).await
}

pub async fn fail_item(
    pool: &SqlitePool,
    item_id: String,
    claim_token: String,
    error_category: Option<String>,
    error_summary: Option<String>,
) -> Result<MaterialProcessingItem, String> {
    let category = validate_error_category(error_category.as_deref())?;
    let summary = sanitize_error_summary(error_summary.as_deref());
    settle_item(
        pool,
        &item_id,
        &claim_token,
        FAILED,
        category.as_deref(),
        summary.as_deref(),
    )
    .await
}

async fn settle_item(
    pool: &SqlitePool,
    item_id: &str,
    claim_token: &str,
    to_status: &str,
    error_category: Option<&str>,
    error_summary: Option<&str>,
) -> Result<MaterialProcessingItem, String> {
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let item: MaterialProcessingItem = sqlx::query_as(
        "UPDATE material_processing_items SET status=?1,completed_at=datetime('now'),\
         error_category=?2,error_summary=?3,updated_at=datetime('now') \
         WHERE id=?4 AND claim_token=?5 AND status='running' \
           AND EXISTS(SELECT 1 FROM material_processing_batches b \
                      WHERE b.id=material_processing_items.batch_id AND b.status='running') \
         RETURNING id,batch_id,case_id,source_path,document_id,ordinal,status,claim_token,\
         claimed_at,completed_at,error_category,error_summary,created_at,updated_at",
    )
    .bind(to_status)
    .bind(error_category)
    .bind(error_summary)
    .bind(item_id)
    .bind(claim_token)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "条目已暂停、取消、失去领取权或不再运行".to_string())?;
    insert_event(
        &mut tx,
        &item.batch_id,
        Some(item_id),
        if to_status == COMPLETED {
            "item_completed"
        } else {
            "item_failed"
        },
        Some(RUNNING),
        Some(to_status),
        "worker",
        error_category,
        error_summary,
    )
    .await
    .map_err(|e| e.to_string())?;

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM material_processing_items WHERE batch_id=?1 \
         AND status NOT IN ('completed','failed','cancelled')",
    )
    .bind(&item.batch_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if remaining == 0 {
        let failed: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM material_processing_items WHERE batch_id=?1 AND status='failed'",
        )
        .bind(&item.batch_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        let batch_to = if failed > 0 { FAILED } else { COMPLETED };
        sqlx::query(
            "UPDATE material_processing_batches SET status=?1,finished_at=datetime('now'),\
             updated_at=datetime('now') WHERE id=?2 AND status='running'",
        )
        .bind(batch_to)
        .bind(&item.batch_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        insert_event(
            &mut tx,
            &item.batch_id,
            None,
            "batch_settled",
            Some(RUNNING),
            Some(batch_to),
            "system",
            None,
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(item)
}

/// 按失败类别忽略材料。决策、文档、队列条目、审计事件和批次终态在同一事务提交。
pub async fn ignore_failed_items(
    pool: &SqlitePool,
    batch_id: &str,
    error_category: Option<&str>,
) -> Result<MaterialBatchDetail, String> {
    let category = validate_error_category(error_category)?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let (case_id, _batch_status): (String, String) =
        sqlx::query_as("SELECT case_id,status FROM material_processing_batches WHERE id=?1")
            .bind(batch_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or_else(|| "识别批次不存在".to_string())?;
    let failed: Vec<MaterialProcessingItem> = sqlx::query_as(
        "SELECT id,batch_id,case_id,source_path,document_id,ordinal,status,claim_token,\
         claimed_at,completed_at,error_category,error_summary,created_at,updated_at \
         FROM material_processing_items WHERE batch_id=?1 AND status='failed' \
         AND (?2 IS NULL OR error_category=?2) ORDER BY ordinal,id",
    )
    .bind(batch_id)
    .bind(category.as_deref())
    .fetch_all(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if failed.is_empty() {
        tx.commit().await.map_err(|e| e.to_string())?;
        return get_batch_detail(pool, batch_id).await;
    }

    let mut paths = HashMap::<String, Option<String>>::new();
    for item in &failed {
        paths
            .entry(item.source_path.clone())
            .and_modify(|current| {
                if current.is_none() {
                    *current = item.document_id.clone();
                }
            })
            .or_insert_with(|| item.document_id.clone());
    }
    for (source_path, document_id) in &paths {
        sqlx::query(
            "INSERT INTO material_source_decisions \
             (case_id,source_path,disposition,document_id,created_at,updated_at) \
             VALUES (?1,?2,'excluded',?3,datetime('now'),datetime('now')) \
             ON CONFLICT(case_id,source_path) DO UPDATE SET disposition='excluded',\
             document_id=excluded.document_id,updated_at=datetime('now')",
        )
        .bind(&case_id)
        .bind(source_path)
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if let Some(document_id) = document_id {
            sqlx::query(
                "UPDATE documents SET extraction_status='skipped',\
                 last_error='用户按失败类别批量忽略' WHERE id=?1 AND case_id=?2",
            )
            .bind(document_id)
            .bind(&case_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    let failed_ids = failed
        .iter()
        .map(|item| item.id.as_str())
        .collect::<HashSet<_>>();
    let mut cancelled_ids = HashSet::<String>::new();
    let mut affected_batches = HashSet::<String>::new();
    for source_path in paths.keys() {
        let candidates: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT id,batch_id,status FROM material_processing_items \
             WHERE case_id=?1 AND source_path=?2 \
             AND status IN ('queued','running','paused','recovery_required','failed')",
        )
        .bind(&case_id)
        .bind(source_path)
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        for (item_id, affected_batch_id, from_status) in candidates {
            let selected_failed = failed_ids.contains(item_id.as_str());
            let unfinished = matches!(
                from_status.as_str(),
                QUEUED | RUNNING | PAUSED | RECOVERY_REQUIRED
            );
            if (!selected_failed && !unfinished) || !cancelled_ids.insert(item_id.clone()) {
                continue;
            }
            sqlx::query(
                "UPDATE material_processing_items SET status='cancelled',claim_token=NULL,\
                 claimed_at=NULL,completed_at=datetime('now'),updated_at=datetime('now') \
                 WHERE id=?1",
            )
            .bind(&item_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            insert_event(
                &mut tx,
                &affected_batch_id,
                Some(&item_id),
                "item_ignored",
                Some(&from_status),
                Some(CANCELLED),
                "user",
                category.as_deref(),
                Some("用户按失败类别批量忽略"),
            )
            .await
            .map_err(|e| e.to_string())?;
            affected_batches.insert(affected_batch_id);
        }
    }

    for affected_batch_id in affected_batches {
        let from: String =
            sqlx::query_scalar("SELECT status FROM material_processing_batches WHERE id=?1")
                .bind(&affected_batch_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
        let (active, failed_count, completed_count): (i64, i64, i64) = sqlx::query_as(
            "SELECT \
             SUM(CASE WHEN status IN ('queued','running','paused','recovery_required') THEN 1 ELSE 0 END),\
             SUM(CASE WHEN status='failed' THEN 1 ELSE 0 END),\
             SUM(CASE WHEN status='completed' THEN 1 ELSE 0 END) \
             FROM material_processing_items WHERE batch_id=?1",
        )
        .bind(&affected_batch_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        if active > 0 {
            continue;
        }
        let to = if failed_count > 0 {
            FAILED
        } else if completed_count > 0 {
            COMPLETED
        } else {
            CANCELLED
        };
        if from == to {
            continue;
        }
        sqlx::query(
            "UPDATE material_processing_batches SET status=?1,finished_at=datetime('now'),\
             error_category=CASE WHEN ?1='failed' THEN error_category ELSE NULL END,\
             error_summary=CASE WHEN ?1='failed' THEN error_summary ELSE NULL END,\
             updated_at=datetime('now') WHERE id=?2",
        )
        .bind(to)
        .bind(&affected_batch_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
        insert_event(
            &mut tx,
            &affected_batch_id,
            None,
            "batch_recomputed_after_ignore",
            Some(&from),
            Some(to),
            "user",
            category.as_deref(),
            None,
        )
        .await
        .map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    get_batch_detail(pool, batch_id).await
}

/// Startup recovery is a single transaction. Nothing is re-queued or claimed.
pub async fn recover_interrupted_material_processing(
    pool: &SqlitePool,
) -> Result<RecoveryResult, sqlx::Error> {
    let mut tx = pool.begin().await?;
    let batches: Vec<String> = sqlx::query_scalar(
        "SELECT id FROM material_processing_batches WHERE status='running' ORDER BY id",
    )
    .fetch_all(&mut *tx)
    .await?;
    let running_items: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,batch_id FROM material_processing_items WHERE status='running' ORDER BY id",
    )
    .fetch_all(&mut *tx)
    .await?;
    let batch_count = sqlx::query(
        "UPDATE material_processing_batches SET status='recovery_required',\
         updated_at=datetime('now') WHERE status='running'",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    let item_count = sqlx::query(
        "UPDATE material_processing_items SET status='recovery_required',claim_token=NULL,\
         updated_at=datetime('now') WHERE status='running'",
    )
    .execute(&mut *tx)
    .await?
    .rows_affected();
    for batch_id in batches {
        insert_event(
            &mut tx,
            &batch_id,
            None,
            "startup_recovery_required",
            Some(RUNNING),
            Some(RECOVERY_REQUIRED),
            "system",
            Some("interrupted"),
            Some("应用上次运行未正常收尾，需要用户确认后重新排队"),
        )
        .await?;
    }
    for (item_id, batch_id) in running_items {
        insert_event(
            &mut tx,
            &batch_id,
            Some(&item_id),
            "startup_recovery_required",
            Some(RUNNING),
            Some(RECOVERY_REQUIRED),
            "system",
            Some("interrupted"),
            Some("处理在应用退出时中断，需要用户确认后重新排队"),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(RecoveryResult {
        batches: batch_count,
        items: item_count,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn pool() -> SqlitePool {
        let pool = db::init_pool(":memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO cases(id,name,case_type,source_folder) \
             VALUES ('case-1','测试案件','诉讼','C:/cases/test')",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    async fn decisions(pool: &SqlitePool, paths: &[&str]) {
        let inputs = paths
            .iter()
            .map(|path| MaterialDecisionInput {
                source_path: (*path).to_string(),
                disposition: "recognize".to_string(),
                document_id: None,
            })
            .collect::<Vec<_>>();
        save_decisions(pool, "case-1", &inputs).await.unwrap();
    }

    async fn batch(pool: &SqlitePool, paths: &[&str]) -> MaterialBatchDetail {
        decisions(pool, paths).await;
        let items = paths
            .iter()
            .map(|path| MaterialQueueItemInput {
                source_path: (*path).to_string(),
                document_id: None,
            })
            .collect::<Vec<_>>();
        create_batch(pool, "case-1", &items).await.unwrap()
    }

    #[tokio::test]
    async fn decisions_persist_all_three_states() {
        let pool = pool().await;
        let rows = save_decisions(
            &pool,
            "case-1",
            &[
                MaterialDecisionInput {
                    source_path: "a.pdf".into(),
                    disposition: "recognize".into(),
                    document_id: None,
                },
                MaterialDecisionInput {
                    source_path: "b.pdf".into(),
                    disposition: "index_only".into(),
                    document_id: None,
                },
                MaterialDecisionInput {
                    source_path: "c.pdf".into(),
                    disposition: "excluded".into(),
                    document_id: None,
                },
            ],
        )
        .await
        .unwrap();
        assert_eq!(
            rows.iter()
                .map(|row| row.disposition.as_str())
                .collect::<Vec<_>>(),
            ["recognize", "index_only", "excluded"]
        );
    }

    #[tokio::test]
    async fn only_recognize_decisions_can_be_queued() {
        let pool = pool().await;
        save_decisions(
            &pool,
            "case-1",
            &[MaterialDecisionInput {
                source_path: "index.pdf".into(),
                disposition: "index_only".into(),
                document_id: None,
            }],
        )
        .await
        .unwrap();
        let err = create_batch(
            &pool,
            "case-1",
            &[MaterialQueueItemInput {
                source_path: "index.pdf".into(),
                document_id: None,
            }],
        )
        .await
        .unwrap_err();
        assert!(err.contains("只有 recognize"));
    }

    #[tokio::test]
    async fn pause_and_cancel_are_claim_boundaries() {
        let pool = pool().await;
        let detail = batch(&pool, &["a.pdf", "b.pdf"]).await;
        start_batch(&pool, &detail.batch.id).await.unwrap();
        let active = claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        let paused = pause_batch(&pool, &detail.batch.id).await.unwrap();
        assert!(paused.items.iter().all(|item| item.status == PAUSED));
        assert!(
            !execution_allowed(&pool, &active.id, active.claim_token.as_deref().unwrap())
                .await
                .unwrap()
        );
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_none());
        let resumed = resume_batch(&pool, &detail.batch.id).await.unwrap();
        assert!(resumed.items.iter().all(|item| item.status == QUEUED));
        // resume 只回 queued，未显式 start 不能领取。
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_none());
        start_batch(&pool, &detail.batch.id).await.unwrap();
        let first = claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        let cancelled = cancel_batch(&pool, &detail.batch.id).await.unwrap();
        assert!(cancelled.items.iter().all(|item| item.status == CANCELLED));
        assert!(
            !execution_allowed(&pool, &first.id, first.claim_token.as_deref().unwrap())
                .await
                .unwrap()
        );
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn ignore_failed_is_atomic_audited_and_recomputes_all_affected_batches() {
        let pool = pool().await;
        for (id, path) in [("doc-done", "done.pdf"), ("doc-bad", "bad.pdf")] {
            sqlx::query(
                "INSERT INTO documents(id,case_id,source_path,filename,size_bytes) \
                 VALUES (?1,'case-1',?2,?2,1)",
            )
            .bind(id)
            .bind(path)
            .execute(&pool)
            .await
            .unwrap();
        }
        save_decisions(
            &pool,
            "case-1",
            &[
                MaterialDecisionInput {
                    source_path: "done.pdf".into(),
                    disposition: "recognize".into(),
                    document_id: Some("doc-done".into()),
                },
                MaterialDecisionInput {
                    source_path: "bad.pdf".into(),
                    disposition: "recognize".into(),
                    document_id: Some("doc-bad".into()),
                },
            ],
        )
        .await
        .unwrap();
        let first = create_batch(
            &pool,
            "case-1",
            &[
                MaterialQueueItemInput {
                    source_path: "done.pdf".into(),
                    document_id: Some("doc-done".into()),
                },
                MaterialQueueItemInput {
                    source_path: "bad.pdf".into(),
                    document_id: Some("doc-bad".into()),
                },
            ],
        )
        .await
        .unwrap();
        start_batch(&pool, &first.batch.id).await.unwrap();
        let done = claim_next(&pool, &first.batch.id).await.unwrap().unwrap();
        finish_item(&pool, &done.id, done.claim_token.as_deref().unwrap())
            .await
            .unwrap();
        let bad = claim_next(&pool, &first.batch.id).await.unwrap().unwrap();
        fail_item(
            &pool,
            bad.id,
            bad.claim_token.unwrap(),
            Some("ocr".into()),
            Some("测试失败".into()),
        )
        .await
        .unwrap();

        // 同一路径另一批次仍在 queued，忽略时也必须封死并重算批次终态。
        save_decisions(
            &pool,
            "case-1",
            &[MaterialDecisionInput {
                source_path: "bad.pdf".into(),
                disposition: "recognize".into(),
                document_id: Some("doc-bad".into()),
            }],
        )
        .await
        .unwrap();
        let second = create_batch(
            &pool,
            "case-1",
            &[MaterialQueueItemInput {
                source_path: "bad.pdf".into(),
                document_id: Some("doc-bad".into()),
            }],
        )
        .await
        .unwrap();

        let after = ignore_failed_items(&pool, &first.batch.id, Some("ocr"))
            .await
            .unwrap();
        assert_eq!(after.batch.status, COMPLETED);
        assert_eq!(after.items[0].status, COMPLETED);
        assert_eq!(after.items[1].status, CANCELLED);
        assert!(after
            .events
            .iter()
            .any(|event| event.event_type == "item_ignored"));
        assert!(after
            .events
            .iter()
            .any(|event| event.event_type == "batch_recomputed_after_ignore"));
        let second_after = get_batch_detail(&pool, &second.batch.id).await.unwrap();
        assert_eq!(second_after.batch.status, CANCELLED);
        assert_eq!(second_after.items[0].status, CANCELLED);
        let disposition: String = sqlx::query_scalar(
            "SELECT disposition FROM material_source_decisions \
             WHERE case_id='case-1' AND source_path='bad.pdf'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(disposition, "excluded");
        let doc_status: String =
            sqlx::query_scalar("SELECT extraction_status FROM documents WHERE id='doc-bad'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(doc_status, "skipped");
    }

    #[tokio::test]
    async fn claim_is_idempotent_across_workers() {
        let pool = pool().await;
        let detail = batch(&pool, &["one.pdf"]).await;
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "start")
            .await
            .unwrap();
        let (first, second) = tokio::join!(
            claim_next(&pool, &detail.batch.id),
            claim_next(&pool, &detail.batch.id)
        );
        let first = first.unwrap();
        let second = second.unwrap();
        assert!(first.is_some());
        assert!(second.is_none());
    }

    #[tokio::test]
    async fn stale_claim_token_cannot_finish_item() {
        let pool = pool().await;
        let detail = batch(&pool, &["one.pdf"]).await;
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "start")
            .await
            .unwrap();
        let item = claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        let err = finish_item(&pool, &item.id, "wrong-token")
            .await
            .unwrap_err();
        assert!(err.contains("失去领取权"));
    }

    #[tokio::test]
    async fn startup_recovery_never_requeues_or_claims() {
        let pool = pool().await;
        let detail = batch(&pool, &["one.pdf"]).await;
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "start")
            .await
            .unwrap();
        claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        let recovered = recover_interrupted_material_processing(&pool)
            .await
            .unwrap();
        assert_eq!(
            recovered,
            RecoveryResult {
                batches: 1,
                items: 1
            }
        );
        let after = get_batch_detail(&pool, &detail.batch.id).await.unwrap();
        assert_eq!(after.batch.status, RECOVERY_REQUIRED);
        assert_eq!(after.items[0].status, RECOVERY_REQUIRED);
        assert!(after.items[0].claim_token.is_none());
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_none());

        let resumed = resume_batch(&pool, &detail.batch.id).await.unwrap();
        assert_eq!(resumed.batch.status, QUEUED);
        assert_eq!(resumed.items[0].status, QUEUED);
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_none());
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "restart")
            .await
            .unwrap();
        assert!(claim_next(&pool, &detail.batch.id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn completed_results_survive_cancel() {
        let pool = pool().await;
        let detail = batch(&pool, &["done.pdf", "waiting.pdf"]).await;
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "start")
            .await
            .unwrap();
        let first = claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        finish_item(&pool, &first.id, first.claim_token.as_deref().unwrap())
            .await
            .unwrap();
        let after = cancel_batch(&pool, &detail.batch.id).await.unwrap();
        assert_eq!(after.items[0].status, COMPLETED);
        assert_eq!(after.items[1].status, CANCELLED);
    }

    #[test]
    fn error_summary_is_redacted_and_bounded() {
        let input = format!(
            "HTTP 429 https://vendor.test/run?api_key=plain \
             Authorization: abc Bearer very.secret.token password=hunter2 {}",
            "x".repeat(600)
        );
        let safe = sanitize_error_summary(Some(&input)).unwrap();
        assert!(!safe.contains("plain"));
        assert!(!safe.contains("abc"));
        assert!(!safe.contains("very.secret.token"));
        assert!(!safe.contains("hunter2"));
        assert!(safe.chars().count() <= 501);
    }

    #[tokio::test]
    async fn audit_events_cover_batch_and_item_transitions() {
        let pool = pool().await;
        let detail = batch(&pool, &["one.pdf"]).await;
        transition_batch(&pool, &detail.batch.id, &[QUEUED], RUNNING, "start")
            .await
            .unwrap();
        let item = claim_next(&pool, &detail.batch.id).await.unwrap().unwrap();
        finish_item(&pool, &item.id, item.claim_token.as_deref().unwrap())
            .await
            .unwrap();
        let after = get_batch_detail(&pool, &detail.batch.id).await.unwrap();
        assert!(after.events.iter().any(|event| event.event_type == "start"));
        assert!(after
            .events
            .iter()
            .any(|event| event.event_type == "item_claimed"));
        assert!(after
            .events
            .iter()
            .any(|event| event.event_type == "item_completed"));
        assert!(after
            .events
            .iter()
            .any(|event| event.event_type == "batch_settled"));
    }

    #[tokio::test]
    async fn database_rejects_cross_case_document_and_batch_links() {
        let pool = pool().await;
        sqlx::query(
            "INSERT INTO cases(id,name,case_type,source_folder) \
             VALUES ('case-2','其他案件','诉讼','C:/cases/other')",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO documents(id,case_id,source_path,filename,size_bytes) \
             VALUES ('doc-2','case-2','other.pdf','other.pdf',1)",
        )
        .execute(&pool)
        .await
        .unwrap();

        let decision_error = sqlx::query(
            "INSERT INTO material_source_decisions \
             (case_id,source_path,disposition,document_id) \
             VALUES ('case-1','other.pdf','recognize','doc-2')",
        )
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(decision_error
            .to_string()
            .contains("document must belong to case"));

        decisions(&pool, &["one.pdf"]).await;
        let detail = batch(&pool, &["one.pdf"]).await;
        save_decisions(
            &pool,
            "case-2",
            &[MaterialDecisionInput {
                source_path: "other.pdf".into(),
                disposition: "recognize".into(),
                document_id: Some("doc-2".into()),
            }],
        )
        .await
        .unwrap();
        let item_error = sqlx::query(
            "INSERT INTO material_processing_items \
             (id,batch_id,case_id,source_path,document_id,ordinal,status) \
             VALUES ('cross-item',?1,'case-2','other.pdf','doc-2',99,'queued')",
        )
        .bind(&detail.batch.id)
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(item_error.to_string().contains("scope mismatch"));
    }

    #[tokio::test]
    async fn database_rejects_event_item_from_another_batch() {
        let pool = pool().await;
        let first = batch(&pool, &["first.pdf"]).await;
        let second = batch(&pool, &["second.pdf"]).await;
        let error = sqlx::query(
            "INSERT INTO material_processing_events \
             (id,batch_id,item_id,event_type,actor) VALUES ('cross-event',?1,?2,'test','system')",
        )
        .bind(&second.batch.id)
        .bind(&first.items[0].id)
        .execute(&pool)
        .await
        .unwrap_err();
        assert!(error.to_string().contains("item must belong to batch"));
    }
}
