//! 飞书案件管理明细的受控双向同步预演。
//!
//! 拉取只写候选变化表；进展、阶段和通讯录业务表必须在用户逐项确认后才更新。

use std::collections::{HashMap, HashSet};

use chrono::{FixedOffset, TimeZone};
use serde_json::Value;
use sqlx::{Sqlite, Transaction};
use uuid::Uuid;

use crate::feishu::{FeishuCaseManagementRecords, FeishuRemoteCaseRecord};

const PROGRESS_CASE_FIELD: &str = "所属案件";
const STAGE_CASE_FIELD: &str = "所属案件";
const CONTACT_CASE_FIELD: &str = "🚩案件总表";
const CONTACT_SLOTS: &[(&str, &str, &str)] = &[
    ("侦办人", "investigation", "侦查人员"),
    ("检察官", "prosecution", "检察官"),
    ("检察官助理", "prosecution", "检察官助理"),
    ("法官", "trial", "法官"),
    ("法官助理", "trial", "法官助理"),
    ("书记员", "trial", "书记员"),
    ("调解员", "trial", "调解员"),
];

#[derive(Debug, Default, Clone, Copy)]
pub struct FeishuEntityImportCounts {
    pub work_items: usize,
    pub stages: usize,
    pub contacts: usize,
    pub archived: usize,
}

fn object(record: &FeishuRemoteCaseRecord) -> Result<&serde_json::Map<String, Value>, String> {
    record
        .fields
        .as_object()
        .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 飞书关联记录 fields 不是对象".to_string())
}

fn collect_text(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(value) if !value.trim().is_empty() => output.push(value.trim().to_string()),
        Value::Number(value) => output.push(value.to_string()),
        Value::Bool(value) => output.push(value.to_string()),
        Value::Array(values) => values.iter().for_each(|value| collect_text(value, output)),
        Value::Object(value) => {
            for key in ["text", "name", "title", "full_name", "value"] {
                if let Some(nested) = value.get(key) {
                    collect_text(nested, output);
                    if !output.is_empty() {
                        break;
                    }
                }
            }
        }
        _ => {}
    }
}

fn field_text(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    let mut output = Vec::new();
    if let Some(value) = fields.get(key) {
        collect_text(value, &mut output);
    }
    let value = output.join("").trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn collect_link_ids(value: &Value, output: &mut HashSet<String>) {
    match value {
        Value::Array(values) => values
            .iter()
            .for_each(|value| collect_link_ids(value, output)),
        Value::Object(value) => {
            for key in ["record_ids", "link_record_ids"] {
                if let Some(Value::Array(ids)) = value.get(key) {
                    for id in ids.iter().filter_map(Value::as_str) {
                        if !id.trim().is_empty() {
                            output.insert(id.trim().to_string());
                        }
                    }
                }
            }
            value
                .values()
                .for_each(|value| collect_link_ids(value, output));
        }
        _ => {}
    }
}

fn link_ids(fields: &serde_json::Map<String, Value>, key: &str) -> HashSet<String> {
    let mut output = HashSet::new();
    if let Some(value) = fields.get(key) {
        collect_link_ids(value, &mut output);
    }
    output
}

fn millis(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(value) => value.parse().ok(),
        Value::Array(values) => values.iter().find_map(millis),
        Value::Object(value) => value.get("value").and_then(millis),
        _ => None,
    }
}

fn field_datetime(fields: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    let timestamp = fields.get(key).and_then(millis)?;
    let utc = chrono::Utc.timestamp_millis_opt(timestamp).single()?;
    let timezone = FixedOffset::east_opt(8 * 3600)?;
    Some(utc.with_timezone(&timezone).to_rfc3339())
}

fn payload(record: &FeishuRemoteCaseRecord, slot: Option<&str>) -> Result<String, String> {
    serde_json::to_string(&serde_json::json!({
        "record_id": record.record_id,
        "last_modified_time": record.last_modified_time,
        "slot": slot,
        "fields": record.fields,
    }))
    .map_err(|_| "FEISHU_RESPONSE_INVALID: 无法保存飞书关联记录原始数据".to_string())
}

