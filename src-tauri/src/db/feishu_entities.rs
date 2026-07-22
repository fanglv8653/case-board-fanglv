//! 飞书案件管理明细的单向入站同步。
//!
//! 这里只写入带 `external_source = 'feishu'` 的进展、阶段和通讯录记录；
//! 案件名称、罪名、当事人、日期等案件业务字段仍由预览/人工确认流程维护。

use std::collections::{HashMap, HashSet};

use chrono::{FixedOffset, TimeZone};
use serde_json::Value;
use sha2::{Digest, Sha256};
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

fn hash(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
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

async fn linked_case(
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

type AuditEntry<'a> = (
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
    &'a str,
);

async fn audit(tx: &mut Transaction<'_, Sqlite>, entry: AuditEntry<'_>) -> Result<(), String> {
    let (run_id, entity_type, local_id, record_id, slot, action, payload_hash) = entry;
    sqlx::query("INSERT INTO feishu_sync_entity_audits (id,run_id,entity_type,local_entity_id,remote_record_id,slot_key,action,payload_hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)")
        .bind(Uuid::new_v4().to_string()).bind(run_id).bind(entity_type).bind(local_id)
        .bind(record_id).bind(slot).bind(action).bind(payload_hash)
        .execute(&mut **tx).await
        .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法保存飞书入站审计记录".to_string())?;
    Ok(())
}

pub async fn import_management_records(
    tx: &mut Transaction<'_, Sqlite>,
    run_id: &str,
    app_token: &str,
    case_table_id: &str,
    bundle: &FeishuCaseManagementRecords,
) -> Result<FeishuEntityImportCounts, String> {
    let link_rows: Vec<(String, String)> = sqlx::query_as(
        "SELECT record_id,local_entity_id FROM feishu_sync_links WHERE app_token=?1 AND table_id=?2 AND entity_type='case' AND slot_key='' AND status='active'",
    )
    .bind(app_token).bind(case_table_id).fetch_all(&mut **tx).await
    .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法读取案件绑定".to_string())?;
    let links: HashMap<String, String> = link_rows.into_iter().collect();
    let bound_case_ids: HashSet<String> = links.values().cloned().collect();
    let mut seen_work = HashSet::new();
    let mut seen_stages = HashSet::new();
    let mut seen_contacts = HashSet::new();
    let mut counts = FeishuEntityImportCounts::default();

    for record in &bundle.progress {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, PROGRESS_CASE_FIELD).await? else {
            continue;
        };
        let raw = payload(record, None)?;
        let existing: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id,external_status,raw_payload_json FROM case_work_items WHERE external_source='feishu' AND external_record_id=?1 LIMIT 1",
        ).bind(&record.record_id).fetch_optional(&mut **tx).await
        .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法读取既有飞书进展".to_string())?;
        let action = match existing.as_ref() {
            None => "insert",
            Some((_, status, _)) if status == "archived" => "restore",
            Some((_, _, previous)) if previous.as_deref() == Some(raw.as_str()) => "unchanged",
            _ => "update",
        };
        let id = existing
            .as_ref()
            .map(|row| row.0.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
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
        if existing.is_some() {
            sqlx::query("UPDATE case_work_items SET case_id=?2,occurred_at=?3,work_type=?4,title=?5,content=?6,duration_minutes=?7,external_updated_at=?8,raw_payload_json=?9,external_status='active',external_last_seen_at=datetime('now'),deleted_at=NULL,updated_at=datetime('now') WHERE id=?1")
                .bind(&id).bind(&case_id).bind(&occurred_at).bind(work_type(kind.as_deref()))
                .bind(kind.as_deref().unwrap_or("飞书进展")).bind(&content).bind(duration)
                .bind(&record.last_modified_time).bind(&raw).execute(&mut **tx).await
                .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法更新飞书进展".to_string())?;
        } else {
            sqlx::query("INSERT INTO case_work_items (id,case_id,occurred_at,work_type,title,content,duration_minutes,source,external_source,external_record_id,external_updated_at,raw_payload_json,external_status,external_last_seen_at) VALUES (?1,?2,?3,?4,?5,?6,?7,'feishu','feishu',?8,?9,?10,'active',datetime('now'))")
                .bind(&id).bind(&case_id).bind(&occurred_at).bind(work_type(kind.as_deref()))
                .bind(kind.as_deref().unwrap_or("飞书进展")).bind(&content).bind(duration)
                .bind(&record.record_id).bind(&record.last_modified_time).bind(&raw).execute(&mut **tx).await
                .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法新增飞书进展".to_string())?;
        }
        audit(
            tx,
            (
                run_id,
                "work_item",
                &id,
                &record.record_id,
                "",
                action,
                &hash(&raw),
            ),
        )
        .await?;
        seen_work.insert(record.record_id.clone());
        counts.work_items += 1;
    }

    for record in &bundle.stages {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, STAGE_CASE_FIELD).await? else {
            continue;
        };
        let raw = payload(record, None)?;
        let existing: Option<(String, String, Option<String>)> = sqlx::query_as(
            "SELECT id,external_status,raw_payload_json FROM case_stage_items WHERE external_source='feishu' AND external_record_id=?1 LIMIT 1",
        ).bind(&record.record_id).fetch_optional(&mut **tx).await
        .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法读取既有飞书阶段".to_string())?;
        let action = match existing.as_ref() {
            None => "insert",
            Some((_, status, _)) if status == "archived" => "restore",
            Some((_, _, previous)) if previous.as_deref() == Some(raw.as_str()) => "unchanged",
            _ => "update",
        };
        let id = existing
            .as_ref()
            .map(|row| row.0.clone())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let stage_label = field_text(fields, "程序")
            .or_else(|| field_text(fields, "阶段"))
            .ok_or_else(|| "FEISHU_SCHEMA_CHANGED: 飞书阶段缺少程序或阶段".to_string())?;
        let status_text = field_text(fields, "🔁【状态】");
        if existing.is_some() {
            sqlx::query("UPDATE case_stage_items SET case_id=?2,major_stage=?3,stage_label=?4,status=?5,started_at=?6,due_at=?7,reminder_at=?8,external_updated_at=?9,raw_payload_json=?10,external_status='active',external_last_seen_at=datetime('now'),deleted_at=NULL,updated_at=datetime('now') WHERE id=?1")
                .bind(&id).bind(&case_id).bind(field_text(fields, "阶段")).bind(&stage_label)
                .bind(stage_status(status_text.as_deref())).bind(field_datetime(fields, "开始时间"))
                .bind(field_datetime(fields, "程序结束时间")).bind(field_datetime(fields, "提醒时间"))
                .bind(&record.last_modified_time).bind(&raw).execute(&mut **tx).await
                .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法更新飞书阶段".to_string())?;
        } else {
            sqlx::query("INSERT INTO case_stage_items (id,case_id,domain,major_stage,stage_label,status,started_at,due_at,reminder_at,source,external_source,external_record_id,external_updated_at,raw_payload_json,external_status,external_last_seen_at) VALUES (?1,?2,'other',?3,?4,?5,?6,?7,?8,'feishu','feishu',?9,?10,?11,'active',datetime('now'))")
                .bind(&id).bind(&case_id).bind(field_text(fields, "阶段")).bind(&stage_label)
                .bind(stage_status(status_text.as_deref())).bind(field_datetime(fields, "开始时间"))
                .bind(field_datetime(fields, "程序结束时间")).bind(field_datetime(fields, "提醒时间"))
                .bind(&record.record_id).bind(&record.last_modified_time).bind(&raw).execute(&mut **tx).await
                .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法新增飞书阶段".to_string())?;
        }
        audit(
            tx,
            (
                run_id,
                "stage",
                &id,
                &record.record_id,
                "",
                action,
                &hash(&raw),
            ),
        )
        .await?;
        seen_stages.insert(record.record_id.clone());
        counts.stages += 1;
    }

    for record in &bundle.contacts {
        let fields = object(record)?;
        let Some(case_id) = linked_case(&links, fields, CONTACT_CASE_FIELD).await? else {
            continue;
        };
        for (slot, stage_scope, role) in CONTACT_SLOTS {
            let Some(contact_name) = field_text(fields, slot) else {
                continue;
            };
            let raw = payload(record, Some(slot))?;
            let existing: Option<(String, String, Option<String>)> = sqlx::query_as(
                "SELECT id,external_status,raw_payload_json FROM case_agency_contacts WHERE external_source='feishu' AND external_record_id=?1 AND external_slot_key=?2 LIMIT 1",
            ).bind(&record.record_id).bind(slot).fetch_optional(&mut **tx).await
            .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法读取既有飞书联系人".to_string())?;
            let action = match existing.as_ref() {
                None => "insert",
                Some((_, status, _)) if status == "archived" => "restore",
                Some((_, _, previous)) if previous.as_deref() == Some(raw.as_str()) => "unchanged",
                _ => "update",
            };
            let id = existing
                .as_ref()
                .map(|row| row.0.clone())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let agency_name = match *stage_scope {
                "investigation" => field_text(fields, "侦查机关"),
                "prosecution" => {
                    field_text(fields, "审查起诉").or_else(|| Some("检察机关".to_string()))
                }
                _ => field_text(fields, "审判机关"),
            };
            if existing.is_some() {
                sqlx::query("UPDATE case_agency_contacts SET case_id=?2,stage_scope=?3,agency_type=?3,agency_name=?4,contact_role=?5,contact_name=?6,case_no=?7,query_code=?8,notes=?9,external_updated_at=?10,external_last_seen_at=datetime('now'),external_status='active',raw_payload_json=?11,deleted_at=NULL,updated_at=datetime('now') WHERE id=?1")
                    .bind(&id).bind(&case_id).bind(stage_scope).bind(&agency_name).bind(role).bind(&contact_name)
                    .bind(field_text(fields, "案号")).bind(field_text(fields, "案件查询码/备注"))
                    .bind(field_text(fields, "备注")).bind(&record.last_modified_time).bind(&raw)
                    .execute(&mut **tx).await
                    .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法更新飞书联系人".to_string())?;
            } else {
                sqlx::query("INSERT INTO case_agency_contacts (id,case_id,stage_scope,agency_type,agency_name,contact_role,contact_name,case_no,query_code,notes,source,external_source,external_record_id,external_slot_key,external_updated_at,external_last_seen_at,external_status,raw_payload_json) VALUES (?1,?2,?3,?3,?4,?5,?6,?7,?8,?9,'feishu','feishu',?10,?11,?12,datetime('now'),'active',?13)")
                    .bind(&id).bind(&case_id).bind(stage_scope).bind(&agency_name).bind(role).bind(&contact_name)
                    .bind(field_text(fields, "案号")).bind(field_text(fields, "案件查询码/备注"))
                    .bind(field_text(fields, "备注")).bind(&record.record_id).bind(slot)
                    .bind(&record.last_modified_time).bind(&raw).execute(&mut **tx).await
                    .map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法新增飞书联系人".to_string())?;
            }
            audit(
                tx,
                (
                    run_id,
                    "contact",
                    &id,
                    &record.record_id,
                    slot,
                    action,
                    &hash(&raw),
                ),
            )
            .await?;
            seen_contacts.insert(format!("{}\u{1f}{}", record.record_id, slot));
            counts.contacts += 1;
        }
    }

    let bound_cases = bound_case_ids.into_iter().collect::<Vec<_>>();
    for case_id in bound_cases {
        let work_rows: Vec<(String, String, String)> = sqlx::query_as("SELECT id,external_record_id,COALESCE(raw_payload_json,'') FROM case_work_items WHERE case_id=?1 AND external_source='feishu' AND external_status='active' AND external_record_id IS NOT NULL")
            .bind(&case_id).fetch_all(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法检查失效进展".to_string())?;
        for (id, record_id, raw) in work_rows
            .into_iter()
            .filter(|row| !seen_work.contains(&row.1))
        {
            sqlx::query("UPDATE case_work_items SET external_status='archived',deleted_at=datetime('now'),updated_at=datetime('now') WHERE id=?1")
                .bind(&id).execute(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法归档失效进展".to_string())?;
            audit(
                tx,
                (
                    run_id,
                    "work_item",
                    &id,
                    &record_id,
                    "",
                    "archive",
                    &hash(&raw),
                ),
            )
            .await?;
            counts.archived += 1;
        }
        let stage_rows: Vec<(String, String, String)> = sqlx::query_as("SELECT id,external_record_id,COALESCE(raw_payload_json,'') FROM case_stage_items WHERE case_id=?1 AND external_source='feishu' AND external_status='active' AND external_record_id IS NOT NULL")
            .bind(&case_id).fetch_all(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法检查失效阶段".to_string())?;
        for (id, record_id, raw) in stage_rows
            .into_iter()
            .filter(|row| !seen_stages.contains(&row.1))
        {
            sqlx::query("UPDATE case_stage_items SET external_status='archived',deleted_at=datetime('now'),updated_at=datetime('now') WHERE id=?1")
                .bind(&id).execute(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法归档失效阶段".to_string())?;
            audit(
                tx,
                (run_id, "stage", &id, &record_id, "", "archive", &hash(&raw)),
            )
            .await?;
            counts.archived += 1;
        }
        let contact_rows: Vec<(String, String, String, String)> = sqlx::query_as("SELECT id,external_record_id,external_slot_key,COALESCE(raw_payload_json,'') FROM case_agency_contacts WHERE case_id=?1 AND external_source='feishu' AND external_status='active' AND external_record_id IS NOT NULL")
            .bind(&case_id).fetch_all(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法检查失效联系人".to_string())?;
        for (id, record_id, slot, raw) in contact_rows
            .into_iter()
            .filter(|row| !seen_contacts.contains(&format!("{}\u{1f}{}", row.1, row.2)))
        {
            sqlx::query("UPDATE case_agency_contacts SET external_status='archived',deleted_at=datetime('now'),updated_at=datetime('now') WHERE id=?1")
                .bind(&id).execute(&mut **tx).await.map_err(|_| "FEISHU_DB_ENTITY_WRITE_FAILED: 无法归档失效联系人".to_string())?;
            audit(
                tx,
                (
                    run_id,
                    "contact",
                    &id,
                    &record_id,
                    &slot,
                    "archive",
                    &hash(&raw),
                ),
            )
            .await?;
            counts.archived += 1;
        }
    }

    Ok(counts)
}
