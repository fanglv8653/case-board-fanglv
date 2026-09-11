use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

use super::cases::Case;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseLifecycleInput {
    pub case_id: String,
    pub occurred_on: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseArchivePreflight {
    pub case_id: String,
    pub open_todo_count: i64,
    pub is_closed: bool,
    pub is_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseArchiveAudit {
    pub id: String,
    pub case_id: String,
    pub action: String,
    pub occurred_at: String,
    pub note: Option<String>,
    pub previous_management_status: String,
    pub new_management_status: String,
    pub created_at: String,
}

fn normalized_date(value: Option<&str>) -> Result<String, String> {
    let value = value.map(str::trim).filter(|value| !value.is_empty());
    match value {
        Some(value) => NaiveDate::parse_from_str(value, "%Y-%m-%d")
            .map(|_| value.to_string())
            .map_err(|_| "CASE_LIFECYCLE_DATE_INVALID: 日期必须为 YYYY-MM-DD".to_string()),
        None => Ok(chrono::Local::now()
            .date_naive()
            .format("%Y-%m-%d")
            .to_string()),
    }
}

fn normalized_note(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(500).collect())
}

pub async fn archive_preflight(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<CaseArchivePreflight, String> {
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT management_status, archived_at FROM cases WHERE id=?")
            .bind(case_id)
            .fetch_optional(pool)
            .await
            .map_err(|e| format!("CASE_LIFECYCLE_READ_FAILED: {e}"))?;
    let (management_status, archived_at) =
        row.ok_or_else(|| "CASE_NOT_FOUND: 案件不存在".to_string())?;
    let open_todo_count = sqlx::query_scalar(
        "SELECT COUNT(*) FROM case_todos WHERE case_id=? AND deleted_at IS NULL AND done=0 AND status NOT IN ('completed','deleted')",
    )
    .bind(case_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("CASE_LIFECYCLE_TODO_READ_FAILED: {e}"))?;
    Ok(CaseArchivePreflight {
        case_id: case_id.to_string(),
        open_todo_count,
        is_closed: management_status == "closed",
        is_archived: archived_at.is_some(),
    })
}

pub async fn close_case(pool: &SqlitePool, input: CaseLifecycleInput) -> Result<Case, String> {
    change_lifecycle(pool, input, "close").await
}

pub async fn reopen_case(pool: &SqlitePool, input: CaseLifecycleInput) -> Result<Case, String> {
    change_lifecycle(pool, input, "reopen").await
}

pub async fn archive_case(pool: &SqlitePool, input: CaseLifecycleInput) -> Result<Case, String> {
    change_lifecycle(pool, input, "archive").await
}

pub async fn restore_case(pool: &SqlitePool, input: CaseLifecycleInput) -> Result<Case, String> {
    change_lifecycle(pool, input, "restore").await
}

async fn change_lifecycle(
    pool: &SqlitePool,
    input: CaseLifecycleInput,
    action: &str,
) -> Result<Case, String> {
    let occurred_at = normalized_date(input.occurred_on.as_deref())?;
    let note = normalized_note(input.note.as_deref());
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| format!("CASE_LIFECYCLE_BEGIN_FAILED: {e}"))?;
    let row: Option<(String, Option<String>)> =
        sqlx::query_as("SELECT management_status, archived_at FROM cases WHERE id=?")
            .bind(&input.case_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| format!("CASE_LIFECYCLE_READ_FAILED: {e}"))?;
    let (previous_status, archived_at) =
        row.ok_or_else(|| "CASE_NOT_FOUND: 案件不存在".to_string())?;

    let new_status = match action {
        "close" => {
            if archived_at.is_some() {
                return Err("CASE_LIFECYCLE_ARCHIVED: 请先从归档恢复".to_string());
            }
            sqlx::query("UPDATE cases SET management_status='closed', management_status_source='manual', closed_at=?, updated_at=datetime('now') WHERE id=?")
                .bind(&occurred_at).bind(&input.case_id).execute(&mut *tx).await
                .map_err(|e| format!("CASE_LIFECYCLE_WRITE_FAILED: {e}"))?;
            "closed"
        }
        "reopen" => {
            if archived_at.is_some() {
                return Err("CASE_LIFECYCLE_ARCHIVED: 请先从归档恢复".to_string());
            }
            sqlx::query("UPDATE cases SET management_status='active', management_status_source='manual', closed_at=NULL, updated_at=datetime('now') WHERE id=?")
                .bind(&input.case_id).execute(&mut *tx).await
                .map_err(|e| format!("CASE_LIFECYCLE_WRITE_FAILED: {e}"))?;
            "active"
        }
        "archive" => {
            if previous_status != "closed" {
                return Err("CASE_LIFECYCLE_NOT_CLOSED: 案件须先标记办结再归档".to_string());
            }
            if archived_at.is_some() {
                return Err("CASE_LIFECYCLE_ALREADY_ARCHIVED: 案件已经归档".to_string());
            }
            sqlx::query("UPDATE cases SET archived_at=?, archive_note=?, updated_at=datetime('now') WHERE id=?")
                .bind(&occurred_at).bind(&note).bind(&input.case_id).execute(&mut *tx).await
                .map_err(|e| format!("CASE_LIFECYCLE_WRITE_FAILED: {e}"))?;
            previous_status.as_str()
        }
        "restore" => {
            if archived_at.is_none() {
                return Err("CASE_LIFECYCLE_NOT_ARCHIVED: 案件尚未归档".to_string());
            }
            sqlx::query("UPDATE cases SET archived_at=NULL, archive_note=NULL, updated_at=datetime('now') WHERE id=?")
                .bind(&input.case_id).execute(&mut *tx).await
                .map_err(|e| format!("CASE_LIFECYCLE_WRITE_FAILED: {e}"))?;
            previous_status.as_str()
        }
        _ => return Err("CASE_LIFECYCLE_ACTION_INVALID".to_string()),
    };

    sqlx::query(
        "INSERT INTO case_archive_audits(id,case_id,action,occurred_at,note,previous_management_status,new_management_status) VALUES(?,?,?,?,?,?,?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&input.case_id)
    .bind(action)
    .bind(&occurred_at)
    .bind(&note)
    .bind(&previous_status)
    .bind(new_status)
    .execute(&mut *tx)
    .await
    .map_err(|e| format!("CASE_LIFECYCLE_AUDIT_FAILED: {e}"))?;
    tx.commit()
        .await
        .map_err(|e| format!("CASE_LIFECYCLE_COMMIT_FAILED: {e}"))?;
    super::cases::get_case(pool, &input.case_id)
        .await
        .map_err(|e| format!("CASE_LIFECYCLE_READ_FAILED: {e}"))?
        .ok_or_else(|| "CASE_NOT_FOUND: 案件不存在".to_string())
}

pub async fn list_audits(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CaseArchiveAudit>, String> {
    sqlx::query_as(
        "SELECT * FROM case_archive_audits WHERE case_id=? ORDER BY created_at DESC,id DESC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("CASE_LIFECYCLE_AUDIT_READ_FAILED: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_archive_restore_keeps_business_rows() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let case = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "归档测试".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/tmp/{}", Uuid::new_v4()),
            },
        )
        .await
        .unwrap();
        let input = |note: &str| CaseLifecycleInput {
            case_id: case.id.clone(),
            occurred_on: Some("2026-09-02".into()),
            note: Some(note.into()),
        };
        close_case(&pool, input("办结")).await.unwrap();
        let archived = archive_case(&pool, input("归档")).await.unwrap();
        assert_eq!(archived.management_status, "closed");
        assert_eq!(archived.archived_at.as_deref(), Some("2026-09-02"));
        let restored = restore_case(&pool, input("恢复")).await.unwrap();
        assert!(restored.archived_at.is_none());
        assert_eq!(restored.management_status, "closed");
        assert_eq!(list_audits(&pool, &case.id).await.unwrap().len(), 3);
    }
}