fn stage_status(value: Option<&str>) -> &'static str {
    match value.unwrap_or_default() {
        value if value.contains("完成") || value.contains("结束") => "completed",
        value if value.contains("进行") || value.contains("在办") => "active",
        _ => "pending",
    }
}

fn work_type(value: Option<&str>) -> &'static str {
    match value.unwrap_or_default() {
        value if value.contains("开庭") || value.contains("庭审") => "hearing",
        value if value.contains("会见") => "meeting",
        value if value.contains("沟通") || value.contains("联系") => "communication",
        value if value.contains("提交") || value.contains("递交") => "filing",
        value if value.contains("研究") || value.contains("分析") => "research",
        value if value.contains("文书") || value.contains("起草") => "drafting",
        _ => "other",
    }
}

fn linked_case(
    links: &HashMap<String, String>,
    fields: &serde_json::Map<String, Value>,
    field: &str,
) -> Result<Option<String>, String> {
    let linked: HashSet<String> = link_ids(fields, field)
        .into_iter()
        .filter_map(|record_id| links.get(&record_id).cloned())
        .collect();
    match linked.len() {
        0 => Ok(None),
        1 => Ok(linked.into_iter().next()),
        _ => Err("FEISHU_SCHEMA_CHANGED: 一条飞书明细关联了多个已绑定案件".to_string()),
    }
}

struct EntityPreviewInput<'a> {
    run_id: &'a str,
    link_id: &'a str,
    entity_type: &'a str,
    local_entity_id: Option<&'a str>,
    app_token: &'a str,
    table_id: &'a str,
    record_id: &'a str,
    slot_key: &'a str,
    case_id: &'a str,
    case_name: &'a str,
    change_kind: &'a str,
    local_value: Option<&'a Value>,
    remote_value: &'a Value,
    mapped_value: &'a Value,
}

async fn save_preview(
    tx: &mut Transaction<'_, Sqlite>,
    input: EntityPreviewInput<'_>,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO feishu_sync_entity_previews
         (id,run_id,link_id,entity_type,local_entity_id,app_token,table_id,record_id,slot_key,
          case_id,case_name,change_kind,local_value_json,feishu_value_json,mapped_value_json)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(input.run_id)
    .bind(input.link_id)
    .bind(input.entity_type)
    .bind(input.local_entity_id)
    .bind(input.app_token)
    .bind(input.table_id)
    .bind(input.record_id)
    .bind(input.slot_key)
    .bind(input.case_id)
    .bind(input.case_name)
    .bind(input.change_kind)
    .bind(input.local_value.map(Value::to_string))
    .bind(input.remote_value.to_string())
    .bind(input.mapped_value.to_string())
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("FEISHU_DB_PREVIEW_WRITE_FAILED: 保存飞书明细候选变化失败: {e}"))?;
    Ok(())
}

fn parse_local(row: &(String, String, String)) -> Option<Value> {
    serde_json::from_str(&row.2).ok()
}

