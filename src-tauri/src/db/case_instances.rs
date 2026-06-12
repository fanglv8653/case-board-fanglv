//! 审级实例(2026-06-11 · case_instances 表)。
//!
//! 一个案件 = N 个审级([仲裁] → 一审 → 二审 → [再审]),每审级一条:
//! 自己的案号 / 承办机关(+类型) / 承办人 / 该审级当事人称谓 / 结果。
//! `seq` 最大者为当前审级(`is_current=1`),cases.agg_* 存当前审级快照。
//! 重抽只覆盖 `source='llm'` 的行,用户手加(`source='user'`)的不动。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CaseInstance {
    pub id: String,
    pub case_id: String,
    pub level: String, // 仲裁 / 一审 / 二审 / 再审
    pub seq: i64,      // 仲裁=1 一审=2 二审=3 再审=4;发回重审等续排;最大=当前
    pub case_no: Option<String>,
    pub authority: Option<String>,
    pub authority_type: Option<String>, // 法院 / 仲裁委 / 其他
    pub handlers: Option<String>,       // JSON [{name,role,phone}]
    pub party_roles: Option<String>,    // JSON [{name,role,is_our_side,note}]
    pub filed_at: Option<String>,
    pub result: Option<String>,
    pub note: Option<String>,
    pub is_current: bool,
    pub source: String, // llm / user
    pub created_at: String,
    pub updated_at: String,
}

/// 新建/重建审级的输入(LLM 聚合或用户手填共用)。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NewInstance {
    pub level: String,
    pub seq: i64,
    pub case_no: Option<String>,
    pub authority: Option<String>,
    pub authority_type: Option<String>,
    pub handlers: Option<String>,
    pub party_roles: Option<String>,
    pub filed_at: Option<String>,
    pub result: Option<String>,
    pub note: Option<String>,
}

