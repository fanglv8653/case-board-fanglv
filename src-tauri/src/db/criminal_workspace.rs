//! 刑辩五区工作台的持久化边界。
//! 所有写操作先校验案件领域；AI/Codex 产物只能以 pending_review 入库。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::io::Read;
use uuid::Uuid;

fn coded(code: &str, message: impl AsRef<str>) -> String {
    format!("{code}: {}", message.as_ref())
}

async fn require_criminal(pool: &SqlitePool, case_id: &str) -> Result<(), String> {
    let domain: Option<String> = sqlx::query_scalar("SELECT legal_domain FROM cases WHERE id=?")
        .bind(case_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    match domain.as_deref() {
        None => Err(coded("CASE_NOT_FOUND", "案件不存在")),
        Some("criminal") => Ok(()),
        _ => Err(coded("DOMAIN_MISMATCH", "只有刑事案件可以使用刑辩工作台")),
    }
}

fn require_revision(actual: i64, expected: i64) -> Result<(), String> {
    if actual == expected {
        Ok(())
    } else {
        Err(coded(
            "REVISION_CONFLICT",
            format!("当前版本为 {actual}，提交版本为 {expected}"),
        ))
    }
}

fn normalized_review_status(
    origin: &str,
    requested: Option<&str>,
    allow_draft: bool,
) -> Result<&'static str, String> {
    if origin != "user" {
        return Ok("pending_review");
    }
    match requested.unwrap_or(if allow_draft {
        "draft"
    } else {
        "pending_review"
    }) {
        "draft" if allow_draft => Ok("draft"),
        "pending_review" => Ok("pending_review"),
        _ => Err(coded("REVIEW_REQUIRED", "新建内容不能直接确认为律师成果")),
    }
}

fn validate_assessment_json(value: Option<&str>) -> Result<(), String> {
    let Some(value) = value else {
        return Ok(());
    };
    let parsed: Value = serde_json::from_str(value)
        .map_err(|_| coded("SOURCE_CITATION_INVALID", "证据审查字段必须是合法 JSON"))?;
    fn walk(v: &Value) -> bool {
        match v {
            Value::Object(map) => map.iter().all(|(key, value)| {
                if key == "status" {
                    value.as_str().is_some_and(|s| {
                        matches!(s, "supported" | "doubtful" | "adverse" | "not_reviewed")
                    })
                } else {
                    walk(value)
                }
            }),
            Value::Array(items) => items.iter().all(walk),
            _ => true,
        }
    }
    if parsed.get("status").and_then(Value::as_str).is_some() && walk(&parsed) {
        Ok(())
    } else {
        Err(coded(
            "SOURCE_CITATION_INVALID",
            "证据审查 status 仅允许 supported/doubtful/adverse/not_reviewed",
        ))
    }
}

async fn require_record_case(
    pool: &SqlitePool,
    table: &str,
    id: &str,
    case_id: &str,
) -> Result<(), String> {
    let sql = format!("SELECT COUNT(*) FROM {table} WHERE id=? AND case_id=?");
    let count: i64 = sqlx::query_scalar(&sql)
        .bind(id)
        .bind(case_id)
        .fetch_one(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    if count == 1 {
        Ok(())
    } else {
        Err(coded("CASE_NOT_FOUND", "记录不存在或不属于本案"))
    }
}

async fn require_artifact_case(
    pool: &SqlitePool,
    kind: &str,
    id: &str,
    case_id: &str,
) -> Result<(), String> {
    let table = match kind {
        "review_note" => "criminal_review_notes",
        "evidence" => "criminal_evidence_items",
        "issue" => "criminal_issues",
        "finding" => "criminal_analysis_findings",
        "draft_version" => "criminal_draft_versions",
        _ => return Err(coded("SOURCE_CITATION_INVALID", "不支持的成果类型")),
    };
    require_record_case(pool, table, id, case_id).await
}

async fn require_citation_owner(
    pool: &SqlitePool,
    owner_type: &str,
    owner_id: &str,
    case_id: &str,
) -> Result<(), String> {
    let kind = match owner_type {
        "issue_link" => {
            require_record_case(pool, "criminal_issue_evidence_links", owner_id, case_id).await?;
            return Ok(());
        }
        other => other,
    };
    require_artifact_case(pool, kind, owner_id, case_id).await
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ReviewNote {
    pub id: String,
    pub case_id: String,
    pub document_id: Option<String>,
    pub title: String,
    pub content: String,
    pub note_type: String,
    pub review_status: String,
    pub author_type: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertReviewNoteInput {
    pub id: Option<String>,
    pub case_id: String,
    pub document_id: Option<String>,
    pub title: String,
    pub content: String,
    pub note_type: Option<String>,
    pub review_status: Option<String>,
    pub author_type: Option<String>,
    pub expected_revision: Option<i64>,
}

#[tauri::command]
pub async fn list_criminal_review_notes(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    document_id: Option<String>,
    review_status: Option<String>,
) -> Result<Vec<ReviewNote>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_review_notes WHERE case_id=? AND deleted_at IS NULL AND (? IS NULL OR document_id=?) AND (? IS NULL OR review_status=?) ORDER BY updated_at DESC")
  .bind(&case_id).bind(&document_id).bind(&document_id).bind(&review_status).bind(&review_status).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))
}