pub async fn preview_management_records(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: &str,
    app_token: &str,
    case_table_id: &str,
    bundle: &FeishuCaseManagementRecords,
) -> Result<FeishuEntityImportCounts, String> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT id,record_id,local_entity_id FROM feishu_sync_links
         WHERE app_token=?1 AND table_id=?2 AND entity_type='case' AND slot_key='' AND status='active'",
    )
    .bind(app_token).bind(case_table_id).fetch_all(&mut **tx).await
    .map_err(|e| format!("FEISHU_DB_PREVIEW_WRITE_FAILED: 无法读取案件绑定: {e}"))?;
    let links: HashMap<String, String> = rows
        .iter()
        .map(|(_, record, case_id)| (record.clone(), case_id.clone()))
        .collect();
    let link_ids_by_case: HashMap<String, String> = rows
        .into_iter()
        .map(|(link, _, case_id)| (case_id, link))
        .collect();
    sqlx::query("UPDATE feishu_sync_entity_previews SET review_status='superseded',resolved_at=datetime('now') WHERE review_status='pending'")
        .execute(&mut **tx).await
        .map_err(|e| format!("FEISHU_DB_PREVIEW_WRITE_FAILED: 无法结束旧的飞书明细候选: {e}"))?;
    let mut counts = FeishuEntityImportCounts::default();

    for record in &bundle.progress {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, PROGRESS_CASE_FIELD)? else {
            continue;
        };
        let existing: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id,external_status,json_object('occurred_at',occurred_at,'work_type',work_type,'title',title,'content',content,'duration_minutes',duration_minutes)
             FROM case_work_items WHERE external_source='feishu' AND external_record_id=?1 LIMIT 1",
        ).bind(&record.record_id).fetch_optional(&mut **tx).await
        .map_err(|e| format!("FEISHU_DB_PREVIEW_WRITE_FAILED: 无法读取既有飞书进展: {e}"))?;
        let occurred_at = field_datetime(fields, "进度日期")
            .or_else(|| field_datetime(fields, "开始时间"))
            .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 飞书进展缺少进度日期".to_string())?;
        let kind = field_text(fields, "进展类型");
        let content = field_text(fields, "进度填写区")
            .unwrap_or_else(|| kind.clone().unwrap_or_else(|| "飞书进展".to_string()));
        let duration = field_text(fields, "小时")
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(0)
            * 60
            + field_text(fields, "分钟")
                .and_then(|v| v.parse::<i64>().ok())
                .unwrap_or(0);
        let values = serde_json::json!({"occurred_at":occurred_at,"work_type":work_type(kind.as_deref()),"title":kind.as_deref().unwrap_or("飞书进展"),"content":content,"duration_minutes":duration});
        let local = existing.as_ref().and_then(parse_local);
        if local.as_ref() != Some(&values)
            || existing.as_ref().is_some_and(|row| row.1 == "archived")
        {
            let change = match existing.as_ref() {
                None => "create",
                Some(row) if row.1 == "archived" => "restore",
                _ => "update",
            };
            let case_name: String = sqlx::query_scalar(
                "SELECT COALESCE(NULLIF(display_name_override,''),name,id) FROM cases WHERE id=?1",
            )
            .bind(&case_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
            let mapped = serde_json::json!({"values":values,"raw_payload_json":payload(record,None)?,"external_updated_at":record.last_modified_time});
            save_preview(
                tx,
                EntityPreviewInput {
                    run_id,
                    link_id: &link_ids_by_case[&case_id],
                    entity_type: "work_item",
                    local_entity_id: existing.as_ref().map(|row| row.0.as_str()),
                    app_token,
                    table_id: &bundle.progress_table_id,
                    record_id: &record.record_id,
                    slot_key: "",
                    case_id: &case_id,
                    case_name: &case_name,
                    change_kind: change,
                    local_value: local.as_ref(),
                    remote_value: &record.fields,
                    mapped_value: &mapped,
                },
            )
            .await?;
            counts.work_items += 1;
        }
    }

    for record in &bundle.stages {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, STAGE_CASE_FIELD)? else {
            continue;
        };
        let existing: Option<(String, String, String)> = sqlx::query_as(
            "SELECT id,external_status,json_object('major_stage',major_stage,'stage_label',stage_label,'status',status,'started_at',started_at,'due_at',due_at,'reminder_at',reminder_at)
             FROM case_stage_items WHERE external_source='feishu' AND external_record_id=?1 LIMIT 1",
        ).bind(&record.record_id).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
        let stage_label = field_text(fields, "程序")
            .or_else(|| field_text(fields, "阶段"))
            .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 飞书阶段缺少程序或阶段".to_string())?;
        let status_text = field_text(fields, "🔣【状态】");
        let values = serde_json::json!({"major_stage":field_text(fields,"阶段"),"stage_label":stage_label,"status":stage_status(status_text.as_deref()),"started_at":field_datetime(fields,"开始时间"),"due_at":field_datetime(fields,"程序结束时间"),"reminder_at":field_datetime(fields,"提醒时间")});
        let local = existing.as_ref().and_then(parse_local);
        if local.as_ref() != Some(&values)
            || existing.as_ref().is_some_and(|row| row.1 == "archived")
        {
            let change = match existing.as_ref() {
                None => "create",
                Some(row) if row.1 == "archived" => "restore",
                _ => "update",
            };
            let (case_name,domain):(String,String)=sqlx::query_as("SELECT COALESCE(NULLIF(display_name_override,''),name,id),legal_domain FROM cases WHERE id=?1").bind(&case_id).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
            let mapped = serde_json::json!({"values":values,"domain":domain,"raw_payload_json":payload(record,None)?,"external_updated_at":record.last_modified_time});
            save_preview(
                tx,
                EntityPreviewInput {
                    run_id,
                    link_id: &link_ids_by_case[&case_id],
                    entity_type: "stage",
                    local_entity_id: existing.as_ref().map(|row| row.0.as_str()),
                    app_token,
                    table_id: &bundle.stage_table_id,
                    record_id: &record.record_id,
                    slot_key: "",
                    case_id: &case_id,
                    case_name: &case_name,
                    change_kind: change,
                    local_value: local.as_ref(),
                    remote_value: &record.fields,
                    mapped_value: &mapped,
                },
            )
            .await?;
            counts.stages += 1;
        }
    }

    for record in &bundle.contacts {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, CONTACT_CASE_FIELD)? else {
            continue;
        };
        for (slot, stage_scope, role) in CONTACT_SLOTS {
            let Some(contact_name) = field_text(fields, slot) else {
                continue;
            };
            let existing: Option<(String,String,String)>=sqlx::query_as(
                "SELECT id,external_status,json_object('stage_scope',stage_scope,'agency_name',agency_name,'contact_role',contact_role,'contact_name',contact_name,'case_no',case_no,'query_code',query_code,'notes',notes)
                 FROM case_agency_contacts WHERE external_source='feishu' AND external_record_id=?1 AND external_slot_key=?2 LIMIT 1"
            ).bind(&record.record_id).bind(slot).fetch_optional(&mut **tx).await.map_err(|e|e.to_string())?;
            let agency_name = match *stage_scope {
                "investigation" => field_text(fields, "侦查机关"),
                "prosecution" => {
                    field_text(fields, "审查起诉").or_else(|| Some("检察机关".to_string()))
                }
                _ => field_text(fields, "审判机关"),
            };
            let values = serde_json::json!({"stage_scope":stage_scope,"agency_name":agency_name,"contact_role":role,"contact_name":contact_name,"case_no":field_text(fields,"案号"),"query_code":field_text(fields,"案件查询码/备注"),"notes":field_text(fields,"备注")});
            let local = existing.as_ref().and_then(parse_local);
            if local.as_ref() != Some(&values)
                || existing.as_ref().is_some_and(|row| row.1 == "archived")
            {
                let change = match existing.as_ref() {
                    None => "create",
                    Some(row) if row.1 == "archived" => "restore",
                    _ => "update",
                };
                let case_name:String=sqlx::query_scalar("SELECT COALESCE(NULLIF(display_name_override,''),name,id) FROM cases WHERE id=?1").bind(&case_id).fetch_one(&mut **tx).await.map_err(|e|e.to_string())?;
                let mapped = serde_json::json!({"values":values,"raw_payload_json":payload(record,Some(slot))?,"external_updated_at":record.last_modified_time});
                save_preview(
                    tx,
                    EntityPreviewInput {
                        run_id,
                        link_id: &link_ids_by_case[&case_id],
                        entity_type: "contact",
                        local_entity_id: existing.as_ref().map(|row| row.0.as_str()),
                        app_token,
                        table_id: &bundle.contact_table_id,
                        record_id: &record.record_id,
                        slot_key: slot,
                        case_id: &case_id,
                        case_name: &case_name,
                        change_kind: change,
                        local_value: local.as_ref(),
                        remote_value: &record.fields,
                        mapped_value: &mapped,
                    },
                )
                .await?;
                counts.contacts += 1;
            }
        }
    }
    // 远端缺失不映射为本地删除或归档；删除仍由用户在本地手工完成。
    Ok(counts)
}
