//! 飞书案件管理同步的只读预览。
//!
//! 本模块只查询 0049/0050 迁移产生的预演表，不联网、不修改飞书，也不写入案件表。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

use crate::feishu::FeishuRemoteCaseRecord;

const ACTIVE_FILTER: &str = "在办";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuPullResult {
    pub run_id: String,
    pub remote_count: usize,
    pub bound_count: usize,
    pub pending_count: usize,
    pub proposed_change_count: usize,
}

#[derive(Debug, Clone)]
struct MappedRemoteCase {
    record_id: String,
    display_name: String,
    legal_type: Option<String>,
    legal_domain: Option<String>,
    case_no: Option<String>,
    stage: Option<String>,
    cause: Option<String>,
    authority: Option<String>,
    party: Option<String>,
    modified_at: Option<String>,
    payload: Value,
}

#[derive(FromRow)]
struct LocalCaseComparison {
    display_name: Option<String>,
    legal_domain: Option<String>,
    case_no: Option<String>,
    management_status: Option<String>,
    stage: Option<String>,
    cause: Option<String>,
    authority: Option<String>,
    party: Option<String>,
}

fn clean(value: String) -> Option<String> {
    let value = value.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn field_text(value: Option<&Value>) -> Option<String> {
    fn collect(value: &Value, output: &mut Vec<String>) {
        match value {
            Value::String(text) => {
                if !text.trim().is_empty() {
                    output.push(text.trim().to_string());
                }
            }
            Value::Number(number) => output.push(number.to_string()),
            Value::Bool(boolean) => output.push(boolean.to_string()),
            Value::Array(items) => items.iter().for_each(|item| collect(item, output)),
            Value::Object(object) => {
                for key in ["text", "name", "title", "value", "full_name"] {
                    if let Some(item) = object.get(key) {
                        collect(item, output);
                        if !output.is_empty() {
                            break;
                        }
                    }
                }
            }
            Value::Null => {}
        }
    }

    let mut parts = Vec::new();
    if let Some(value) = value {
        collect(value, &mut parts);
    }
    clean(parts.join("、"))
}

fn map_legal_domain(legal_type: Option<&str>) -> Option<String> {
    let value = legal_type?.trim();
    if value.contains("刑事") {
        Some("criminal".into())
    } else if value.contains("民事") {
        Some("civil".into())
    } else if value.contains("执行") {
        Some("other".into())
    } else {
        None
    }
}

fn map_remote(record: &FeishuRemoteCaseRecord) -> Result<MappedRemoteCase, String> {
    let fields = record
        .fields
        .as_object()
        .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 案件记录 fields 不是对象".to_string())?;
    let status = field_text(fields.get("☑状态"))
        .or_else(|| field_text(fields.get("☑️状态")))
        .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 找不到“☑状态”字段".to_string())?;
    if status != ACTIVE_FILTER {
        return Err("FEISHU_FILTER_MISMATCH: 飞书返回了非在办案件，已拒绝写入预演".to_string());
    }
    let display_name = field_text(fields.get("案件名称")).unwrap_or_default();
    let legal_type = field_text(fields.get("类型"));
    let case_no = field_text(fields.get("案号"));
    let payload = json!({
        "display_name": display_name,
        "legal_type": legal_type,
        "legal_domain": map_legal_domain(legal_type.as_deref()),
        "case_no": case_no,
        "status": status,
        "stage": field_text(fields.get("案件进度")),
        "cause": field_text(fields.get("案由")),
        "authority": field_text(fields.get("管辖法院")),
        "party": field_text(fields.get("当事人")),
    });
    Ok(MappedRemoteCase {
        record_id: record.record_id.clone(),
        display_name,
        legal_domain: map_legal_domain(legal_type.as_deref()),
        legal_type,
        case_no,
        stage: field_text(fields.get("案件进度")),
        cause: field_text(fields.get("案由")),
        authority: field_text(fields.get("管辖法院")),
        party: field_text(fields.get("当事人")),
        modified_at: record.last_modified_time.clone(),
        payload,
    })
}

fn stable_error(error: &str) -> (String, String) {
    let (code, message) = error
        .split_once(':')
        .map(|(code, message)| (code.trim(), message.trim()))
        .unwrap_or(("FEISHU_PULL_FAILED", "本次读取失败"));
    let allowed =
        code.starts_with("FEISHU_") && code.bytes().all(|b| b.is_ascii_uppercase() || b == b'_');
    (
        if allowed { code } else { "FEISHU_PULL_FAILED" }.to_string(),
        message.chars().take(160).collect(),
    )
}

pub async fn start_pull_run(pool: &SqlitePool) -> Result<String, String> {
    let run_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO feishu_sync_runs (id,mode,status,active_case_filter) VALUES (?1,'pull','running',?2)")
        .bind(&run_id)
        .bind(ACTIVE_FILTER)
        .execute(pool)
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 无法创建预演运行记录".to_string())?;
    Ok(run_id)
}

pub async fn fail_pull_run(pool: &SqlitePool, run_id: &str, error: &str) {
    let (code, message) = stable_error(error);
    let _ = sqlx::query("UPDATE feishu_sync_runs SET status='failed',completed_at=datetime('now'),error_code=?2,error_message=?3 WHERE id=?1")
        .bind(run_id).bind(code).bind(message).execute(pool).await;
}

fn json_value(value: Option<&str>) -> Option<String> {
    value.map(|value| serde_json::to_string(value).unwrap_or_else(|_| "null".to_string()))
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '，' | ',' | '、' | '。'))
        .collect::<String>()
        .to_lowercase()
}

