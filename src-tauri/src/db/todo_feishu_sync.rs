//! v0.8.4 飞书“收件箱”待办同步账本与只读预演。

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, FixedOffset, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use super::todos::Todo;
use crate::feishu::FeishuRemoteCaseRecord;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TodoSyncPayload {
    pub title: String,
    pub content: String,
    pub kind: String,
    pub priority: String,
    pub tags: Vec<String>,
    pub next_action: Option<String>,
    pub status: String,
    pub due_at: Option<String>,
    pub remind_at: Option<String>,
    pub done_at: Option<String>,
    pub source: String,
    pub source_message_id: Option<String>,
    pub source_at: Option<String>,
    pub delete_requested_at: Option<String>,
    pub delete_reason: Option<String>,
    pub deleted: bool,
    pub remote_case_text: Option<String>,
}

#[derive(Debug, Clone)]
struct ParsedRemote {
    record_id: String,
    business_key: String,
    payload: TodoSyncPayload,
    payload_json: String,
    payload_hash: String,
    modified_at: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct LinkRow {
    id: String,
    item_id: Option<String>,
    record_id: String,
    remote_business_key: String,
    remote_case_text: Option<String>,
    base_payload_hash: Option<String>,
    remote_modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TodoFeishuPreviewRow {
    pub id: String,
    pub run_id: String,
    pub link_id: Option<String>,
    pub item_id: Option<String>,
    pub record_id: Option<String>,
    pub remote_business_key: Option<String>,
    pub change_kind: String,
    pub local_payload_json: Option<String>,
    pub remote_payload_json: Option<String>,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    pub remote_modified_at: Option<String>,
    pub case_hint: Option<String>,
    pub status: String,
    pub error_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct TodoFeishuRunRow {
    pub id: String,
    pub status: String,
    pub remote_count: i64,
    pub preview_count: i64,
    pub conflict_count: i64,
    pub error_code: Option<String>,
    pub started_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodoFeishuPreview {
    pub rows: Vec<TodoFeishuPreviewRow>,
    pub recent_runs: Vec<TodoFeishuRunRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TodoFeishuPullResult {
    pub run_id: String,
    pub remote_count: usize,
    pub preview_count: usize,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, FromRow)]
pub struct PendingTodoPreview {
    pub id: String,
    pub link_id: Option<String>,
    pub item_id: Option<String>,
    pub record_id: Option<String>,
    pub remote_business_key: Option<String>,
    pub change_kind: String,
    pub local_payload_json: Option<String>,
    pub remote_payload_json: Option<String>,
    pub local_hash: Option<String>,
    pub remote_hash: Option<String>,
    pub remote_modified_at: Option<String>,
}

fn sync_error(code: &str, message: &str) -> String {
    format!("{code}: {message}")
}

fn hash_json(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn canonical(payload: &TodoSyncPayload) -> Result<(String, String), String> {
    let json = serde_json::to_string(payload)
        .map_err(|_| sync_error("FEISHU_TODO_METADATA_INVALID", "无法规范化事项"))?;
    let hash = hash_json(&json);
    Ok((json, hash))
}

fn text_from_value(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(value) => {
            let value = value.trim();
            (!value.is_empty()).then(|| value.to_string())
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Array(values) => {
            let joined = values
                .iter()
                .filter_map(|value| match value {
                    Value::String(value) => Some(value.as_str()),
                    Value::Object(value) => value
                        .get("text")
                        .or_else(|| value.get("name"))
                        .and_then(Value::as_str),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("");
            let joined = joined.trim().to_string();
            (!joined.is_empty()).then_some(joined)
        }
        Value::Object(value) => value
            .get("text")
            .or_else(|| value.get("name"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

fn tags_from_value(value: Option<&Value>) -> Vec<String> {
    let mut values = match value {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(|value| text_from_value(Some(value)))
            .collect::<Vec<_>>(),
        value => text_from_value(value).into_iter().collect(),
    };
    values.sort();
    values.dedup();
    values
}

fn timestamp_from_value(value: Option<&Value>) -> Result<Option<String>, String> {
    let Some(value) = value else { return Ok(None) };
    let parsed = match value {
        Value::Null => return Ok(None),
        Value::Number(value) => value.as_i64().and_then(|milliseconds| {
            Utc.timestamp_millis_opt(milliseconds)
                .single()
                .map(|time| time.to_rfc3339())
        }),
        Value::String(value) if value.trim().is_empty() => return Ok(None),
        Value::String(value) => DateTime::parse_from_rfc3339(value.trim())
            .ok()
            .map(|time| time.to_rfc3339()),
        _ => None,
    };
    parsed
        .map(Some)
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "时间字段格式无效"))
}

fn map_choice(
    value: Option<String>,
    choices: &[(&str, &str)],
    field: &str,
) -> Result<String, String> {
    let value = value
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", &format!("{field}不能为空")))?;
    choices
        .iter()
        .find_map(|(remote, local)| (*remote == value).then(|| (*local).to_string()))
        .ok_or_else(|| {
            sync_error(
                "FEISHU_TODO_METADATA_INVALID",
                &format!("{field}包含未知选项"),
            )
        })
}

fn parse_remote(record: &FeishuRemoteCaseRecord) -> Result<ParsedRemote, String> {
    let fields = record
        .fields
        .as_object()
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "记录 fields 不是对象"))?;
    let business_key = text_from_value(fields.get("事项编号"))
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "事项编号不能为空"))?;
    let title = text_from_value(fields.get("事项"))
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "事项标题不能为空"))?;
    let kind = map_choice(
        text_from_value(fields.get("类型")),
        &[
            ("想法", "idea"),
            ("待办", "todo"),
            ("提醒", "reminder"),
            ("资料", "reference"),
            ("备忘", "memo"),
        ],
        "类型",
    )?;
    let status = map_choice(
        text_from_value(fields.get("状态")),
        &[
            ("收件箱", "inbox"),
            ("进行中", "in_progress"),
            ("等待", "waiting"),
            ("完成", "completed"),
            ("删除待确认", "delete_pending"),
            ("已删除", "deleted"),
        ],
        "状态",
    )?;
    let priority = map_choice(
        text_from_value(fields.get("优先级")),
        &[("高", "high"), ("中", "medium"), ("低", "low")],
        "优先级",
    )?;
    let source_message_id = text_from_value(fields.get("来源消息ID"));
    let source = match source_message_id.as_deref() {
        Some(value) if value.starts_with("caseboard:") => "caseboard",
        Some(_) => "hermes",
        None => "feishu",
    };
    let payload = TodoSyncPayload {
        title,
        content: text_from_value(fields.get("原始内容")).unwrap_or_default(),
        kind,
        priority,
        tags: tags_from_value(fields.get("标签")),
        next_action: text_from_value(fields.get("下一步动作")),
        status: status.clone(),
        due_at: timestamp_from_value(fields.get("截止时间"))?,
        remind_at: timestamp_from_value(fields.get("提醒时间"))?,
        done_at: timestamp_from_value(fields.get("完成时间"))?,
        source: source.into(),
        source_message_id,
        source_at: timestamp_from_value(fields.get("来源时间"))?,
        delete_requested_at: timestamp_from_value(fields.get("删除请求时间"))?,
        delete_reason: text_from_value(fields.get("删除原因")),
        deleted: status == "deleted",
        remote_case_text: text_from_value(fields.get("关联案件")),
    };
    if matches!(status.as_str(), "inbox" | "in_progress" | "waiting") && payload.done_at.is_some() {
        return Err(sync_error(
            "FEISHU_TODO_METADATA_INVALID",
            "未完成事项不能带完成时间",
        ));
    }
    let (payload_json, payload_hash) = canonical(&payload)?;
    Ok(ParsedRemote {
        record_id: record.record_id.clone(),
        business_key,
        payload,
        payload_json,
        payload_hash,
        modified_at: record.last_modified_time.clone(),
    })
}

fn local_payload(
    todo: &Todo,
    remote_case_text: Option<String>,
) -> Result<(TodoSyncPayload, String, String), String> {
    let tags = serde_json::from_str::<Vec<String>>(&todo.tags_json)
        .map_err(|_| sync_error("FEISHU_TODO_METADATA_INVALID", "本地标签 JSON 无效"))?;
    let payload = TodoSyncPayload {
        title: todo.title.clone(),
        content: todo.content.clone(),
        kind: todo.kind.clone(),
        priority: todo.priority.clone(),
        tags,
        next_action: todo.next_action.clone(),
        status: todo.status.clone(),
        due_at: todo.due_at.clone(),
        remind_at: todo.remind_at.clone(),
        done_at: todo.done_at.clone(),
        source: todo.source.clone(),
        source_message_id: todo
            .source_message_id
            .clone()
            .or_else(|| (todo.source == "caseboard").then(|| format!("caseboard:{}", todo.id))),
        source_at: todo.source_at.clone(),
        delete_requested_at: todo.delete_requested_at.clone(),
        delete_reason: todo.delete_reason.clone(),
        deleted: todo.deleted_at.is_some(),
        remote_case_text,
    };
    let (json, hash) = canonical(&payload)?;
    Ok((payload, json, hash))
}

pub async fn start_pull_run(
    pool: &SqlitePool,
    app_token: &str,
    table_id: &str,
    view_id: &str,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO todo_feishu_sync_runs (id,app_token,table_id,view_id,status) VALUES (?,?,?,?,'running')",
    )
    .bind(&id)
    .bind(app_token)
    .bind(table_id)
    .bind(view_id)
    .execute(pool)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法建立预演批次"))?;
    Ok(id)
}

pub async fn fail_pull_run(pool: &SqlitePool, run_id: &str, error: &str) {
    let code = error.split(':').next().unwrap_or("FEISHU_TODO_PULL_FAILED");
    let _ = sqlx::query(
        "UPDATE todo_feishu_sync_runs SET status='failed',error_code=?,completed_at=datetime('now') WHERE id=? AND status='running'",
    )
    .bind(code)
    .bind(run_id)
    .execute(pool)
    .await;
}

#[allow(clippy::too_many_arguments)]
async fn insert_preview(
    transaction: &mut Transaction<'_, Sqlite>,
    run_id: &str,
    link_id: Option<&str>,
    item_id: Option<&str>,
    record_id: Option<&str>,
    key: Option<&str>,
    kind: &str,
    base_hash: Option<&str>,
    local_json: Option<&str>,
    local_hash: Option<&str>,
    remote_json: Option<&str>,
    remote_hash: Option<&str>,
    modified_at: Option<&str>,
    case_hint: Option<&str>,
    error_code: Option<&str>,
) -> Result<String, String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO todo_feishu_sync_previews (
            id,run_id,link_id,item_id,record_id,remote_business_key,change_kind,base_hash,
            local_payload_json,local_hash,remote_payload_json,remote_hash,remote_modified_at,
            case_hint,status,error_code
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,'pending',?)",
    )
    .bind(&id)
    .bind(run_id)
    .bind(link_id)
    .bind(item_id)
    .bind(record_id)
    .bind(key)
    .bind(kind)
    .bind(base_hash)
    .bind(local_json)
    .bind(local_hash)
    .bind(remote_json)
    .bind(remote_hash)
    .bind(modified_at)
    .bind(case_hint)
    .bind(error_code)
    .execute(&mut **transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法写入预演候选"))?;
    if matches!(
        kind,
        "conflict" | "duplicate_id" | "metadata_invalid" | "remote_missing"
    ) {
        sqlx::query(
            "INSERT INTO todo_feishu_sync_conflicts (
                id,preview_id,item_id,conflict_type,details_json
             ) VALUES (?,?,?,?,?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&id)
        .bind(item_id)
        .bind(error_code.unwrap_or("FEISHU_TODO_CONFLICT"))
        .bind("{}")
        .execute(&mut **transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法写入冲突台账"))?;
    }
    Ok(id)
}

pub async fn complete_pull(
    pool: &SqlitePool,
    run_id: &str,
    app_token: &str,
    table_id: &str,
    records: Vec<FeishuRemoteCaseRecord>,
) -> Result<TodoFeishuPullResult, String> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法开始预演事务"))?;
    sqlx::query("UPDATE todo_feishu_sync_previews SET status='superseded' WHERE status='pending'")
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法收敛旧预演"))?;
    sqlx::query("UPDATE todo_feishu_sync_conflicts SET status='dismissed',resolved_at=datetime('now') WHERE status='pending'")
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法收敛旧冲突"))?;
    let links = sqlx::query_as::<_, LinkRow>(
        "SELECT id,item_id,record_id,remote_business_key,remote_case_text,base_payload_hash,remote_modified_at
         FROM todo_feishu_sync_links WHERE app_token=? AND table_id=?",
    )
    .bind(app_token)
    .bind(table_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取同步链接"))?;
    let links_by_key = links
        .iter()
        .map(|link| (link.remote_business_key.as_str(), link))
        .collect::<HashMap<_, _>>();
    let todos = sqlx::query_as::<_, Todo>("SELECT * FROM case_todos")
        .fetch_all(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取本地事项"))?;
    let todos_by_id = todos
        .iter()
        .map(|todo| (todo.id.as_str(), todo))
        .collect::<HashMap<_, _>>();
    let mut key_counts = HashMap::<String, usize>::new();
    let mut source_counts = HashMap::<String, usize>::new();
    for record in &records {
        if let Some(fields) = record.fields.as_object() {
            if let Some(key) = text_from_value(fields.get("事项编号")) {
                *key_counts.entry(key).or_default() += 1;
            }
            if let Some(source) = text_from_value(fields.get("来源消息ID")) {
                *source_counts.entry(source).or_default() += 1;
            }
        }
    }
    let mut seen_record_ids = HashSet::new();
    let mut linked_item_ids = HashSet::new();
    let mut preview_count = 0usize;
    let mut conflict_count = 0usize;
    for record in &records {
        seen_record_ids.insert(record.record_id.as_str());
        let key = record
            .fields
            .as_object()
            .and_then(|fields| text_from_value(fields.get("事项编号")));
        let source = record
            .fields
            .as_object()
            .and_then(|fields| text_from_value(fields.get("来源消息ID")));
        let duplicated = key
            .as_ref()
            .is_some_and(|key| key_counts.get(key).copied().unwrap_or(0) > 1)
            || source
                .as_ref()
                .is_some_and(|value| source_counts.get(value).copied().unwrap_or(0) > 1);
        if duplicated {
            insert_preview(
                &mut transaction,
                run_id,
                None,
                None,
                Some(&record.record_id),
                key.as_deref(),
                "duplicate_id",
                None,
                None,
                None,
                Some(&record.fields.to_string()),
                None,
                record.last_modified_time.as_deref(),
                None,
                Some("FEISHU_TODO_DUPLICATE_ID"),
            )
            .await?;
            preview_count += 1;
            conflict_count += 1;
            continue;
        }
        let remote = match parse_remote(record) {
            Ok(remote) => remote,
            Err(error) => {
                insert_preview(
                    &mut transaction,
                    run_id,
                    None,
                    None,
                    Some(&record.record_id),
                    key.as_deref(),
                    "metadata_invalid",
                    None,
                    None,
                    None,
                    Some(&record.fields.to_string()),
                    None,
                    record.last_modified_time.as_deref(),
                    None,
                    Some(
                        error
                            .split(':')
                            .next()
                            .unwrap_or("FEISHU_TODO_METADATA_INVALID"),
                    ),
                )
                .await?;
                preview_count += 1;
                conflict_count += 1;
                continue;
            }
        };
        let link = links_by_key.get(remote.business_key.as_str()).copied();
        let local = link
            .and_then(|link| link.item_id.as_deref())
            .and_then(|item_id| todos_by_id.get(item_id).copied());
        if let Some(item) = local {
            linked_item_ids.insert(item.id.as_str());
            let (_, local_json, local_hash) =
                local_payload(item, link.and_then(|link| link.remote_case_text.clone()))?;
            let base = link.and_then(|link| link.base_payload_hash.as_deref());
            let (kind, error) = if item.source != remote.payload.source {
                ("conflict", Some("FEISHU_TODO_CONFLICT"))
            } else if local_hash == remote.payload_hash {
                ("noop", None)
            } else if base == Some(local_hash.as_str()) {
                (
                    if remote.payload.deleted {
                        "soft_delete_local"
                    } else {
                        "pull_to_local"
                    },
                    None,
                )
            } else if base == Some(remote.payload_hash.as_str()) {
                ("push_to_remote", None)
            } else {
                ("conflict", Some("FEISHU_TODO_CONFLICT"))
            };
            if kind == "noop" {
                if let Some(link) = link {
                    sqlx::query(
                        "UPDATE todo_feishu_sync_links SET base_payload_hash=?,last_local_hash=?,
                            last_remote_hash=?,remote_modified_at=?,status='active',last_synced_at=datetime('now'),
                            updated_at=datetime('now') WHERE id=?",
                    )
                    .bind(&local_hash)
                    .bind(&local_hash)
                    .bind(&remote.payload_hash)
                    .bind(&remote.modified_at)
                    .bind(&link.id)
                    .execute(&mut *transaction)
                    .await
                    .map_err(|_| {
                        sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法推进无变化基线")
                    })?;
                }
                continue;
            }
            insert_preview(
                &mut transaction,
                run_id,
                link.map(|link| link.id.as_str()),
                Some(&item.id),
                Some(&remote.record_id),
                Some(&remote.business_key),
                kind,
                base,
                Some(&local_json),
                Some(&local_hash),
                Some(&remote.payload_json),
                Some(&remote.payload_hash),
                remote.modified_at.as_deref(),
                remote.payload.remote_case_text.as_deref(),
                error,
            )
            .await?;
            preview_count += 1;
            if error.is_some() {
                conflict_count += 1;
            }
        } else {
            insert_preview(
                &mut transaction,
                run_id,
                link.map(|link| link.id.as_str()),
                None,
                Some(&remote.record_id),
                Some(&remote.business_key),
                "create_local",
                None,
                None,
                None,
                Some(&remote.payload_json),
                Some(&remote.payload_hash),
                remote.modified_at.as_deref(),
                remote.payload.remote_case_text.as_deref(),
                None,
            )
            .await?;
            preview_count += 1;
        }
    }
    for link in &links {
        if !seen_record_ids.contains(link.record_id.as_str()) {
            let local = link
                .item_id
                .as_deref()
                .and_then(|item_id| todos_by_id.get(item_id).copied());
            let local_state = match local {
                Some(todo) => {
                    let (_, json, hash) =
                        local_payload(todo, link.remote_case_text.clone())?;
                    Some((json, hash))
                }
                None => None,
            };
            if let Some(item_id) = link.item_id.as_deref() {
                linked_item_ids.insert(item_id);
            }
            insert_preview(
                &mut transaction,
                run_id,
                Some(&link.id),
                link.item_id.as_deref(),
                Some(&link.record_id),
                Some(&link.remote_business_key),
                "remote_missing",
                link.base_payload_hash.as_deref(),
                local_state.as_ref().map(|(json, _)| json.as_str()),
                local_state.as_ref().map(|(_, hash)| hash.as_str()),
                None,
                None,
                link.remote_modified_at.as_deref(),
                None,
                Some("FEISHU_TODO_REMOTE_MISSING"),
            )
            .await?;
            preview_count += 1;
            conflict_count += 1;
        }
    }
    for todo in &todos {
        if !linked_item_ids.contains(todo.id.as_str()) {
            let (_, local_json, local_hash) = local_payload(todo, None)?;
            insert_preview(
                &mut transaction,
                run_id,
                None,
                Some(&todo.id),
                None,
                None,
                "create_remote",
                None,
                Some(&local_json),
                Some(&local_hash),
                None,
                None,
                None,
                None,
                None,
            )
            .await?;
            preview_count += 1;
        }
    }
    sqlx::query(
        "UPDATE todo_feishu_sync_runs SET status='succeeded',remote_count=?,preview_count=?,conflict_count=?,completed_at=datetime('now') WHERE id=? AND status='running'",
    )
    .bind(records.len() as i64)
    .bind(preview_count as i64)
    .bind(conflict_count as i64)
    .bind(run_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成预演批次"))?;
    transaction
        .commit()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法提交预演事务"))?;
    Ok(TodoFeishuPullResult {
        run_id: run_id.into(),
        remote_count: records.len(),
        preview_count,
        conflict_count,
    })
}

pub async fn get_preview(pool: &SqlitePool) -> Result<TodoFeishuPreview, String> {
    let rows = sqlx::query_as::<_, TodoFeishuPreviewRow>(
        "SELECT id,run_id,link_id,item_id,record_id,remote_business_key,change_kind,
                local_payload_json,remote_payload_json,local_hash,remote_hash,remote_modified_at,
                case_hint,status,error_code,created_at
         FROM todo_feishu_sync_previews
         WHERE status='pending' ORDER BY created_at, id",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取待办预演"))?;
    let recent_runs = sqlx::query_as::<_, TodoFeishuRunRow>(
        "SELECT id,status,remote_count,preview_count,conflict_count,error_code,started_at,completed_at
         FROM todo_feishu_sync_runs ORDER BY started_at DESC LIMIT 10",
    )
    .fetch_all(pool)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取待办同步批次"))?;
    Ok(TodoFeishuPreview { rows, recent_runs })
}

pub async fn pending_preview(pool: &SqlitePool, id: &str) -> Result<PendingTodoPreview, String> {
    sqlx::query_as::<_, PendingTodoPreview>(
        "SELECT id,link_id,item_id,record_id,remote_business_key,change_kind,
                local_payload_json,remote_payload_json,local_hash,remote_hash,remote_modified_at
         FROM todo_feishu_sync_previews WHERE id=? AND status='pending'",
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取待办候选"))?
    .ok_or_else(|| sync_error("FEISHU_TODO_ALREADY_RESOLVED", "候选不存在或已经处理"))
}

fn shanghai_date(value: Option<&str>) -> Option<String> {
    let offset = FixedOffset::east_opt(8 * 60 * 60)?;
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.with_timezone(&offset).format("%Y-%m-%d").to_string())
}

pub fn parse_remote_for_verification(
    record: &FeishuRemoteCaseRecord,
) -> Result<(String, String, TodoSyncPayload), String> {
    let parsed = parse_remote(record)?;
    Ok((parsed.business_key, parsed.payload_hash, parsed.payload))
}

#[allow(clippy::too_many_arguments)]
pub async fn apply_remote(
    pool: &SqlitePool,
    preview: &PendingTodoPreview,
    verified_payload: TodoSyncPayload,
    verified_hash: &str,
    case_id: Option<String>,
    app_token: &str,
    table_id: &str,
    view_id: &str,
    action_id: &str,
) -> Result<String, String> {
    if preview.remote_hash.as_deref() != Some(verified_hash) {
        return Err(sync_error("FEISHU_TODO_STALE", "飞书事项在预演后已经变化"));
    }
    if !matches!(
        preview.change_kind.as_str(),
        "create_local" | "pull_to_local" | "soft_delete_local" | "conflict"
    ) {
        return Err(sync_error("FEISHU_TODO_CONFLICT", "该候选不能采用飞书版本"));
    }
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法开始应用事务"))?;
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM todo_feishu_sync_operation_audits WHERE action_id=?)",
    )
    .bind(action_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取动作审计"))?;
    if exists == 1 {
        return Err(sync_error("FEISHU_TODO_ALREADY_RESOLVED", "动作已经执行"));
    }
    if let Some(case_id) = case_id.as_deref() {
        let exists = sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM cases WHERE id=?)")
            .bind(case_id)
            .fetch_one(&mut *transaction)
            .await
            .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法验证案件"))?;
        if exists != 1 {
            return Err(sync_error("TODO_CASE_NOT_FOUND", "案件不存在"));
        }
    }
    let item_id = preview
        .item_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let tags_json = serde_json::to_string(&verified_payload.tags)
        .map_err(|_| sync_error("FEISHU_TODO_METADATA_INVALID", "标签无法序列化"))?;
    let done =
        i64::from(verified_payload.status == "completed" || verified_payload.done_at.is_some());
    let deleted_at = verified_payload.deleted.then(|| Utc::now().to_rfc3339());
    if preview.item_id.is_some() {
        let current = sqlx::query_as::<_, Todo>("SELECT * FROM case_todos WHERE id=?")
            .bind(&item_id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法重读本地事项"))?
            .ok_or_else(|| sync_error("FEISHU_TODO_STALE", "本地事项已经不存在"))?;
        let (_, _, current_hash) =
            local_payload(&current, verified_payload.remote_case_text.clone())?;
        if preview.local_hash.as_deref() != Some(current_hash.as_str()) {
            return Err(sync_error("FEISHU_TODO_STALE", "本地事项在预演后已经变化"));
        }
        if current.source != verified_payload.source {
            return Err(sync_error("FEISHU_TODO_CONFLICT", "来源不可变"));
        }
        sqlx::query(
            "UPDATE case_todos SET case_id=?,title=?,content=?,kind=?,priority=?,tags_json=?,next_action=?,
                status=?,done=?,done_at=?,due_at=?,due_date=?,remind_at=?,source_message_id=?,source_at=?,
                delete_requested_at=?,delete_reason=?,deleted_at=?,updated_at=datetime('now') WHERE id=?",
        )
        .bind(case_id.as_deref().or(current.case_id.as_deref()))
        .bind(&verified_payload.title)
        .bind(&verified_payload.content)
        .bind(&verified_payload.kind)
        .bind(&verified_payload.priority)
        .bind(&tags_json)
        .bind(&verified_payload.next_action)
        .bind(&verified_payload.status)
        .bind(done)
        .bind(&verified_payload.done_at)
        .bind(&verified_payload.due_at)
        .bind(shanghai_date(verified_payload.due_at.as_deref()))
        .bind(&verified_payload.remind_at)
        .bind(&verified_payload.source_message_id)
        .bind(&verified_payload.source_at)
        .bind(&verified_payload.delete_requested_at)
        .bind(&verified_payload.delete_reason)
        .bind(deleted_at)
        .bind(&item_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法更新本地事项"))?;
    } else {
        sqlx::query(
            "INSERT INTO case_todos (
                id,case_id,title,content,kind,priority,tags_json,next_action,status,done,done_at,
                due_at,due_date,remind_at,source,source_message_id,source_at,delete_requested_at,
                delete_reason,deleted_at
             ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
        )
        .bind(&item_id)
        .bind(&case_id)
        .bind(&verified_payload.title)
        .bind(&verified_payload.content)
        .bind(&verified_payload.kind)
        .bind(&verified_payload.priority)
        .bind(&tags_json)
        .bind(&verified_payload.next_action)
        .bind(&verified_payload.status)
        .bind(done)
        .bind(&verified_payload.done_at)
        .bind(&verified_payload.due_at)
        .bind(shanghai_date(verified_payload.due_at.as_deref()))
        .bind(&verified_payload.remind_at)
        .bind(&verified_payload.source)
        .bind(&verified_payload.source_message_id)
        .bind(&verified_payload.source_at)
        .bind(&verified_payload.delete_requested_at)
        .bind(&verified_payload.delete_reason)
        .bind(deleted_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法创建本地事项"))?;
    }
    let link_id = preview
        .link_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let record_id = preview
        .record_id
        .as_deref()
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "候选缺少 record_id"))?;
    let business_key = preview
        .remote_business_key
        .as_deref()
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "候选缺少事项编号"))?;
    sqlx::query(
        "INSERT INTO todo_feishu_sync_links (
            id,item_id,app_token,table_id,view_id,record_id,remote_business_key,remote_case_text,
            mapped_case_id,base_payload_hash,last_local_hash,last_remote_hash,remote_modified_at,status,last_synced_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,'active',datetime('now'))
         ON CONFLICT(app_token,table_id,record_id) DO UPDATE SET
            item_id=excluded.item_id,view_id=excluded.view_id,remote_business_key=excluded.remote_business_key,
            remote_case_text=excluded.remote_case_text,mapped_case_id=excluded.mapped_case_id,
            base_payload_hash=excluded.base_payload_hash,last_local_hash=excluded.last_local_hash,
            last_remote_hash=excluded.last_remote_hash,remote_modified_at=excluded.remote_modified_at,
            status='active',last_synced_at=datetime('now'),updated_at=datetime('now')",
    )
    .bind(&link_id)
    .bind(&item_id)
    .bind(app_token)
    .bind(table_id)
    .bind(view_id)
    .bind(record_id)
    .bind(business_key)
    .bind(&verified_payload.remote_case_text)
    .bind(&case_id)
    .bind(verified_hash)
    .bind(verified_hash)
    .bind(verified_hash)
    .bind(&preview.remote_modified_at)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法更新同步链接"))?;
    sqlx::query(
        "UPDATE todo_feishu_sync_previews SET status='applied_local',resolved_at=datetime('now') WHERE id=? AND status='pending'",
    )
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成候选"))?;
    let conflict_resolution = if preview.link_id.is_some() && preview.item_id.is_none() {
        "keep_both"
    } else {
        "feishu"
    };
    sqlx::query(
        "UPDATE todo_feishu_sync_conflicts SET status='resolved',resolution=?,resolved_at=datetime('now')
         WHERE preview_id=? AND status='pending'",
    )
    .bind(conflict_resolution)
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成冲突台账"))?;
    sqlx::query(
        "INSERT INTO todo_feishu_sync_operation_audits (
            action_id,preview_id,direction,status,before_hash,after_hash,completed_at
         ) VALUES (?,?,'local','succeeded',?,?,datetime('now'))",
    )
    .bind(action_id)
    .bind(&preview.id)
    .bind(&preview.local_hash)
    .bind(verified_hash)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法写入动作审计"))?;
    transaction
        .commit()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法提交应用事务"))?;
    Ok(item_id)
}

