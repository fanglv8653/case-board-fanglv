//! 刑事辩护 SOP 任务引擎。
//!
//! 法定期限仍由 `criminal_deadline_items` 独占管理；本模块只关联其主键。
//! 模板版本在案件首次实例化时固定，刷新只追加由已确认事件触发的任务。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

const TEMPLATE_CODE: &str = "criminal_defense_standard_v1";
const SOP_SOURCE: &str = "criminal_sop";
const SOURCE_MANUAL_CONFIRMED: &str = "manual_confirmed";
const SOURCE_ACCEPTED_EXTRACTION_CANDIDATE: &str = "accepted_extraction_candidate";
const SOURCE_WORKFLOW_CONFIRMED: &str = "workflow_confirmed";

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalWorkflow {
    pub id: String,
    pub case_id: String,
    pub template_version_id: String,
    pub status: String,
    pub current_stage_code: Option<String>,
    pub started_at: String,
    pub closed_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalWorkflowTask {
    pub id: String,
    pub workflow_id: String,
    pub case_id: String,
    pub template_node_id: String,
    pub node_code: String,
    pub title: String,
    pub stage_code: String,
    pub stage_sort: i64,
    pub node_sort: i64,
    pub task_type: String,
    pub applicability_status: String,
    pub status: String,
    pub occurrence_key: String,
    pub occurrence_no: i64,
    pub trigger_event: String,
    pub trigger_event_id: String,
    pub trigger_source_type: String,
    pub trigger_source_ref_id: Option<String>,
    pub planned_at: Option<String>,
    pub original_planned_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub deferred_at: Option<String>,
    pub ignored_at: Option<String>,
    pub reopened_at: Option<String>,
    pub result: Option<String>,
    pub next_action: Option<String>,
    pub duration_minutes: Option<i64>,
    pub disposition_reason: Option<String>,
    pub client_feedback_recorded: bool,
    pub time_nature: String,
    pub deadline_item_id: Option<String>,
    pub work_item_id: Option<String>,
    pub assigned_to: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalTaskEvent {
    pub id: String,
    pub task_id: String,
    pub case_id: String,
    pub event_type: String,
    pub actor: String,
    pub event_id: Option<String>,
    pub source_type: Option<String>,
    pub source_ref_id: Option<String>,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub reason: Option<String>,
    pub payload_json: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalReminderDelivery {
    pub id: String,
    pub task_id: String,
    pub case_id: String,
    pub reminder_key: String,
    pub channel: String,
    pub scheduled_for: String,
    pub status: String,
    pub claimed_at: Option<String>,
    pub sent_at: Option<String>,
    pub failed_at: Option<String>,
    pub error_message: Option<String>,
    pub attempt_count: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RefreshCriminalWorkflowInput {
    pub case_id: String,
    pub event_code: String,
    pub event_id: String,
    pub source_type: String,
    pub source_ref_id: Option<String>,
    pub confirmed_by: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshCriminalWorkflowResult {
    pub workflow: CriminalWorkflow,
    pub generated_count: i64,
    pub preserved_count: i64,
    pub tasks: Vec<CriminalWorkflowTask>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CriminalTaskActionInput {
    pub task_id: String,
    pub action: String,
    pub actor: String,
    pub planned_at: Option<String>,
    pub result: Option<String>,
    pub next_action: Option<String>,
    pub duration_minutes: Option<i64>,
    pub reason: Option<String>,
    pub client_feedback_recorded: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CreateCriminalTaskOccurrenceInput {
    pub case_id: String,
    pub node_code: String,
    pub actor: String,
    pub occurrence_key: Option<String>,
    pub planned_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CriminalTaskFilter {
    pub case_id: Option<String>,
    pub statuses: Option<Vec<String>>,
    pub planned_from: Option<String>,
    pub planned_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalTaskSummaryRow {
    pub case_id: String,
    pub case_name: String,
    pub task_id: String,
    pub title: String,
    pub stage_code: String,
    pub task_type: String,
    pub status: String,
    pub applicability_status: String,
    pub planned_at: Option<String>,
    pub client_feedback_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CriminalDeadlineCalendarRow {
    pub deadline_id: String,
    pub case_id: String,
    pub case_name: String,
    pub title: String,
    pub rule_code: Option<String>,
    pub deadline_at: String,
    pub status: String,
    pub applicability_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClaimCriminalRemindersInput {
    pub now: String,
    pub channel: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MarkCriminalReminderInput {
    pub delivery_id: String,
    pub sent: bool,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, FromRow)]
struct TemplateNode {
    id: String,
    node_code: String,
    title: String,
    stage_code: String,
    stage_sort: i64,
    node_sort: i64,
    trigger_event: String,
    task_type: String,
    default_applicability: String,
    repeatable: bool,
    time_nature: String,
    deadline_rule_codes_json: String,
}

const TASK_SELECT: &str = r#"SELECT id,workflow_id,case_id,template_node_id,node_code,title,
stage_code,stage_sort,node_sort,task_type,applicability_status,status,occurrence_key,occurrence_no,
trigger_event,trigger_event_id,trigger_source_type,trigger_source_ref_id,planned_at,original_planned_at,
started_at,completed_at,deferred_at,ignored_at,reopened_at,result,next_action,duration_minutes,
disposition_reason,client_feedback_recorded,time_nature,deadline_item_id,work_item_id,assigned_to,
created_at,updated_at FROM criminal_case_tasks"#;

const WORKFLOW_SELECT: &str = r#"SELECT id,case_id,template_version_id,status,current_stage_code,
started_at,closed_at,created_at,updated_at FROM criminal_case_workflows"#;

fn require(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{label}不能为空"))
    } else {
        Ok(())
    }
}

fn validate_refresh_event(event_code: &str) -> Result<(), String> {
    if matches!(
        event_code,
        "case_created"
            | "engagement_decision_confirmed"
            | "detention_confirmed"
            | "arrest_review_request_confirmed"
            | "non_arrest_confirmed"
            | "arrest_confirmed"
            | "prosecution_transfer_confirmed"
            | "plea_process_confirmed"
            | "public_prosecution_confirmed"
            | "court_acceptance_confirmed"
            | "hearing_scheduled"
            | "hearing_completed"
            | "first_instance_judgment_received"
            | "appeal_intention_confirmed"
            | "appeal_confirmed"
            | "second_instance_procedure_confirmed"
            | "second_instance_decision_received"
            | "second_instance_closed"
    ) {
        Ok(())
    } else {
        Err(format!("event_code 不是 SOP-N0 合法确认事件: {event_code}"))
    }
}

fn validate_confirmed_source(source_type: &str, source_ref_id: Option<&str>) -> Result<(), String> {
    match source_type {
        SOURCE_MANUAL_CONFIRMED => Ok(()),
        SOURCE_ACCEPTED_EXTRACTION_CANDIDATE | SOURCE_WORKFLOW_CONFIRMED => {
            if source_ref_id.is_some_and(|value| !value.trim().is_empty()) {
                Ok(())
            } else {
                Err(format!("{source_type} 触发 SOP 必须提供 source_ref_id"))
            }
        }
        _ => Err(format!(
            "source_type 未证明已经人工确认，拒绝刷新刑事 SOP: {source_type}"
        )),
    }
}

pub async fn refresh(
    pool: &SqlitePool,
    input: RefreshCriminalWorkflowInput,
) -> Result<RefreshCriminalWorkflowResult, String> {
    require(&input.case_id, "case_id")?;
    require(&input.event_code, "event_code")?;
    require(&input.event_id, "event_id")?;
    require(&input.source_type, "source_type")?;
    require(&input.confirmed_by, "confirmed_by")?;
    validate_refresh_event(&input.event_code)?;
    validate_confirmed_source(&input.source_type, input.source_ref_id.as_deref())?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;

    let template_id: String = sqlx::query_scalar(
        "SELECT id FROM criminal_workflow_template_versions WHERE template_code=? AND status='published' ORDER BY version DESC LIMIT 1",
    ).bind(TEMPLATE_CODE).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
    let workflow_id = format!("criminal-sop:{}", input.case_id);
    sqlx::query("INSERT INTO criminal_case_workflows(id,case_id,template_version_id) VALUES(?,?,?) ON CONFLICT(case_id) DO NOTHING")
        .bind(&workflow_id).bind(&input.case_id).bind(&template_id)
        .execute(&mut *tx).await.map_err(|e| e.to_string())?;
    let workflow: CriminalWorkflow = sqlx::query_as(&format!("{WORKFLOW_SELECT} WHERE case_id=?"))
        .bind(&input.case_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;

    let nodes = sqlx::query_as::<_, TemplateNode>(
        "SELECT id,node_code,title,stage_code,stage_sort,node_sort,trigger_event,task_type,default_applicability,repeatable,time_nature,deadline_rule_codes_json FROM criminal_workflow_template_nodes WHERE template_version_id=? AND trigger_event=? AND enabled=1 ORDER BY stage_sort,node_sort",
    ).bind(&workflow.template_version_id).bind(&input.event_code).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;

    let mut generated = 0_i64;
    for node in &nodes {
        let occurrence_key = format!("event:{}", input.event_id);
        if insert_task(
            &mut tx,
            InsertTaskInput {
                workflow: &workflow,
                node,
                occurrence_key: &occurrence_key,
                occurrence_no: 1,
                event_id: &input.event_id,
                source_type: &input.source_type,
                source_ref_id: input.source_ref_id.as_deref(),
                planned_at: None,
            },
        )
        .await?
        {
            generated += 1;
            let task_id: String = sqlx::query_scalar("SELECT id FROM criminal_case_tasks WHERE workflow_id=? AND node_code=? AND occurrence_key=?")
                .bind(&workflow.id).bind(&node.node_code).bind(&occurrence_key).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
            insert_event(
                &mut tx,
                &task_id,
                &input.case_id,
                "generated",
                &input.confirmed_by,
                Some(&input.event_id),
                Some(&input.source_type),
                input.source_ref_id.as_deref(),
                None,
                initial_status(&node.default_applicability),
                None,
                "{}",
            )
            .await?;
        }
    }

    if is_major_procedure_event(&input.event_code) {
        if let Some(feedback) = template_node(
            &mut tx,
            &workflow.template_version_id,
            "common_client_feedback",
        )
        .await?
        {
            let key = format!("procedure-feedback:{}", input.event_id);
            let occurrence_no =
                next_occurrence_no(&mut tx, &workflow.id, &feedback.node_code).await?;
            if insert_task(
                &mut tx,
                InsertTaskInput {
                    workflow: &workflow,
                    node: &feedback,
                    occurrence_key: &key,
                    occurrence_no,
                    event_id: &input.event_id,
                    source_type: &input.source_type,
                    source_ref_id: input.source_ref_id.as_deref(),
                    planned_at: None,
                },
            )
            .await?
            {
                generated += 1;
                let task_id: String = sqlx::query_scalar("SELECT id FROM criminal_case_tasks WHERE workflow_id=? AND node_code=? AND occurrence_key=?")
                    .bind(&workflow.id).bind(&feedback.node_code).bind(&key).fetch_one(&mut *tx).await.map_err(|e| e.to_string())?;
                insert_event(
                    &mut tx,
                    &task_id,
                    &input.case_id,
                    "generated",
                    &input.confirmed_by,
                    Some(&input.event_id),
                    Some(&input.source_type),
                    input.source_ref_id.as_deref(),
                    None,
                    initial_status(&feedback.default_applicability),
                    None,
                    "{}",
                )
                .await?;
            }
        }
    }
    link_missing_deadlines(&mut tx, &workflow).await?;
    if let Some(stage) = nodes
        .iter()
        .map(|n| n.stage_code.as_str())
        .find(|s| *s != "current")
    {
        sqlx::query("UPDATE criminal_case_workflows SET current_stage_code=?,updated_at=datetime('now') WHERE id=?")
            .bind(stage).bind(&workflow.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    let tasks = list_tasks(
        pool,
        CriminalTaskFilter {
            case_id: Some(input.case_id.clone()),
            ..Default::default()
        },
    )
    .await?;
    let workflow = get_workflow(pool, &input.case_id)
        .await?
        .ok_or_else(|| "流程实例读取失败".to_string())?;
    Ok(RefreshCriminalWorkflowResult {
        workflow,
        generated_count: generated,
        preserved_count: nodes.len() as i64 - generated.min(nodes.len() as i64),
        tasks,
    })
}

struct InsertTaskInput<'a> {
    workflow: &'a CriminalWorkflow,
    node: &'a TemplateNode,
    occurrence_key: &'a str,
    occurrence_no: i64,
    event_id: &'a str,
    source_type: &'a str,
    source_ref_id: Option<&'a str>,
    planned_at: Option<&'a str>,
}

async fn insert_task(
    tx: &mut Transaction<'_, Sqlite>,
    input: InsertTaskInput<'_>,
) -> Result<bool, String> {
    let InsertTaskInput {
        workflow,
        node,
        occurrence_key,
        occurrence_no,
        event_id,
        source_type,
        source_ref_id,
        planned_at,
    } = input;
    let status = if planned_at.is_some() && node.default_applicability == "applicable" {
        "pending"
    } else {
        initial_status(&node.default_applicability)
    };
    let deadline_id = find_deadline(tx, &workflow.case_id, &node.deadline_rule_codes_json).await?;
    let result = sqlx::query("INSERT OR IGNORE INTO criminal_case_tasks(id,workflow_id,case_id,template_node_id,node_code,title,stage_code,stage_sort,node_sort,task_type,applicability_status,status,occurrence_key,occurrence_no,trigger_event,trigger_event_id,trigger_source_type,trigger_source_ref_id,planned_at,time_nature,deadline_item_id) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(&workflow.id).bind(&workflow.case_id).bind(&node.id)
        .bind(&node.node_code).bind(&node.title).bind(&node.stage_code).bind(node.stage_sort).bind(node.node_sort)
        .bind(&node.task_type).bind(&node.default_applicability).bind(status).bind(occurrence_key).bind(occurrence_no)
        .bind(&node.trigger_event).bind(event_id).bind(source_type).bind(source_ref_id).bind(planned_at)
        .bind(&node.time_nature).bind(deadline_id).execute(&mut **tx).await.map_err(|e| e.to_string())?;
    Ok(result.rows_affected() == 1)
}

fn initial_status(applicability: &str) -> &'static str {
    if applicability == "pending_confirmation" {
        "pending_confirmation"
    } else {
        "unscheduled"
    }
}

fn is_major_procedure_event(event: &str) -> bool {
    matches!(
        event,
        "detention_confirmed"
            | "arrest_review_request_confirmed"
            | "non_arrest_confirmed"
            | "arrest_confirmed"
            | "prosecution_transfer_confirmed"
            | "public_prosecution_confirmed"
            | "court_acceptance_confirmed"
            | "hearing_completed"
            | "first_instance_judgment_received"
            | "appeal_confirmed"
            | "second_instance_decision_received"
    )
}

async fn find_deadline(
    tx: &mut Transaction<'_, Sqlite>,
    case_id: &str,
    rules_json: &str,
) -> Result<Option<String>, String> {
    let rules: Vec<String> = serde_json::from_str(rules_json).unwrap_or_default();
    for rule in rules {
        let found = sqlx::query_scalar("SELECT id FROM criminal_deadline_items WHERE case_id=? AND rule_code=? AND deleted_at IS NULL ORDER BY updated_at DESC LIMIT 1")
            .bind(case_id).bind(rule).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())?;
        if found.is_some() {
            return Ok(found);
        }
    }
    Ok(None)
}

async fn template_node(
    tx: &mut Transaction<'_, Sqlite>,
    template_id: &str,
    code: &str,
) -> Result<Option<TemplateNode>, String> {
    sqlx::query_as("SELECT id,node_code,title,stage_code,stage_sort,node_sort,trigger_event,task_type,default_applicability,repeatable,time_nature,deadline_rule_codes_json FROM criminal_workflow_template_nodes WHERE template_version_id=? AND node_code=? AND enabled=1")
        .bind(template_id).bind(code).fetch_optional(&mut **tx).await.map_err(|e| e.to_string())
}

async fn link_missing_deadlines(
    tx: &mut Transaction<'_, Sqlite>,
    workflow: &CriminalWorkflow,
) -> Result<(), String> {
    let nodes: Vec<(String, String)> = sqlx::query_as(
        "SELECT id,deadline_rule_codes_json FROM criminal_workflow_template_nodes WHERE template_version_id=? AND deadline_rule_codes_json<>'[]'",
    )
    .bind(&workflow.template_version_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    for (node_id, rules_json) in nodes {
        if let Some(deadline_id) = find_deadline(tx, &workflow.case_id, &rules_json).await? {
            sqlx::query("UPDATE criminal_case_tasks SET deadline_item_id=?,updated_at=datetime('now') WHERE workflow_id=? AND template_node_id=? AND deadline_item_id IS NULL")
                .bind(deadline_id)
                .bind(&workflow.id)
                .bind(node_id)
                .execute(&mut **tx)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn next_occurrence_no(
    tx: &mut Transaction<'_, Sqlite>,
    workflow_id: &str,
    node_code: &str,
) -> Result<i64, String> {
    let n: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(occurrence_no),0)+1 FROM criminal_case_tasks WHERE workflow_id=? AND node_code=?")
        .bind(workflow_id).bind(node_code).fetch_one(&mut **tx).await.map_err(|e| e.to_string())?;
    Ok(n)
}

#[allow(clippy::too_many_arguments)]
async fn insert_event(
    tx: &mut Transaction<'_, Sqlite>,
    task_id: &str,
    case_id: &str,
    event_type: &str,
    actor: &str,
    event_id: Option<&str>,
    source_type: Option<&str>,
    source_ref_id: Option<&str>,
    from_status: Option<&str>,
    to_status: &str,
    reason: Option<&str>,
    payload: &str,
) -> Result<(), String> {
    sqlx::query("INSERT INTO criminal_task_events(id,task_id,case_id,event_type,actor,event_id,source_type,source_ref_id,from_status,to_status,reason,payload_json) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(Uuid::new_v4().to_string()).bind(task_id).bind(case_id).bind(event_type).bind(actor).bind(event_id).bind(source_type).bind(source_ref_id).bind(from_status).bind(to_status).bind(reason).bind(payload)
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    Ok(())
}

pub async fn get_workflow(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<Option<CriminalWorkflow>, String> {
    sqlx::query_as(&format!("{WORKFLOW_SELECT} WHERE case_id=?"))
        .bind(case_id)
        .fetch_optional(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_tasks(
    pool: &SqlitePool,
    filter: CriminalTaskFilter,
) -> Result<Vec<CriminalWorkflowTask>, String> {
    let statuses_json = filter
        .statuses
        .map(|v| serde_json::to_string(&v).unwrap_or_else(|_| "[]".into()));
    let sql = format!("{TASK_SELECT} WHERE (?1 IS NULL OR case_id=?1) AND (?2 IS NULL OR status IN (SELECT value FROM json_each(?2))) AND (?3 IS NULL OR planned_at>=?3) AND (?4 IS NULL OR planned_at<=?4) ORDER BY stage_sort,node_sort,occurrence_no");
    sqlx::query_as(&sql)
        .bind(filter.case_id)
        .bind(statuses_json)
        .bind(filter.planned_from)
        .bind(filter.planned_to)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())
}

pub async fn list_task_events(
    pool: &SqlitePool,
    task_id: &str,
) -> Result<Vec<CriminalTaskEvent>, String> {
    sqlx::query_as("SELECT id,task_id,case_id,event_type,actor,event_id,source_type,source_ref_id,from_status,to_status,reason,payload_json,created_at FROM criminal_task_events WHERE task_id=? ORDER BY created_at,id")
        .bind(task_id).fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn create_occurrence(
    pool: &SqlitePool,
    input: CreateCriminalTaskOccurrenceInput,
) -> Result<CriminalWorkflowTask, String> {
    require(&input.case_id, "case_id")?;
    require(&input.node_code, "node_code")?;
    require(&input.actor, "actor")?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let workflow: CriminalWorkflow = sqlx::query_as(&format!(
        "{WORKFLOW_SELECT} WHERE case_id=? AND status='active'"
    ))
    .bind(&input.case_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| "案件尚未初始化刑事SOP".to_string())?;
    let node = template_node(&mut tx, &workflow.template_version_id, &input.node_code)
        .await?
        .ok_or_else(|| "模板节点不存在".to_string())?;
    if !node.repeatable {
        return Err("该节点不允许新增重复事项".to_string());
    }
    let occurrence_no = next_occurrence_no(&mut tx, &workflow.id, &node.node_code).await?;
    let key = input
        .occurrence_key
        .unwrap_or_else(|| format!("manual:{}", Uuid::new_v4()));
    let event_id = format!("manual-occurrence:{key}");
    let inserted = insert_task(
        &mut tx,
        InsertTaskInput {
            workflow: &workflow,
            node: &node,
            occurrence_key: &key,
            occurrence_no,
            event_id: &event_id,
            source_type: "manual",
            source_ref_id: None,
            planned_at: input.planned_at.as_deref(),
        },
    )
    .await?;
    let task: CriminalWorkflowTask = sqlx::query_as(&format!(
        "{TASK_SELECT} WHERE workflow_id=? AND node_code=? AND occurrence_key=?"
    ))
    .bind(&workflow.id)
    .bind(&node.node_code)
    .bind(&key)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    if inserted {
        insert_event(
            &mut tx,
            &task.id,
            &task.case_id,
            "manual_occurrence",
            &input.actor,
            Some(&event_id),
            Some("manual"),
            None,
            None,
            &task.status,
            None,
            "{}",
        )
        .await?;
    }
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(task)
}

pub async fn apply_action(
    pool: &SqlitePool,
    input: CriminalTaskActionInput,
) -> Result<CriminalWorkflowTask, String> {
    require(&input.task_id, "task_id")?;
    require(&input.action, "action")?;
    require(&input.actor, "actor")?;
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let task: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
        .bind(&input.task_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|_| "任务不存在".to_string())?;
    let target = target_status(&task, &input)?;
    if matches!(input.action.as_str(), "not_applicable" | "defer" | "ignore")
        && input.reason.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err("该操作必须填写原因".to_string());
    }
    let old = task.status.clone();
    match input.action.as_str() {
        "schedule" => {
            sqlx::query("UPDATE criminal_case_tasks SET status=?,applicability_status='applicable',planned_at=?,original_planned_at=COALESCE(original_planned_at,?),updated_at=datetime('now') WHERE id=?").bind(target).bind(&input.planned_at).bind(&input.planned_at).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "start" => {
            sqlx::query("UPDATE criminal_case_tasks SET status=?,started_at=COALESCE(started_at,datetime('now')),updated_at=datetime('now') WHERE id=?").bind(target).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "confirm_applicable" => {
            sqlx::query("UPDATE criminal_case_tasks SET status=?,applicability_status='applicable',planned_at=COALESCE(?,planned_at),original_planned_at=COALESCE(original_planned_at,?),updated_at=datetime('now') WHERE id=?").bind(target).bind(&input.planned_at).bind(&input.planned_at).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "not_applicable" => {
            sqlx::query("UPDATE criminal_case_tasks SET status='not_applicable',applicability_status='not_applicable',disposition_reason=?,updated_at=datetime('now') WHERE id=?").bind(&input.reason).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "defer" => {
            sqlx::query("UPDATE criminal_case_tasks SET status='deferred',original_planned_at=COALESCE(original_planned_at,planned_at),planned_at=?,deferred_at=datetime('now'),disposition_reason=?,updated_at=datetime('now') WHERE id=?").bind(&input.planned_at).bind(&input.reason).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "ignore" => {
            sqlx::query("UPDATE criminal_case_tasks SET status='ignored',ignored_at=datetime('now'),disposition_reason=?,updated_at=datetime('now') WHERE id=?").bind(&input.reason).bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "reopen" => {
            sqlx::query("UPDATE criminal_case_tasks SET status='reopened',reopened_at=datetime('now'),completed_at=NULL,ignored_at=NULL,updated_at=datetime('now') WHERE id=?").bind(&task.id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
        }
        "complete" => complete_in_transaction(&mut tx, &task, &input).await?,
        _ => return Err("未知任务操作".to_string()),
    }
    insert_event(
        &mut tx,
        &task.id,
        &task.case_id,
        &input.action,
        &input.actor,
        None,
        Some("manual"),
        None,
        Some(&old),
        target,
        input.reason.as_deref(),
        "{}",
    )
    .await?;
    let updated: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
        .bind(&task.id)
        .fetch_one(&mut *tx)
        .await
        .map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(updated)
}

fn target_status(
    task: &CriminalWorkflowTask,
    input: &CriminalTaskActionInput,
) -> Result<&'static str, String> {
    match input.action.as_str() {
        "confirm_applicable" if task.status == "pending_confirmation" => {
            Ok(if input.planned_at.is_some() {
                "pending"
            } else {
                "unscheduled"
            })
        }
        "not_applicable" if task.status == "pending_confirmation" => Ok("not_applicable"),
        "schedule"
            if !matches!(
                task.status.as_str(),
                "completed" | "ignored" | "not_applicable" | "pending_confirmation"
            ) && input.planned_at.is_some() =>
        {
            Ok("pending")
        }
        "start"
            if matches!(
                task.status.as_str(),
                "pending" | "unscheduled" | "deferred" | "reopened"
            ) =>
        {
            Ok("in_progress")
        }
        "defer"
            if !matches!(
                task.status.as_str(),
                "completed" | "ignored" | "not_applicable" | "pending_confirmation"
            ) && input.planned_at.is_some() =>
        {
            Ok("deferred")
        }
        "complete"
            if matches!(
                task.status.as_str(),
                "pending" | "unscheduled" | "in_progress" | "deferred" | "reopened"
            ) =>
        {
            Ok("completed")
        }
        "ignore" if !matches!(task.status.as_str(), "completed" | "not_applicable") => {
            Ok("ignored")
        }
        "reopen" if matches!(task.status.as_str(), "completed" | "ignored") => Ok("reopened"),
        _ => Err(format!(
            "当前状态 {} 不允许操作 {} 或缺少必要参数",
            task.status, input.action
        )),
    }
}

async fn complete_in_transaction(
    tx: &mut Transaction<'_, Sqlite>,
    task: &CriminalWorkflowTask,
    input: &CriminalTaskActionInput,
) -> Result<(), String> {
    if input.result.as_deref().unwrap_or("").trim().is_empty() {
        return Err("完成任务必须填写办理结果".to_string());
    }
    let work_required: bool = sqlx::query_scalar(
        "SELECT work_record_required FROM criminal_workflow_template_nodes WHERE id=?",
    )
    .bind(&task.template_node_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    let mut work_id = task.work_item_id.clone();
    if work_required {
        let id = format!("criminal-sop-work:{}", task.id);
        sqlx::query("INSERT OR IGNORE INTO case_work_items(id,case_id,occurred_at,work_type,title,content,result,next_action,duration_minutes,source,external_source,external_record_id,confirmation_status) VALUES(?,?,datetime('now'),?,?,?,?,?,?,?,'criminal_sop',?,'confirmed')")
            .bind(&id).bind(&task.case_id).bind(&task.task_type).bind(&task.title).bind(input.result.as_deref().unwrap_or(""))
            .bind(&input.result).bind(&input.next_action).bind(input.duration_minutes).bind(SOP_SOURCE).bind(&task.id)
            .execute(&mut **tx).await.map_err(|e| e.to_string())?;
        work_id = Some(sqlx::query_scalar("SELECT id FROM case_work_items WHERE external_source=? AND external_record_id=? AND deleted_at IS NULL")
            .bind(SOP_SOURCE).bind(&task.id).fetch_one(&mut **tx).await.map_err(|e| e.to_string())?);
    }
    sqlx::query("UPDATE criminal_case_tasks SET status='completed',applicability_status='applicable',completed_at=datetime('now'),result=?,next_action=?,duration_minutes=?,client_feedback_recorded=?,work_item_id=?,updated_at=datetime('now') WHERE id=?")
        .bind(&input.result).bind(&input.next_action).bind(input.duration_minutes).bind(input.client_feedback_recorded.unwrap_or(false)).bind(work_id).bind(&task.id)
        .execute(&mut **tx).await.map_err(|e| e.to_string())?;
    let feedback_required: bool = sqlx::query_scalar(
        "SELECT client_feedback_required FROM criminal_workflow_template_nodes WHERE id=?",
    )
    .bind(&task.template_node_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(|e| e.to_string())?;
    if feedback_required && !input.client_feedback_recorded.unwrap_or(false) {
        let workflow: CriminalWorkflow = sqlx::query_as(&format!("{WORKFLOW_SELECT} WHERE id=?"))
            .bind(&task.workflow_id)
            .fetch_one(&mut **tx)
            .await
            .map_err(|e| e.to_string())?;
        if let Some(node) =
            template_node(tx, &workflow.template_version_id, "common_client_feedback").await?
        {
            let key = format!("task-feedback:{}", task.id);
            let no = next_occurrence_no(tx, &workflow.id, &node.node_code).await?;
            let event_id = format!("important-task-completed:{}", task.id);
            if insert_task(
                tx,
                InsertTaskInput {
                    workflow: &workflow,
                    node: &node,
                    occurrence_key: &key,
                    occurrence_no: no,
                    event_id: &event_id,
                    source_type: "system",
                    source_ref_id: Some(&task.id),
                    planned_at: None,
                },
            )
            .await?
            {
                let feedback_task_id: String = sqlx::query_scalar("SELECT id FROM criminal_case_tasks WHERE workflow_id=? AND node_code=? AND occurrence_key=?")
                    .bind(&workflow.id).bind(&node.node_code).bind(&key).fetch_one(&mut **tx).await.map_err(|e| e.to_string())?;
                insert_event(
                    tx,
                    &feedback_task_id,
                    &task.case_id,
                    "generated",
                    &input.actor,
                    Some(&format!("important-task-completed:{}", task.id)),
                    Some("system"),
                    Some(&task.id),
                    None,
                    initial_status(&node.default_applicability),
                    None,
                    "{}",
                )
                .await?;
            }
        }
    }
    Ok(())
}

pub async fn list_summary(pool: &SqlitePool) -> Result<Vec<CriminalTaskSummaryRow>, String> {
    sqlx::query_as("SELECT t.case_id,c.name AS case_name,t.id AS task_id,t.title,t.stage_code,t.task_type,t.status,t.applicability_status,t.planned_at,n.client_feedback_required FROM criminal_case_tasks t JOIN cases c ON c.id=t.case_id JOIN criminal_workflow_template_nodes n ON n.id=t.template_node_id WHERE t.status NOT IN ('completed','not_applicable') ORDER BY CASE WHEN t.planned_at IS NULL THEN 1 ELSE 0 END,t.planned_at,t.stage_sort,t.node_sort")
        .fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn list_calendar(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<Vec<CriminalTaskSummaryRow>, String> {
    sqlx::query_as("SELECT t.case_id,c.name AS case_name,t.id AS task_id,t.title,t.stage_code,t.task_type,t.status,t.applicability_status,t.planned_at,n.client_feedback_required FROM criminal_case_tasks t JOIN cases c ON c.id=t.case_id JOIN criminal_workflow_template_nodes n ON n.id=t.template_node_id WHERE t.planned_at>=? AND t.planned_at<=? AND t.status NOT IN ('completed','ignored','not_applicable') ORDER BY t.planned_at,t.stage_sort,t.node_sort")
        .bind(from).bind(to).fetch_all(pool).await.map_err(|e| e.to_string())
}

pub async fn list_deadline_calendar(
    pool: &SqlitePool,
    from: &str,
    to: &str,
) -> Result<Vec<CriminalDeadlineCalendarRow>, String> {
    require(from, "from")?;
    require(to, "to")?;
    if from > to {
        return Err("from 不得晚于 to".to_string());
    }
    sqlx::query_as(
        "SELECT d.id AS deadline_id,d.case_id,c.name AS case_name,d.title,d.rule_code,d.effective_due_at AS deadline_at,d.status,d.applicability_status \
         FROM criminal_deadline_items d \
         JOIN cases c ON c.id=d.case_id \
         WHERE d.deleted_at IS NULL \
           AND d.effective_due_at IS NOT NULL \
           AND d.effective_due_at>=? \
           AND d.effective_due_at<=? \
           AND d.applicability_status='confirmed' \
           AND d.status NOT IN ('done','completed','ignored') \
         ORDER BY d.effective_due_at,c.name,d.id",
    )
    .bind(from)
    .bind(to)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())
}

pub async fn scan_reminder_candidates(pool: &SqlitePool, now: &str) -> Result<i64, String> {
    let result = sqlx::query("INSERT OR IGNORE INTO criminal_reminder_deliveries(id,task_id,case_id,reminder_key,scheduled_for) SELECT lower(hex(randomblob(16))),id,case_id,'due:'||planned_at,planned_at FROM criminal_case_tasks WHERE planned_at IS NOT NULL AND planned_at<=? AND status IN ('pending','in_progress','deferred','reopened')")
        .bind(now).execute(pool).await.map_err(|e| e.to_string())?;
    Ok(result.rows_affected() as i64)
}

pub async fn claim_reminders(
    pool: &SqlitePool,
    input: ClaimCriminalRemindersInput,
) -> Result<Vec<CriminalReminderDelivery>, String> {
    require(&input.now, "now")?;
    let channel = input.channel.unwrap_or_else(|| "windows".to_string());
    let limit = input.limit.unwrap_or(20).clamp(1, 100);
    let mut tx = pool.begin().await.map_err(|e| e.to_string())?;
    let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM criminal_reminder_deliveries WHERE channel=? AND scheduled_for<=? AND status IN ('candidate','failed') ORDER BY scheduled_for,id LIMIT ?")
        .bind(&channel).bind(&input.now).bind(limit).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    for id in &ids {
        sqlx::query("UPDATE criminal_reminder_deliveries SET status='claimed',claimed_at=datetime('now'),attempt_count=attempt_count+1,error_message=NULL,updated_at=datetime('now') WHERE id=? AND status IN ('candidate','failed')")
            .bind(id).execute(&mut *tx).await.map_err(|e| e.to_string())?;
    }
    let ids_json = serde_json::to_string(&ids).map_err(|e| e.to_string())?;
    let rows = sqlx::query_as("SELECT id,task_id,case_id,reminder_key,channel,scheduled_for,status,claimed_at,sent_at,failed_at,error_message,attempt_count,created_at,updated_at FROM criminal_reminder_deliveries WHERE id IN (SELECT value FROM json_each(?)) ORDER BY scheduled_for,id")
        .bind(ids_json).fetch_all(&mut *tx).await.map_err(|e| e.to_string())?;
    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(rows)
}

pub async fn mark_reminder(
    pool: &SqlitePool,
    input: MarkCriminalReminderInput,
) -> Result<CriminalReminderDelivery, String> {
    require(&input.delivery_id, "delivery_id")?;
    let (status, stamp) = if input.sent {
        ("sent", "sent_at")
    } else {
        ("failed", "failed_at")
    };
    let sql = format!("UPDATE criminal_reminder_deliveries SET status=?,{stamp}=datetime('now'),error_message=?,updated_at=datetime('now') WHERE id=? AND status='claimed'");
    let result = sqlx::query(&sql)
        .bind(status)
        .bind(if input.sent {
            None
        } else {
            input.error_message
        })
        .bind(&input.delivery_id)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?;
    if result.rows_affected() != 1 {
        return Err("提醒不存在或当前未被认领".to_string());
    }
    sqlx::query_as("SELECT id,task_id,case_id,reminder_key,channel,scheduled_for,status,claimed_at,sent_at,failed_at,error_message,attempt_count,created_at,updated_at FROM criminal_reminder_deliveries WHERE id=?")
        .bind(&input.delivery_id).fetch_one(pool).await.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    async fn setup() -> SqlitePool {
        let pool = db::init_pool(":memory:").await.unwrap();
        sqlx::query("INSERT INTO cases(id,name,source_folder) VALUES('case-sop','测试刑事案件','D:/synthetic/sop')").execute(&pool).await.unwrap();
        pool
    }

    fn event(code: &str, id: &str) -> RefreshCriminalWorkflowInput {
        RefreshCriminalWorkflowInput {
            case_id: "case-sop".into(),
            event_code: code.into(),
            event_id: id.into(),
            source_type: "manual_confirmed".into(),
            source_ref_id: Some(id.into()),
            confirmed_by: "tester".into(),
        }
    }

    #[tokio::test]
    async fn template_has_35_unique_nodes() {
        let pool = setup().await;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM criminal_workflow_template_nodes WHERE template_version_id='criminal_defense_standard_v1:1'").fetch_one(&pool).await.unwrap();
        let unique:i64=sqlx::query_scalar("SELECT COUNT(DISTINCT node_code) FROM criminal_workflow_template_nodes WHERE template_version_id='criminal_defense_standard_v1:1'").fetch_one(&pool).await.unwrap();
        assert_eq!(count, 35);
        assert_eq!(unique, 35);
    }

    #[tokio::test]
    async fn unconfirmed_sources_and_unreferenced_accepted_candidate_write_nothing() {
        let pool = setup().await;
        for source_type in ["extraction_candidate", "ocr", "model_inference"] {
            let mut input = event("detention_confirmed", &format!("bad-source-{source_type}"));
            input.source_type = source_type.to_string();
            assert!(
                refresh(&pool, input).await.is_err(),
                "{source_type} 必须被拒绝"
            );
        }
        let mut missing_ref = event("detention_confirmed", "accepted-without-ref");
        missing_ref.source_type = SOURCE_ACCEPTED_EXTRACTION_CANDIDATE.to_string();
        missing_ref.source_ref_id = None;
        assert!(refresh(&pool, missing_ref).await.is_err());
        let mut workflow_missing_ref = event("detention_confirmed", "workflow-without-ref");
        workflow_missing_ref.source_type = SOURCE_WORKFLOW_CONFIRMED.to_string();
        workflow_missing_ref.source_ref_id = Some("   ".to_string());
        assert!(refresh(&pool, workflow_missing_ref).await.is_err());

        let workflows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_workflows")
            .fetch_one(&pool)
            .await
            .unwrap();
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(workflows, 0);
        assert_eq!(tasks, 0);
    }

    #[tokio::test]
    async fn illegal_event_is_rejected_before_any_workflow_write() {
        let pool = setup().await;
        let result = refresh(&pool, event("model_guessed_detention", "illegal-event")).await;
        assert!(result.is_err());
        let workflows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_workflows")
            .fetch_one(&pool)
            .await
            .unwrap();
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(workflows, 0);
        assert_eq!(tasks, 0);
    }

    #[tokio::test]
    async fn accepted_extraction_candidate_requires_and_keeps_source_reference() {
        let pool = setup().await;
        let mut input = event("case_created", "accepted-candidate-event");
        input.source_type = SOURCE_ACCEPTED_EXTRACTION_CANDIDATE.to_string();
        input.source_ref_id = Some("candidate-batch-1".to_string());
        let result = refresh(&pool, input).await.unwrap();
        assert_eq!(result.generated_count, 1);
        assert_eq!(
            result.tasks[0].trigger_source_type,
            SOURCE_ACCEPTED_EXTRACTION_CANDIDATE
        );
        assert_eq!(
            result.tasks[0].trigger_source_ref_id.as_deref(),
            Some("candidate-batch-1")
        );
    }

    #[tokio::test]
    async fn refresh_is_idempotent_and_conditions_wait_for_confirmation() {
        let pool = setup().await;
        let first = refresh(&pool, event("detention_confirmed", "detention-1"))
            .await
            .unwrap();
        let again = refresh(&pool, event("detention_confirmed", "detention-1"))
            .await
            .unwrap();
        assert_eq!(first.generated_count, 4); // 三个阶段节点 + 程序变化反馈
        assert_eq!(again.generated_count, 0);
        let tasks = list_tasks(
            &pool,
            CriminalTaskFilter {
                case_id: Some("case-sop".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert!(tasks.iter().any(
            |t| t.node_code == "detention_investigation" && t.status == "pending_confirmation"
        ));
        let conditional = tasks
            .iter()
            .find(|t| t.node_code == "detention_investigation")
            .unwrap();
        assert!(apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: conditional.id.clone(),
                action: "schedule".into(),
                actor: "tester".into(),
                planned_at: Some("2026-01-03T09:00:00Z".into()),
                ..Default::default()
            }
        )
        .await
        .is_err());
        let confirmed = apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: conditional.id.clone(),
                action: "confirm_applicable".into(),
                actor: "tester".into(),
                planned_at: Some("2026-01-03T09:00:00Z".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(confirmed.status, "pending");
        assert_eq!(
            confirmed.planned_at.as_deref(),
            Some("2026-01-03T09:00:00Z")
        );
        assert_eq!(
            confirmed.original_planned_at.as_deref(),
            Some("2026-01-03T09:00:00Z")
        );
        let generated_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM criminal_task_events WHERE event_type='generated'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(generated_events, 4);
    }

    #[tokio::test]
    async fn refresh_preserves_manual_schedule_defer_and_ignore_state() {
        let pool = setup().await;
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        let task = list_tasks(
            &pool,
            CriminalTaskFilter {
                case_id: Some("case-sop".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .remove(0);
        let scheduled = apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id.clone(),
                action: "schedule".into(),
                actor: "tester".into(),
                planned_at: Some("2026-02-01T09:00:00Z".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(scheduled.status, "pending");
        apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id.clone(),
                action: "start".into(),
                actor: "tester".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id.clone(),
                action: "defer".into(),
                actor: "tester".into(),
                planned_at: Some("2026-02-05T09:00:00Z".into()),
                reason: Some("等待调取材料".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        let deferred: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
            .bind(&task.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(deferred.status, "deferred");
        assert_eq!(deferred.planned_at.as_deref(), Some("2026-02-05T09:00:00Z"));
        assert_eq!(
            deferred.original_planned_at.as_deref(),
            Some("2026-02-01T09:00:00Z")
        );
        assert_eq!(deferred.disposition_reason.as_deref(), Some("等待调取材料"));
        apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id.clone(),
                action: "ignore".into(),
                actor: "tester".into(),
                reason: Some("委托人明确暂不办理".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        let ignored: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
            .bind(&task.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(ignored.status, "ignored");
        assert_eq!(
            ignored.disposition_reason.as_deref(),
            Some("委托人明确暂不办理")
        );
    }

    #[tokio::test]
    async fn repeatable_meeting_and_complete_create_one_work_record() {
        let pool = setup().await;
        refresh(&pool, event("detention_confirmed", "detention-1"))
            .await
            .unwrap();
        let extra = create_occurrence(
            &pool,
            CreateCriminalTaskOccurrenceInput {
                case_id: "case-sop".into(),
                node_code: "detention_first_meeting".into(),
                actor: "tester".into(),
                occurrence_key: Some("meeting-2".into()),
                planned_at: None,
            },
        )
        .await
        .unwrap();
        let completed = apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: extra.id.clone(),
                action: "complete".into(),
                actor: "tester".into(),
                result: Some("已完成第二次会见".into()),
                client_feedback_recorded: Some(true),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(completed.status, "completed");
        assert!(completed.work_item_id.is_some());
        let work_count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM case_work_items WHERE external_source='criminal_sop' AND external_record_id=?").bind(&extra.id).fetch_one(&pool).await.unwrap();
        assert_eq!(work_count, 1);
    }

    #[tokio::test]
    async fn completion_rolls_back_when_work_record_insert_fails() {
        let pool = setup().await;
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        let task = list_tasks(
            &pool,
            CriminalTaskFilter {
                case_id: Some("case-sop".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .remove(0);
        sqlx::query("CREATE TRIGGER fail_criminal_sop_work BEFORE INSERT ON case_work_items WHEN NEW.external_source='criminal_sop' BEGIN SELECT RAISE(ABORT,'forced work item failure'); END")
            .execute(&pool).await.unwrap();
        let result = apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id.clone(),
                action: "complete".into(),
                actor: "tester".into(),
                result: Some("本次应回滚".into()),
                ..Default::default()
            },
        )
        .await;
        assert!(result.is_err());
        let stored: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
            .bind(&task.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(stored.status, "unscheduled");
        assert!(stored.completed_at.is_none());
        assert!(stored.work_item_id.is_none());
        let work_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM case_work_items WHERE external_record_id=?")
                .bind(&task.id)
                .fetch_one(&pool)
                .await
                .unwrap();
        let complete_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM criminal_task_events WHERE task_id=? AND event_type='complete'",
        )
        .bind(&task.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(work_count, 0);
        assert_eq!(complete_events, 0);
    }

    #[tokio::test]
    async fn refresh_links_but_never_mutates_statutory_deadline() {
        let pool = setup().await;
        refresh(&pool, event("detention_confirmed", "detention-1"))
            .await
            .unwrap();
        let before = list_tasks(
            &pool,
            CriminalTaskFilter {
                case_id: Some("case-sop".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let meeting = before
            .iter()
            .find(|t| t.node_code == "detention_first_meeting")
            .unwrap();
        assert!(meeting.deadline_item_id.is_none());
        sqlx::query("INSERT INTO criminal_deadline_items(id,case_id,rule_code,title,manual_due_at,effective_due_at,status,source_type) VALUES('deadline-1','case-sop','CRIM_DETENTION_INTERROGATION_24H','24小时讯问期限','2026-03-02T10:00:00Z','2026-03-02T10:00:00Z','overridden','manual')")
            .execute(&pool).await.unwrap();
        refresh(&pool, event("detention_confirmed", "detention-1"))
            .await
            .unwrap();
        let linked: CriminalWorkflowTask = sqlx::query_as(&format!("{TASK_SELECT} WHERE id=?"))
            .bind(&meeting.id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(linked.deadline_item_id.as_deref(), Some("deadline-1"));
        apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: meeting.id.clone(),
                action: "schedule".into(),
                actor: "tester".into(),
                planned_at: Some("2026-03-01T09:00:00Z".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        let deadline:(Option<String>,Option<String>,String,String)=sqlx::query_as("SELECT manual_due_at,effective_due_at,status,source_type FROM criminal_deadline_items WHERE id='deadline-1'").fetch_one(&pool).await.unwrap();
        assert_eq!(deadline.0.as_deref(), Some("2026-03-02T10:00:00Z"));
        assert_eq!(deadline.1.as_deref(), Some("2026-03-02T10:00:00Z"));
        assert_eq!(deadline.2, "overridden");
        assert_eq!(deadline.3, "manual");
    }

    #[tokio::test]
    async fn deadline_calendar_filters_range_status_and_orders_each_deadline_once() {
        let pool = setup().await;
        sqlx::query("INSERT INTO cases(id,name,source_folder) VALUES('case-calendar-b','日历测试乙案','D:/synthetic/calendar-b')")
            .execute(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO criminal_deadline_items(id,case_id,rule_code,title,effective_due_at,status,applicability_status,deleted_at) VALUES
             ('deadline-before','case-sop','BEFORE','范围前','2026-07-31T23:59:59Z','pending','confirmed',NULL),
             ('deadline-early','case-calendar-b','EARLY','起始边界','2026-08-01T00:00:00Z','pending','confirmed',NULL),
             ('deadline-overridden','case-sop','OVERRIDDEN','人工覆盖后期限','2026-08-02T12:00:00Z','overridden','confirmed',NULL),
             ('deadline-late','case-sop','LATE','结束边界','2026-08-03T23:59:59Z','pending','confirmed',NULL),
             ('deadline-after','case-sop','AFTER','范围后','2026-08-04T00:00:00Z','pending','confirmed',NULL),
             ('deadline-ignored','case-sop','IGNORED','已忽略','2026-08-02T08:00:00Z','ignored','confirmed',NULL),
             ('deadline-done','case-sop','DONE','已完成','2026-08-02T09:00:00Z','done','confirmed',NULL),
             ('deadline-completed','case-sop','COMPLETED','已完成兼容状态','2026-08-02T10:00:00Z','completed','confirmed',NULL),
             ('deadline-needs-confirmation','case-sop','CONDITIONAL','待确认','2026-08-02T11:00:00Z','pending','needs_confirmation',NULL),
             ('deadline-not-applicable','case-sop','NOT_APPLICABLE','不适用','2026-08-02T13:00:00Z','pending','not_applicable',NULL),
             ('deadline-deleted','case-sop','DELETED','已删除','2026-08-02T14:00:00Z','pending','confirmed',datetime('now'))",
        ).execute(&pool).await.unwrap();

        let rows = list_deadline_calendar(&pool, "2026-08-01T00:00:00Z", "2026-08-03T23:59:59Z")
            .await
            .unwrap();
        let ids: Vec<&str> = rows.iter().map(|row| row.deadline_id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["deadline-early", "deadline-overridden", "deadline-late"]
        );
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), rows.len());
        assert_eq!(rows[0].case_name, "日历测试乙案");
        assert_eq!(rows[1].deadline_at, "2026-08-02T12:00:00Z");
        assert_eq!(rows[1].status, "overridden");
        assert_eq!(rows[1].applicability_status, "confirmed");
    }

    #[tokio::test]
    async fn deadline_calendar_is_read_only_and_rejects_reversed_range() {
        let pool = setup().await;
        sqlx::query("INSERT INTO criminal_deadline_items(id,case_id,rule_code,title,effective_due_at,status,applicability_status) VALUES('deadline-read-only','case-sop','READ_ONLY','只读期限','2026-08-02T12:00:00Z','pending','confirmed')")
            .execute(&pool).await.unwrap();
        for trigger in [
            "CREATE TRIGGER deny_deadline_insert BEFORE INSERT ON criminal_deadline_items BEGIN SELECT RAISE(ABORT,'calendar must not insert'); END",
            "CREATE TRIGGER deny_deadline_update BEFORE UPDATE ON criminal_deadline_items BEGIN SELECT RAISE(ABORT,'calendar must not update'); END",
            "CREATE TRIGGER deny_deadline_delete BEFORE DELETE ON criminal_deadline_items BEGIN SELECT RAISE(ABORT,'calendar must not delete'); END",
        ] {
            sqlx::query(trigger).execute(&pool).await.unwrap();
        }

        let rows = list_deadline_calendar(&pool, "2026-08-01T00:00:00Z", "2026-08-03T23:59:59Z")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].deadline_id, "deadline-read-only");
        assert!(list_deadline_calendar(&pool, "2026-08-04", "2026-08-01")
            .await
            .is_err());
        let workflows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_workflows")
            .fetch_one(&pool)
            .await
            .unwrap();
        let tasks: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM criminal_case_tasks")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(workflows, 0);
        assert_eq!(tasks, 0);
    }

    #[tokio::test]
    async fn template_upgrade_keeps_existing_case_on_v1_snapshot() {
        let pool = setup().await;
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        sqlx::query("INSERT INTO criminal_workflow_template_versions(id,template_code,version,name,scope_note,status,published_at) VALUES('criminal_defense_standard_v1:2','criminal_defense_standard_v1',2,'测试 V2','仅用于升级隔离测试','published',datetime('now'))").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO criminal_workflow_template_nodes(id,template_version_id,node_code,title,stage_code,stage_sort,node_sort,trigger_event,prerequisite_codes_json,task_type,default_applicability,repeatable,client_feedback_required,time_nature,deadline_rule_codes_json,work_record_required,guidance_json,enabled) SELECT 'sop2:intake_consultation','criminal_defense_standard_v1:2',node_code,title,stage_code,stage_sort,node_sort,trigger_event,prerequisite_codes_json,task_type,default_applicability,repeatable,client_feedback_required,time_nature,deadline_rule_codes_json,work_record_required,guidance_json,enabled FROM criminal_workflow_template_nodes WHERE id='sop1:intake_consultation'").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO cases(id,name,source_folder) VALUES('case-sop-v2','测试刑事案件V2','D:/synthetic/sop-v2')").execute(&pool).await.unwrap();
        let old = refresh(
            &pool,
            event("detention_confirmed", "detention-after-upgrade"),
        )
        .await
        .unwrap();
        let new = refresh(
            &pool,
            RefreshCriminalWorkflowInput {
                case_id: "case-sop-v2".into(),
                event_code: "case_created".into(),
                event_id: "created-v2".into(),
                source_type: "manual_confirmed".into(),
                source_ref_id: Some("created-v2".into()),
                confirmed_by: "tester".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            old.workflow.template_version_id,
            "criminal_defense_standard_v1:1"
        );
        assert_eq!(
            new.workflow.template_version_id,
            "criminal_defense_standard_v1:2"
        );
        assert_eq!(new.generated_count, 1);
    }

    #[tokio::test]
    async fn reminder_claim_is_unique_and_case_delete_cascades() {
        let pool = setup().await;
        refresh(&pool, event("case_created", "created-1"))
            .await
            .unwrap();
        let task = list_tasks(
            &pool,
            CriminalTaskFilter {
                case_id: Some("case-sop".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap()
        .remove(0);
        apply_action(
            &pool,
            CriminalTaskActionInput {
                task_id: task.id,
                action: "schedule".into(),
                actor: "tester".into(),
                planned_at: Some("2026-01-01T00:00:00Z".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            scan_reminder_candidates(&pool, "2026-01-02T00:00:00Z")
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            scan_reminder_candidates(&pool, "2026-01-02T00:00:00Z")
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            claim_reminders(
                &pool,
                ClaimCriminalRemindersInput {
                    now: "2026-01-02T00:00:00Z".into(),
                    ..Default::default()
                }
            )
            .await
            .unwrap()
            .len(),
            1
        );
        sqlx::query("DELETE FROM cases WHERE id='case-sop'")
            .execute(&pool)
            .await
            .unwrap();
        for table in [
            "criminal_case_workflows",
            "criminal_case_tasks",
            "criminal_task_events",
            "criminal_reminder_deliveries",
        ] {
            let left: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(left, 0, "{table} 应随案件删除级联清理");
        }
    }
}