fn classify(local: Option<&str>, remote: Option<&str>) -> (&'static str, &'static str) {
    match (
        local.map(str::trim).filter(|v| !v.is_empty()),
        remote.map(str::trim).filter(|v| !v.is_empty()),
    ) {
        (None, None) => ("equal", "none"),
        (None, Some(_)) => ("remote_candidate", "pull_to_local"),
        (Some(_), None) => ("local_only", "none"),
        (Some(local), Some(remote)) if local == remote => ("equal", "none"),
        (Some(local), Some(remote)) if normalize(local) == normalize(remote) => {
            ("semantic_equivalent", "none")
        }
        _ => ("needs_review", "review"),
    }
}

pub async fn complete_pull_preview(
    pool: &SqlitePool,
    run_id: &str,
    app_token: &str,
    table_id: &str,
    records: Vec<FeishuRemoteCaseRecord>,
) -> Result<FeishuPullResult, String> {
    let mapped = records
        .iter()
        .map(map_remote)
        .collect::<Result<Vec<_>, _>>()?;
    let remote_ids: HashSet<&str> = mapped.iter().map(|item| item.record_id.as_str()).collect();
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 无法开始预演事务".to_string())?;
    let mut bound_count = 0usize;
    let mut pending_count = 0usize;
    let mut proposed_change_count = 0usize;

    for remote in &mapped {
        let existing_link: Option<(String, String)> = sqlx::query_as(
            "SELECT id,local_entity_id FROM feishu_sync_links WHERE app_token=?1 AND table_id=?2 AND record_id=?3 AND entity_type='case' AND slot_key='' LIMIT 1",
        )
        .bind(app_token)
        .bind(table_id)
        .bind(&remote.record_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 读取既有绑定失败".to_string())?;

        let (link_id, case_id, link_source) = if let Some((link_id, case_id)) = existing_link {
            (Some(link_id), Some(case_id), None)
        } else {
            let mut matches: Vec<String> = Vec::new();
            let mut source = None;
            if let Some(case_no) = remote.case_no.as_deref() {
                matches = sqlx::query_scalar(
                    "SELECT id FROM cases WHERE trim(COALESCE(NULLIF(agg_case_no,''),case_no,''))=trim(?1)",
                )
                .bind(case_no)
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 案号匹配失败".to_string())?;
                if matches.len() == 1 {
                    source = Some("exact_case_no");
                }
            }
            if matches.len() != 1 && !remote.display_name.trim().is_empty() {
                matches = sqlx::query_scalar(
                    "SELECT id FROM cases WHERE trim(COALESCE(NULLIF(display_name_override,''),name))=trim(?1)",
                )
                .bind(&remote.display_name)
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 案件名称匹配失败".to_string())?;
                source = (matches.len() == 1).then_some("exact_display_name");
            }
            if matches.len() == 1 {
                let link_id = Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO feishu_sync_links (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status,confirmed_at) VALUES (?1,'case',?2,?3,?4,?5,?6,'active',datetime('now'))")
                    .bind(&link_id).bind(&matches[0]).bind(app_token).bind(table_id)
                    .bind(&remote.record_id).bind(source.unwrap_or("exact_display_name"))
                    .execute(&mut *tx).await
                    .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 创建案件绑定失败".to_string())?;
                (Some(link_id), Some(matches[0].clone()), source)
            } else {
                (None, None, None)
            }
        };

        let payload_json = serde_json::to_string(&remote.payload)
            .map_err(|_| "FEISHU_RESPONSE_INVALID: 无法规范化飞书字段".to_string())?;
        let inbox_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"INSERT INTO feishu_sync_inbox
               (id,app_token,table_id,record_id,display_name,legal_type,case_no,remote_modified_at,mapped_payload_json,status,bound_case_id,resolved_at)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,CASE WHEN ?11 IS NULL THEN NULL ELSE datetime('now') END)
               ON CONFLICT(app_token,table_id,record_id) DO UPDATE SET
                 display_name=excluded.display_name,legal_type=excluded.legal_type,case_no=excluded.case_no,
                 remote_modified_at=excluded.remote_modified_at,mapped_payload_json=excluded.mapped_payload_json,
                 status=CASE WHEN feishu_sync_inbox.status='ignored' THEN 'ignored' ELSE excluded.status END,
                 bound_case_id=CASE WHEN feishu_sync_inbox.status='ignored' THEN feishu_sync_inbox.bound_case_id ELSE excluded.bound_case_id END,
                 resolved_at=CASE WHEN feishu_sync_inbox.status='ignored' THEN feishu_sync_inbox.resolved_at ELSE excluded.resolved_at END,
                 updated_at=datetime('now')"#,
        )
        .bind(inbox_id).bind(app_token).bind(table_id).bind(&remote.record_id)
        .bind(&remote.display_name).bind(&remote.legal_type).bind(&remote.case_no)
        .bind(&remote.modified_at).bind(&payload_json)
        .bind(if case_id.is_some() { "bound" } else { "pending_binding" })
        .bind(&case_id)
        .execute(&mut *tx).await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 更新待绑定预演失败".to_string())?;

        if let (Some(link_id), Some(case_id)) = (link_id.as_deref(), case_id.as_deref()) {
            bound_count += 1;
            sqlx::query("UPDATE feishu_sync_links SET status='active',last_feishu_modified_at=?2,last_synced_at=datetime('now'),updated_at=datetime('now') WHERE id=?1")
                .bind(link_id).bind(&remote.modified_at).execute(&mut *tx).await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 更新绑定状态失败".to_string())?;
            let payload_hash = format!("{:x}", Sha256::digest(payload_json.as_bytes()));
            sqlx::query("INSERT OR IGNORE INTO feishu_sync_snapshots (id,link_id,feishu_modified_at,payload_hash,mapped_payload_json) VALUES (?1,?2,?3,?4,?5)")
                .bind(Uuid::new_v4().to_string()).bind(link_id).bind(&remote.modified_at)
                .bind(payload_hash).bind(&payload_json).execute(&mut *tx).await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 保存只读快照失败".to_string())?;

            let local: LocalCaseComparison = sqlx::query_as(
                r#"SELECT COALESCE(NULLIF(display_name_override,''),name) AS display_name,
                          legal_domain,
                          COALESCE(NULLIF(agg_case_no,''),case_no) AS case_no,
                          management_status,
                          COALESCE(NULLIF(stage,''),NULL) AS stage,
                          COALESCE(NULLIF(agg_cause,''),cause) AS cause,
                          COALESCE(NULLIF(agg_court,''),court) AS authority,
                          CASE WHEN legal_domain='criminal' THEN json_extract(agg_defendants,'$[0]') ELSE json_extract(agg_plaintiffs,'$[0]') END AS party
                   FROM cases WHERE id=?1"#,
            ).bind(case_id).fetch_one(&mut *tx).await
            .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 读取本地案件用于比对失败".to_string())?;
            let comparisons = [
                (
                    "display_name",
                    "案件名称",
                    local.display_name.as_deref(),
                    clean(remote.display_name.clone()),
                ),
                (
                    "legal_domain",
                    "案件类型",
                    local.legal_domain.as_deref(),
                    remote.legal_domain.clone(),
                ),
                (
                    "case_no",
                    "案号",
                    local.case_no.as_deref(),
                    remote.case_no.clone(),
                ),
                (
                    "management_status",
                    "案件状态",
                    local.management_status.as_deref(),
                    Some("active".to_string()),
                ),
                (
                    "stage",
                    "案件阶段",
                    local.stage.as_deref(),
                    remote.stage.clone(),
                ),
                (
                    "cause",
                    "案由/罪名",
                    local.cause.as_deref(),
                    remote.cause.clone(),
                ),
                (
                    "authority",
                    "承办机关",
                    local.authority.as_deref(),
                    remote.authority.clone(),
                ),
                (
                    "party",
                    "当事人",
                    local.party.as_deref(),
                    remote.party.clone(),
                ),
            ];
            for (field_key, field_label, local_value, remote_value) in comparisons {
                let (classification, proposed_action) =
                    classify(local_value, remote_value.as_deref());
                if proposed_action != "none" {
                    proposed_change_count += 1;
                }
                sqlx::query("INSERT INTO feishu_sync_field_previews (id,run_id,link_id,field_key,field_label,local_value_json,feishu_value_json,classification,proposed_action) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)")
                    .bind(Uuid::new_v4().to_string()).bind(run_id).bind(link_id).bind(field_key).bind(field_label)
                    .bind(json_value(local_value)).bind(json_value(remote_value.as_deref())).bind(classification).bind(proposed_action)
                    .execute(&mut *tx).await
                    .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 保存字段差异预演失败".to_string())?;
            }
            let _ = link_source;
        } else {
            pending_count += 1;
        }
    }

    let previous_pending: Vec<String> = sqlx::query_scalar(
        "SELECT record_id FROM feishu_sync_inbox WHERE app_token=?1 AND table_id=?2 AND status='pending_binding'",
    ).bind(app_token).bind(table_id).fetch_all(&mut *tx).await
    .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 检查历史待绑定记录失败".to_string())?;
    for record_id in previous_pending {
        if !remote_ids.contains(record_id.as_str()) {
            sqlx::query("UPDATE feishu_sync_inbox SET status='archived',updated_at=datetime('now') WHERE app_token=?1 AND table_id=?2 AND record_id=?3 AND status='pending_binding'")
                .bind(app_token).bind(table_id).bind(record_id).execute(&mut *tx).await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 归档过期待绑定记录失败".to_string())?;
        }
    }
    let counts = json!({"remote": mapped.len(), "bound": bound_count, "pending": pending_count, "proposed_changes": proposed_change_count});
    sqlx::query("UPDATE feishu_sync_runs SET status='succeeded',completed_at=datetime('now'),counts_json=?2,error_code=NULL,error_message=NULL WHERE id=?1")
        .bind(run_id).bind(counts.to_string()).execute(&mut *tx).await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 无法完成预演运行记录".to_string())?;
    tx.commit()
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 提交预演事务失败".to_string())?;
    Ok(FeishuPullResult {
        run_id: run_id.to_string(),
        remote_count: mapped.len(),
        bound_count,
        pending_count,
        proposed_change_count,
    })
}

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
               WHERE mode IN ('readonly_preflight','pull')
                 AND status IN ('succeeded','partial')
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

    #[tokio::test]
    async fn pull_preview_is_idempotent_and_never_updates_case_business_fields() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO cases (id,name,case_type,source_folder,case_no,legal_domain,management_status) VALUES ('case-1','测试案件','诉讼','C:/preview','（2026）测1号','civil','active')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let remote = FeishuRemoteCaseRecord {
            record_id: "rec-1".into(),
            fields: json!({
                "案件名称": "测试案件",
                "类型": "民事诉讼",
                "案号": "（2026）测1号",
                "☑状态": "在办",
                "案件进度": "一审"
            }),
            last_modified_time: Some("1784518994000".into()),
        };

        for _ in 0..2 {
            let run_id = start_pull_run(&pool).await.unwrap();
            let result = complete_pull_preview(
                &pool,
                &run_id,
                "bascn_test",
                "tbl_test",
                vec![remote.clone()],
            )
            .await
            .unwrap();
            assert_eq!(result.bound_count, 1);
            assert_eq!(result.pending_count, 0);
        }

        let link_count: (i64,) = sqlx::query_as("SELECT count(*) FROM feishu_sync_links")
            .fetch_one(&pool)
            .await
            .unwrap();
        let inbox_count: (i64,) = sqlx::query_as("SELECT count(*) FROM feishu_sync_inbox")
            .fetch_one(&pool)
            .await
            .unwrap();
        let snapshot_count: (i64,) = sqlx::query_as("SELECT count(*) FROM feishu_sync_snapshots")
            .fetch_one(&pool)
            .await
            .unwrap();
        let case_after: (String, String, String, String) = sqlx::query_as(
            "SELECT name,case_no,legal_domain,management_status FROM cases WHERE id='case-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(link_count.0, 1);
        assert_eq!(inbox_count.0, 1);
        assert_eq!(snapshot_count.0, 1);
        assert_eq!(
            case_after,
            (
                "测试案件".into(),
                "（2026）测1号".into(),
                "civil".into(),
                "active".into()
            )
        );
    }
}