/// 按 seq 倒序(最新审级在前,详情页直接按此顺序渲染)。
pub async fn list_by_case(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Vec<CaseInstance>, sqlx::Error> {
    sqlx::query_as::<_, CaseInstance>(
        "SELECT * FROM case_instances WHERE case_id = ? ORDER BY seq DESC, created_at ASC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
}

/// LLM 重抽:删旧 llm 行 → 插新行 → 重算 is_current。user 行保留。
/// 返回重算后的全量列表(seq 倒序)。
pub async fn replace_llm_instances(
    pool: &SqlitePool,
    case_id: &str,
    items: &[NewInstance],
) -> Result<Vec<CaseInstance>, sqlx::Error> {
    sqlx::query("DELETE FROM case_instances WHERE case_id = ? AND source = 'llm'")
        .bind(case_id)
        .execute(pool)
        .await?;

    for it in items {
        insert(pool, case_id, it, "llm").await?;
    }
    recompute_current(pool, case_id).await?;
    list_by_case(pool, case_id).await
}

/// 用户手加一条审级。
pub async fn add_user_instance(
    pool: &SqlitePool,
    case_id: &str,
    it: &NewInstance,
) -> Result<CaseInstance, sqlx::Error> {
    let id = insert(pool, case_id, it, "user").await?;
    recompute_current(pool, case_id).await?;
    sqlx::query_as::<_, CaseInstance>("SELECT * FROM case_instances WHERE id = ?")
        .bind(&id)
        .fetch_one(pool)
        .await
}

/// 用户改一条审级(整行字段更新,改完标记 source='user' 防止重抽覆盖)。
pub async fn update_instance(
    pool: &SqlitePool,
    id: &str,
    it: &NewInstance,
) -> Result<u64, sqlx::Error> {
    let r = sqlx::query(
        "UPDATE case_instances SET level=?, seq=?, case_no=?, authority=?, authority_type=?, \
         handlers=?, party_roles=?, filed_at=?, result=?, note=?, source='user', \
         updated_at=datetime('now') WHERE id=?",
    )
    .bind(&it.level)
    .bind(it.seq)
    .bind(&it.case_no)
    .bind(&it.authority)
    .bind(&it.authority_type)
    .bind(&it.handlers)
    .bind(&it.party_roles)
    .bind(&it.filed_at)
    .bind(&it.result)
    .bind(&it.note)
    .bind(id)
    .execute(pool)
    .await?;
    // seq 可能变了,重算 is_current
    if let Some(case_id) =
        sqlx::query_scalar::<_, String>("SELECT case_id FROM case_instances WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?
    {
        recompute_current(pool, &case_id).await?;
    }
    Ok(r.rows_affected())
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, sqlx::Error> {
    let case_id =
        sqlx::query_scalar::<_, String>("SELECT case_id FROM case_instances WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    let r = sqlx::query("DELETE FROM case_instances WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    if let Some(cid) = case_id {
        recompute_current(pool, &cid).await?;
    }
    Ok(r.rows_affected())
}

async fn insert(
    pool: &SqlitePool,
    case_id: &str,
    it: &NewInstance,
    source: &str,
) -> Result<String, sqlx::Error> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO case_instances \
         (id, case_id, level, seq, case_no, authority, authority_type, handlers, party_roles, \
          filed_at, result, note, source) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(case_id)
    .bind(&it.level)
    .bind(it.seq)
    .bind(&it.case_no)
    .bind(&it.authority)
    .bind(&it.authority_type)
    .bind(&it.handlers)
    .bind(&it.party_roles)
    .bind(&it.filed_at)
    .bind(&it.result)
    .bind(&it.note)
    .bind(source)
    .execute(pool)
    .await?;
    Ok(id)
}

/// seq 最大者(同 seq 取 created_at 最新)标 is_current=1,其余 0。
async fn recompute_current(pool: &SqlitePool, case_id: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE case_instances SET is_current = 0 WHERE case_id = ?")
        .bind(case_id)
        .execute(pool)
        .await?;
    sqlx::query(
        "UPDATE case_instances SET is_current = 1 WHERE id = \
         (SELECT id FROM case_instances WHERE case_id = ? \
          ORDER BY seq DESC, created_at DESC LIMIT 1)",
    )
    .bind(case_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::cases::{create_case, NewCase};
    use crate::db::init_pool;

    async fn test_case(pool: &SqlitePool) -> String {
        create_case(
            pool,
            NewCase {
                name: "测试".into(),
                case_type: "诉讼".into(),
                source_folder: "/tmp/test-instances".into(),
            },
        )
        .await
        .unwrap()
        .id
    }

    fn inst(level: &str, seq: i64, case_no: &str) -> NewInstance {
        NewInstance {
            level: level.into(),
            seq,
            case_no: Some(case_no.into()),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn replace_llm_keeps_user_rows_and_marks_current() {
        let pool = init_pool(":memory:").await.unwrap();
        let cid = test_case(&pool).await;

        // 用户手加一条仲裁
        add_user_instance(&pool, &cid, &inst("仲裁", 1, "锡劳人仲案字(2025)1号"))
            .await
            .unwrap();

        // LLM 抽出一审 + 二审
        let list = replace_llm_instances(
            &pool,
            &cid,
            &[
                inst("一审", 2, "(2025)苏0205民初100号"),
                inst("二审", 3, "(2026)苏02民终200号"),
            ],
        )
        .await
        .unwrap();
        assert_eq!(list.len(), 3);
        // seq 倒序:二审在前且是当前
        assert_eq!(list[0].level, "二审");
        assert!(list[0].is_current);
        assert!(!list[1].is_current);
        assert!(!list[2].is_current);

        // 重抽只换 llm 行,user 仲裁行保留
        let list2 = replace_llm_instances(&pool, &cid, &[inst("一审", 2, "(2025)苏0205民初100号")])
            .await
            .unwrap();
        assert_eq!(list2.len(), 2);
        assert_eq!(list2[0].level, "一审");
        assert!(list2[0].is_current);
        assert_eq!(list2[1].level, "仲裁");
        assert_eq!(list2[1].source, "user");
    }

    #[tokio::test]
    async fn update_and_delete_recompute_current() {
        let pool = init_pool(":memory:").await.unwrap();
        let cid = test_case(&pool).await;

        let a = add_user_instance(&pool, &cid, &inst("一审", 2, "民初1号"))
            .await
            .unwrap();
        let b = add_user_instance(&pool, &cid, &inst("二审", 3, "民终2号"))
            .await
            .unwrap();
        assert!(!list_by_case(&pool, &cid).await.unwrap()[1].is_current);

        // 删掉二审 → 一审成为当前
        delete(&pool, &b.id).await.unwrap();
        let list = list_by_case(&pool, &cid).await.unwrap();
        assert_eq!(list.len(), 1);
        assert!(list[0].is_current);

        // 改 level/seq
        let mut upd = inst("再审", 4, "民再3号");
        upd.note = Some("发回重审前的再审".into());
        update_instance(&pool, &a.id, &upd).await.unwrap();
        let list = list_by_case(&pool, &cid).await.unwrap();
        assert_eq!(list[0].level, "再审");
        assert!(list[0].is_current);
    }
}
