//! 飞书案件管理同步的只读预览。
//!
//! 本模块只查询 0049/0050 迁移产生的预演表，不联网、不修改飞书，也不写入案件表。

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{FromRow, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

use crate::feishu::{FeishuCaseManagementRecords, FeishuRemoteCaseRecord};

const ACTIVE_FILTER: &str = "在办";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuPullResult {
    pub run_id: String,
    pub remote_count: usize,
    pub bound_count: usize,
    pub pending_count: usize,
    pub proposed_change_count: usize,
    pub work_item_count: usize,
    pub stage_count: usize,
    pub contact_count: usize,
    pub archived_entity_count: usize,
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
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '，' | ','
                        | '、'
                        | '。'
                        | '.'
                        | '-'
                        | '_'
                        | '—'
                        | '－'
                        | '/'
                        | '\\'
                        | '（'
                        | '）'
                        | '('
                        | ')'
                        | '：'
                        | ':'
                )
        })
        .collect::<String>()
        .to_lowercase()
}

fn valid_date_parts(year: &str, month: &str, day: &str) -> bool {
    let Ok(year) = year.parse::<u16>() else {
        return false;
    };
    let Ok(month) = month.parse::<u8>() else {
        return false;
    };
    let Ok(day) = day.parse::<u8>() else {
        return false;
    };
    (2000..=2099).contains(&year) && (1..=12).contains(&month) && (1..=31).contains(&day)
}

/// 仅用于候选匹配，绝不回写本地文件夹名或飞书案件名。
fn strip_common_date_prefix(value: &str) -> &str {
    let value = value.trim();
    let bytes = value.as_bytes();
    let mut end = None;
    if bytes.len() >= 8
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && valid_date_parts(&value[..4], &value[4..6], &value[6..8])
    {
        end = Some(8);
    } else if bytes.len() >= 10
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && matches!(bytes[4], b'-' | b'.' | b'/' | b'_')
        && bytes[5..7].iter().all(u8::is_ascii_digit)
        && matches!(bytes[7], b'-' | b'.' | b'/' | b'_')
        && bytes[8..10].iter().all(u8::is_ascii_digit)
        && valid_date_parts(&value[..4], &value[5..7], &value[8..10])
    {
        end = Some(10);
    }
    let Some(mut end) = end else {
        return value;
    };
    while let Some(character) = value[end..].chars().next() {
        if character.is_whitespace()
            || matches!(character, '-' | '_' | '—' | '－' | '.' | '/' | '：' | ':')
        {
            end += character.len_utf8();
        } else {
            break;
        }
    }
    value[end..].trim()
}

