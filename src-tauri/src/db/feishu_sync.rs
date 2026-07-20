//! 飞书案件管理同步的只读预览。
//!
//! 本模块只查询 0049/0050 迁移产生的预演表，不联网、不修改飞书，也不写入案件表。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuSyncLinkPreview {
    pub id: String,
    pub local_case_id: String,
    pub local_case_name: String,
    pub record_id: String,
    pub link_source: String,
    pub status: String,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuSyncInboxPreview {
    pub id: String,
    pub record_id: String,
    pub display_name: String,
    pub legal_type: Option<String>,
    pub case_no: Option<String>,
    pub remote_modified_at: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuSyncChangePreview {
    pub id: String,
    pub case_name: String,
    pub field_key: String,
    pub field_label: String,
    pub local_value_json: Option<String>,
    pub feishu_value_json: Option<String>,
    pub classification: String,
    pub proposed_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuSyncConflictPreview {
    pub id: String,
    pub case_name: String,
    pub field_key: String,
    pub local_value_json: Option<String>,
    pub feishu_value_json: Option<String>,
    pub status: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuSyncRunPreview {
    pub id: String,
    pub mode: String,
    pub status: String,
    pub active_case_filter: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub counts_json: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuSyncPreview {
    pub bound_cases: Vec<FeishuSyncLinkPreview>,
    pub pending_cases: Vec<FeishuSyncInboxPreview>,
    pub proposed_changes: Vec<FeishuSyncChangePreview>,
    pub conflicts: Vec<FeishuSyncConflictPreview>,
    pub recent_runs: Vec<FeishuSyncRunPreview>,
}

pub async fn get_preview(pool: &SqlitePool) -> Result<FeishuSyncPreview, String> {
    let bound_cases = sqlx::query_as::<_, FeishuSyncLinkPreview>(
        r#"SELECT l.id,
                  l.local_entity_id AS local_case_id,
                  COALESCE(
                    NULLIF(trim(c.display_name_override), ''),
                    CASE WHEN trim(COALESCE(c.agg_cause, c.cause, '')) <> '' THEN
                      CASE
                        WHEN trim(COALESCE(CASE WHEN c.legal_domain = 'criminal'
                          THEN json_extract(c.agg_defendants, '$[0]')
                          ELSE json_extract(c.agg_plaintiffs, '$[0]') END, '')) <> ''
                         AND instr(COALESCE(c.agg_cause, c.cause, ''),
                           CASE WHEN c.legal_domain = 'criminal'
                             THEN json_extract(c.agg_defendants, '$[0]')
                             ELSE json_extract(c.agg_plaintiffs, '$[0]') END) = 0
                        THEN (CASE WHEN c.legal_domain = 'criminal'
                          THEN json_extract(c.agg_defendants, '$[0]')
                          ELSE json_extract(c.agg_plaintiffs, '$[0]') END)
                          || COALESCE(c.agg_cause, c.cause)
                        ELSE COALESCE(c.agg_cause, c.cause)
                      END
                    END,
                    CASE WHEN trim(COALESCE(p.suspect_or_defendant_name, '')) <> ''
                           AND trim(COALESCE(p.suspected_charge, '')) <> ''
                      THEN p.suspect_or_defendant_name || p.suspected_charge END,
                    c.name, l.local_entity_id)
                    AS local_case_name,
                  l.record_id, l.link_source, l.status, l.last_synced_at
           FROM feishu_sync_links l
           LEFT JOIN cases c ON l.entity_type = 'case' AND c.id = l.local_entity_id
           LEFT JOIN criminal_case_profiles p ON p.case_id = c.id
           WHERE l.entity_type = 'case' AND l.status = 'active'
           ORDER BY local_case_name COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取已绑定案件失败: {e}"))?;

    let pending_cases = sqlx::query_as::<_, FeishuSyncInboxPreview>(
        r#"SELECT id, record_id, display_name, legal_type, case_no,
                  remote_modified_at, status
           FROM feishu_sync_inbox
           WHERE status = 'pending_binding'
           ORDER BY updated_at DESC, display_name COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取待绑定案件失败: {e}"))?;

    let proposed_changes = sqlx::query_as::<_, FeishuSyncChangePreview>(
        r#"SELECT ch.id,
                  COALESCE(NULLIF(trim(c.display_name_override), ''),
                           CASE WHEN trim(COALESCE(c.agg_cause, c.cause, '')) <> '' THEN
                             CASE WHEN trim(COALESCE(CASE WHEN c.legal_domain = 'criminal'
                               THEN json_extract(c.agg_defendants, '$[0]')
                               ELSE json_extract(c.agg_plaintiffs, '$[0]') END, '')) <> ''
                               AND instr(COALESCE(c.agg_cause, c.cause, ''),
                                 CASE WHEN c.legal_domain = 'criminal'
                                   THEN json_extract(c.agg_defendants, '$[0]')
                                   ELSE json_extract(c.agg_plaintiffs, '$[0]') END) = 0
                               THEN (CASE WHEN c.legal_domain = 'criminal'
                                 THEN json_extract(c.agg_defendants, '$[0]')
                                 ELSE json_extract(c.agg_plaintiffs, '$[0]') END)
                                 || COALESCE(c.agg_cause, c.cause)
                               ELSE COALESCE(c.agg_cause, c.cause) END END,
                           CASE WHEN trim(COALESCE(p.suspect_or_defendant_name, '')) <> ''
                             AND trim(COALESCE(p.suspected_charge, '')) <> ''
                             THEN p.suspect_or_defendant_name || p.suspected_charge END,
                           COALESCE(c.agg_cause, c.cause), c.name,
                           l.local_entity_id, '未绑定案件') AS case_name,
                  ch.field_key, ch.field_label, ch.local_value_json,
                  ch.feishu_value_json, ch.classification, ch.proposed_action
           FROM feishu_sync_field_previews ch
           LEFT JOIN feishu_sync_links l ON ch.link_id = l.id
           LEFT JOIN cases c ON l.entity_type = 'case' AND c.id = l.local_entity_id
           LEFT JOIN criminal_case_profiles p ON p.case_id = c.id
           WHERE ch.run_id = (
               SELECT id FROM feishu_sync_runs
               WHERE mode = 'readonly_preflight'
               ORDER BY started_at DESC LIMIT 1
           ) AND ch.proposed_action <> 'none'
           ORDER BY ch.created_at DESC, case_name COLLATE NOCASE, ch.field_key"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取拟更新字段失败: {e}"))?;

    let conflicts = sqlx::query_as::<_, FeishuSyncConflictPreview>(
        r#"SELECT cf.id,
                  COALESCE(NULLIF(trim(c.display_name_override), ''),
                           CASE WHEN trim(COALESCE(c.agg_cause, c.cause, '')) <> '' THEN
                             CASE WHEN trim(COALESCE(CASE WHEN c.legal_domain = 'criminal'
                               THEN json_extract(c.agg_defendants, '$[0]')
                               ELSE json_extract(c.agg_plaintiffs, '$[0]') END, '')) <> ''
                               AND instr(COALESCE(c.agg_cause, c.cause, ''),
                                 CASE WHEN c.legal_domain = 'criminal'
                                   THEN json_extract(c.agg_defendants, '$[0]')
                                   ELSE json_extract(c.agg_plaintiffs, '$[0]') END) = 0
                               THEN (CASE WHEN c.legal_domain = 'criminal'
                                 THEN json_extract(c.agg_defendants, '$[0]')
                                 ELSE json_extract(c.agg_plaintiffs, '$[0]') END)
                                 || COALESCE(c.agg_cause, c.cause)
                               ELSE COALESCE(c.agg_cause, c.cause) END END,
                           CASE WHEN trim(COALESCE(p.suspect_or_defendant_name, '')) <> ''
                             AND trim(COALESCE(p.suspected_charge, '')) <> ''
                             THEN p.suspect_or_defendant_name || p.suspected_charge END,
                           COALESCE(c.agg_cause, c.cause), c.name,
                           l.local_entity_id) AS case_name,
                  cf.field_key, cf.local_value_json, cf.feishu_value_json,
                  cf.status, cf.created_at
           FROM feishu_sync_conflicts cf
           JOIN feishu_sync_links l ON cf.link_id = l.id
           LEFT JOIN cases c ON l.entity_type = 'case' AND c.id = l.local_entity_id
           LEFT JOIN criminal_case_profiles p ON p.case_id = c.id
           WHERE cf.status = 'pending'
           ORDER BY cf.created_at DESC"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取冲突字段失败: {e}"))?;

    let recent_runs = sqlx::query_as::<_, FeishuSyncRunPreview>(
        r#"SELECT id, mode, status, active_case_filter, started_at, completed_at,
                  counts_json, error_code, error_message
           FROM feishu_sync_runs
           ORDER BY started_at DESC
           LIMIT 10"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取同步运行记录失败: {e}"))?;

    Ok(FeishuSyncPreview {
        bound_cases,
        pending_cases,
        proposed_changes,
        conflicts,
        recent_runs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preview_reads_all_sections_without_mutating_cases() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let case_id = "preview-case";
        sqlx::query(
            "INSERT INTO cases (id,name,case_type,source_folder,management_status) VALUES (?1,?2,'诉讼','C:/preview','active')",
        )
        .bind(case_id)
        .bind("本地案件")
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO feishu_sync_links (id,entity_type,local_entity_id,app_token,table_id,record_id,status) VALUES ('link','case',?1,'app','table','record','active')")
            .bind(case_id).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO feishu_sync_runs (id,mode,status) VALUES ('run','readonly_preflight','succeeded')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO feishu_sync_field_previews (id,run_id,link_id,field_key,field_label,local_value_json,feishu_value_json,classification,proposed_action) VALUES ('change','run','link','stage','案件阶段','\"侦查\"','\"审查起诉\"','fill_local_blank','pull_to_local')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO feishu_sync_conflicts (id,link_id,field_key,local_value_json,feishu_value_json) VALUES ('conflict','link','court','\"本地法院\"','\"飞书法院\"')")
            .execute(&pool).await.unwrap();

        let before: (String,) = sqlx::query_as("SELECT name FROM cases WHERE id = ?1")
            .bind(case_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        let preview = get_preview(&pool).await.unwrap();
        let after: (String,) = sqlx::query_as("SELECT name FROM cases WHERE id = ?1")
            .bind(case_id)
            .fetch_one(&pool)
            .await
            .unwrap();

        assert_eq!(preview.bound_cases.len(), 1);
        assert_eq!(preview.proposed_changes.len(), 1);
        assert_eq!(preview.conflicts.len(), 1);
        assert_eq!(preview.recent_runs.len(), 1);
        assert_eq!(before, after);
    }
}
