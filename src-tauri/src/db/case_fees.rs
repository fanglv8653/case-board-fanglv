use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseFee {
    pub id: String,
    pub case_id: String,
    pub item_name: String,
    pub amount: f64,
    pub charged_at: Option<String>,
    pub receipt_no: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub source: String,
    pub updated_at: String,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseFeeInput {
    pub id: Option<String>,
    pub case_id: String,
    pub item_name: String,
    pub amount: f64,
    pub charged_at: Option<String>,
    pub receipt_no: Option<String>,
    pub notes: Option<String>,
}

fn clean_optional(value: Option<String>, max: usize) -> Option<String> {
    value
        .map(|value| value.trim().chars().take(max).collect::<String>())
        .filter(|value| !value.is_empty())
}

fn validate(input: &mut CaseFeeInput) -> Result<(), String> {
    input.item_name = input.item_name.trim().chars().take(100).collect();
    if input.item_name.is_empty() {
        return Err("CASE_FEE_ITEM_REQUIRED: 收费项目不能为空".into());
    }
    if !input.amount.is_finite() || input.amount < 0.0 {
        return Err("CASE_FEE_AMOUNT_INVALID: 金额必须为非负数".into());
    }
    input.charged_at = clean_optional(input.charged_at.take(), 10);
    if let Some(date) = input.charged_at.as_deref() {
        NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| "CASE_FEE_DATE_INVALID: 日期必须为 YYYY-MM-DD".to_string())?;
    }
    input.receipt_no = clean_optional(input.receipt_no.take(), 100);
    input.notes = clean_optional(input.notes.take(), 500);
    Ok(())
}

pub async fn list(pool: &SqlitePool, case_id: &str) -> Result<Vec<CaseFee>, String> {
    sqlx::query_as("SELECT * FROM case_fees WHERE case_id=? ORDER BY deleted_at IS NOT NULL, charged_at DESC, created_at DESC")
        .bind(case_id)
        .fetch_all(pool)
        .await
        .map_err(|e| format!("CASE_FEE_LIST_FAILED: {e}"))
}

pub async fn upsert(pool: &SqlitePool, mut input: CaseFeeInput) -> Result<CaseFee, String> {
    validate(&mut input)?;
    let id = input
        .id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    if input.id.is_some() {
        let affected = sqlx::query("UPDATE case_fees SET item_name=?,amount=?,charged_at=?,receipt_no=?,notes=?,updated_at=datetime('now') WHERE id=? AND case_id=? AND source='manual'")
            .bind(&input.item_name).bind(input.amount).bind(&input.charged_at)
            .bind(&input.receipt_no).bind(&input.notes).bind(&id).bind(&input.case_id)
            .execute(pool).await.map_err(|e| format!("CASE_FEE_WRITE_FAILED: {e}"))?.rows_affected();
        if affected == 0 {
            return Err("CASE_FEE_NOT_EDITABLE: 记录不存在或不是人工记录".into());
        }
    } else {
        sqlx::query("INSERT INTO case_fees(id,case_id,item_name,amount,charged_at,receipt_no,notes,source,updated_at) VALUES(?,?,?,?,?,?,?,'manual',datetime('now'))")
            .bind(&id).bind(&input.case_id).bind(&input.item_name).bind(input.amount)
            .bind(&input.charged_at).bind(&input.receipt_no).bind(&input.notes)
            .execute(pool).await.map_err(|e| format!("CASE_FEE_WRITE_FAILED: {e}"))?;
    }
    sqlx::query_as("SELECT * FROM case_fees WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("CASE_FEE_READ_FAILED: {e}"))
}

pub async fn set_deleted(pool: &SqlitePool, id: &str, deleted: bool) -> Result<CaseFee, String> {
    let affected = if deleted {
        sqlx::query("UPDATE case_fees SET deleted_at=datetime('now'),updated_at=datetime('now') WHERE id=? AND source='manual'")
            .bind(id).execute(pool).await
    } else {
        sqlx::query("UPDATE case_fees SET deleted_at=NULL,updated_at=datetime('now') WHERE id=? AND source='manual'")
            .bind(id).execute(pool).await
    }.map_err(|e| format!("CASE_FEE_DELETE_FAILED: {e}"))?.rows_affected();
    if affected == 0 {
        return Err("CASE_FEE_NOT_EDITABLE: 记录不存在或不是人工记录".into());
    }
    sqlx::query_as("SELECT * FROM case_fees WHERE id=?")
        .bind(id)
        .fetch_one(pool)
        .await
        .map_err(|e| format!("CASE_FEE_READ_FAILED: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn manual_fee_crud_is_soft_delete_and_protected_from_aggregate_refresh() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let case = crate::db::cases::create_case(
            &pool,
            crate::db::cases::NewCase {
                name: "收费测试".into(),
                case_type: "criminal".into(),
                source_folder: format!("D:/tmp/{}", Uuid::new_v4()),
            },
        )
        .await
        .unwrap();
        let fee = upsert(
            &pool,
            CaseFeeInput {
                id: None,
                case_id: case.id.clone(),
                item_name: "律师代理费".into(),
                amount: 5000.0,
                charged_at: Some("2026-09-02".into()),
                receipt_no: None,
                notes: None,
            },
        )
        .await
        .unwrap();
        assert_eq!(fee.source, "manual");
        let dirty: (String, String, String) = sqlx::query_as(
            "SELECT 'case_fee',entity_id,action FROM device_sync_case_fee_dirty_entities WHERE entity_id=?",
        )
        .bind(&fee.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(dirty, ("case_fee".into(), fee.id.clone(), "upsert".into()));
        assert!(set_deleted(&pool, &fee.id, true)
            .await
            .unwrap()
            .deleted_at
            .is_some());
        assert!(set_deleted(&pool, &fee.id, false)
            .await
            .unwrap()
            .deleted_at
            .is_none());
        assert!(upsert(
            &pool,
            CaseFeeInput {
                amount: -1.0,
                ..CaseFeeInput {
                    id: None,
                    case_id: case.id,
                    item_name: "测试".into(),
                    amount: 0.0,
                    charged_at: None,
                    receipt_no: None,
                    notes: None
                }
            }
        )
        .await
        .is_err());
    }
}