fn normalize_case_name(value: &str) -> String {
    normalize(strip_common_date_prefix(value))
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

async fn complete_pull_internal(
    pool: &SqlitePool,
    run_id: &str,
    app_token: &str,
    table_id: &str,
    records: Vec<FeishuRemoteCaseRecord>,
    management: Option<&FeishuCaseManagementRecords>,
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
            "SELECT id,local_entity_id FROM feishu_sync_links WHERE app_token=?1 AND table_id=?2 AND record_id=?3 AND entity_type='case' AND slot_key='' AND status='active' LIMIT 1",
        )
        .bind(app_token)
        .bind(table_id)
        .bind(&remote.record_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 读取既有绑定失败".to_string())?;

        let inbox_state: Option<(String, String, i64)> = sqlx::query_as(
            "SELECT id,status,auto_bind_suppressed FROM feishu_sync_inbox WHERE app_token=?1 AND table_id=?2 AND record_id=?3 LIMIT 1",
        )
        .bind(app_token)
        .bind(table_id)
        .bind(&remote.record_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 读取待绑定状态失败".to_string())?;
        let allow_auto_bind = inbox_state
            .as_ref()
            .is_none_or(|(_, status, suppressed)| status != "ignored" && *suppressed == 0);

        let (link_id, case_id, auto_bound) = if let Some((link_id, case_id)) = existing_link {
            (Some(link_id), Some(case_id), false)
        } else {
            // 名称（包括剥离本地日期前缀后的名称）只生成推荐，不在拉取时直接绑定。
            // 唯一、精确案号是唯一允许的自动绑定条件。
            let matches: Vec<String> = if allow_auto_bind {
                if let Some(case_no) = remote.case_no.as_deref() {
                    sqlx::query_scalar(
                    "SELECT id FROM cases WHERE trim(COALESCE(NULLIF(agg_case_no,''),case_no,''))=trim(?1)",
                )
                .bind(case_no)
                .fetch_all(&mut *tx)
                .await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 案号匹配失败".to_string())?
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };
            if matches.len() == 1 {
                let link_id = Uuid::new_v4().to_string();
                sqlx::query("INSERT INTO feishu_sync_links (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status,confirmed_at) VALUES (?1,'case',?2,?3,?4,?5,?6,'active',datetime('now'))")
                    .bind(&link_id).bind(&matches[0]).bind(app_token).bind(table_id)
                    .bind(&remote.record_id).bind("exact_case_no")
                    .execute(&mut *tx).await
                    .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 创建案件绑定失败".to_string())?;
                (Some(link_id), Some(matches[0].clone()), true)
            } else {
                (None, None, false)
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

        let inbox_id: String = sqlx::query_scalar(
            "SELECT id FROM feishu_sync_inbox WHERE app_token=?1 AND table_id=?2 AND record_id=?3",
        )
        .bind(app_token)
        .bind(table_id)
        .bind(&remote.record_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 读取待绑定记录失败".to_string())?;
        if auto_bound {
            sqlx::query("INSERT INTO feishu_sync_binding_audits (id,inbox_id,action,previous_status,next_status,next_case_id) VALUES (?1,?2,'auto_bind',?3,'bound',?4)")
                .bind(Uuid::new_v4().to_string())
                .bind(&inbox_id)
                .bind(inbox_state.as_ref().map(|(_, status, _)| status.as_str()))
                .bind(case_id.as_deref())
                .execute(&mut *tx).await
                .map_err(|_| "FEISHU_DB_PREVIEW_WRITE_FAILED: 保存自动绑定审计失败".to_string())?;
        }

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
    let entity_counts = if let Some(bundle) = management {
        crate::db::feishu_entities::import_management_records(
            &mut tx,
            run_id,
            app_token,
            table_id,
            bundle,
        )
        .await?
    } else {
        Default::default()
    };
    let counts = json!({
        "remote": mapped.len(),
        "bound": bound_count,
        "pending": pending_count,
        "proposed_changes": proposed_change_count,
        "work_items": entity_counts.work_items,
        "stages": entity_counts.stages,
        "contacts": entity_counts.contacts,
        "archived_entities": entity_counts.archived,
    });
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
        work_item_count: entity_counts.work_items,
        stage_count: entity_counts.stages,
        contact_count: entity_counts.contacts,
        archived_entity_count: entity_counts.archived,
    })
}

pub async fn complete_pull_preview(
    pool: &SqlitePool,
    run_id: &str,
    app_token: &str,
    table_id: &str,
    records: Vec<FeishuRemoteCaseRecord>,
) -> Result<FeishuPullResult, String> {
    complete_pull_internal(pool, run_id, app_token, table_id, records, None).await
}

pub async fn complete_pull_with_entities(
    pool: &SqlitePool,
    run_id: &str,
    app_token: &str,
    table_id: &str,
    bundle: FeishuCaseManagementRecords,
) -> Result<FeishuPullResult, String> {
    let records = bundle.cases.clone();
    complete_pull_internal(
        pool,
        run_id,
        app_token,
        table_id,
        records,
        Some(&bundle),
    )
    .await
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
    pub recommended_case_id: Option<String>,
    pub recommendation_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct FeishuLocalCaseOption {
    pub id: String,
    pub display_name: String,
    pub legal_domain: String,
    pub case_no: Option<String>,
    pub cause: Option<String>,
    pub party: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct FeishuInboxRow {
    id: String,
    record_id: String,
    display_name: String,
    legal_type: Option<String>,
    case_no: Option<String>,
    remote_modified_at: Option<String>,
    status: String,
    mapped_payload_json: String,
}

fn values_overlap(left: &str, right: &str) -> bool {
    let left = normalize(left);
    let right = normalize(right);
    !left.is_empty() && !right.is_empty() && (left.contains(&right) || right.contains(&left))
}

fn recommendation_for(
    inbox: &FeishuInboxRow,
    local_cases: &[FeishuLocalCaseOption],
) -> (Option<String>, Option<String>) {
    let payload: Value = serde_json::from_str(&inbox.mapped_payload_json).unwrap_or(Value::Null);
    let remote_domain = payload
        .get("legal_domain")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let remote_party = payload
        .get("party")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let remote_cause = payload
        .get("cause")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let remote_name = normalize_case_name(&inbox.display_name);
    let mut matches = Vec::new();
    for local in local_cases {
        if remote_domain.is_empty()
            || remote_domain == "unknown"
            || local.legal_domain == "unknown"
            || local.legal_domain != remote_domain
            || normalize_case_name(&local.display_name) != remote_name
        {
            continue;
        }
        let party_matches = local.party.as_deref().is_some_and(|party| {
            (!remote_party.is_empty() && values_overlap(party, remote_party))
                || values_overlap(party, &inbox.display_name)
        });
        let cause_matches = local.cause.as_deref().is_some_and(|cause| {
            (!remote_cause.is_empty() && values_overlap(cause, remote_cause))
                || values_overlap(cause, &inbox.display_name)
        });
        if party_matches && cause_matches {
            matches.push(local.id.clone());
        }
    }
    match matches.as_slice() {
        [case_id] => (
            Some(case_id.clone()),
            Some("日期前缀归一化后名称一致，且案件领域、当事人和案由/罪名均通过校验".to_string()),
        ),
        [] => (None, None),
        _ => (None, Some("存在多个同名候选，请人工选择并确认".to_string())),
    }
}

fn inbox_preview(
    row: FeishuInboxRow,
    local_cases: &[FeishuLocalCaseOption],
) -> FeishuSyncInboxPreview {
    let (recommended_case_id, recommendation_reason) = recommendation_for(&row, local_cases);
    FeishuSyncInboxPreview {
        id: row.id,
        record_id: row.record_id,
        display_name: row.display_name,
        legal_type: row.legal_type,
        case_no: row.case_no,
        remote_modified_at: row.remote_modified_at,
        status: row.status,
        recommended_case_id,
        recommendation_reason,
    }
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
    pub ignored_cases: Vec<FeishuSyncInboxPreview>,
    pub available_local_cases: Vec<FeishuLocalCaseOption>,
    pub proposed_changes: Vec<FeishuSyncChangePreview>,
    pub conflicts: Vec<FeishuSyncConflictPreview>,
    pub recent_runs: Vec<FeishuSyncRunPreview>,
}

pub async fn get_preview(pool: &SqlitePool) -> Result<FeishuSyncPreview, String> {
    let available_local_cases = sqlx::query_as::<_, FeishuLocalCaseOption>(
        r#"SELECT c.id,
                  COALESCE(NULLIF(trim(c.display_name_override), ''), c.name, c.id) AS display_name,
                  COALESCE(NULLIF(c.legal_domain, ''), 'unknown') AS legal_domain,
                  COALESCE(NULLIF(c.agg_case_no, ''), NULLIF(c.case_no, '')) AS case_no,
                  COALESCE(NULLIF(c.agg_cause, ''), NULLIF(c.cause, '')) AS cause,
                  CASE WHEN c.legal_domain='criminal'
                    THEN COALESCE(json_extract(c.agg_defendants,'$[0]'), p.suspect_or_defendant_name)
                    ELSE json_extract(c.agg_plaintiffs,'$[0]') END AS party
           FROM cases c
           LEFT JOIN criminal_case_profiles p ON p.case_id=c.id
           WHERE NOT EXISTS (
             SELECT 1 FROM feishu_sync_links l
             WHERE l.entity_type='case' AND l.local_entity_id=c.id AND l.status='active'
           )
           ORDER BY display_name COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取可绑定本地案件失败: {e}"))?;

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

    let pending_rows = sqlx::query_as::<_, FeishuInboxRow>(
        r#"SELECT id, record_id, display_name, legal_type, case_no,
                  remote_modified_at, status, mapped_payload_json
           FROM feishu_sync_inbox
           WHERE status = 'pending_binding'
           ORDER BY updated_at DESC, display_name COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取待绑定案件失败: {e}"))?;
    let pending_cases = pending_rows
        .into_iter()
        .map(|row| inbox_preview(row, &available_local_cases))
        .collect();

    let ignored_rows = sqlx::query_as::<_, FeishuInboxRow>(
        r#"SELECT id, record_id, display_name, legal_type, case_no,
                  remote_modified_at, status, mapped_payload_json
           FROM feishu_sync_inbox
           WHERE status = 'ignored'
           ORDER BY updated_at DESC, display_name COLLATE NOCASE"#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| format!("读取已忽略案件失败: {e}"))?;
    let ignored_cases = ignored_rows
        .into_iter()
        .map(|row| inbox_preview(row, &available_local_cases))
        .collect();

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
        ignored_cases,
        available_local_cases,
        proposed_changes,
        conflicts,
        recent_runs,
    })
}

pub async fn bind_case(pool: &SqlitePool, inbox_id: &str, case_id: &str) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法开始绑定事务".to_string())?;
    let inbox: (String, String, String, String, Option<String>) = sqlx::query_as(
        "SELECT status,app_token,table_id,record_id,bound_case_id FROM feishu_sync_inbox WHERE id=?1",
    )
    .bind(inbox_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法读取待绑定案件".to_string())?
    .ok_or_else(|| "FEISHU_BINDING_NOT_FOUND: 待绑定案件不存在".to_string())?;
    if inbox.0 != "pending_binding" {
        return Err("FEISHU_BINDING_STATE_INVALID: 只有待绑定案件可以确认绑定".to_string());
    }
    let case_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM cases WHERE id=?1)")
        .bind(case_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法校验本地案件".to_string())?;
    if !case_exists {
        return Err("FEISHU_BINDING_CASE_NOT_FOUND: 本地案件不存在".to_string());
    }
    let local_conflict: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM feishu_sync_links WHERE entity_type='case' AND local_entity_id=?1 AND table_id=?2 AND slot_key='' AND status='active')",
    )
    .bind(case_id)
    .bind(&inbox.2)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法校验本地案件绑定".to_string())?;
    if local_conflict {
        return Err("FEISHU_BINDING_CONFLICT: 该本地案件已经绑定其他飞书记录".to_string());
    }
    let remote_link: Option<(String, String)> = sqlx::query_as(
        "SELECT id,status FROM feishu_sync_links WHERE entity_type='case' AND app_token=?1 AND table_id=?2 AND record_id=?3 AND slot_key='' LIMIT 1",
    )
    .bind(&inbox.1)
    .bind(&inbox.2)
    .bind(&inbox.3)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法校验飞书记录绑定".to_string())?;
    if remote_link
        .as_ref()
        .is_some_and(|(_, status)| status == "active")
    {
        return Err("FEISHU_BINDING_CONFLICT: 该飞书案件已经绑定本地案件".to_string());
    }
    if let Some((link_id, _)) = remote_link {
        sqlx::query("UPDATE feishu_sync_links SET local_entity_id=?2,link_source='manual',status='active',confirmed_at=datetime('now'),updated_at=datetime('now') WHERE id=?1")
            .bind(link_id).bind(case_id).execute(&mut *tx).await
            .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法恢复本地绑定".to_string())?;
    } else {
        sqlx::query("INSERT INTO feishu_sync_links (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status,confirmed_at) VALUES (?1,'case',?2,?3,?4,?5,'manual','active',datetime('now'))")
            .bind(Uuid::new_v4().to_string()).bind(case_id).bind(&inbox.1).bind(&inbox.2).bind(&inbox.3)
            .execute(&mut *tx).await
            .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法创建本地绑定".to_string())?;
    }
    sqlx::query("UPDATE feishu_sync_inbox SET status='bound',bound_case_id=?2,resolved_at=datetime('now'),auto_bind_suppressed=0,updated_at=datetime('now') WHERE id=?1")
        .bind(inbox_id).bind(case_id).execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法更新待绑定状态".to_string())?;
    sqlx::query("INSERT INTO feishu_sync_binding_audits (id,inbox_id,action,previous_status,next_status,previous_case_id,next_case_id) VALUES (?1,?2,'manual_bind',?3,'bound',?4,?5)")
        .bind(Uuid::new_v4().to_string()).bind(inbox_id).bind(&inbox.0).bind(&inbox.4).bind(case_id)
        .execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法保存绑定审计".to_string())?;
    tx.commit()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法提交绑定事务".to_string())?;
    Ok(())
}

pub async fn unbind_case(pool: &SqlitePool, link_id: &str) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法开始解除事务".to_string())?;
    let link: (String, String, String, String, String) = sqlx::query_as(
        "SELECT local_entity_id,app_token,table_id,record_id,status FROM feishu_sync_links WHERE id=?1 AND entity_type='case'",
    )
    .bind(link_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法读取绑定".to_string())?
    .ok_or_else(|| "FEISHU_BINDING_NOT_FOUND: 绑定不存在".to_string())?;
    if link.4 != "active" {
        return Err("FEISHU_BINDING_STATE_INVALID: 该绑定已经解除".to_string());
    }
    let inbox: (String, String) = sqlx::query_as(
        "SELECT id,status FROM feishu_sync_inbox WHERE app_token=?1 AND table_id=?2 AND record_id=?3",
    )
    .bind(&link.1).bind(&link.2).bind(&link.3)
    .fetch_optional(&mut *tx).await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法读取关联收件箱".to_string())?
    .ok_or_else(|| "FEISHU_BINDING_NOT_FOUND: 关联收件箱不存在".to_string())?;
    sqlx::query(
        "UPDATE feishu_sync_links SET status='archived',updated_at=datetime('now') WHERE id=?1",
    )
    .bind(link_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法解除绑定".to_string())?;
    sqlx::query("UPDATE feishu_sync_inbox SET status='pending_binding',bound_case_id=NULL,resolved_at=NULL,auto_bind_suppressed=1,updated_at=datetime('now') WHERE id=?1")
        .bind(&inbox.0).execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法恢复待绑定状态".to_string())?;
    sqlx::query("INSERT INTO feishu_sync_binding_audits (id,inbox_id,action,previous_status,next_status,previous_case_id) VALUES (?1,?2,'unbind',?3,'pending_binding',?4)")
        .bind(Uuid::new_v4().to_string()).bind(&inbox.0).bind(&inbox.1).bind(&link.0)
        .execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法保存解除审计".to_string())?;
    tx.commit()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法提交解除事务".to_string())?;
    Ok(())
}

async fn change_inbox_status(
    pool: &SqlitePool,
    inbox_id: &str,
    expected: &str,
    next: &str,
    action: &str,
) -> Result<(), String> {
    let mut tx = pool
        .begin()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法开始状态事务".to_string())?;
    let current: (String, Option<String>) =
        sqlx::query_as("SELECT status,bound_case_id FROM feishu_sync_inbox WHERE id=?1")
            .bind(inbox_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法读取案件状态".to_string())?
            .ok_or_else(|| "FEISHU_BINDING_NOT_FOUND: 待绑定案件不存在".to_string())?;
    if current.0 != expected {
        return Err("FEISHU_BINDING_STATE_INVALID: 案件状态已变化，请刷新后重试".to_string());
    }
    sqlx::query("UPDATE feishu_sync_inbox SET status=?2,bound_case_id=NULL,resolved_at=CASE WHEN ?2='ignored' THEN datetime('now') ELSE NULL END,updated_at=datetime('now') WHERE id=?1")
        .bind(inbox_id).bind(next).execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法更新案件状态".to_string())?;
    sqlx::query("INSERT INTO feishu_sync_binding_audits (id,inbox_id,action,previous_status,next_status,previous_case_id) VALUES (?1,?2,?3,?4,?5,?6)")
        .bind(Uuid::new_v4().to_string()).bind(inbox_id).bind(action).bind(expected).bind(next).bind(&current.1)
        .execute(&mut *tx).await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法保存状态审计".to_string())?;
    tx.commit()
        .await
        .map_err(|_| "FEISHU_BINDING_DB_FAILED: 无法提交状态事务".to_string())?;
    Ok(())
}

pub async fn ignore_case(pool: &SqlitePool, inbox_id: &str) -> Result<(), String> {
    change_inbox_status(pool, inbox_id, "pending_binding", "ignored", "ignore").await
}

pub async fn restore_case(pool: &SqlitePool, inbox_id: &str) -> Result<(), String> {
    change_inbox_status(pool, inbox_id, "ignored", "pending_binding", "restore").await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn active_case_record() -> FeishuRemoteCaseRecord {
        FeishuRemoteCaseRecord {
            record_id: "remote-case-1".into(),
            fields: json!({
                "案件名称": "陈某诈骗案",
                "类型": "刑事诉讼",
                "案号": "（2026）粤01刑初1号",
                "☑状态": "在办",
                "案件进度": [{"record_ids": ["progress-1"]}],
                "☑️阶段表": [{"record_ids": ["stage-1"]}],
                "案件联系表": [{"record_ids": ["contact-1"]}]
            }),
            last_modified_time: Some("1784518994000".into()),
        }
    }

    fn management_bundle(include_children: bool) -> FeishuCaseManagementRecords {
        let mut bundle = FeishuCaseManagementRecords {
            cases: vec![active_case_record()],
            progress: Vec::new(),
            stages: Vec::new(),
            contacts: Vec::new(),
        };
        if include_children {
            bundle.progress.push(FeishuRemoteCaseRecord {
                record_id: "progress-1".into(),
                fields: json!({
                    "所属案件": {"link_record_ids": ["remote-case-1"]},
                    "进度日期": 1784682000000_i64,
                    "进度填写区": [{"text": "已与承办法官沟通"}],
                    "进展类型": "沟通",
                    "小时": 1,
                    "分钟": 15
                }),
                last_modified_time: Some("1784682000000".into()),
            });
            bundle.stages.push(FeishuRemoteCaseRecord {
                record_id: "stage-1".into(),
                fields: json!({
                    "所属案件": {"link_record_ids": ["remote-case-1"]},
                    "程序": "一审",
                    "阶段": "审判",
                    "开始时间": 1784595600000_i64,
                    "提醒时间": {"type": 5, "value": [1784768400000_i64]},
                    "🔁【状态】": {"type": 1, "value": [{"text": "进行中"}]}
                }),
                last_modified_time: Some("1784682000000".into()),
            });
            bundle.contacts.push(FeishuRemoteCaseRecord {
                record_id: "contact-1".into(),
                fields: json!({
                    "🚩案件总表": {"link_record_ids": ["remote-case-1"]},
                    "审判机关": "广州市中级人民法院",
                    "法官": [{"text": "张法官"}],
                    "书记员": [{"text": "李书记员"}],
                    "案号": "（2026）粤01刑初1号",
                    "案件查询码/备注": "查询码123"
                }),
                last_modified_time: Some("1784682000000".into()),
            });
        }
        bundle
    }

    async fn inbound_fixture() -> SqlitePool {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query("INSERT INTO cases (id,name,case_type,source_folder,case_no,legal_domain,management_status,display_name_override) VALUES ('case-1','20260721陈某诈骗案','诉讼','C:/cases/20260721陈某诈骗案','（2026）粤01刑初1号','criminal','active','陈某诈骗案')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO feishu_sync_links (id,entity_type,local_entity_id,app_token,table_id,record_id,link_source,status,confirmed_at) VALUES ('link-1','case','case-1','app','table','remote-case-1','manual','active',datetime('now'))")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO case_work_items (id,case_id,occurred_at,work_type,title,content,source) VALUES ('manual-work','case-1','2026-07-01','other','人工记录','不得覆盖','manual')")
            .execute(&pool).await.unwrap();
        pool
    }

    #[tokio::test]
    async fn inbound_entities_are_idempotent_and_manual_rows_are_preserved() {
        let pool = inbound_fixture().await;
        for _ in 0..2 {
            let run_id = start_pull_run(&pool).await.unwrap();
            let result = complete_pull_with_entities(
                &pool,
                &run_id,
                "app",
                "table",
                management_bundle(true),
            )
            .await
            .unwrap();
            assert_eq!(result.work_item_count, 1);
            assert_eq!(result.stage_count, 1);
            assert_eq!(result.contact_count, 2);
        }
        let work_count: i64 = sqlx::query_scalar("SELECT count(*) FROM case_work_items")
            .fetch_one(&pool).await.unwrap();
        let stage_count: i64 = sqlx::query_scalar("SELECT count(*) FROM case_stage_items")
            .fetch_one(&pool).await.unwrap();
        let contact_count: i64 = sqlx::query_scalar("SELECT count(*) FROM case_agency_contacts")
            .fetch_one(&pool).await.unwrap();
        let manual_content: String = sqlx::query_scalar("SELECT content FROM case_work_items WHERE id='manual-work'")
            .fetch_one(&pool).await.unwrap();
        let case_fields: (String, String) = sqlx::query_as("SELECT name,display_name_override FROM cases WHERE id='case-1'")
            .fetch_one(&pool).await.unwrap();
        let unchanged_audits: i64 = sqlx::query_scalar("SELECT count(*) FROM feishu_sync_entity_audits WHERE action='unchanged'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!((work_count, stage_count, contact_count), (2, 1, 2));
        assert_eq!(manual_content, "不得覆盖");
        assert_eq!(case_fields, ("20260721陈某诈骗案".into(), "陈某诈骗案".into()));
        assert_eq!(unchanged_audits, 4);
    }

    #[tokio::test]
    async fn missing_remote_entities_are_soft_archived() {
        let pool = inbound_fixture().await;
        let first = start_pull_run(&pool).await.unwrap();
        complete_pull_with_entities(&pool, &first, "app", "table", management_bundle(true))
            .await.unwrap();
        let second = start_pull_run(&pool).await.unwrap();
        let result = complete_pull_with_entities(&pool, &second, "app", "table", management_bundle(false))
            .await.unwrap();
        let visible_remote: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM case_work_items WHERE external_source='feishu' AND deleted_at IS NULL) + (SELECT count(*) FROM case_stage_items WHERE external_source='feishu' AND deleted_at IS NULL) + (SELECT count(*) FROM case_agency_contacts WHERE external_source='feishu' AND deleted_at IS NULL)")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(result.archived_entity_count, 4);
        assert_eq!(visible_remote, 0);
    }

    #[tokio::test]
    async fn invalid_child_rolls_back_preview_and_entity_transaction() {
        let pool = inbound_fixture().await;
        let run_id = start_pull_run(&pool).await.unwrap();
        let mut bundle = management_bundle(true);
        bundle.progress[0].fields.as_object_mut().unwrap().remove("进度日期");
        let error = complete_pull_with_entities(&pool, &run_id, "app", "table", bundle)
            .await.unwrap_err();
        assert!(error.contains("缺少进度日期"));
        let snapshot_count: i64 = sqlx::query_scalar("SELECT count(*) FROM feishu_sync_snapshots")
            .fetch_one(&pool).await.unwrap();
        let remote_entity_count: i64 = sqlx::query_scalar("SELECT count(*) FROM case_work_items WHERE external_source='feishu'")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(snapshot_count, 0);
        assert_eq!(remote_entity_count, 0);
    }

    #[test]
    fn date_prefix_is_removed_only_for_matching() {
        assert_eq!(
            strip_common_date_prefix("20260721杨某买卖合同纠纷"),
            "杨某买卖合同纠纷"
        );
        assert_eq!(
            strip_common_date_prefix("2026-07-21_杨某买卖合同纠纷"),
            "杨某买卖合同纠纷"
        );
        assert_eq!(
            strip_common_date_prefix("杨某买卖合同纠纷"),
            "杨某买卖合同纠纷"
        );
        assert_eq!(
            normalize_case_name("2026.07.21 杨某、买卖合同纠纷"),
            normalize_case_name("杨某买卖合同纠纷")
        );
    }

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
        sqlx::query("INSERT INTO case_work_items (id,case_id,occurred_at,work_type,title,content,source,external_source,external_record_id) VALUES ('work-1','case-1','2026-07-21','沟通','联系当事人','已完成沟通','feishu','feishu','work-rec-1')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO case_stage_items (id,case_id,domain,stage_label,status,source,external_source,external_record_id) VALUES ('stage-1','case-1','civil','一审','active','feishu','feishu','stage-rec-1')")
            .execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO case_agency_contacts (id,case_id,agency_name,contact_name,source,external_record_id) VALUES ('contact-1','case-1','测试法院','测试联系人','feishu','contact-rec-1')")
            .execute(&pool).await.unwrap();
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
        let linked_business_before: (String, String, String, String, String, String) = sqlx::query_as(
            "SELECT w.external_record_id,s.external_record_id,c.external_record_id,w.content,s.stage_label,c.contact_name FROM case_work_items w JOIN case_stage_items s ON s.case_id=w.case_id JOIN case_agency_contacts c ON c.case_id=w.case_id WHERE w.case_id='case-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

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
        let linked_business_after: (String, String, String, String, String, String) = sqlx::query_as(
            "SELECT w.external_record_id,s.external_record_id,c.external_record_id,w.content,s.stage_label,c.contact_name FROM case_work_items w JOIN case_stage_items s ON s.case_id=w.case_id JOIN case_agency_contacts c ON c.case_id=w.case_id WHERE w.case_id='case-1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(link_count.0, 1);
        assert_eq!(inbox_count.0, 1);
        assert_eq!(snapshot_count.0, 1);
        assert_eq!(linked_business_before, linked_business_after);
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

    #[tokio::test]
    async fn normalized_name_is_recommended_but_requires_manual_confirmation() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query(
            r#"INSERT INTO cases
               (id,name,case_type,source_folder,legal_domain,management_status,agg_plaintiffs,agg_cause)
               VALUES ('case-name','20260721杨某买卖合同纠纷','诉讼','C:/preview','civil','active','["杨某"]','买卖合同纠纷')"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        let run_id = start_pull_run(&pool).await.unwrap();
        let result = complete_pull_preview(
            &pool,
            &run_id,
            "app",
            "table",
            vec![FeishuRemoteCaseRecord {
                record_id: "record-name".into(),
                fields: json!({
                    "案件名称": "杨某买卖合同纠纷",
                    "类型": "民事诉讼",
                    "☑状态": "在办",
                    "当事人": "杨某",
                    "案由": "买卖合同纠纷"
                }),
                last_modified_time: None,
            }],
        )
        .await
        .unwrap();
        assert_eq!(result.bound_count, 0);
        assert_eq!(result.pending_count, 1);
        let preview = get_preview(&pool).await.unwrap();
        assert_eq!(
            preview.pending_cases[0].recommended_case_id.as_deref(),
            Some("case-name")
        );
        let link_count: i64 = sqlx::query_scalar("SELECT count(*) FROM feishu_sync_links")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(link_count, 0);
    }

    #[tokio::test]
    async fn manual_binding_actions_are_reversible_audited_and_do_not_touch_case_fields() {
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        sqlx::query(
            "INSERT INTO cases (id,name,case_type,source_folder,case_no,legal_domain,management_status) VALUES ('case-actions','20260721测试案件','诉讼','C:/preview','（2026）测2号','civil','active')",
        )
        .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO feishu_sync_inbox (id,app_token,table_id,record_id,display_name,case_no,mapped_payload_json) VALUES ('inbox-actions','app','table','record-actions','测试案件','（2026）测2号','{}')",
        )
        .execute(&pool).await.unwrap();
        let before: (String, String, String) =
            sqlx::query_as("SELECT name,case_no,legal_domain FROM cases WHERE id='case-actions'")
                .fetch_one(&pool)
                .await
                .unwrap();

        bind_case(&pool, "inbox-actions", "case-actions")
            .await
            .unwrap();
        let link_id: String = sqlx::query_scalar(
            "SELECT id FROM feishu_sync_links WHERE record_id='record-actions' AND status='active'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        unbind_case(&pool, &link_id).await.unwrap();
        let state_after_unbind: (String, Option<String>, i64) = sqlx::query_as(
            "SELECT status,bound_case_id,auto_bind_suppressed FROM feishu_sync_inbox WHERE id='inbox-actions'",
        )
        .fetch_one(&pool).await.unwrap();
        assert_eq!(state_after_unbind, ("pending_binding".into(), None, 1));

        ignore_case(&pool, "inbox-actions").await.unwrap();
        restore_case(&pool, "inbox-actions").await.unwrap();
        let final_status: String =
            sqlx::query_scalar("SELECT status FROM feishu_sync_inbox WHERE id='inbox-actions'")
                .fetch_one(&pool)
                .await
                .unwrap();
        let actions: Vec<String> = sqlx::query_scalar(
            "SELECT action FROM feishu_sync_binding_audits WHERE inbox_id='inbox-actions' ORDER BY created_at,id",
        )
        .fetch_all(&pool).await.unwrap();
        let after: (String, String, String) =
            sqlx::query_as("SELECT name,case_no,legal_domain FROM cases WHERE id='case-actions'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(final_status, "pending_binding");
        assert!(actions.contains(&"manual_bind".to_string()));
        assert!(actions.contains(&"unbind".to_string()));
        assert!(actions.contains(&"ignore".to_string()));
        assert!(actions.contains(&"restore".to_string()));
        assert_eq!(before, after);
    }
}
