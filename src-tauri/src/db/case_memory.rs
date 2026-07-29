//! 案件隔离记忆。
//!
//! 所有案件记忆 API 都显式要求 `case_id`，并在 SQL 中同时匹配
//! `case_id + memory_id`。AI/工具只能创建候选；只有人工确认过的
//! active revision 可以进入逐轮注入预览。

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

pub const CASE_MEMORY_BUDGET_CHARS: usize = 4_500;
pub const PREFERENCE_MEMORY_BUDGET_CHARS: usize = 1_500;
pub const SYSTEM_RULES_VERSION: &str = "fanglv-memory-v1";

const NOT_FOUND: &str = "记忆不存在或不属于当前案件";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseMemory {
    pub id: String,
    pub case_id: String,
    pub memory_type: String,
    pub status: String,
    pub verification_status: String,
    pub injection_mode: String,
    pub current_revision_no: i64,
    pub active_revision_no: Option<i64>,
    pub title: String,
    pub content: String,
    pub revision_no: i64,
    pub source_count: i64,
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorySourceInput {
    pub source_type: String,
    pub document_id: Option<String>,
    pub chat_message_id: Option<String>,
    pub locator: Option<String>,
    pub excerpt: Option<String>,
    pub external_ref: Option<String>,
    pub verification_status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryInput {
    pub memory_type: String,
    pub title: String,
    pub content: String,
    pub verification_status: Option<String>,
    pub injection_mode: Option<String>,
    pub change_reason: Option<String>,
    pub source: Option<MemorySourceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviseMemoryInput {
    pub expected_revision: i64,
    pub title: String,
    pub content: String,
    pub change_reason: String,
    pub source: Option<MemorySourceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct MemoryCandidate {
    pub id: String,
    pub case_id: String,
    pub proposed_type: String,
    pub proposed_title: String,
    pub proposed_content: String,
    pub proposed_by_type: String,
    pub source_message_id: Option<String>,
    pub status: String,
    pub decided_by: Option<String>,
    pub decided_at: Option<String>,
    pub decision_reason: Option<String>,
    pub accepted_memory_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateCandidateInput {
    pub proposed_type: String,
    pub proposed_title: String,
    pub proposed_content: String,
    pub proposed_by_type: String,
    pub source_message_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcceptCandidateInput {
    pub title: String,
    pub content: String,
    pub memory_type: String,
    pub verification_status: Option<String>,
    pub source: Option<MemorySourceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct UserMemoryPreference {
    pub id: String,
    pub title: String,
    pub content: String,
    pub status: String,
    pub injection_mode: String,
    pub current_revision_no: i64,
    pub confirmed_by: Option<String>,
    pub confirmed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePreferenceInput {
    pub title: String,
    pub content: String,
    pub injection_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionPreviewEntry {
    pub scope: String,
    pub id: String,
    pub revision_no: i64,
    pub title: String,
    pub content: String,
    pub verification_status: Option<String>,
    pub char_count: usize,
    pub selected: bool,
    pub omitted_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInjectionPreview {
    pub id: String,
    pub case_id: String,
    pub task_type: Option<String>,
    pub entries: Vec<InjectionPreviewEntry>,
    pub case_used_chars: usize,
    pub preference_used_chars: usize,
    pub preview_sha256: String,
    pub prompt_markdown: String,
    pub status: String,
}

const MEMORY_SELECT: &str = r#"
SELECT
    i.id, i.case_id, i.memory_type, i.status, i.verification_status,
    i.injection_mode, i.current_revision_no, i.active_revision_no,
    r.title, r.content, r.revision_no,
    (
      SELECT COUNT(*) FROM case_memory_sources s
      WHERE s.case_id = i.case_id
        AND s.memory_id = i.id
        AND s.revision_no = r.revision_no
    ) AS source_count,
    i.confirmed_by, i.confirmed_at, i.created_at, i.updated_at
FROM case_memory_items i
JOIN case_memory_revisions r
  ON r.case_id = i.case_id
 AND r.memory_id = i.id
 AND r.revision_no = i.current_revision_no
"#;

pub async fn list_case_memories(
    pool: &SqlitePool,
    case_id: &str,
    include_deleted: bool,
) -> Result<Vec<CaseMemory>, String> {
    ensure_case_id(case_id)?;
    let sql = format!(
        "{MEMORY_SELECT} WHERE i.case_id = ?1 AND (?2 = 1 OR i.status != 'deleted') ORDER BY i.updated_at DESC"
    );
    sqlx::query_as::<_, CaseMemory>(&sql)
        .bind(case_id)
        .bind(if include_deleted { 1 } else { 0 })
        .fetch_all(pool)
        .await
        .map_err(db_error)
}

pub async fn get_case_memory(
    pool: &SqlitePool,
    case_id: &str,
    memory_id: &str,
) -> Result<CaseMemory, String> {
    ensure_case_id(case_id)?;
    let sql = format!("{MEMORY_SELECT} WHERE i.case_id = ?1 AND i.id = ?2");
    sqlx::query_as::<_, CaseMemory>(&sql)
        .bind(case_id)
        .bind(memory_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| NOT_FOUND.to_string())
}

pub async fn create_case_memory_draft(
    pool: &SqlitePool,
    case_id: &str,
    input: CreateMemoryInput,
    actor: &str,
) -> Result<CaseMemory, String> {
    ensure_case_exists(pool, case_id).await?;
    validate_memory_type(&input.memory_type)?;
    validate_text(&input.title, 120, "标题")?;
    validate_text(&input.content, 4_000, "正文")?;
    let verification = validate_verification(input.verification_status.as_deref())?;
    if verification == "verified" {
        validate_verified_source(input.source.as_ref())?;
    }
    let injection_mode = validate_injection_mode(input.injection_mode.as_deref())?;
    let now = now();
    let id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await.map_err(db_error)?;

    sqlx::query(
        r#"
        INSERT INTO case_memory_items (
            id, case_id, memory_type, status, verification_status,
            injection_mode, current_revision_no, active_revision_no,
            created_by_type, created_by_id, created_at, updated_at
        ) VALUES (?1, ?2, ?3, 'draft', ?4, ?5, 1, NULL, 'user', ?6, ?7, ?7)
        "#,
    )
    .bind(&id)
    .bind(case_id)
    .bind(&input.memory_type)
    .bind(verification)
    .bind(injection_mode)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;

    sqlx::query(
        r#"
        INSERT INTO case_memory_revisions (
            memory_id, case_id, revision_no, title, content, change_reason,
            verification_status, authored_by, authored_at, content_sha256
        ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(&id)
    .bind(case_id)
    .bind(input.title.trim())
    .bind(input.content.trim())
    .bind(
        input
            .change_reason
            .unwrap_or_else(|| "首次创建".to_string()),
    )
    .bind(verification)
    .bind(actor)
    .bind(&now)
    .bind(sha256_hex(input.content.trim()))
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;

    if let Some(source) = input.source {
        insert_source(&mut tx, case_id, &id, 1, source, actor).await?;
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "case_memory",
        &id,
        "created",
        "user",
        actor,
        Some(1),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_case_memory(pool, case_id, &id).await
}

pub async fn confirm_case_memory(
    pool: &SqlitePool,
    case_id: &str,
    memory_id: &str,
    expected_revision: i64,
    actor: &str,
) -> Result<CaseMemory, String> {
    let now = now();
    let mut tx = pool.begin().await.map_err(db_error)?;
    let verified_source_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM case_memory_revisions r
        JOIN case_memory_sources s
          ON s.case_id = r.case_id
         AND s.memory_id = r.memory_id
         AND s.revision_no = r.revision_no
        WHERE r.case_id = ?1
          AND r.memory_id = ?2
          AND r.revision_no = ?3
          AND r.verification_status = 'verified'
          AND s.verification_status = 'verified'
          AND s.source_type IN ('document','chat_user','chat_assistant','case_field')
        "#,
    )
    .bind(case_id)
    .bind(memory_id)
    .bind(expected_revision)
    .fetch_one(&mut *tx)
    .await
    .map_err(db_error)?;
    let revision_verification: Option<String> = sqlx::query_scalar(
        "SELECT verification_status FROM case_memory_revisions WHERE case_id=?1 AND memory_id=?2 AND revision_no=?3",
    )
    .bind(case_id)
    .bind(memory_id)
    .bind(expected_revision)
    .fetch_optional(&mut *tx)
    .await
    .map_err(db_error)?;
    if revision_verification.as_deref() == Some("verified") && verified_source_count == 0 {
        return Err("标记为已核验的记忆必须有一条已核验且可定位的案件来源".to_string());
    }
    let revision_changed = sqlx::query(
        r#"
        UPDATE case_memory_revisions
        SET confirmed_by = ?4, confirmed_at = ?5
        WHERE case_id = ?1 AND memory_id = ?2 AND revision_no = ?3
        "#,
    )
    .bind(case_id)
    .bind(memory_id)
    .bind(expected_revision)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if revision_changed != 1 {
        return Err("记忆版本已变化、已删除或不属于当前案件".to_string());
    }
    let changed = sqlx::query(
        r#"
        UPDATE case_memory_items
        SET status = 'active',
            active_revision_no = current_revision_no,
            confirmed_by = ?4,
            confirmed_at = ?5,
            disabled_at = NULL,
            updated_at = ?5
        WHERE case_id = ?1
          AND id = ?2
          AND current_revision_no = ?3
          AND status IN ('draft','active','disabled')
        "#,
    )
    .bind(case_id)
    .bind(memory_id)
    .bind(expected_revision)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("记忆版本已变化、已删除或不属于当前案件".to_string());
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "case_memory",
        memory_id,
        "confirmed",
        "user",
        actor,
        Some(expected_revision),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_case_memory(pool, case_id, memory_id).await
}

pub async fn revise_case_memory(
    pool: &SqlitePool,
    case_id: &str,
    memory_id: &str,
    input: ReviseMemoryInput,
    actor: &str,
) -> Result<CaseMemory, String> {
    validate_text(&input.title, 120, "标题")?;
    validate_text(&input.content, 4_000, "正文")?;
    validate_text(&input.change_reason, 500, "修改原因")?;
    let now = now();
    let next_revision = input.expected_revision + 1;
    let mut tx = pool.begin().await.map_err(db_error)?;
    let changed = sqlx::query(
        r#"
        UPDATE case_memory_items
        SET current_revision_no = ?4,
            verification_status = 'unverified',
            updated_at = ?5
        WHERE case_id = ?1
          AND id = ?2
          AND current_revision_no = ?3
          AND status != 'deleted'
        "#,
    )
    .bind(case_id)
    .bind(memory_id)
    .bind(input.expected_revision)
    .bind(next_revision)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("记忆版本已变化、已删除或不属于当前案件".to_string());
    }
    sqlx::query(
        r#"
        INSERT INTO case_memory_revisions (
            memory_id, case_id, revision_no, title, content, change_reason,
            verification_status, authored_by, authored_at, content_sha256
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'unverified', ?7, ?8, ?9)
        "#,
    )
    .bind(memory_id)
    .bind(case_id)
    .bind(next_revision)
    .bind(input.title.trim())
    .bind(input.content.trim())
    .bind(input.change_reason.trim())
    .bind(actor)
    .bind(&now)
    .bind(sha256_hex(input.content.trim()))
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    if let Some(source) = input.source {
        insert_source(&mut tx, case_id, memory_id, next_revision, source, actor).await?;
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "case_memory",
        memory_id,
        "revised",
        "user",
        actor,
        Some(next_revision),
        Some(input.change_reason.trim()),
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_case_memory(pool, case_id, memory_id).await
}

pub async fn set_case_memory_status(
    pool: &SqlitePool,
    case_id: &str,
    memory_id: &str,
    status: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<CaseMemory, String> {
    let (event, timestamp_field) = match status {
        "disabled" => ("disabled", "disabled_at"),
        "deleted" => ("deleted", "deleted_at"),
        _ => return Err("只允许停用或删除记忆".to_string()),
    };
    let now = now();
    let sql = format!(
        "UPDATE case_memory_items SET status = ?3, {timestamp_field} = ?4, updated_at = ?4 WHERE case_id = ?1 AND id = ?2 AND status != 'deleted'"
    );
    let mut tx = pool.begin().await.map_err(db_error)?;
    let changed = sqlx::query(&sql)
        .bind(case_id)
        .bind(memory_id)
        .bind(status)
        .bind(&now)
        .execute(&mut *tx)
        .await
        .map_err(db_error)?
        .rows_affected();
    if changed != 1 {
        return Err(NOT_FOUND.to_string());
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "case_memory",
        memory_id,
        event,
        "user",
        actor,
        None,
        reason,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_case_memory(pool, case_id, memory_id).await
}

pub async fn create_memory_candidate(
    pool: &SqlitePool,
    case_id: &str,
    input: CreateCandidateInput,
) -> Result<MemoryCandidate, String> {
    ensure_case_exists(pool, case_id).await?;
    validate_memory_type(&input.proposed_type)?;
    validate_text(&input.proposed_title, 120, "候选标题")?;
    validate_text(&input.proposed_content, 4_000, "候选正文")?;
    if !matches!(
        input.proposed_by_type.as_str(),
        "user" | "assistant" | "tool"
    ) {
        return Err("无效的候选来源".to_string());
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO case_memory_candidates (
            id, case_id, proposed_type, proposed_title, proposed_content,
            proposed_by_type, source_message_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
    )
    .bind(&id)
    .bind(case_id)
    .bind(&input.proposed_type)
    .bind(input.proposed_title.trim())
    .bind(input.proposed_content.trim())
    .bind(&input.proposed_by_type)
    .bind(&input.source_message_id)
    .execute(pool)
    .await
    .map_err(|e| safe_constraint_error(e, "候选来源消息不存在或不属于当前案件"))?;
    get_candidate(pool, case_id, &id).await
}

pub async fn list_memory_candidates(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<MemoryCandidate>, String> {
    ensure_case_id(case_id)?;
    sqlx::query_as::<_, MemoryCandidate>(
        "SELECT * FROM case_memory_candidates WHERE case_id = ?1 ORDER BY created_at DESC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)
}

pub async fn accept_memory_candidate(
    pool: &SqlitePool,
    case_id: &str,
    candidate_id: &str,
    input: AcceptCandidateInput,
    actor: &str,
) -> Result<CaseMemory, String> {
    validate_memory_type(&input.memory_type)?;
    validate_text(&input.title, 120, "标题")?;
    validate_text(&input.content, 4_000, "正文")?;
    let candidate = get_candidate(pool, case_id, candidate_id).await?;
    if candidate.status != "pending" {
        return Err("候选已处理".to_string());
    }
    let verification = validate_verification(input.verification_status.as_deref())?;
    if verification == "verified" {
        validate_verified_source(input.source.as_ref())?;
    }
    let memory_id = Uuid::new_v4().to_string();
    let now = now();
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query(
        r#"
        INSERT INTO case_memory_items (
            id, case_id, memory_type, status, verification_status,
            injection_mode, current_revision_no, created_by_type, created_by_id,
            created_at, updated_at
        ) VALUES (?1, ?2, ?3, 'draft', ?4, 'archive_only', 1, ?5, ?6, ?7, ?7)
        "#,
    )
    .bind(&memory_id)
    .bind(case_id)
    .bind(&input.memory_type)
    .bind(verification)
    .bind(&candidate.proposed_by_type)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    sqlx::query(
        r#"
        INSERT INTO case_memory_revisions (
            memory_id, case_id, revision_no, title, content, change_reason,
            verification_status, authored_by, authored_at, content_sha256
        ) VALUES (?1, ?2, 1, ?3, ?4, '接受候选并编辑', ?5, ?6, ?7, ?8)
        "#,
    )
    .bind(&memory_id)
    .bind(case_id)
    .bind(input.title.trim())
    .bind(input.content.trim())
    .bind(verification)
    .bind(actor)
    .bind(&now)
    .bind(sha256_hex(input.content.trim()))
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    if let Some(source) = input.source {
        insert_source(&mut tx, case_id, &memory_id, 1, source, actor).await?;
    }
    let changed = sqlx::query(
        r#"
        UPDATE case_memory_candidates
        SET status = 'accepted', decided_by = ?3, decided_at = ?4,
            accepted_memory_id = ?5, updated_at = ?4
        WHERE case_id = ?1 AND id = ?2 AND status = 'pending'
        "#,
    )
    .bind(case_id)
    .bind(candidate_id)
    .bind(actor)
    .bind(&now)
    .bind(&memory_id)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("候选版本已变化或不属于当前案件".to_string());
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "candidate",
        candidate_id,
        "candidate_accepted",
        "user",
        actor,
        Some(1),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_case_memory(pool, case_id, &memory_id).await
}

pub async fn reject_memory_candidate(
    pool: &SqlitePool,
    case_id: &str,
    candidate_id: &str,
    actor: &str,
    reason: Option<&str>,
) -> Result<(), String> {
    let now = now();
    let mut tx = pool.begin().await.map_err(db_error)?;
    let changed = sqlx::query(
        r#"
        UPDATE case_memory_candidates
        SET status = 'rejected', decided_by = ?3, decided_at = ?4,
            decision_reason = ?5, updated_at = ?4
        WHERE case_id = ?1 AND id = ?2 AND status = 'pending'
        "#,
    )
    .bind(case_id)
    .bind(candidate_id)
    .bind(actor)
    .bind(&now)
    .bind(reason)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("候选已处理或不属于当前案件".to_string());
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "candidate",
        candidate_id,
        "candidate_rejected",
        "user",
        actor,
        None,
        reason,
    )
    .await?;
    tx.commit().await.map_err(db_error)
}

pub async fn list_user_memory_preferences(
    pool: &SqlitePool,
    include_deleted: bool,
) -> Result<Vec<UserMemoryPreference>, String> {
    sqlx::query_as::<_, UserMemoryPreference>(
        r#"
        SELECT p.id, p.title, r.content, p.status, p.injection_mode,
               p.current_revision_no, p.confirmed_by, p.confirmed_at,
               p.created_at, p.updated_at
        FROM user_memory_preferences p
        JOIN user_memory_preference_revisions r
          ON r.preference_id = p.id AND r.revision_no = p.current_revision_no
        WHERE (?1 = 1 OR p.status != 'deleted')
        ORDER BY p.updated_at DESC
        "#,
    )
    .bind(if include_deleted { 1 } else { 0 })
    .fetch_all(pool)
    .await
    .map_err(db_error)
}

pub async fn create_user_memory_preference(
    pool: &SqlitePool,
    input: CreatePreferenceInput,
    actor: &str,
) -> Result<UserMemoryPreference, String> {
    validate_text(&input.title, 120, "标题")?;
    validate_text(&input.content, 2_000, "偏好正文")?;
    reject_case_specific_preference(&input.content)?;
    let injection_mode = validate_injection_mode(input.injection_mode.as_deref())?;
    let id = Uuid::new_v4().to_string();
    let now = now();
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query(
        r#"
        INSERT INTO user_memory_preferences (
            id, title, status, injection_mode, current_revision_no,
            created_at, updated_at
        ) VALUES (?1, ?2, 'draft', ?3, 1, ?4, ?4)
        "#,
    )
    .bind(&id)
    .bind(input.title.trim())
    .bind(injection_mode)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    sqlx::query(
        r#"
        INSERT INTO user_memory_preference_revisions (
            preference_id, revision_no, title, content, change_reason,
            authored_by, authored_at, content_sha256
        ) VALUES (?1, 1, ?2, ?3, '首次创建', ?4, ?5, ?6)
        "#,
    )
    .bind(&id)
    .bind(input.title.trim())
    .bind(input.content.trim())
    .bind(actor)
    .bind(&now)
    .bind(sha256_hex(input.content.trim()))
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    insert_audit(
        &mut tx,
        None,
        "user_preference",
        &id,
        "created",
        "user",
        actor,
        Some(1),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_user_preference(pool, &id).await
}

pub async fn confirm_user_memory_preference(
    pool: &SqlitePool,
    preference_id: &str,
    expected_revision: i64,
    actor: &str,
) -> Result<UserMemoryPreference, String> {
    let now = now();
    let mut tx = pool.begin().await.map_err(db_error)?;
    let revision_changed = sqlx::query(
        r#"
        UPDATE user_memory_preference_revisions
        SET confirmed_by = ?3, confirmed_at = ?4
        WHERE preference_id = ?1 AND revision_no = ?2
          AND confirmed_by IS NULL AND confirmed_at IS NULL
        "#,
    )
    .bind(preference_id)
    .bind(expected_revision)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if revision_changed != 1 {
        return Err("用户偏好版本已变化、已确认或不存在".to_string());
    }
    let changed = sqlx::query(
        r#"
        UPDATE user_memory_preferences
        SET status = 'active', confirmed_by = ?3, confirmed_at = ?4,
            disabled_at = NULL, updated_at = ?4
        WHERE id = ?1 AND current_revision_no = ?2
          AND status IN ('draft','active','disabled')
        "#,
    )
    .bind(preference_id)
    .bind(expected_revision)
    .bind(actor)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("用户偏好版本已变化或已删除".to_string());
    }
    insert_audit(
        &mut tx,
        None,
        "user_preference",
        preference_id,
        "confirmed",
        "user",
        actor,
        Some(expected_revision),
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    get_user_preference(pool, preference_id).await
}

pub async fn preview_memory_injection(
    pool: &SqlitePool,
    case_id: &str,
    task_type: Option<String>,
    selected_memory_ids: Vec<String>,
    selected_preference_ids: Vec<String>,
) -> Result<MemoryInjectionPreview, String> {
    ensure_case_exists(pool, case_id).await?;
    let mut entries = Vec::new();
    let mut case_used = 0usize;
    for id in selected_memory_ids {
        let row = sqlx::query_as::<_, (String, i64, String, String, String)>(
            r#"
            SELECT i.id, i.active_revision_no, r.title, r.content, i.verification_status
            FROM case_memory_items i
            JOIN case_memory_revisions r
              ON r.case_id = i.case_id
             AND r.memory_id = i.id
             AND r.revision_no = i.active_revision_no
            WHERE i.case_id = ?1
              AND i.id = ?2
              AND i.status = 'active'
              AND i.injection_mode = 'manual_each_turn'
            "#,
        )
        .bind(case_id)
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "所选记忆未启用、版本无效或不属于当前案件".to_string())?;
        let count = row.3.chars().count();
        let omitted = if case_used + count > CASE_MEMORY_BUDGET_CHARS {
            Some("超过案件记忆预算".to_string())
        } else {
            case_used += count;
            None
        };
        entries.push(InjectionPreviewEntry {
            scope: "case".to_string(),
            id: row.0,
            revision_no: row.1,
            title: row.2,
            content: row.3,
            verification_status: Some(row.4),
            char_count: count,
            selected: omitted.is_none(),
            omitted_reason: omitted,
        });
    }
    let mut preference_used = 0usize;
    for id in selected_preference_ids {
        let row = sqlx::query_as::<_, (String, i64, String, String)>(
            r#"
            SELECT p.id, p.current_revision_no, r.title, r.content
            FROM user_memory_preferences p
            JOIN user_memory_preference_revisions r
              ON r.preference_id = p.id AND r.revision_no = p.current_revision_no
            WHERE p.id = ?1
              AND p.status = 'active'
              AND p.injection_mode = 'manual_each_turn'
            "#,
        )
        .bind(&id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "所选用户偏好未启用或版本无效".to_string())?;
        let count = row.3.chars().count();
        let omitted = if preference_used + count > PREFERENCE_MEMORY_BUDGET_CHARS {
            Some("超过用户偏好预算".to_string())
        } else {
            preference_used += count;
            None
        };
        entries.push(InjectionPreviewEntry {
            scope: "user_preference".to_string(),
            id: row.0,
            revision_no: row.1,
            title: row.2,
            content: row.3,
            verification_status: None,
            char_count: count,
            selected: omitted.is_none(),
            omitted_reason: omitted,
        });
    }
    let prompt_markdown = build_injection_markdown(&entries);
    let preview_sha256 = sha256_hex(&canonical_preview(case_id, &task_type, &entries));
    let run_id = Uuid::new_v4().to_string();
    let mut tx = pool.begin().await.map_err(db_error)?;
    sqlx::query(
        r#"
        INSERT INTO memory_injection_runs (
            id, case_id, task_type, system_rules_version,
            case_budget_chars, preference_budget_chars,
            case_used_chars, preference_used_chars, preview_sha256, status
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'preview')
        "#,
    )
    .bind(&run_id)
    .bind(case_id)
    .bind(&task_type)
    .bind(SYSTEM_RULES_VERSION)
    .bind(CASE_MEMORY_BUDGET_CHARS as i64)
    .bind(PREFERENCE_MEMORY_BUDGET_CHARS as i64)
    .bind(case_used as i64)
    .bind(preference_used as i64)
    .bind(&preview_sha256)
    .execute(&mut *tx)
    .await
    .map_err(db_error)?;
    for (index, entry) in entries.iter().enumerate() {
        if entry.scope == "case" {
            sqlx::query(
                r#"
                INSERT INTO memory_injection_case_entries (
                    run_id, case_id, memory_id, revision_no, display_order,
                    selected, char_count, omitted_reason
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                "#,
            )
            .bind(&run_id)
            .bind(case_id)
            .bind(&entry.id)
            .bind(entry.revision_no)
            .bind(index as i64)
            .bind(if entry.selected { 1 } else { 0 })
            .bind(entry.char_count as i64)
            .bind(&entry.omitted_reason)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        } else {
            sqlx::query(
                r#"
                INSERT INTO memory_injection_preference_entries (
                    run_id, preference_id, revision_no, display_order,
                    selected, char_count, omitted_reason
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                "#,
            )
            .bind(&run_id)
            .bind(&entry.id)
            .bind(entry.revision_no)
            .bind(index as i64)
            .bind(if entry.selected { 1 } else { 0 })
            .bind(entry.char_count as i64)
            .bind(&entry.omitted_reason)
            .execute(&mut *tx)
            .await
            .map_err(db_error)?;
        }
    }
    insert_audit(
        &mut tx,
        Some(case_id),
        "injection",
        &run_id,
        "previewed",
        "user",
        "local-user",
        None,
        None,
    )
    .await?;
    tx.commit().await.map_err(db_error)?;
    Ok(MemoryInjectionPreview {
        id: run_id,
        case_id: case_id.to_string(),
        task_type,
        entries,
        case_used_chars: case_used,
        preference_used_chars: preference_used,
        preview_sha256,
        prompt_markdown,
        status: "preview".to_string(),
    })
}

pub async fn confirm_memory_injection(
    pool: &SqlitePool,
    case_id: &str,
    run_id: &str,
    preview_sha256: &str,
    actor: &str,
) -> Result<(), String> {
    let now = now();
    let changed = sqlx::query(
        r#"
        UPDATE memory_injection_runs
        SET status = 'confirmed', confirmed_by = ?4, confirmed_at = ?5
        WHERE id = ?1 AND case_id = ?2 AND preview_sha256 = ?3 AND status = 'preview'
        "#,
    )
    .bind(run_id)
    .bind(case_id)
    .bind(preview_sha256)
    .bind(actor)
    .bind(now)
    .execute(pool)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("注入预览已失效、哈希不匹配或不属于当前案件".to_string());
    }
    Ok(())
}

pub async fn load_confirmed_injection(
    pool: &SqlitePool,
    case_id: &str,
    run_id: &str,
    preview_sha256: &str,
) -> Result<String, String> {
    let status: Option<(String,)> = sqlx::query_as(
        "SELECT status FROM memory_injection_runs WHERE id = ?1 AND case_id = ?2 AND preview_sha256 = ?3",
    )
    .bind(run_id)
    .bind(case_id)
    .bind(preview_sha256)
    .fetch_optional(pool)
    .await
    .map_err(db_error)?;
    if !matches!(status.as_ref().map(|v| v.0.as_str()), Some("confirmed")) {
        return Err("注入预览未确认、已失效或不属于当前案件".to_string());
    }
    let mut entries = Vec::new();
    let case_rows = sqlx::query_as::<_, (String, i64, String, String, String, i64)>(
        r#"
        SELECT e.memory_id, e.revision_no, r.title, r.content,
               i.verification_status, e.display_order
        FROM memory_injection_case_entries e
        JOIN memory_injection_runs run ON run.id = e.run_id AND run.case_id = e.case_id
        JOIN case_memory_items i ON i.case_id = e.case_id AND i.id = e.memory_id
        JOIN case_memory_revisions r
          ON r.case_id = e.case_id
         AND r.memory_id = e.memory_id
         AND r.revision_no = e.revision_no
        WHERE e.run_id = ?1 AND e.case_id = ?2 AND e.selected = 1
          AND i.status = 'active' AND i.active_revision_no = e.revision_no
        "#,
    )
    .bind(run_id)
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    for row in case_rows {
        entries.push((
            row.5,
            InjectionPreviewEntry {
                scope: "case".to_string(),
                id: row.0,
                revision_no: row.1,
                title: row.2,
                content: row.3.clone(),
                verification_status: Some(row.4),
                char_count: row.3.chars().count(),
                selected: true,
                omitted_reason: None,
            },
        ));
    }
    let pref_rows = sqlx::query_as::<_, (String, i64, String, String, i64)>(
        r#"
        SELECT e.preference_id, e.revision_no, r.title, r.content, e.display_order
        FROM memory_injection_preference_entries e
        JOIN user_memory_preferences p ON p.id = e.preference_id
        JOIN user_memory_preference_revisions r
          ON r.preference_id = e.preference_id AND r.revision_no = e.revision_no
        WHERE e.run_id = ?1 AND e.selected = 1
          AND p.status = 'active' AND p.current_revision_no = e.revision_no
        "#,
    )
    .bind(run_id)
    .fetch_all(pool)
    .await
    .map_err(db_error)?;
    for row in pref_rows {
        entries.push((
            row.4,
            InjectionPreviewEntry {
                scope: "user_preference".to_string(),
                id: row.0,
                revision_no: row.1,
                title: row.2,
                content: row.3.clone(),
                verification_status: None,
                char_count: row.3.chars().count(),
                selected: true,
                omitted_reason: None,
            },
        ));
    }
    entries.sort_by_key(|v| v.0);
    if entries.is_empty() {
        return Err("记忆版本已变化；请重新预览并确认".to_string());
    }
    Ok(build_injection_markdown(
        &entries.into_iter().map(|v| v.1).collect::<Vec<_>>(),
    ))
}

pub async fn mark_memory_injected(
    pool: &SqlitePool,
    case_id: &str,
    run_id: &str,
) -> Result<(), String> {
    let now = now();
    let changed = sqlx::query(
        "UPDATE memory_injection_runs SET status = 'injected', injected_at = ?3 WHERE id = ?1 AND case_id = ?2 AND status = 'confirmed'",
    )
    .bind(run_id)
    .bind(case_id)
    .bind(now)
    .execute(pool)
    .await
    .map_err(db_error)?
    .rows_affected();
    if changed != 1 {
        return Err("记忆注入未确认、已完成或不属于当前案件".to_string());
    }
    Ok(())
}

async fn get_candidate(
    pool: &SqlitePool,
    case_id: &str,
    candidate_id: &str,
) -> Result<MemoryCandidate, String> {
    sqlx::query_as::<_, MemoryCandidate>(
        "SELECT * FROM case_memory_candidates WHERE case_id = ?1 AND id = ?2",
    )
    .bind(case_id)
    .bind(candidate_id)
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "候选不存在或不属于当前案件".to_string())
}

async fn get_user_preference(
    pool: &SqlitePool,
    preference_id: &str,
) -> Result<UserMemoryPreference, String> {
    sqlx::query_as::<_, UserMemoryPreference>(
        r#"
        SELECT p.id, p.title, r.content, p.status, p.injection_mode,
               p.current_revision_no, p.confirmed_by, p.confirmed_at,
               p.created_at, p.updated_at
        FROM user_memory_preferences p
        JOIN user_memory_preference_revisions r
          ON r.preference_id = p.id AND r.revision_no = p.current_revision_no
        WHERE p.id = ?1
        "#,
    )
    .bind(preference_id)
    .fetch_optional(pool)
    .await
    .map_err(db_error)?
    .ok_or_else(|| "用户偏好不存在".to_string())
}

async fn insert_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    case_id: &str,
    memory_id: &str,
    revision_no: i64,
    source: MemorySourceInput,
    actor: &str,
) -> Result<(), String> {
    validate_source(&source)?;
    let verification = validate_verification(source.verification_status.as_deref())?;
    sqlx::query(
        r#"
        INSERT INTO case_memory_sources (
            id, case_id, memory_id, revision_no, source_type,
            document_id, chat_message_id, locator, excerpt, external_ref,
            source_sha256, verification_status, created_by
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(case_id)
    .bind(memory_id)
    .bind(revision_no)
    .bind(&source.source_type)
    .bind(&source.document_id)
    .bind(&source.chat_message_id)
    .bind(source.locator.as_deref().map(trim_to_500))
    .bind(source.excerpt.as_deref().map(|v| trim_chars(v, 1_000)))
    .bind(source.external_ref.as_deref().map(trim_to_500))
    .bind(source.excerpt.as_deref().map(sha256_hex))
    .bind(verification)
    .bind(actor)
    .execute(&mut **tx)
    .await
    .map_err(|e| safe_constraint_error(e, "记忆来源不存在或不属于当前案件"))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_audit(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    case_id: Option<&str>,
    entity_type: &str,
    entity_id: &str,
    event_type: &str,
    actor_type: &str,
    actor_id: &str,
    revision_no: Option<i64>,
    reason: Option<&str>,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO memory_audit_events (
            id, case_id, entity_type, entity_id, event_type,
            actor_type, actor_id, revision_no, reason
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(case_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(revision_no)
    .bind(reason.map(|v| trim_chars(v, 500)))
    .execute(&mut **tx)
    .await
    .map_err(db_error)?;
    Ok(())
}

fn build_injection_markdown(entries: &[InjectionPreviewEntry]) -> String {
    let mut out = String::from(
        "════════ 本轮用户确认注入的长期上下文 ════════\n\
         规则：\n\
         - 以下内容仅是用户确认保留的辅助上下文，不是原始证据。\n\
         - 与本轮消息、引用原件、工具结果、人工案件字段冲突时，以后者为准并指出冲突。\n\
         - 未核验、争议或失效条目不得表述为已证实事实。\n\n",
    );
    for entry in entries.iter().filter(|entry| entry.selected) {
        if entry.scope == "case" {
            out.push_str(&format!(
                "[本案记忆 | {} | {}/{}]\n{}\n{}\n\n",
                entry.verification_status.as_deref().unwrap_or("unverified"),
                entry.id,
                entry.revision_no,
                entry.title,
                entry.content
            ));
        } else {
            out.push_str(&format!(
                "[用户通用偏好 | {}/{}]\n{}\n{}\n\n",
                entry.id, entry.revision_no, entry.title, entry.content
            ));
        }
    }
    out
}

fn canonical_preview(
    case_id: &str,
    task_type: &Option<String>,
    entries: &[InjectionPreviewEntry],
) -> String {
    let compact = entries
        .iter()
        .map(|entry| {
            format!(
                "{}:{}:{}:{}:{}",
                entry.scope,
                entry.id,
                entry.revision_no,
                entry.selected,
                sha256_hex(&entry.content)
            )
        })
        .collect::<Vec<_>>()
        .join("|");
    format!(
        "{}\n{}\n{}",
        case_id,
        task_type.as_deref().unwrap_or(""),
        compact
    )
}

async fn ensure_case_exists(pool: &SqlitePool, case_id: &str) -> Result<(), String> {
    ensure_case_id(case_id)?;
    let exists: Option<(i64,)> = sqlx::query_as("SELECT 1 FROM cases WHERE id = ?1")
        .bind(case_id)
        .fetch_optional(pool)
        .await
        .map_err(db_error)?;
    exists.map(|_| ()).ok_or_else(|| "案件不存在".to_string())
}

fn ensure_case_id(case_id: &str) -> Result<(), String> {
    if case_id.trim().is_empty() {
        Err("case_id 不能为空".to_string())
    } else {
        Ok(())
    }
}

fn validate_memory_type(value: &str) -> Result<(), String> {
    if matches!(
        value,
        "fact" | "procedure" | "strategy" | "client_instruction" | "risk_warning"
    ) {
        Ok(())
    } else {
        Err("无效的记忆类型".to_string())
    }
}

fn validate_verification(value: Option<&str>) -> Result<&str, String> {
    let value = value.unwrap_or("unverified");
    if matches!(value, "unverified" | "verified" | "disputed" | "stale") {
        Ok(value)
    } else {
        Err("无效的核验状态".to_string())
    }
}

fn validate_injection_mode(value: Option<&str>) -> Result<&str, String> {
    let value = value.unwrap_or("archive_only");
    if matches!(value, "archive_only" | "manual_each_turn") {
        Ok(value)
    } else {
        Err("无效的注入模式".to_string())
    }
}

fn validate_source(source: &MemorySourceInput) -> Result<(), String> {
    match source.source_type.as_str() {
        "document" if source.document_id.is_some() && source.chat_message_id.is_none() => Ok(()),
        "chat_user" | "chat_assistant"
            if source.chat_message_id.is_some() && source.document_id.is_none() =>
        {
            Ok(())
        }
        "manual_assertion" | "tool_result" | "case_field"
            if source.document_id.is_none() && source.chat_message_id.is_none() =>
        {
            Ok(())
        }
        _ => Err("记忆来源类型与引用不匹配".to_string()),
    }
}

fn validate_verified_source(source: Option<&MemorySourceInput>) -> Result<(), String> {
    let source = source.ok_or_else(|| "标记为已核验时必须提供可定位来源".to_string())?;
    if source.verification_status.as_deref() != Some("verified") {
        return Err("标记为已核验时，来源也必须明确标记为已核验".to_string());
    }
    match source.source_type.as_str() {
        "document" | "chat_user" | "chat_assistant" => Ok(()),
        "case_field"
            if source
                .locator
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty()) =>
        {
            Ok(())
        }
        _ => Err("已核验记忆只接受案件文档、案件聊天或可定位案件字段作为来源".to_string()),
    }
}

fn validate_text(value: &str, max_chars: usize, label: &str) -> Result<(), String> {
    let count = value.trim().chars().count();
    if count == 0 {
        return Err(format!("{label}不能为空"));
    }
    if count > max_chars {
        return Err(format!("{label}不能超过 {max_chars} 个字符"));
    }
    Ok(())
}

fn reject_case_specific_preference(content: &str) -> Result<(), String> {
    let compact = content.replace(' ', "");
    let explicit_markers = [
        "案号：",
        "案号:",
        "当事人：",
        "当事人:",
        "证据编号：",
        "证据编号:",
    ];
    if explicit_markers
        .iter()
        .any(|marker| compact.contains(marker))
    {
        return Err("通用偏好疑似包含具体案件信息，请改存到对应案件记忆".to_string());
    }
    Ok(())
}

fn trim_chars(value: &str, max_chars: usize) -> String {
    value.trim().chars().take(max_chars).collect()
}

fn trim_to_500(value: &str) -> String {
    trim_chars(value, 500)
}

fn sha256_hex(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn db_error(error: sqlx::Error) -> String {
    format!("数据库操作失败: {error}")
}

fn safe_constraint_error(error: sqlx::Error, fallback: &str) -> String {
    match error {
        sqlx::Error::Database(_) => fallback.to_string(),
        other => db_error(other),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn pool() -> SqlitePool {
        crate::db::init_pool(":memory:").await.unwrap()
    }

    async fn seed_case(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO cases (id, name, case_type, case_status, source_folder, created_at, updated_at) VALUES (?1, '测试案', '刑事', 'active', ?2, datetime('now'), datetime('now'))",
        )
        .bind(id)
        .bind(format!("C:/test/{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    fn input() -> CreateMemoryInput {
        CreateMemoryInput {
            memory_type: "fact".to_string(),
            title: "关键日期".to_string(),
            content: "2026-07-01 收到起诉书。".to_string(),
            verification_status: Some("unverified".to_string()),
            injection_mode: Some("manual_each_turn".to_string()),
            change_reason: None,
            source: Some(MemorySourceInput {
                source_type: "manual_assertion".to_string(),
                document_id: None,
                chat_message_id: None,
                locator: None,
                excerpt: None,
                external_ref: None,
                verification_status: None,
            }),
        }
    }

    #[tokio::test]
    async fn case_memory_isolation_and_confirmation_are_fail_closed() {
        let pool = pool().await;
        seed_case(&pool, "case-a").await;
        seed_case(&pool, "case-b").await;
        let draft = create_case_memory_draft(&pool, "case-a", input(), "lawyer")
            .await
            .unwrap();
        assert_eq!(draft.status, "draft");
        assert!(get_case_memory(&pool, "case-b", &draft.id).await.is_err());
        let active = confirm_case_memory(&pool, "case-a", &draft.id, 1, "lawyer")
            .await
            .unwrap();
        assert_eq!(active.active_revision_no, Some(1));
        assert!(confirm_case_memory(&pool, "case-a", &draft.id, 2, "lawyer")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn verified_memory_requires_a_verified_locatable_case_source() {
        let pool = pool().await;
        seed_case(&pool, "case-verified").await;
        let mut draft_input = input();
        draft_input.verification_status = Some("verified".to_string());
        draft_input.source = Some(MemorySourceInput {
            source_type: "manual_assertion".to_string(),
            document_id: None,
            chat_message_id: None,
            locator: Some("memory-ui".to_string()),
            excerpt: None,
            external_ref: None,
            verification_status: Some("verified".to_string()),
        });
        let error = create_case_memory_draft(&pool, "case-verified", draft_input, "lawyer")
            .await
            .unwrap_err();
        assert!(error.contains("可定位"));
    }

    #[tokio::test]
    async fn candidate_never_becomes_active_without_two_human_gates() {
        let pool = pool().await;
        seed_case(&pool, "case-a").await;
        let candidate = create_memory_candidate(
            &pool,
            "case-a",
            CreateCandidateInput {
                proposed_type: "risk_warning".to_string(),
                proposed_title: "证据矛盾".to_string(),
                proposed_content: "两份笔录日期不一致。".to_string(),
                proposed_by_type: "assistant".to_string(),
                source_message_id: None,
            },
        )
        .await
        .unwrap();
        let draft = accept_memory_candidate(
            &pool,
            "case-a",
            &candidate.id,
            AcceptCandidateInput {
                title: "证据矛盾".to_string(),
                content: "两份笔录日期不一致，待核原件。".to_string(),
                memory_type: "risk_warning".to_string(),
                verification_status: None,
                source: None,
            },
            "lawyer",
        )
        .await
        .unwrap();
        assert_eq!(draft.status, "draft");
        let previews =
            preview_memory_injection(&pool, "case-a", None, vec![draft.id], vec![]).await;
        assert!(previews.is_err());
    }

    #[tokio::test]
    async fn confirmed_preview_rejects_cross_case_and_version_drift() {
        let pool = pool().await;
        seed_case(&pool, "case-a").await;
        seed_case(&pool, "case-b").await;
        let draft = create_case_memory_draft(&pool, "case-a", input(), "lawyer")
            .await
            .unwrap();
        let active = confirm_case_memory(&pool, "case-a", &draft.id, 1, "lawyer")
            .await
            .unwrap();
        let preview =
            preview_memory_injection(&pool, "case-a", None, vec![active.id.clone()], vec![])
                .await
                .unwrap();
        assert!(confirm_memory_injection(
            &pool,
            "case-b",
            &preview.id,
            &preview.preview_sha256,
            "lawyer"
        )
        .await
        .is_err());
        confirm_memory_injection(
            &pool,
            "case-a",
            &preview.id,
            &preview.preview_sha256,
            "lawyer",
        )
        .await
        .unwrap();
        revise_case_memory(
            &pool,
            "case-a",
            &active.id,
            ReviseMemoryInput {
                expected_revision: 1,
                title: "关键日期修订".to_string(),
                content: "2026-07-02 收到起诉书。".to_string(),
                change_reason: "核对原件".to_string(),
                source: None,
            },
            "lawyer",
        )
        .await
        .unwrap();
        // 旧 active revision 仍可使用，直到新 revision 被人工确认。
        assert!(
            load_confirmed_injection(&pool, "case-a", &preview.id, &preview.preview_sha256)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn active_memory_inserts_are_blocked_and_preference_confirmation_is_revision_gated() {
        let pool = pool().await;
        seed_case(&pool, "case-active-guard").await;
        let direct_case_memory = sqlx::query(
            r#"
            INSERT INTO case_memory_items (
                id, case_id, memory_type, status, verification_status,
                injection_mode, current_revision_no, active_revision_no,
                created_by_type, confirmed_by, confirmed_at
            ) VALUES (
                'direct-active', 'case-active-guard', 'fact', 'active', 'verified',
                'archive_only', 1, 1, 'user', 'lawyer', datetime('now')
            )
            "#,
        )
        .execute(&pool)
        .await;
        assert!(direct_case_memory.is_err());

        let direct_preference = sqlx::query(
            r#"
            INSERT INTO user_memory_preferences (
                id, title, status, injection_mode, current_revision_no,
                confirmed_by, confirmed_at
            ) VALUES (
                'direct-preference', '直接激活', 'active', 'archive_only', 1,
                'lawyer', datetime('now')
            )
            "#,
        )
        .execute(&pool)
        .await;
        assert!(direct_preference.is_err());

        let draft = create_user_memory_preference(
            &pool,
            CreatePreferenceInput {
                title: "回答偏好".to_string(),
                content: "先给结论，再列明依据。".to_string(),
                injection_mode: Some("archive_only".to_string()),
            },
            "lawyer",
        )
        .await
        .unwrap();
        assert_eq!(draft.status, "draft");
        let active = confirm_user_memory_preference(&pool, &draft.id, 1, "lawyer")
            .await
            .unwrap();
        assert_eq!(active.status, "active");
    }
}