pub async fn dismiss(pool: &SqlitePool, preview_id: &str, action_id: &str) -> Result<(), String> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法开始关闭事务"))?;
    let result = sqlx::query(
        "UPDATE todo_feishu_sync_previews SET status='dismissed',resolved_at=datetime('now') WHERE id=? AND status='pending'",
    )
    .bind(preview_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法关闭候选"))?;
    if result.rows_affected() != 1 {
        return Err(sync_error("FEISHU_TODO_ALREADY_RESOLVED", "候选已经处理"));
    }
    sqlx::query(
        "UPDATE todo_feishu_sync_conflicts SET status='dismissed',resolution='dismiss',resolved_at=datetime('now')
         WHERE preview_id=? AND status='pending'",
    )
    .bind(preview_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法关闭冲突台账"))?;
    sqlx::query(
        "INSERT INTO todo_feishu_sync_operation_audits (action_id,preview_id,direction,status,completed_at)
         VALUES (?,?,'dismiss','succeeded',datetime('now'))",
    )
    .bind(action_id)
    .bind(preview_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法写入动作审计"))?;
    transaction
        .commit()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法提交关闭事务"))?;
    Ok(())
}

pub async fn confirm_remote_missing_deleted(
    pool: &SqlitePool,
    preview: &PendingTodoPreview,
    action_id: &str,
) -> Result<String, String> {
    if preview.change_kind != "remote_missing" {
        return Err(sync_error("FEISHU_TODO_CONFLICT", "候选不是远端缺失"));
    }
    let item_id = preview
        .item_id
        .as_deref()
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "候选缺少本地事项"))?;
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法开始确认事务"))?;
    let todo = sqlx::query_as::<_, Todo>("SELECT * FROM case_todos WHERE id=?")
        .bind(item_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法重读本地事项"))?
        .ok_or_else(|| sync_error("FEISHU_TODO_STALE", "本地事项已经不存在"))?;
    let remote_case_text = if let Some(link_id) = preview.link_id.as_deref() {
        sqlx::query_scalar::<_, Option<String>>(
            "SELECT remote_case_text FROM todo_feishu_sync_links WHERE id=?",
        )
        .bind(link_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取同步链接"))?
        .flatten()
    } else {
        None
    };
    let (_, _, current_hash) = local_payload(&todo, remote_case_text)?;
    if preview.local_hash.as_deref() != Some(current_hash.as_str()) {
        return Err(sync_error(
            "FEISHU_TODO_STALE",
            "本地事项在预演后已经变化",
        ));
    }
    sqlx::query(
        "UPDATE case_todos SET status='deleted',deleted_at=COALESCE(deleted_at,datetime('now')),
            updated_at=datetime('now') WHERE id=?",
    )
    .bind(item_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法软删除本地事项"))?;
    if let Some(link_id) = preview.link_id.as_deref() {
        sqlx::query(
            "UPDATE todo_feishu_sync_links SET status='archived',updated_at=datetime('now') WHERE id=?",
        )
        .bind(link_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法归档同步链接"))?;
    }
    sqlx::query(
        "UPDATE todo_feishu_sync_previews SET status='applied_local',resolved_at=datetime('now')
         WHERE id=? AND status='pending'",
    )
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成候选"))?;
    sqlx::query(
        "UPDATE todo_feishu_sync_conflicts SET status='resolved',resolution='confirm_remote_deleted',
            resolved_at=datetime('now') WHERE preview_id=? AND status='pending'",
    )
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成冲突台账"))?;
    sqlx::query(
        "INSERT INTO todo_feishu_sync_operation_audits (
            action_id,preview_id,direction,status,before_hash,completed_at
         ) VALUES (?,?,'local','succeeded',?,datetime('now'))",
    )
    .bind(action_id)
    .bind(&preview.id)
    .bind(&current_hash)
    .execute(&mut *transaction)
    .await
    .map_err(|error| {
        if error.to_string().contains("UNIQUE") {
            sync_error("FEISHU_TODO_ALREADY_RESOLVED", "动作已经执行")
        } else {
            sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法写入动作审计")
        }
    })?;
    transaction
        .commit()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法提交确认事务"))?;
    Ok(item_id.to_string())
}

fn milliseconds(value: Option<&str>) -> Result<Value, String> {
    match value {
        None => Ok(Value::Null),
        Some(value) => DateTime::parse_from_rfc3339(value)
            .map(|time| Value::from(time.timestamp_millis()))
            .map_err(|_| sync_error("FEISHU_TODO_METADATA_INVALID", "本地时间缺少明确时区")),
    }
}

pub fn payload_to_feishu_fields(
    payload: &TodoSyncPayload,
    business_key: &str,
    item_id: &str,
) -> Result<Value, String> {
    let kind = match payload.kind.as_str() {
        "idea" => "想法",
        "todo" => "待办",
        "reminder" => "提醒",
        "reference" => "资料",
        "memo" => "备忘",
        _ => return Err(sync_error("FEISHU_TODO_METADATA_INVALID", "本地类型无效")),
    };
    let status = match payload.status.as_str() {
        "inbox" => "收件箱",
        "in_progress" => "进行中",
        "waiting" => "等待",
        "completed" => "完成",
        "delete_pending" => "删除待确认",
        "deleted" => "已删除",
        _ => return Err(sync_error("FEISHU_TODO_METADATA_INVALID", "本地状态无效")),
    };
    let priority = match payload.priority.as_str() {
        "high" => "高",
        "medium" => "中",
        "low" => "低",
        "unjudged" => {
            return Err(sync_error(
                "FEISHU_TODO_METADATA_INVALID",
                "同步到飞书前必须选择优先级",
            ));
        }
        _ => return Err(sync_error("FEISHU_TODO_METADATA_INVALID", "本地优先级无效")),
    };
    let source_message_id = payload
        .source_message_id
        .clone()
        .unwrap_or_else(|| format!("caseboard:{item_id}"));
    Ok(serde_json::json!({
        "事项": payload.title,
        "事项编号": business_key,
        "原始内容": payload.content,
        "类型": kind,
        "状态": status,
        "优先级": priority,
        "下一步动作": payload.next_action,
        "截止时间": milliseconds(payload.due_at.as_deref())?,
        "提醒时间": milliseconds(payload.remind_at.as_deref())?,
        "关联案件": payload.remote_case_text,
        "标签": payload.tags,
        "完成时间": milliseconds(payload.done_at.as_deref())?,
        "删除请求时间": milliseconds(payload.delete_requested_at.as_deref())?,
        "删除原因": payload.delete_reason,
        "来源消息ID": source_message_id,
        "来源时间": milliseconds(payload.source_at.as_deref())?,
    }))
}

pub async fn prepare_remote_action(
    pool: &SqlitePool,
    preview: &PendingTodoPreview,
    action_id: &str,
) -> Result<(String, String, TodoSyncPayload), String> {
    if !matches!(
        preview.change_kind.as_str(),
        "create_remote" | "push_to_remote" | "conflict" | "remote_missing"
    ) {
        return Err(sync_error("FEISHU_TODO_CONFLICT", "该候选不能采用本地版本"));
    }
    let item_id = preview
        .item_id
        .as_deref()
        .ok_or_else(|| sync_error("FEISHU_TODO_METADATA_INVALID", "候选缺少本地事项"))?;
    let todo = sqlx::query_as::<_, Todo>("SELECT * FROM case_todos WHERE id=?")
        .bind(item_id)
        .fetch_optional(pool)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法重读本地事项"))?
        .ok_or_else(|| sync_error("FEISHU_TODO_STALE", "本地事项已经不存在"))?;
    let (payload, _, hash) = local_payload(
        &todo,
        preview
            .remote_payload_json
            .as_deref()
            .and_then(|value| serde_json::from_str::<TodoSyncPayload>(value).ok())
            .and_then(|payload| payload.remote_case_text),
    )?;
    if preview.local_hash.as_deref() != Some(hash.as_str()) {
        return Err(sync_error("FEISHU_TODO_STALE", "本地事项在预演后已经变化"));
    }
    let business_key = preview
        .remote_business_key
        .clone()
        .unwrap_or_else(|| format!("CB-{}", item_id.replace('-', "").to_uppercase()));
    let result = sqlx::query(
        "INSERT INTO todo_feishu_sync_operation_audits (
            action_id,preview_id,direction,status,before_hash,after_hash
         ) VALUES (?,?,'remote','started',?,?)",
    )
    .bind(action_id)
    .bind(&preview.id)
    .bind(&preview.remote_hash)
    .bind(&hash)
    .execute(pool)
    .await;
    if let Err(error) = result {
        if error.to_string().contains("UNIQUE") {
            return Err(sync_error("FEISHU_TODO_ALREADY_RESOLVED", "动作已经执行"));
        }
        return Err(sync_error(
            "FEISHU_TODO_DB_WRITE_FAILED",
            "无法建立动作审计",
        ));
    }
    Ok((item_id.to_string(), business_key, payload))
}

pub async fn mark_remote_action(
    pool: &SqlitePool,
    action_id: &str,
    status: &str,
    error_code: Option<&str>,
) {
    let _ = sqlx::query(
        "UPDATE todo_feishu_sync_operation_audits SET status=?,error_code=?,completed_at=datetime('now')
         WHERE action_id=? AND status='started'",
    )
    .bind(status)
    .bind(error_code)
    .bind(action_id)
    .execute(pool)
    .await;
    if status == "write_uncertain" {
        let _ = sqlx::query(
            "UPDATE todo_feishu_sync_previews SET status='write_uncertain',error_code='FEISHU_TODO_WRITE_UNCERTAIN',
                resolved_at=datetime('now')
             WHERE id=(SELECT preview_id FROM todo_feishu_sync_operation_audits WHERE action_id=?)
               AND status='pending'",
        )
        .bind(action_id)
        .execute(pool)
        .await;
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn finish_remote_action(
    pool: &SqlitePool,
    preview: &PendingTodoPreview,
    action_id: &str,
    item_id: &str,
    app_token: &str,
    table_id: &str,
    view_id: &str,
    record_id: &str,
    business_key: &str,
    payload: &TodoSyncPayload,
    payload_hash: &str,
    modified_at: Option<&str>,
) -> Result<(), String> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法开始写回确认事务"))?;
    let current_status = sqlx::query_scalar::<_, String>(
        "SELECT status FROM todo_feishu_sync_operation_audits WHERE action_id=?",
    )
    .bind(action_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法读取动作审计"))?
    .ok_or_else(|| sync_error("FEISHU_TODO_ALREADY_RESOLVED", "动作不存在"))?;
    if current_status != "started" {
        return Err(sync_error("FEISHU_TODO_ALREADY_RESOLVED", "动作已经结束"));
    }
    let mut link_updated = 0;
    if let Some(link_id) = preview.link_id.as_deref() {
        link_updated = sqlx::query(
            "UPDATE todo_feishu_sync_links SET item_id=?,view_id=?,record_id=?,remote_business_key=?,
                remote_case_text=?,base_payload_hash=?,last_local_hash=?,last_remote_hash=?,
                remote_modified_at=?,status='active',last_synced_at=datetime('now'),updated_at=datetime('now')
             WHERE id=? AND app_token=? AND table_id=?",
        )
        .bind(item_id)
        .bind(view_id)
        .bind(record_id)
        .bind(business_key)
        .bind(&payload.remote_case_text)
        .bind(payload_hash)
        .bind(payload_hash)
        .bind(payload_hash)
        .bind(modified_at)
        .bind(link_id)
        .bind(app_token)
        .bind(table_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法更新同步链接"))?
        .rows_affected();
    }
    if link_updated == 0 {
        sqlx::query(
            "INSERT INTO todo_feishu_sync_links (
            id,item_id,app_token,table_id,view_id,record_id,remote_business_key,remote_case_text,
            base_payload_hash,last_local_hash,last_remote_hash,remote_modified_at,status,last_synced_at
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,'active',datetime('now'))
         ON CONFLICT(app_token,table_id,record_id) DO UPDATE SET
            item_id=excluded.item_id,view_id=excluded.view_id,remote_business_key=excluded.remote_business_key,
            remote_case_text=excluded.remote_case_text,base_payload_hash=excluded.base_payload_hash,
            last_local_hash=excluded.last_local_hash,last_remote_hash=excluded.last_remote_hash,
            remote_modified_at=excluded.remote_modified_at,status='active',last_synced_at=datetime('now'),
            updated_at=datetime('now')",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(item_id)
        .bind(app_token)
        .bind(table_id)
        .bind(view_id)
        .bind(record_id)
        .bind(business_key)
        .bind(&payload.remote_case_text)
        .bind(payload_hash)
        .bind(payload_hash)
        .bind(payload_hash)
        .bind(modified_at)
        .execute(&mut *transaction)
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法更新同步链接"))?;
    }
    let affected = sqlx::query(
        "UPDATE todo_feishu_sync_previews SET status='applied_remote',resolved_at=datetime('now')
         WHERE id=? AND status='pending'",
    )
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成候选"))?
    .rows_affected();
    if affected != 1 {
        return Err(sync_error("FEISHU_TODO_ALREADY_RESOLVED", "候选已经处理"));
    }
    sqlx::query(
        "UPDATE todo_feishu_sync_conflicts SET status='resolved',resolution='local',resolved_at=datetime('now')
         WHERE preview_id=? AND status='pending'",
    )
    .bind(&preview.id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成冲突台账"))?;
    sqlx::query(
        "UPDATE todo_feishu_sync_operation_audits SET status='succeeded',error_code=NULL,
            after_hash=?,completed_at=datetime('now') WHERE action_id=? AND status='started'",
    )
    .bind(payload_hash)
    .bind(action_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法完成动作审计"))?;
    transaction
        .commit()
        .await
        .map_err(|_| sync_error("FEISHU_TODO_DB_WRITE_FAILED", "无法提交写回确认"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote(fields: Value) -> FeishuRemoteCaseRecord {
        FeishuRemoteCaseRecord {
            record_id: "rec_test".into(),
            fields,
            last_modified_time: Some("1776614400000".into()),
        }
    }

    #[test]
    fn parses_audited_remote_choices_and_keeps_empty_times_null() {
        let parsed = parse_remote(&remote(serde_json::json!({
            "事项": "提交代理词",
            "事项编号": "P0017",
            "原始内容": "核对附件",
            "类型": "待办",
            "状态": "收件箱",
            "优先级": "高",
            "标签": ["文书", "紧急"],
            "来源消息ID": "hermes-message-17",
            "来源时间": 1776528000000_i64,
            "截止时间": null,
            "提醒时间": null
        })))
        .expect("parse remote inbox row");
        assert_eq!(parsed.business_key, "P0017");
        assert_eq!(parsed.payload.kind, "todo");
        assert_eq!(parsed.payload.status, "inbox");
        assert_eq!(parsed.payload.source, "hermes");
        assert_eq!(parsed.payload.due_at, None);
        assert_eq!(parsed.payload.remind_at, None);
    }

    #[test]
    fn outbound_requires_an_explicit_remote_priority() {
        let payload = TodoSyncPayload {
            title: "test".into(),
            content: String::new(),
            kind: "todo".into(),
            priority: "unjudged".into(),
            tags: vec![],
            next_action: None,
            status: "inbox".into(),
            due_at: None,
            remind_at: None,
            done_at: None,
            source: "caseboard".into(),
            source_message_id: None,
            source_at: None,
            delete_requested_at: None,
            delete_reason: None,
            deleted: false,
            remote_case_text: None,
        };
        assert!(payload_to_feishu_fields(&payload, "CB-1", "item-1")
            .unwrap_err()
            .starts_with("FEISHU_TODO_METADATA_INVALID:"));
    }
}