pub async fn save_review_note(
    pool: &SqlitePool,
    input: UpsertReviewNoteInput,
) -> Result<ReviewNote, String> {
    require_criminal(pool, &input.case_id).await?;
    if input.title.trim().is_empty() {
        return Err(coded("SOURCE_CITATION_INVALID", "笔记标题不能为空"));
    }
    let author = input.author_type.as_deref().unwrap_or("user");
    let status = normalized_review_status(author, input.review_status.as_deref(), true)?;
    let row_id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(id) = input.id.as_deref() {
        require_record_case(pool, "criminal_review_notes", id, &input.case_id).await?;
        let current: ReviewNote =
            sqlx::query_as("SELECT * FROM criminal_review_notes WHERE id=? AND deleted_at IS NULL")
                .bind(id)
                .fetch_optional(pool)
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?
                .ok_or_else(|| coded("CASE_NOT_FOUND", "笔记不存在"))?;
        require_revision(
            current.revision,
            input
                .expected_revision
                .ok_or_else(|| coded("REVISION_CONFLICT", "缺少 expected_revision"))?,
        )?;
        sqlx::query("UPDATE criminal_review_notes SET document_id=?,title=?,content=?,note_type=?,review_status=?,author_type=?,reviewed_by=NULL,reviewed_at=NULL,review_note=NULL,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?")
   .bind(input.document_id).bind(input.title.trim()).bind(input.content).bind(input.note_type.as_deref().unwrap_or("general")).bind(status).bind(author).bind(id).bind(current.revision).execute(pool).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_review_notes(id,case_id,document_id,title,content,note_type,review_status,author_type) VALUES(?,?,?,?,?,?,?,?)")
   .bind(&row_id).bind(&input.case_id).bind(input.document_id).bind(input.title.trim()).bind(input.content).bind(input.note_type.as_deref().unwrap_or("general")).bind(status).bind(author).execute(pool).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    let row: ReviewNote = sqlx::query_as("SELECT * FROM criminal_review_notes WHERE id=?")
        .bind(row_id)
        .fetch_one(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    Ok(row)
}
#[tauri::command]
pub async fn upsert_criminal_review_note(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertReviewNoteInput,
) -> Result<ReviewNote, String> {
    save_review_note(pool.inner(), input).await
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct EvidenceItem {
    pub id: String,
    pub case_id: String,
    pub name: String,
    pub evidence_type: String,
    pub proof_purpose: String,
    pub source_description: String,
    pub originality_status: String,
    pub authenticity_assessment_json: String,
    pub legality_assessment_json: String,
    pub relevance_assessment_json: String,
    pub admissibility_assessment_json: String,
    pub probative_force_assessment_json: String,
    pub corroboration_assessment_json: String,
    pub exclusion_clue_assessment_json: String,
    pub reasonable_doubt_impact_json: String,
    pub review_status: String,
    pub origin: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertEvidenceInput {
    pub id: Option<String>,
    pub case_id: String,
    pub name: String,
    pub evidence_type: Option<String>,
    pub proof_purpose: Option<String>,
    pub source_description: Option<String>,
    pub originality_status: Option<String>,
    pub authenticity_assessment_json: Option<String>,
    pub legality_assessment_json: Option<String>,
    pub relevance_assessment_json: Option<String>,
    pub admissibility_assessment_json: Option<String>,
    pub probative_force_assessment_json: Option<String>,
    pub corroboration_assessment_json: Option<String>,
    pub exclusion_clue_assessment_json: Option<String>,
    pub reasonable_doubt_impact_json: Option<String>,
    pub origin: Option<String>,
    pub expected_revision: Option<i64>,
}
const EVIDENCE_SELECT: &str = "SELECT * FROM criminal_evidence_items";
#[tauri::command]
pub async fn list_criminal_evidence_items(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<Vec<EvidenceItem>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as(&format!(
        "{EVIDENCE_SELECT} WHERE case_id=? AND deleted_at IS NULL ORDER BY updated_at DESC"
    ))
    .bind(case_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn upsert_criminal_evidence_item(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertEvidenceInput,
) -> Result<EvidenceItem, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    for field in [
        &input.authenticity_assessment_json,
        &input.legality_assessment_json,
        &input.relevance_assessment_json,
        &input.admissibility_assessment_json,
        &input.probative_force_assessment_json,
        &input.corroboration_assessment_json,
        &input.exclusion_clue_assessment_json,
        &input.reasonable_doubt_impact_json,
    ] {
        validate_assessment_json(field.as_deref())?;
    }
    let origin = input.origin.as_deref().unwrap_or("user");
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(expected) = input.expected_revision {
        require_record_case(pool.inner(), "criminal_evidence_items", &id, &input.case_id).await?;
        let actual: i64 =
            sqlx::query_scalar("SELECT revision FROM criminal_evidence_items WHERE id=?")
                .bind(&id)
                .fetch_one(pool.inner())
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
        require_revision(actual, expected)?;
        sqlx::query("UPDATE criminal_evidence_items SET name=?,evidence_type=?,proof_purpose=?,source_description=?,originality_status=?,authenticity_assessment_json=?,legality_assessment_json=?,relevance_assessment_json=?,admissibility_assessment_json=?,probative_force_assessment_json=?,corroboration_assessment_json=?,exclusion_clue_assessment_json=?,reasonable_doubt_impact_json=?,review_status='pending_review',origin=?,reviewed_by=NULL,reviewed_at=NULL,review_note=NULL,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?")
 .bind(&input.name).bind(input.evidence_type.as_deref().unwrap_or("other")).bind(input.proof_purpose.as_deref().unwrap_or("")).bind(input.source_description.as_deref().unwrap_or("")).bind(input.originality_status.as_deref().unwrap_or("not_reviewed")).bind(input.authenticity_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.legality_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.relevance_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.admissibility_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.probative_force_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.corroboration_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.exclusion_clue_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.reasonable_doubt_impact_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(origin).bind(&id).bind(expected).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_evidence_items(id,case_id,name,evidence_type,proof_purpose,source_description,originality_status,authenticity_assessment_json,legality_assessment_json,relevance_assessment_json,admissibility_assessment_json,probative_force_assessment_json,corroboration_assessment_json,exclusion_clue_assessment_json,reasonable_doubt_impact_json,review_status,origin) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
 .bind(&id).bind(&input.case_id).bind(&input.name).bind(input.evidence_type.as_deref().unwrap_or("other")).bind(input.proof_purpose.as_deref().unwrap_or("")).bind(input.source_description.as_deref().unwrap_or("")).bind(input.originality_status.as_deref().unwrap_or("not_reviewed")).bind(input.authenticity_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.legality_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.relevance_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.admissibility_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.probative_force_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.corroboration_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.exclusion_clue_assessment_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind(input.reasonable_doubt_impact_json.as_deref().unwrap_or("{\"status\":\"not_reviewed\"}")).bind("pending_review").bind(origin).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    let row: EvidenceItem = sqlx::query_as(&format!("{EVIDENCE_SELECT} WHERE id=?"))
        .bind(&id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    Ok(row)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Issue {
    pub id: String,
    pub case_id: String,
    pub issue_type: String,
    pub neutral_title: String,
    pub description: String,
    pub status: String,
    pub position: String,
    pub origin: String,
    pub review_status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertIssueInput {
    pub id: Option<String>,
    pub case_id: String,
    pub issue_type: String,
    pub neutral_title: String,
    pub description: Option<String>,
    pub status: Option<String>,
    pub position: Option<String>,
    pub origin: Option<String>,
    pub expected_revision: Option<i64>,
}
#[tauri::command]
pub async fn list_criminal_issues(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<Vec<Issue>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_issues WHERE case_id=? AND deleted_at IS NULL ORDER BY updated_at DESC").bind(case_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))
}
#[tauri::command]
pub async fn upsert_criminal_issue(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertIssueInput,
) -> Result<Issue, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let origin = input.origin.as_deref().unwrap_or("user");
    if let Some(expected) = input.expected_revision {
        require_record_case(pool.inner(), "criminal_issues", &id, &input.case_id).await?;
        let actual: i64 = sqlx::query_scalar("SELECT revision FROM criminal_issues WHERE id=?")
            .bind(&id)
            .fetch_one(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
        require_revision(actual, expected)?;
        sqlx::query("UPDATE criminal_issues SET issue_type=?,neutral_title=?,description=?,status=?,position=?,origin=?,review_status='pending_review',reviewed_by=NULL,reviewed_at=NULL,review_note=NULL,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&input.issue_type).bind(&input.neutral_title).bind(input.description.as_deref().unwrap_or("")).bind(input.status.as_deref().unwrap_or("open")).bind(input.position.as_deref().unwrap_or("neutral")).bind(origin).bind(&id).bind(expected).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_issues(id,case_id,issue_type,neutral_title,description,status,position,origin,review_status) VALUES(?,?,?,?,?,?,?,?, 'pending_review')").bind(&id).bind(&input.case_id).bind(&input.issue_type).bind(&input.neutral_title).bind(input.description.as_deref().unwrap_or("")).bind(input.status.as_deref().unwrap_or("open")).bind(input.position.as_deref().unwrap_or("neutral")).bind(origin).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    sqlx::query_as("SELECT * FROM criminal_issues WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IssueEvidenceLink {
    pub id: String,
    pub case_id: String,
    pub issue_id: String,
    pub evidence_id: Option<String>,
    pub relation: String,
    pub explanation: String,
    pub origin: String,
    pub review_status: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertIssueEvidenceLinkInput {
    pub id: Option<String>,
    pub case_id: String,
    pub issue_id: String,
    pub evidence_id: Option<String>,
    pub relation: String,
    pub explanation: Option<String>,
    pub origin: Option<String>,
    pub expected_revision: Option<i64>,
}
#[tauri::command]
pub async fn upsert_criminal_issue_evidence_link(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertIssueEvidenceLinkInput,
) -> Result<IssueEvidenceLink, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    require_record_case(
        pool.inner(),
        "criminal_issues",
        &input.issue_id,
        &input.case_id,
    )
    .await?;
    if let Some(e) = input.evidence_id.as_deref() {
        require_record_case(pool.inner(), "criminal_evidence_items", e, &input.case_id).await?;
    }
    if (input.relation == "gap") != input.evidence_id.is_none() {
        return Err(coded(
            "SOURCE_CITATION_INVALID",
            "gap 必须无 evidence_id，其他关系必须有 evidence_id",
        ));
    }
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let origin = input.origin.as_deref().unwrap_or("user");
    if let Some(expected) = input.expected_revision {
        require_record_case(
            pool.inner(),
            "criminal_issue_evidence_links",
            &id,
            &input.case_id,
        )
        .await?;
        let actual: i64 =
            sqlx::query_scalar("SELECT revision FROM criminal_issue_evidence_links WHERE id=?")
                .bind(&id)
                .fetch_one(pool.inner())
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
        require_revision(actual, expected)?;
        sqlx::query("UPDATE criminal_issue_evidence_links SET issue_id=?,evidence_id=?,relation=?,explanation=?,origin=?,review_status='pending_review',reviewed_by=NULL,reviewed_at=NULL,review_note=NULL,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&input.issue_id).bind(&input.evidence_id).bind(&input.relation).bind(input.explanation.as_deref().unwrap_or("")).bind(origin).bind(&id).bind(expected).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_issue_evidence_links(id,case_id,issue_id,evidence_id,relation,explanation,origin,review_status) VALUES(?,?,?,?,?,?,?,'pending_review')").bind(&id).bind(&input.case_id).bind(&input.issue_id).bind(&input.evidence_id).bind(&input.relation).bind(input.explanation.as_deref().unwrap_or("")).bind(origin).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    sqlx::query_as("SELECT * FROM criminal_issue_evidence_links WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn list_criminal_issue_evidence_links(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    issue_id: Option<String>,
) -> Result<Vec<IssueEvidenceLink>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_issue_evidence_links WHERE case_id=? AND deleted_at IS NULL AND (? IS NULL OR issue_id=?) ORDER BY updated_at DESC").bind(case_id).bind(&issue_id).bind(&issue_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))
}
#[tauri::command]
pub async fn review_criminal_issue_evidence_link(
    pool: tauri::State<'_, SqlitePool>,
    input: ReviewRecordInput,
) -> Result<Value, String> {
    review_record(
        pool.inner(),
        "criminal_issue_evidence_links",
        "issue_link",
        &input,
        false,
    )
    .await
}

async fn soft_delete_record(
    pool: &SqlitePool,
    table: &str,
    id: &str,
    expected: i64,
) -> Result<(), String> {
    let sql = format!("SELECT case_id,revision FROM {table} WHERE id=? AND deleted_at IS NULL");
    let (case_id, actual): (String, i64) = sqlx::query_as(&sql)
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?
        .ok_or_else(|| coded("CASE_NOT_FOUND", "记录不存在"))?;
    require_criminal(pool, &case_id).await?;
    require_revision(actual, expected)?;
    let update=format!("UPDATE {table} SET deleted_at=datetime('now'),revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?");
    sqlx::query(&update)
        .bind(id)
        .bind(expected)
        .execute(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    Ok(())
}
#[tauri::command]
pub async fn delete_criminal_review_note(
    pool: tauri::State<'_, SqlitePool>,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    soft_delete_record(
        pool.inner(),
        "criminal_review_notes",
        &id,
        expected_revision,
    )
    .await
}
#[tauri::command]
pub async fn delete_criminal_evidence_item(
    pool: tauri::State<'_, SqlitePool>,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    soft_delete_record(
        pool.inner(),
        "criminal_evidence_items",
        &id,
        expected_revision,
    )
    .await
}
#[tauri::command]
pub async fn delete_criminal_issue(
    pool: tauri::State<'_, SqlitePool>,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    soft_delete_record(pool.inner(), "criminal_issues", &id, expected_revision).await
}
#[tauri::command]
pub async fn delete_criminal_issue_evidence_link(
    pool: tauri::State<'_, SqlitePool>,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    soft_delete_record(
        pool.inner(),
        "criminal_issue_evidence_links",
        &id,
        expected_revision,
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Finding {
    pub id: String,
    pub case_id: String,
    pub run_id: Option<String>,
    pub finding_type: String,
    pub title: String,
    pub content: String,
    pub confidence: Option<f64>,
    pub review_status: String,
    pub origin: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertFindingInput {
    pub id: Option<String>,
    pub case_id: String,
    pub run_id: Option<String>,
    pub finding_type: String,
    pub title: String,
    pub content: String,
    pub confidence: Option<f64>,
    pub origin: Option<String>,
    pub expected_revision: Option<i64>,
}
#[tauri::command]
pub async fn list_criminal_analysis_findings(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    run_id: Option<String>,
) -> Result<Vec<Finding>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_analysis_findings WHERE case_id=? AND deleted_at IS NULL AND (? IS NULL OR run_id=?) ORDER BY created_at DESC").bind(case_id).bind(&run_id).bind(&run_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))
}
#[tauri::command]
pub async fn upsert_criminal_analysis_finding(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertFindingInput,
) -> Result<Finding, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let origin = input.origin.as_deref().unwrap_or("user");
    if let Some(expected) = input.expected_revision {
        require_record_case(
            pool.inner(),
            "criminal_analysis_findings",
            &id,
            &input.case_id,
        )
        .await?;
        let actual: i64 =
            sqlx::query_scalar("SELECT revision FROM criminal_analysis_findings WHERE id=?")
                .bind(&id)
                .fetch_one(pool.inner())
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
        require_revision(actual, expected)?;
        sqlx::query("UPDATE criminal_analysis_findings SET finding_type=?,title=?,content=?,confidence=?,origin=?,review_status='pending_review',reviewed_by=NULL,reviewed_at=NULL,review_note=NULL,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&input.finding_type).bind(&input.title).bind(&input.content).bind(input.confidence).bind(origin).bind(&id).bind(expected).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_analysis_findings(id,case_id,run_id,finding_type,title,content,confidence,review_status,origin) VALUES(?,?,?,?,?,?,?,'pending_review',?)").bind(&id).bind(&input.case_id).bind(input.run_id).bind(&input.finding_type).bind(&input.title).bind(&input.content).bind(input.confidence).bind(origin).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    sqlx::query_as("SELECT * FROM criminal_analysis_findings WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRecordInput {
    pub id: String,
    pub decision: String,
    pub actor: String,
    pub note: Option<String>,
    pub expected_revision: i64,
}
async fn review_record(
    pool: &SqlitePool,
    table: &str,
    _kind: &str,
    input: &ReviewRecordInput,
    material_fact: bool,
) -> Result<Value, String> {
    let query = format!(
        "SELECT case_id,review_status,revision FROM {table} WHERE id=? AND deleted_at IS NULL"
    );
    let (case_id, from, revision): (String, String, i64) = sqlx::query_as(&query)
        .bind(&input.id)
        .fetch_optional(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?
        .ok_or_else(|| coded("CASE_NOT_FOUND", "记录不存在"))?;
    require_criminal(pool, &case_id).await?;
    require_revision(revision, input.expected_revision)?;
    let to = match input.decision.as_str() {
        "confirm" => "confirmed",
        "reject" => "rejected",
        "reopen" => "pending_review",
        _ => {
            return Err(coded(
                "INVALID_STATE_TRANSITION",
                "审核 decision 仅允许 confirm/reject/reopen",
            ))
        }
    };
    let allowed = matches!(
        (from.as_str(), input.decision.as_str()),
        ("pending_review", "confirm") | ("pending_review", "reject") | ("rejected", "reopen")
    );
    if !allowed {
        return Err(coded(
            "INVALID_STATE_TRANSITION",
            format!("不允许从 {from} 执行 {}", input.decision),
        ));
    }
    if input.actor.trim().is_empty() {
        return Err(coded("REVIEW_REQUIRED", "律师审核人不能为空"));
    }
    if input.decision == "confirm" && material_fact {
        let valid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_source_citations WHERE owner_type='finding' AND owner_id=? AND citation_kind='material' AND integrity_status='valid' AND deleted_at IS NULL").bind(&input.id).fetch_one(pool).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        if valid == 0 {
            return Err(coded(
                "SOURCE_CITATION_INVALID",
                "材料事实至少需要一个有效材料引用",
            ));
        }
    }
    let update=format!("UPDATE {table} SET review_status=?,reviewed_by=?,reviewed_at=datetime('now'),review_note=?,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?");
    let result = sqlx::query(&update)
        .bind(to)
        .bind(input.actor.trim())
        .bind(&input.note)
        .bind(&input.id)
        .bind(revision)
        .execute(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    if result.rows_affected() != 1 {
        return Err(coded("REVISION_CONFLICT", "记录已被其他窗口修改"));
    }
    Ok(json!({"id":input.id,"review_status":to,"revision":revision+1}))
}
#[tauri::command]
pub async fn review_criminal_review_note(
    pool: tauri::State<'_, SqlitePool>,
    input: ReviewRecordInput,
) -> Result<Value, String> {
    review_record(
        pool.inner(),
        "criminal_review_notes",
        "review_note",
        &input,
        false,
    )
    .await
}
#[tauri::command]
pub async fn review_criminal_evidence_item(
    pool: tauri::State<'_, SqlitePool>,
    input: ReviewRecordInput,
) -> Result<Value, String> {
    review_record(
        pool.inner(),
        "criminal_evidence_items",
        "evidence",
        &input,
        false,
    )
    .await
}
#[tauri::command]
pub async fn review_criminal_issue(
    pool: tauri::State<'_, SqlitePool>,
    input: ReviewRecordInput,
) -> Result<Value, String> {
    review_record(pool.inner(), "criminal_issues", "issue", &input, false).await
}
#[tauri::command]
pub async fn review_criminal_analysis_finding(
    pool: tauri::State<'_, SqlitePool>,
    input: ReviewRecordInput,
) -> Result<Value, String> {
    let material: Option<String> =
        sqlx::query_scalar("SELECT finding_type FROM criminal_analysis_findings WHERE id=?")
            .bind(&input.id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    if input.decision == "confirm" && material.as_deref() == Some("legal_rule") {
        let verified:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_source_citations WHERE owner_type='finding' AND owner_id=? AND citation_kind='legal' AND verification_status='verified' AND deleted_at IS NULL").bind(&input.id).fetch_one(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        if verified == 0 {
            return Err(coded(
                "LEGAL_SOURCE_UNVERIFIED",
                "法律依据必须具备已核验法源引用",
            ));
        }
    }
    review_record(
        pool.inner(),
        "criminal_analysis_findings",
        "finding",
        &input,
        material.as_deref() == Some("material_fact"),
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Citation {
    pub id: String,
    pub case_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub citation_kind: String,
    pub document_id: Option<String>,
    pub source_filename_snapshot: Option<String>,
    pub source_path_snapshot: Option<String>,
    pub source_fingerprint: Option<String>,
    pub page_start: Option<i64>,
    pub page_end: Option<i64>,
    pub locator_json: String,
    pub location_precision: String,
    pub excerpt: String,
    pub legal_title: Option<String>,
    pub legal_article: Option<String>,
    pub legal_url: Option<String>,
    pub verification_status: String,
    pub integrity_status: String,
    pub checked_at: Option<String>,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpsertCitationInput {
    pub id: Option<String>,
    pub case_id: String,
    pub owner_type: String,
    pub owner_id: String,
    pub citation_kind: String,
    pub document_id: Option<String>,
    pub page_start: Option<i64>,
    pub page_end: Option<i64>,
    pub locator_json: Option<String>,
    pub location_precision: Option<String>,
    pub excerpt: Option<String>,
    pub legal_title: Option<String>,
    pub legal_article: Option<String>,
    pub legal_url: Option<String>,
    pub verification_status: Option<String>,
    pub expected_revision: Option<i64>,
}
#[tauri::command]
pub async fn list_criminal_source_citations(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    owner_type: Option<String>,
    owner_id: Option<String>,
) -> Result<Vec<Citation>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_source_citations WHERE case_id=? AND deleted_at IS NULL AND (? IS NULL OR owner_type=?) AND (? IS NULL OR owner_id=?) ORDER BY created_at")
        .bind(case_id).bind(&owner_type).bind(&owner_type).bind(&owner_id).bind(&owner_id)
        .fetch_all(pool.inner()).await.map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn delete_criminal_source_citation(
    pool: tauri::State<'_, SqlitePool>,
    id: String,
    expected_revision: i64,
) -> Result<(), String> {
    soft_delete_record(
        pool.inner(),
        "criminal_source_citations",
        &id,
        expected_revision,
    )
    .await
}
fn fingerprint(path: &str) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Some(format!("sha256:{:x}", digest.finalize()))
}
#[tauri::command]
pub async fn upsert_criminal_source_citation(
    pool: tauri::State<'_, SqlitePool>,
    input: UpsertCitationInput,
) -> Result<Citation, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    require_citation_owner(
        pool.inner(),
        &input.owner_type,
        &input.owner_id,
        &input.case_id,
    )
    .await?;
    let locator = input.locator_json.as_deref().unwrap_or("{}");
    if input.citation_kind == "material"
        && (input.document_id.is_none() || (input.page_start.is_none() && locator == "{}"))
    {
        return Err(coded(
            "SOURCE_CITATION_INVALID",
            "材料引用必须包含文档及页码或定位信息",
        ));
    }
    if input.citation_kind == "legal"
        && input
            .legal_title
            .as_deref()
            .is_none_or(|v| v.trim().is_empty())
    {
        return Err(coded("SOURCE_CITATION_INVALID", "法律引用必须包含法源标题"));
    }
    let (filename, path, fp, integrity) = if let Some(doc) = input.document_id.as_deref() {
        let row:Option<(String,String,bool,Option<String>)>=sqlx::query_as("SELECT filename,source_path,missing,cache_key FROM documents WHERE id=? AND case_id=? AND deleted_at IS NULL").bind(doc).bind(&input.case_id).fetch_optional(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        let (f, p, m, cache) =
            row.ok_or_else(|| coded("SOURCE_CITATION_INVALID", "来源文档不存在或不属于本案"))?;
        let disk = fingerprint(&p).or(cache);
        let state = if m || !std::path::Path::new(&p).exists() {
            "missing"
        } else {
            "valid"
        };
        (Some(f), Some(p), disk, state)
    } else {
        (None, None, None, "valid")
    };
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if let Some(expected) = input.expected_revision {
        require_record_case(
            pool.inner(),
            "criminal_source_citations",
            &id,
            &input.case_id,
        )
        .await?;
        let actual: i64 =
            sqlx::query_scalar("SELECT revision FROM criminal_source_citations WHERE id=?")
                .bind(&id)
                .fetch_one(pool.inner())
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
        require_revision(actual, expected)?;
        sqlx::query("UPDATE criminal_source_citations SET document_id=?,source_filename_snapshot=?,source_path_snapshot=?,source_fingerprint=?,page_start=?,page_end=?,locator_json=?,location_precision=?,excerpt=?,legal_title=?,legal_article=?,legal_url=?,verification_status=?,integrity_status=?,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&input.document_id).bind(filename).bind(path).bind(fp).bind(input.page_start).bind(input.page_end).bind(locator).bind(input.location_precision.as_deref().unwrap_or("exact")).bind(input.excerpt.as_deref().unwrap_or("")).bind(&input.legal_title).bind(&input.legal_article).bind(&input.legal_url).bind(input.verification_status.as_deref().unwrap_or("unchecked")).bind(integrity).bind(&id).bind(expected).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    } else {
        sqlx::query("INSERT INTO criminal_source_citations(id,case_id,owner_type,owner_id,citation_kind,document_id,source_filename_snapshot,source_path_snapshot,source_fingerprint,page_start,page_end,locator_json,location_precision,excerpt,legal_title,legal_article,legal_url,verification_status,integrity_status) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)").bind(&id).bind(&input.case_id).bind(&input.owner_type).bind(&input.owner_id).bind(&input.citation_kind).bind(&input.document_id).bind(filename).bind(path).bind(fp).bind(input.page_start).bind(input.page_end).bind(locator).bind(input.location_precision.as_deref().unwrap_or("exact")).bind(input.excerpt.as_deref().unwrap_or("")).bind(&input.legal_title).bind(&input.legal_article).bind(&input.legal_url).bind(input.verification_status.as_deref().unwrap_or("unchecked")).bind(integrity).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    }
    sqlx::query_as("SELECT * FROM criminal_source_citations WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityRefreshReport {
    pub checked: i64,
    pub valid: i64,
    pub missing: i64,
    pub changed: i64,
}
type CitationIntegrityRow = (String, Option<String>, Option<String>, Option<String>);
#[tauri::command]
pub async fn refresh_criminal_source_integrity(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
    document_id: Option<String>,
) -> Result<IntegrityRefreshReport, String> {
    require_criminal(pool.inner(), &case_id).await?;
    let rows: Vec<CitationIntegrityRow> = sqlx::query_as("SELECT id,source_path_snapshot,source_fingerprint,document_id FROM criminal_source_citations WHERE case_id=? AND citation_kind='material' AND deleted_at IS NULL AND (? IS NULL OR document_id=?)").bind(&case_id).bind(&document_id).bind(&document_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    let mut report = IntegrityRefreshReport {
        checked: 0,
        valid: 0,
        missing: 0,
        changed: 0,
    };
    for (id, path, stored, doc) in rows {
        let current = if let Some(doc) = doc {
            let live:Option<(String,bool,Option<String>)>=sqlx::query_as("SELECT source_path,missing,cache_key FROM documents WHERE id=? AND deleted_at IS NULL").bind(doc).fetch_optional(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
            match live {
                None => "missing",
                Some((_p, true, _)) => "missing",
                Some((p, false, cache)) => {
                    if !std::path::Path::new(&p).exists() {
                        "missing"
                    } else if stored.is_some() && fingerprint(&p).or(cache) != stored {
                        "changed"
                    } else {
                        "valid"
                    }
                }
            }
        } else if path
            .as_deref()
            .is_some_and(|p| std::path::Path::new(p).exists())
        {
            "valid"
        } else {
            "missing"
        };
        sqlx::query("UPDATE criminal_source_citations SET integrity_status=?,checked_at=datetime('now'),updated_at=datetime('now') WHERE id=?").bind(current).bind(id).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        report.checked += 1;
        match current {
            "valid" => report.valid += 1,
            "changed" => report.changed += 1,
            _ => report.missing += 1,
        }
    }
    Ok(report)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AnalysisRun {
    pub id: String,
    pub case_id: String,
    pub template_code: String,
    pub template_version: i64,
    pub requested_provider: String,
    pub actual_provider: String,
    pub status: String,
    pub request_id: String,
    pub input_snapshot_json: String,
    pub fallback_from: Option<String>,
    pub fallback_to: Option<String>,
    pub fallback_reason: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisCapabilities {
    pub manual: bool,
    pub native_llm: ProviderCapability,
    pub codex: ProviderCapability,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderCapability {
    pub available: bool,
    pub experimental: bool,
    pub reason: Option<String>,
}
#[tauri::command]
pub async fn get_criminal_analysis_capabilities(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<AnalysisCapabilities, String> {
    require_criminal(pool.inner(), &case_id).await?;
    // An API key alone is not a provider health check. Keep the native adapter
    // unavailable until its structured workflow adapter is actually callable.
    let native = false;
    Ok(AnalysisCapabilities {
        manual: true,
        native_llm: ProviderCapability {
            available: native,
            experimental: false,
            reason: (!native).then(|| "未配置原生模型".into()),
        },
        codex: ProviderCapability {
            available: false,
            experimental: true,
            reason: Some("未探测到可调用的 Codex 适配器".into()),
        },
    })
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartAnalysisInput {
    pub case_id: String,
    pub request_id: String,
    pub template_code: String,
    pub requested_provider: String,
    pub input_snapshot_json: String,
    pub allow_fallback: Option<bool>,
}
#[tauri::command]
pub async fn start_criminal_analysis(
    pool: tauri::State<'_, SqlitePool>,
    input: StartAnalysisInput,
) -> Result<AnalysisRun, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    if let Some(existing) =
        sqlx::query_as::<_, AnalysisRun>("SELECT * FROM criminal_analysis_runs WHERE request_id=?")
            .bind(&input.request_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?
    {
        if existing.case_id != input.case_id {
            return Err(coded(
                "SOURCE_CITATION_INVALID",
                "request_id 已被其他案件使用",
            ));
        }
        return Ok(existing);
    }
    let caps = get_criminal_analysis_capabilities(pool.clone(), input.case_id.clone()).await?;
    let (requested, actual, fallback) = match input.requested_provider.as_str() {
        "manual" => ("manual", "manual_template", None),
        "native_llm" if caps.native_llm.available => ("native_llm", "native_llm", None),
        "codex" if caps.codex.available => ("codex", "codex", None),
        "codex" if input.allow_fallback.unwrap_or(true) && caps.native_llm.available => {
            ("codex", "native_llm", Some("Codex 不可用，降级到原生模型"))
        }
        "codex" | "native_llm" if input.allow_fallback.unwrap_or(true) => (
            input.requested_provider.as_str(),
            "manual_template",
            Some("模型不可用，降级到人工模板"),
        ),
        _ => {
            return Err(coded(
                "PROVIDER_UNAVAILABLE",
                "所选 provider 不可用且未允许降级",
            ))
        }
    };
    let id = Uuid::new_v4().to_string();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    sqlx::query("INSERT INTO criminal_analysis_runs(id,case_id,template_code,template_version,requested_provider,actual_provider,status,request_id,input_snapshot_json,fallback_from,fallback_to,fallback_reason,started_at,completed_at) VALUES(?,?,?,1,?,?, 'succeeded',?,?,?,?,?,datetime('now'),datetime('now'))").bind(&id).bind(&input.case_id).bind(&input.template_code).bind(requested).bind(actual).bind(&input.request_id).bind(&input.input_snapshot_json).bind(fallback.map(|_|requested)).bind(fallback.map(|_|actual)).bind(fallback).execute(&mut*tx).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    if actual == "manual_template" {
        for (kind, title) in [
            ("material_fact", "材料事实"),
            ("unverified_fact", "待核实事实"),
            ("legal_rule", "法律依据"),
            ("analysis", "分析判断"),
            ("defense_strategy", "辩护策略"),
        ] {
            sqlx::query("INSERT INTO criminal_analysis_findings(id,case_id,run_id,finding_type,title,content,review_status,origin) VALUES(?,?,?,?,?,'','pending_review','user')").bind(Uuid::new_v4().to_string()).bind(&input.case_id).bind(&id).bind(kind).bind(title).execute(&mut*tx).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        }
    }
    tx.commit()
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_analysis_runs WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn get_criminal_analysis_run(
    pool: tauri::State<'_, SqlitePool>,
    run_id: String,
) -> Result<AnalysisRun, String> {
    let run: AnalysisRun = sqlx::query_as("SELECT * FROM criminal_analysis_runs WHERE id=?")
        .bind(run_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &run.case_id).await?;
    Ok(run)
}
#[tauri::command]
pub async fn list_criminal_analysis_runs(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<Vec<AnalysisRun>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_analysis_runs WHERE case_id=? ORDER BY created_at DESC")
        .bind(case_id)
        .fetch_all(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DraftDocument {
    pub id: String,
    pub case_id: String,
    pub document_type: String,
    pub title: String,
    pub status: String,
    pub current_version_id: Option<String>,
    pub created_by: String,
    pub revision: i64,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DraftVersion {
    pub id: String,
    pub case_id: String,
    pub draft_id: String,
    pub version_no: i64,
    pub content_json: String,
    pub rendered_markdown: String,
    pub status: String,
    pub origin: String,
    pub source_snapshot_json: String,
    pub quality_report_json: String,
    pub reviewed_by: Option<String>,
    pub reviewed_at: Option<String>,
    pub review_note: Option<String>,
    pub approved_at: Option<String>,
    pub revision: i64,
    pub created_at: String,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDraftInput {
    pub case_id: String,
    pub document_type: String,
    pub title: String,
    pub created_by: String,
}

pub const CRIMINAL_DRAFT_DOCUMENT_TYPES: [&str; 13] = [
    "defense_statement",
    "evidence_objection",
    "hearing_questions",
    "first_meeting_record",
    "followup_meeting_record",
    "bail_application",
    "non_arrest_opinion",
    "custody_necessity_application",
    "prosecution_legal_opinion",
    "sentencing_opinion",
    "criminal_appeal",
    "evidence_list",
    "other",
];

fn validate_draft_document_type(document_type: &str) -> Result<&str, String> {
    let normalized = document_type.trim();
    if CRIMINAL_DRAFT_DOCUMENT_TYPES.contains(&normalized) {
        Ok(normalized)
    } else {
        Err(coded(
            "INVALID_STATE_TRANSITION",
            format!("不支持的刑事文书类型: {normalized}"),
        ))
    }
}

async fn create_draft_record(
    pool: &SqlitePool,
    input: CreateDraftInput,
) -> Result<DraftDocument, String> {
    require_criminal(pool, &input.case_id).await?;
    let document_type = validate_draft_document_type(&input.document_type)?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO criminal_draft_documents(id,case_id,document_type,title,created_by) VALUES(?,?,?,?,?)")
        .bind(&id).bind(&input.case_id).bind(document_type).bind(&input.title).bind(&input.created_by)
        .execute(pool).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}

#[tauri::command]
pub async fn create_criminal_draft(
    pool: tauri::State<'_, SqlitePool>,
    input: CreateDraftInput,
) -> Result<DraftDocument, String> {
    create_draft_record(pool.inner(), input).await
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateDraftVersionInput {
    pub draft_id: String,
    pub content_json: String,
    pub rendered_markdown: String,
    pub origin: Option<String>,
    pub source_snapshot_json: Option<String>,
}
#[tauri::command]
pub async fn create_criminal_draft_version(
    pool: tauri::State<'_, SqlitePool>,
    input: CreateDraftVersionInput,
) -> Result<DraftVersion, String> {
    let draft:DraftDocument=sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE id=? AND status='active' AND deleted_at IS NULL").bind(&input.draft_id).fetch_one(pool.inner()).await.map_err(|e|coded("CASE_NOT_FOUND",e.to_string()))?;
    require_criminal(pool.inner(), &draft.case_id).await?;
    let no: i64 = sqlx::query_scalar(
        "SELECT coalesce(max(version_no),0)+1 FROM criminal_draft_versions WHERE draft_id=?",
    )
    .bind(&input.draft_id)
    .fetch_one(pool.inner())
    .await
    .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    let id = Uuid::new_v4().to_string();
    let origin = input.origin.as_deref().unwrap_or("user");
    let status = if origin == "user" {
        "draft"
    } else {
        "pending_review"
    };
    sqlx::query("INSERT INTO criminal_draft_versions(id,case_id,draft_id,version_no,content_json,rendered_markdown,status,origin,source_snapshot_json) VALUES(?,?,?,?,?,?,?,?,?)").bind(&id).bind(&draft.case_id).bind(&input.draft_id).bind(no).bind(&input.content_json).bind(&input.rendered_markdown).bind(status).bind(origin).bind(input.source_snapshot_json.as_deref().unwrap_or("{}")).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
fn quality_gate_failures(markdown: &str, citations: &[(String, String, String)]) -> Vec<Value> {
    let mut failures = Vec::new();
    for marker in ["TODO", "待填写", "{{", "[["] {
        if markdown.contains(marker) {
            failures.push(json!({"code":"PLACEHOLDER","message":format!("正文含占位符 {marker}")}));
        }
    }
    for (_, kind, status) in citations {
        if kind == "legal" && status != "verified" {
            failures.push(json!({"code":"LEGAL_SOURCE_UNVERIFIED","message":"存在未核验法律依据"}));
        }
        if status == "missing" || status == "changed" {
            failures.push(json!({"code":"SOURCE_CITATION_INVALID","message":"存在失效材料引用"}));
        }
    }
    failures
}
fn snapshot_ids(snapshot: &str, key: &str) -> Vec<String> {
    serde_json::from_str::<Value>(snapshot)
        .ok()
        .and_then(|v| v.get(key).cloned())
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApproveDraftInput {
    pub version_id: String,
    pub actor: String,
    pub review_note: Option<String>,
    pub expected_revision: i64,
}
#[tauri::command]
pub async fn submit_criminal_draft_version_for_review(
    pool: tauri::State<'_, SqlitePool>,
    version_id: String,
    expected_revision: i64,
) -> Result<DraftVersion, String> {
    let row: DraftVersion = sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(&version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &row.case_id).await?;
    require_revision(row.revision, expected_revision)?;
    if row.status != "draft" {
        return Err(coded("INVALID_STATE_TRANSITION", "只有草稿可以提交审核"));
    }
    sqlx::query("UPDATE criminal_draft_versions SET status='pending_review',revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&version_id).bind(expected_revision).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn approve_criminal_draft_version(
    pool: tauri::State<'_, SqlitePool>,
    input: ApproveDraftInput,
) -> Result<DraftVersion, String> {
    let row: DraftVersion = sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(&input.version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &row.case_id).await?;
    require_revision(row.revision, input.expected_revision)?;
    if row.status != "pending_review" || input.actor.trim().is_empty() {
        return Err(coded("REVIEW_REQUIRED", "文书必须先提交并由明确律师审核"));
    }
    let citations:Vec<(String,String,String)>=sqlx::query_as("SELECT id,citation_kind,CASE WHEN citation_kind='legal' THEN verification_status ELSE integrity_status END FROM criminal_source_citations WHERE owner_type='draft_version' AND owner_id=? AND deleted_at IS NULL").bind(&input.version_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    let failures = quality_gate_failures(&row.rendered_markdown, &citations);
    let mut failures = failures;
    let evidence_ids = snapshot_ids(&row.source_snapshot_json, "evidence_ids");
    let finding_ids = snapshot_ids(&row.source_snapshot_json, "finding_ids");
    if evidence_ids.is_empty() && finding_ids.is_empty() {
        failures.push(json!({"code":"QUALITY_GATE_FAILED","message":"正式版本必须选择经律师确认的结构化来源"}));
    }
    for id in evidence_ids {
        let status:Option<String>=sqlx::query_scalar("SELECT review_status FROM criminal_evidence_items WHERE id=? AND case_id=? AND deleted_at IS NULL").bind(id).bind(&row.case_id).fetch_optional(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        if status.as_deref() != Some("confirmed") {
            failures.push(json!({"code":"REVIEW_REQUIRED","message":"所选证据尚未由律师确认"}));
        }
    }
    for id in finding_ids {
        let finding:Option<(String,String)>=sqlx::query_as("SELECT finding_type,review_status FROM criminal_analysis_findings WHERE id=? AND case_id=? AND deleted_at IS NULL").bind(&id).bind(&row.case_id).fetch_optional(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        if finding.as_ref().is_none_or(|(_, s)| s != "confirmed") {
            failures.push(json!({"code":"REVIEW_REQUIRED","message":"所选分析结论尚未由律师确认"}));
        }
        if finding
            .as_ref()
            .is_some_and(|(kind, _)| kind == "material_fact")
        {
            let valid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_source_citations WHERE owner_type='finding' AND owner_id=? AND citation_kind='material' AND integrity_status='valid' AND deleted_at IS NULL").bind(&id).fetch_one(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
            if valid == 0 {
                failures.push(
                    json!({"code":"SOURCE_CITATION_INVALID","message":"确定材料事实缺少有效来源"}),
                );
            }
        }
        let invalid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_source_citations WHERE owner_type='finding' AND owner_id=? AND deleted_at IS NULL AND (integrity_status<>'valid' OR (citation_kind='legal' AND verification_status<>'verified'))").bind(&id).fetch_one(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
        if invalid > 0 {
            failures.push(json!({"code":"SOURCE_CITATION_INVALID","message":"所选分析结论存在失效材料或未核验法源"}));
        }
    }
    let document_type: String =
        sqlx::query_scalar("SELECT document_type FROM criminal_draft_documents WHERE id=?")
            .bind(&row.draft_id)
            .fetch_one(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    if document_type == "first_meeting_record" || document_type == "followup_meeting_record" {
        for required in ["签名", "日期"] {
            if !row.rendered_markdown.contains(required) {
                failures.push(json!({"code":"DOCUMENT_TYPE_GATE","message":format!("会见笔录缺少{required}确认区")}));
            }
        }
    }
    if document_type == "evidence_objection"
        && citations.iter().all(|(_, kind, _)| kind != "material")
    {
        failures
            .push(json!({"code":"DOCUMENT_TYPE_GATE","message":"质证意见必须逐证据包含材料定位"}));
    }
    if document_type == "criminal_appeal"
        && (!row.rendered_markdown.contains("上诉人")
            || row.rendered_markdown.contains("本律师认为"))
    {
        failures.push(json!({"code":"DOCUMENT_TYPE_GATE","message":"刑事上诉状必须采用被告人/上诉人视角，不得采用律师第一人称"}));
    }
    if !failures.is_empty() {
        return Err(coded(
            "QUALITY_GATE_FAILED",
            serde_json::to_string(&failures).unwrap_or_default(),
        ));
    }
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    sqlx::query("UPDATE criminal_draft_versions SET status='superseded',updated_at=datetime('now') WHERE draft_id=? AND status='approved'").bind(&row.draft_id).execute(&mut*tx).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query("UPDATE criminal_draft_versions SET status='approved',reviewed_by=?,reviewed_at=datetime('now'),review_note=?,approved_at=datetime('now'),quality_report_json=?,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(input.actor.trim()).bind(&input.review_note).bind(json!({"passed":true,"failures":[]}).to_string()).bind(&input.version_id).bind(input.expected_revision).execute(&mut*tx).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query("UPDATE criminal_draft_documents SET current_version_id=?,revision=revision+1,updated_at=datetime('now') WHERE id=?").bind(&input.version_id).bind(&row.draft_id).execute(&mut*tx).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    tx.commit()
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(input.version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn return_criminal_draft_version(
    pool: tauri::State<'_, SqlitePool>,
    input: ApproveDraftInput,
) -> Result<DraftVersion, String> {
    let row: DraftVersion = sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(&input.version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &row.case_id).await?;
    require_revision(row.revision, input.expected_revision)?;
    if row.status != "pending_review" || input.actor.trim().is_empty() {
        return Err(coded("INVALID_STATE_TRANSITION", "只有待审核版本可退回"));
    }
    sqlx::query("UPDATE criminal_draft_versions SET status='draft',reviewed_by=?,reviewed_at=datetime('now'),review_note=?,revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(input.actor.trim()).bind(&input.review_note).bind(&input.version_id).bind(input.expected_revision).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(input.version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn list_criminal_drafts(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<Vec<DraftDocument>, String> {
    require_criminal(pool.inner(), &case_id).await?;
    sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE case_id=? AND deleted_at IS NULL ORDER BY updated_at DESC").bind(case_id).fetch_all(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))
}
#[tauri::command]
pub async fn get_criminal_draft(
    pool: tauri::State<'_, SqlitePool>,
    draft_id: String,
) -> Result<(DraftDocument, Vec<DraftVersion>), String> {
    let draft: DraftDocument =
        sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE id=? AND deleted_at IS NULL")
            .bind(&draft_id)
            .fetch_one(pool.inner())
            .await
            .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &draft.case_id).await?;
    let versions = sqlx::query_as(
        "SELECT * FROM criminal_draft_versions WHERE draft_id=? ORDER BY version_no DESC",
    )
    .bind(draft_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    Ok((draft, versions))
}
#[tauri::command]
pub async fn archive_criminal_draft(
    pool: tauri::State<'_, SqlitePool>,
    draft_id: String,
    expected_revision: i64,
) -> Result<DraftDocument, String> {
    let draft: DraftDocument =
        sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE id=? AND deleted_at IS NULL")
            .bind(&draft_id)
            .fetch_one(pool.inner())
            .await
            .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &draft.case_id).await?;
    require_revision(draft.revision, expected_revision)?;
    sqlx::query("UPDATE criminal_draft_documents SET status='archived',revision=revision+1,updated_at=datetime('now') WHERE id=? AND revision=?").bind(&draft_id).bind(expected_revision).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_draft_documents WHERE id=?")
        .bind(draft_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportDraftInput {
    pub version_id: String,
    pub mode: String,
    pub output_path: String,
}
#[tauri::command]
pub async fn export_criminal_draft_version(
    pool: tauri::State<'_, SqlitePool>,
    input: ExportDraftInput,
) -> Result<String, String> {
    let row: DraftVersion = sqlx::query_as("SELECT * FROM criminal_draft_versions WHERE id=?")
        .bind(&input.version_id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("CASE_NOT_FOUND", e.to_string()))?;
    require_criminal(pool.inner(), &row.case_id).await?;
    if input.mode == "formal" && row.status != "approved" {
        return Err(coded("REVIEW_REQUIRED", "正式导出仅允许已批准版本"));
    }
    let invalid:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_source_citations WHERE owner_type='draft_version' AND owner_id=? AND deleted_at IS NULL AND (integrity_status<>'valid' OR (citation_kind='legal' AND verification_status<>'verified'))").bind(&input.version_id).fetch_one(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    if input.mode == "formal" && invalid > 0 {
        return Err(coded("QUALITY_GATE_FAILED", "来源已失效或法律依据未核验"));
    }
    let output = std::path::Path::new(&input.output_path);
    if !output.is_absolute() || output.parent().is_none_or(|p| !p.is_dir()) {
        return Err(coded(
            "SOURCE_CITATION_INVALID",
            "导出路径必须是已存在目录内的绝对路径",
        ));
    }
    if !matches!(
        output
            .extension()
            .and_then(|v| v.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("md") | Some("txt")
    ) {
        return Err(coded(
            "SOURCE_CITATION_INVALID",
            "结构化草稿后端只允许导出 .md/.txt",
        ));
    }
    let text = if input.mode == "working" && row.status != "approved" {
        format!("# 草稿—待律师审核\n\n{}", row.rendered_markdown)
    } else {
        row.rendered_markdown
    };
    std::fs::write(&input.output_path, text)
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    let audit_result = sqlx::query("INSERT INTO criminal_workspace_audit_events(id,case_id,aggregate_type,aggregate_id,event_type,actor,payload_json) VALUES(?,?,?,?,?,'user',?)")
        .bind(Uuid::new_v4().to_string()).bind(&row.case_id).bind("draft_version").bind(&row.id)
        .bind(if input.mode=="formal"{"formal_exported"}else{"working_exported"})
        .bind(json!({"output_filename":output.file_name().and_then(|v|v.to_str())}).to_string())
        .execute(pool.inner()).await;
    if let Err(error) = audit_result {
        let _ = std::fs::remove_file(&input.output_path);
        return Err(coded("DATABASE_WRITE_FAILED", error.to_string()));
    }
    Ok(input.output_path)
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TaskArtifact {
    pub id: String,
    pub case_id: String,
    pub task_id: String,
    pub artifact_type: String,
    pub artifact_id: String,
    pub relation: String,
    pub created_by: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkTaskInput {
    pub case_id: String,
    pub task_id: String,
    pub artifact_type: String,
    pub artifact_id: String,
    pub relation: String,
    pub created_by: String,
}
#[tauri::command]
pub async fn link_criminal_workspace_artifact_to_task(
    pool: tauri::State<'_, SqlitePool>,
    input: LinkTaskInput,
) -> Result<TaskArtifact, String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    let exists: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_tasks WHERE id=? AND case_id=?")
            .bind(&input.task_id)
            .bind(&input.case_id)
            .fetch_one(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    if exists != 1 {
        return Err(coded("CASE_NOT_FOUND", "SOP 任务不存在或不属于本案"));
    }
    require_artifact_case(
        pool.inner(),
        &input.artifact_type,
        &input.artifact_id,
        &input.case_id,
    )
    .await?;
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO criminal_workspace_task_links(id,case_id,task_id,artifact_type,artifact_id,relation,created_by) VALUES(?,?,?,?,?,?,?)").bind(&id).bind(&input.case_id).bind(&input.task_id).bind(&input.artifact_type).bind(&input.artifact_id).bind(&input.relation).bind(&input.created_by).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    sqlx::query_as("SELECT * FROM criminal_workspace_task_links WHERE id=?")
        .bind(id)
        .fetch_one(pool.inner())
        .await
        .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn list_criminal_task_artifacts(
    pool: tauri::State<'_, SqlitePool>,
    task_id: String,
) -> Result<Vec<TaskArtifact>, String> {
    let case_id: Option<String> =
        sqlx::query_scalar("SELECT case_id FROM criminal_case_tasks WHERE id=?")
            .bind(&task_id)
            .fetch_optional(pool.inner())
            .await
            .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?;
    require_criminal(
        pool.inner(),
        case_id
            .as_deref()
            .ok_or_else(|| coded("CASE_NOT_FOUND", "任务不存在"))?,
    )
    .await?;
    sqlx::query_as(
        "SELECT * FROM criminal_workspace_task_links WHERE task_id=? ORDER BY created_at DESC",
    )
    .bind(task_id)
    .fetch_all(pool.inner())
    .await
    .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))
}
#[tauri::command]
pub async fn unlink_criminal_workspace_artifact_from_task(
    pool: tauri::State<'_, SqlitePool>,
    input: LinkTaskInput,
) -> Result<(), String> {
    require_criminal(pool.inner(), &input.case_id).await?;
    let result=sqlx::query("DELETE FROM criminal_workspace_task_links WHERE case_id=? AND task_id=? AND artifact_type=? AND artifact_id=? AND relation=?").bind(&input.case_id).bind(&input.task_id).bind(&input.artifact_type).bind(&input.artifact_id).bind(&input.relation).execute(pool.inner()).await.map_err(|e|coded("DATABASE_WRITE_FAILED",e.to_string()))?;
    if result.rows_affected() != 1 {
        return Err(coded("CASE_NOT_FOUND", "任务成果关联不存在"));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkspaceSummary {
    pub case_id: String,
    pub review_notes: i64,
    pub evidence_items: i64,
    pub issues: i64,
    pub findings: i64,
    pub drafts: i64,
    pub pending_review: i64,
    pub invalid_citations: i64,
    pub open_tasks: i64,
}
#[tauri::command]
pub async fn get_criminal_defense_workspace(
    pool: tauri::State<'_, SqlitePool>,
    case_id: String,
) -> Result<WorkspaceSummary, String> {
    require_criminal(pool.inner(), &case_id).await?;
    macro_rules! count {
        ($q:expr) => {
            sqlx::query_scalar::<_, i64>($q)
                .bind(&case_id)
                .fetch_one(pool.inner())
                .await
                .map_err(|e| coded("DATABASE_WRITE_FAILED", e.to_string()))?
        };
    }
    Ok(WorkspaceSummary{case_id:case_id.clone(),review_notes:count!("SELECT COUNT(*) FROM criminal_review_notes WHERE case_id=? AND deleted_at IS NULL"),evidence_items:count!("SELECT COUNT(*) FROM criminal_evidence_items WHERE case_id=? AND deleted_at IS NULL"),issues:count!("SELECT COUNT(*) FROM criminal_issues WHERE case_id=? AND deleted_at IS NULL"),findings:count!("SELECT COUNT(*) FROM criminal_analysis_findings WHERE case_id=? AND deleted_at IS NULL"),drafts:count!("SELECT COUNT(*) FROM criminal_draft_documents WHERE case_id=? AND deleted_at IS NULL"),pending_review:count!("SELECT (SELECT COUNT(*) FROM criminal_review_notes WHERE case_id=?1 AND review_status='pending_review' AND deleted_at IS NULL)+(SELECT COUNT(*) FROM criminal_evidence_items WHERE case_id=?1 AND review_status='pending_review' AND deleted_at IS NULL)+(SELECT COUNT(*) FROM criminal_issues WHERE case_id=?1 AND review_status='pending_review' AND deleted_at IS NULL)+(SELECT COUNT(*) FROM criminal_analysis_findings WHERE case_id=?1 AND review_status='pending_review' AND deleted_at IS NULL)"),invalid_citations:count!("SELECT COUNT(*) FROM criminal_source_citations WHERE case_id=? AND deleted_at IS NULL AND integrity_status<>'valid'"),open_tasks:count!("SELECT COUNT(*) FROM criminal_case_tasks WHERE case_id=? AND status NOT IN ('completed','ignored','not_applicable')")})
}

#[cfg(test)]
mod tests {
    use super::*;
    async fn seeded(domain: &str) -> SqlitePool {
        let p = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query("INSERT INTO cases(id,name,case_type,legal_domain,domain_source,source_folder) VALUES('c','案','诉讼',?,'manual','C:/case')").bind(domain).execute(&p).await.unwrap();
        p
    }
    #[tokio::test]
    async fn domain_gate_rejects_civil_without_writes() {
        let p = seeded("civil").await;
        let err = save_review_note(
            &p,
            UpsertReviewNoteInput {
                case_id: "c".into(),
                title: "x".into(),
                content: "x".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("DOMAIN_MISMATCH:"));
        let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_review_notes")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(n, 0);
    }
    #[tokio::test]
    async fn ai_note_is_always_pending_review() {
        let p = seeded("criminal").await;
        let row = save_review_note(
            &p,
            UpsertReviewNoteInput {
                case_id: "c".into(),
                title: "x".into(),
                content: "x".into(),
                author_type: Some("codex".into()),
                review_status: Some("confirmed".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(row.review_status, "pending_review");
    }
    #[tokio::test]
    async fn material_fact_requires_valid_citation() {
        let p = seeded("criminal").await;
        sqlx::query("INSERT INTO criminal_analysis_findings(id,case_id,finding_type,title,content,review_status,origin) VALUES('f','c','material_fact','事实','内容','pending_review','user')").execute(&p).await.unwrap();
        let err = review_record(
            &p,
            "criminal_analysis_findings",
            "finding",
            &ReviewRecordInput {
                id: "f".into(),
                decision: "confirm".into(),
                actor: "律师".into(),
                note: None,
                expected_revision: 1,
            },
            true,
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("SOURCE_CITATION_INVALID:"));
    }
    #[tokio::test]
    async fn confirmed_record_cannot_be_reopened_by_review_action() {
        let p = seeded("criminal").await;
        sqlx::query("INSERT INTO criminal_issues(id,case_id,issue_type,neutral_title,review_status,origin) VALUES('i','c','fact','事实','confirmed','user')").execute(&p).await.unwrap();
        let err = review_record(
            &p,
            "criminal_issues",
            "issue",
            &ReviewRecordInput {
                id: "i".into(),
                decision: "reopen".into(),
                actor: "律师".into(),
                note: None,
                expected_revision: 1,
            },
            false,
        )
        .await
        .unwrap_err();
        assert!(err.starts_with("INVALID_STATE_TRANSITION:"));
    }
    #[tokio::test]
    async fn cross_case_record_validation_rejects_foreign_id() {
        let p = seeded("criminal").await;
        sqlx::query("INSERT INTO cases(id,name,case_type,legal_domain,domain_source,source_folder) VALUES('c2','案2','诉讼','criminal','manual','C:/case2')").execute(&p).await.unwrap();
        sqlx::query("INSERT INTO criminal_issues(id,case_id,issue_type,neutral_title,review_status,origin) VALUES('i','c2','fact','事实','pending_review','user')").execute(&p).await.unwrap();
        let err = require_record_case(&p, "criminal_issues", "i", "c")
            .await
            .unwrap_err();
        assert!(err.starts_with("CASE_NOT_FOUND:"));
    }
    #[tokio::test]
    async fn audit_failure_rolls_business_write_back() {
        let p = seeded("criminal").await;
        sqlx::query("DROP TABLE criminal_workspace_audit_events")
            .execute(&p)
            .await
            .unwrap();
        let result = save_review_note(
            &p,
            UpsertReviewNoteInput {
                case_id: "c".into(),
                title: "标题".into(),
                content: "内容".into(),
                ..Default::default()
            },
        )
        .await;
        assert!(result.is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_review_notes")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
    #[test]
    fn assessment_status_is_closed_enum() {
        assert!(validate_assessment_json(Some(r#"{"status":"doubtful"}"#)).is_ok());
        assert!(validate_assessment_json(Some(r#"{"status":"certain"}"#))
            .unwrap_err()
            .starts_with("SOURCE_CITATION_INVALID:"));
    }
    #[test]
    fn ai_origin_cannot_request_confirmed() {
        assert_eq!(
            normalized_review_status("codex", Some("confirmed"), true).unwrap(),
            "pending_review"
        );
        assert_eq!(
            normalized_review_status("native_ai", Some("draft"), true).unwrap(),
            "pending_review"
        );
    }
    #[test]
    fn source_fingerprint_tracks_content_not_metadata() {
        let path = std::env::temp_dir().join(format!("caseboard-fingerprint-{}", Uuid::new_v4()));
        std::fs::write(&path, b"same-size-A").unwrap();
        let first = fingerprint(path.to_str().unwrap()).unwrap();
        std::fs::write(&path, b"same-size-B").unwrap();
        let second = fingerprint(path.to_str().unwrap()).unwrap();
        assert_ne!(first, second);
        let copied =
            std::env::temp_dir().join(format!("caseboard-fingerprint-copy-{}", Uuid::new_v4()));
        std::fs::copy(&path, &copied).unwrap();
        assert_eq!(second, fingerprint(copied.to_str().unwrap()).unwrap());
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(copied);
    }
    #[test]
    fn frozen_draft_document_type_enum_accepts_all_thirteen_types() {
        assert_eq!(CRIMINAL_DRAFT_DOCUMENT_TYPES.len(), 13);
        for document_type in CRIMINAL_DRAFT_DOCUMENT_TYPES {
            assert_eq!(
                validate_draft_document_type(document_type).unwrap(),
                document_type
            );
        }
    }
    #[tokio::test]
    async fn unsupported_draft_document_type_returns_stable_error_and_zero_writes() {
        let p = seeded("criminal").await;
        let error = create_draft_record(
            &p,
            CreateDraftInput {
                case_id: "c".into(),
                document_type: "defense_opinion".into(),
                title: "契约外文书".into(),
                created_by: "律师".into(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(
            error,
            "INVALID_STATE_TRANSITION: 不支持的刑事文书类型: defense_opinion"
        );
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_draft_documents")
            .fetch_one(&p)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
