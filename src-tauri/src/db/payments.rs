//! 还款记录(2026-05-25 · case_payments 表)
//!
//! 律师在执行案件里手工录入对方实际还款,App 自动计算剩余执行款。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Payment {
    pub id: String,
    pub case_id: String,
    pub amount: f64,
    pub paid_at: String, // YYYY-MM-DD
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewPayment {
    pub case_id: String,
    pub amount: f64,
    pub paid_at: String,
    pub note: Option<String>,
}

pub async fn add(pool: &SqlitePool, p: NewPayment) -> Result<Payment, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO case_payments (id, case_id, amount, paid_at, note) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&p.case_id)
    .bind(p.amount)
    .bind(&p.paid_at)
    .bind(&p.note)
    .execute(pool)
    .await?;

    sqlx::query_as::<_, Payment>("SELECT * FROM case_payments WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
}

pub async fn list_by_case(pool: &SqlitePool, case_id: &str) -> Result<Vec<Payment>, sqlx::Error> {
    sqlx::query_as::<_, Payment>(
        "SELECT * FROM case_payments WHERE case_id = ? ORDER BY paid_at DESC, created_at DESC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let r = sqlx::query("DELETE FROM case_payments WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::cases::{create_case, NewCase};
    use crate::db::init_pool;

    #[tokio::test]
    async fn payments_round_trip() {
        let pool = init_pool(":memory:").await.unwrap();
        let case = create_case(
            &pool,
            NewCase {
                name: "测试".into(),
                case_type: "诉讼".into(),
                source_folder: "/tmp/test".into(),
            },
        )
        .await
        .unwrap();

        let p1 = add(
            &pool,
            NewPayment {
                case_id: case.id.clone(),
                amount: 50000.0,
                paid_at: "2026-04-29".into(),
                note: Some("首期".into()),
            },
        )
        .await
        .unwrap();
        assert_eq!(p1.amount, 50000.0);

        let _p2 = add(
            &pool,
            NewPayment {
                case_id: case.id.clone(),
                amount: 30000.0,
                paid_at: "2026-05-31".into(),
                note: None,
            },
        )
        .await
        .unwrap();

        let list = list_by_case(&pool, &case.id).await.unwrap();
        assert_eq!(list.len(), 2);
        // 倒序:5月底的在前
        assert_eq!(list[0].paid_at, "2026-05-31");

        let n = delete(&pool, &p1.id).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(list_by_case(&pool, &case.id).await.unwrap().len(), 1);
    }
}
