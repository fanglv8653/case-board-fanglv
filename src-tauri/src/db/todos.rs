//! v0.8.4 全局待办事项模型。`done/done_at/due_date` 仅保留为 0.8.3 兼容投影。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

const ACTIVE_STATUSES: &[&str] = &["inbox", "in_progress", "waiting"];

#[derive(Debug, Clone, Serialize)]
pub struct TodoError {
    pub code: String,
    pub message: String,
}

impl TodoError {
    fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }

    fn db(error: sqlx::Error) -> Self {
        Self::new("TODO_DB_ERROR", error.to_string())
    }
}

impl std::fmt::Display for TodoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for TodoError {}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Todo {
    pub id: String,
    pub case_id: Option<String>,
    pub title: String,
    pub content: String,
    pub kind: String,
    pub priority: String,
    pub tags_json: String,
    pub next_action: Option<String>,
    pub status: String,
    pub done: i64,
    pub done_at: Option<String>,
    pub due_at: Option<String>,
    pub remind_at: Option<String>,
    pub due_date: Option<String>,
    pub source: String,
    pub source_message_id: Option<String>,
    pub source_at: Option<String>,
    pub delete_requested_at: Option<String>,
    pub delete_reason: Option<String>,
    pub deleted_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewTodo {
    #[serde(default)]
    pub case_id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    #[serde(default)]
    pub next_action: Option<String>,
    #[serde(default)]
    pub due_at: Option<String>,
    #[serde(default)]
    pub remind_at: Option<String>,
    #[serde(default)]
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdateTodo {
    pub title: Option<String>,
    pub content: Option<String>,
    pub kind: Option<String>,
    pub priority: Option<String>,
    pub tags: Option<Vec<String>>,
    pub next_action: Option<String>,
    pub status: Option<String>,
    pub done: Option<i64>,
    pub due_at: Option<String>,
    pub remind_at: Option<String>,
    pub due_date: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TodoFilter {
    pub state: Option<String>,
    pub case_id: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct OpenTodoRow {
    pub id: String,
    pub case_id: Option<String>,
    pub case_name: Option<String>,
    pub title: String,
    pub due_date: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CopyTodoResult {
    pub work_item_id: String,
    pub case_id: String,
    pub created: bool,
    pub outcome_code: String,
}

fn trimmed_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        (!value.is_empty()).then_some(value)
    })
}

fn validate_choice(value: &str, allowed: &[&str], code: &str) -> Result<(), TodoError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(TodoError::new(code, "字段值不在允许集合中"))
    }
}

fn normalize_tags(tags: Option<Vec<String>>) -> Result<String, TodoError> {
    let mut normalized = tags
        .unwrap_or_default()
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    serde_json::to_string(&normalized)
        .map_err(|error| TodoError::new("TODO_INVALID_TAGS", error.to_string()))
}

fn normalize_date(value: Option<String>) -> Result<Option<String>, TodoError> {
    let value = trimmed_optional(value);
    if let Some(value) = value {
        chrono::NaiveDate::parse_from_str(&value, "%Y-%m-%d")
            .map_err(|_| TodoError::new("TODO_INVALID_DATE", "日期必须使用 YYYY-MM-DD"))?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

fn normalize_timestamp(
    value: Option<String>,
    code: &'static str,
) -> Result<Option<String>, TodoError> {
    let value = trimmed_optional(value);
    if let Some(value) = value {
        let parsed = chrono::DateTime::parse_from_rfc3339(&value)
            .map_err(|_| TodoError::new(code, "时间必须包含明确时区"))?;
        Ok(Some(parsed.to_rfc3339()))
    } else {
        Ok(None)
    }
}

fn compatible_due_at(
    due_at: Option<String>,
    due_date: Option<String>,
) -> Result<Option<String>, TodoError> {
    let due_at = normalize_timestamp(due_at, "TODO_INVALID_DUE_AT")?;
    if due_at.is_some() {
        Ok(due_at)
    } else {
        Ok(normalize_date(due_date)?.map(|date| format!("{date}T00:00:00+08:00")))
    }
}

fn projected_due_date(due_at: Option<&str>, legacy: Option<String>) -> Option<String> {
    due_at
        .and_then(|value| {
            chrono::DateTime::parse_from_rfc3339(value)
                .ok()
                .and_then(|date_time| {
                    chrono::FixedOffset::east_opt(8 * 60 * 60).map(|offset| {
                        date_time
                            .with_timezone(&offset)
                            .format("%Y-%m-%d")
                            .to_string()
                    })
                })
                .or_else(|| value.get(0..10).map(ToOwned::to_owned))
        })
        .or_else(|| trimmed_optional(legacy))
}

async fn ensure_case_exists(
    transaction: &mut Transaction<'_, Sqlite>,
    case_id: &str,
) -> Result<(), TodoError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT EXISTS(SELECT 1 FROM cases WHERE id = ?)")
        .bind(case_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(TodoError::db)?;
    if exists == 1 {
        Ok(())
    } else {
        Err(TodoError::new("TODO_CASE_NOT_FOUND", "案件不存在"))
    }
}

pub async fn add(pool: &SqlitePool, input: NewTodo) -> Result<Todo, TodoError> {
    let title = input.title.trim().to_string();
    if title.is_empty() {
        return Err(TodoError::new("TODO_TITLE_REQUIRED", "标题不能为空"));
    }
    let kind = input.kind.unwrap_or_else(|| "todo".into());
    let priority = input.priority.unwrap_or_else(|| "unjudged".into());
    validate_choice(
        &kind,
        &["idea", "todo", "reminder", "reference", "memo"],
        "TODO_INVALID_KIND",
    )?;
    validate_choice(
        &priority,
        &["high", "medium", "low", "unjudged"],
        "TODO_INVALID_PRIORITY",
    )?;
    let tags_json = normalize_tags(input.tags)?;
    let due_at = compatible_due_at(input.due_at, input.due_date.clone())?;
    let due_date = projected_due_date(due_at.as_deref(), input.due_date);
    let remind_at = normalize_timestamp(input.remind_at, "TODO_INVALID_REMIND_AT")?;
    let case_id = trimmed_optional(input.case_id);
    let mut transaction = pool.begin().await.map_err(TodoError::db)?;
    if let Some(case_id) = &case_id {
        ensure_case_exists(&mut transaction, case_id).await?;
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO case_todos (
            id, case_id, title, content, kind, priority, tags_json, next_action,
            status, done, due_at, remind_at, due_date, source
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'inbox', 0, ?, ?, ?, 'caseboard')",
    )
    .bind(&id)
    .bind(case_id)
    .bind(title)
    .bind(input.content.unwrap_or_default())
    .bind(kind)
    .bind(priority)
    .bind(tags_json)
    .bind(trimmed_optional(input.next_action))
    .bind(due_at)
    .bind(remind_at)
    .bind(due_date)
    .execute(&mut *transaction)
    .await
    .map_err(TodoError::db)?;
    transaction.commit().await.map_err(TodoError::db)?;
    get(pool, &id, true)
        .await?
        .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "写入后未找到事项"))
}

async fn get(
    pool: &SqlitePool,
    id: &str,
    include_deleted: bool,
) -> Result<Option<Todo>, TodoError> {
    sqlx::query_as::<_, Todo>("SELECT * FROM case_todos WHERE id = ? AND (? OR deleted_at IS NULL)")
        .bind(id)
        .bind(include_deleted)
        .fetch_optional(pool)
        .await
        .map_err(TodoError::db)
}

pub async fn list_by_case(pool: &SqlitePool, case_id: &str) -> Result<Vec<Todo>, TodoError> {
    sqlx::query_as::<_, Todo>(
        "SELECT * FROM case_todos
         WHERE case_id = ? AND deleted_at IS NULL
         ORDER BY done ASC, due_at IS NULL, due_at ASC, updated_at DESC",
    )
    .bind(case_id)
    .fetch_all(pool)
    .await
    .map_err(TodoError::db)
}

pub async fn list_global(pool: &SqlitePool, filter: TodoFilter) -> Result<Vec<Todo>, TodoError> {
    let state = filter.state.as_deref().unwrap_or("open");
    validate_choice(
        state,
        &["open", "completed", "deleted", "all"],
        "TODO_INVALID_FILTER",
    )?;
    let query = trimmed_optional(filter.query).map(|value| format!("%{value}%"));
    sqlx::query_as::<_, Todo>(
        "SELECT * FROM case_todos
         WHERE (? = 'all'
             OR (? = 'open' AND deleted_at IS NULL AND status IN ('inbox','in_progress','waiting','delete_pending'))
             OR (? = 'completed' AND deleted_at IS NULL AND status = 'completed')
             OR (? = 'deleted' AND deleted_at IS NOT NULL))
           AND (? IS NULL OR case_id = ?)
           AND (? IS NULL OR title LIKE ? OR content LIKE ? OR COALESCE(next_action, '') LIKE ?)
         ORDER BY
           CASE status WHEN 'inbox' THEN 0 WHEN 'in_progress' THEN 1 WHEN 'waiting' THEN 2 WHEN 'delete_pending' THEN 3 WHEN 'completed' THEN 4 ELSE 5 END,
           due_at IS NULL, due_at ASC, updated_at DESC",
    )
    .bind(state)
    .bind(state)
    .bind(state)
    .bind(state)
    .bind(&filter.case_id)
    .bind(&filter.case_id)
    .bind(&query)
    .bind(&query)
    .bind(&query)
    .bind(&query)
    .fetch_all(pool)
    .await
    .map_err(TodoError::db)
}

pub async fn list_open(pool: &SqlitePool) -> Result<Vec<OpenTodoRow>, TodoError> {
    sqlx::query_as::<_, OpenTodoRow>(
        "SELECT t.id, t.case_id, c.name AS case_name, t.title, t.due_date, t.created_at
         FROM case_todos t LEFT JOIN cases c ON t.case_id = c.id
         WHERE t.deleted_at IS NULL
           AND t.kind IN ('todo','reminder')
           AND t.status IN ('inbox','in_progress','waiting')
         ORDER BY c.name IS NULL, c.name ASC, t.due_at IS NULL, t.due_at ASC, t.updated_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(TodoError::db)
}

pub async fn update(pool: &SqlitePool, id: &str, update: &UpdateTodo) -> Result<Todo, TodoError> {
    let existing = get(pool, id, false)
        .await?
        .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "事项不存在或已删除"))?;
    let title = update
        .title
        .as_deref()
        .map(str::trim)
        .unwrap_or(&existing.title)
        .to_string();
    if title.is_empty() {
        return Err(TodoError::new("TODO_TITLE_REQUIRED", "标题不能为空"));
    }
    let kind = update.kind.clone().unwrap_or(existing.kind);
    let priority = update.priority.clone().unwrap_or(existing.priority);
    validate_choice(
        &kind,
        &["idea", "todo", "reminder", "reference", "memo"],
        "TODO_INVALID_KIND",
    )?;
    validate_choice(
        &priority,
        &["high", "medium", "low", "unjudged"],
        "TODO_INVALID_PRIORITY",
    )?;
    let mut status = update.status.clone().unwrap_or(existing.status);
    validate_choice(
        &status,
        &[
            "inbox",
            "in_progress",
            "waiting",
            "completed",
            "delete_pending",
        ],
        "TODO_INVALID_STATUS",
    )?;
    if let Some(done) = update.done {
        if ![0, 1].contains(&done) {
            return Err(TodoError::new("TODO_INVALID_DONE", "完成投影只能是 0 或 1"));
        }
        status = if done == 1 { "completed" } else { "inbox" }.into();
    }
    let done = if status == "delete_pending" {
        existing.done
    } else if status == "completed" {
        1
    } else {
        0
    };
    let due_changed = update.due_at.is_some() || update.due_date.is_some();
    let due_at = if due_changed {
        compatible_due_at(update.due_at.clone(), update.due_date.clone())?
    } else {
        existing.due_at
    };
    let due_date = if due_changed {
        projected_due_date(due_at.as_deref(), update.due_date.clone())
    } else {
        existing.due_date
    };
    let remind_at = if let Some(value) = update.remind_at.clone() {
        normalize_timestamp(Some(value), "TODO_INVALID_REMIND_AT")?
    } else {
        existing.remind_at
    };
    let tags_json = if let Some(tags) = update.tags.clone() {
        normalize_tags(Some(tags))?
    } else {
        existing.tags_json
    };
    sqlx::query(
        "UPDATE case_todos SET
            title = ?, content = ?, kind = ?, priority = ?, tags_json = ?, next_action = ?,
            status = ?, done = ?,
            done_at = CASE WHEN ? = 1 AND done = 0 THEN datetime('now') WHEN ? = 0 THEN NULL ELSE done_at END,
            due_at = ?, remind_at = ?, due_date = ?, updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(title)
    .bind(update.content.clone().unwrap_or(existing.content))
    .bind(kind)
    .bind(priority)
    .bind(tags_json)
    .bind(match update.next_action.clone() {
        Some(value) => trimmed_optional(Some(value)),
        None => existing.next_action,
    })
    .bind(status)
    .bind(done)
    .bind(done)
    .bind(done)
    .bind(due_at)
    .bind(remind_at)
    .bind(due_date)
    .bind(id)
    .execute(pool)
    .await
    .map_err(TodoError::db)?;
    get(pool, id, false)
        .await?
        .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "更新后未找到事项"))
}

pub async fn set_case(
    pool: &SqlitePool,
    id: &str,
    case_id: Option<String>,
) -> Result<Todo, TodoError> {
    let case_id = trimmed_optional(case_id);
    let mut transaction = pool.begin().await.map_err(TodoError::db)?;
    if let Some(case_id) = &case_id {
        ensure_case_exists(&mut transaction, case_id).await?;
    }
    let affected = sqlx::query(
        "UPDATE case_todos SET case_id = ?, updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(case_id)
    .bind(id)
    .execute(&mut *transaction)
    .await
    .map_err(TodoError::db)?
    .rows_affected();
    if affected != 1 {
        return Err(TodoError::new("TODO_NOT_FOUND", "事项不存在或已删除"));
    }
    transaction.commit().await.map_err(TodoError::db)?;
    get(pool, id, false)
        .await?
        .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "关联后未找到事项"))
}

pub async fn soft_delete(pool: &SqlitePool, id: &str) -> Result<u64, TodoError> {
    sqlx::query(
        "UPDATE case_todos SET status = 'deleted', deleted_at = datetime('now'), updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .map(|result| result.rows_affected())
    .map_err(TodoError::db)
}

pub async fn restore(pool: &SqlitePool, id: &str) -> Result<Todo, TodoError> {
    let affected = sqlx::query(
        "UPDATE case_todos SET
            status = CASE WHEN done = 1 THEN 'completed' ELSE 'inbox' END,
            deleted_at = NULL, updated_at = datetime('now')
         WHERE id = ? AND deleted_at IS NOT NULL",
    )
    .bind(id)
    .execute(pool)
    .await
    .map_err(TodoError::db)?
    .rows_affected();
    if affected != 1 {
        return Err(TodoError::new("TODO_NOT_FOUND", "回收站中没有该事项"));
    }
    get(pool, id, false)
        .await?
        .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "恢复后未找到事项"))
}

pub async fn delete(pool: &SqlitePool, id: &str) -> Result<u64, TodoError> {
    soft_delete(pool, id).await
}

#[derive(FromRow)]
struct ExistingCopy {
    id: String,
    case_id: Option<String>,
    deleted_at: Option<String>,
}

async fn existing_copy(
    transaction: &mut Transaction<'_, Sqlite>,
    todo_id: &str,
) -> Result<Option<ExistingCopy>, TodoError> {
    sqlx::query_as::<_, ExistingCopy>(
        "SELECT id, case_id, deleted_at FROM case_work_items
         WHERE external_source = 'case_todo' AND external_record_id = ?",
    )
    .bind(todo_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(TodoError::db)
}

fn existing_copy_result(existing: ExistingCopy, target: &str) -> Result<CopyTodoResult, TodoError> {
    if existing.case_id.as_deref() != Some(target) {
        return Err(TodoError::new(
            "TODO_PROGRESS_LINK_CONFLICT",
            "该事项已经复制到其他案件",
        ));
    }
    Ok(CopyTodoResult {
        work_item_id: existing.id,
        case_id: target.into(),
        created: false,
        outcome_code: if existing.deleted_at.is_some() {
            "TODO_PROGRESS_ALREADY_EXISTS_DELETED"
        } else {
            "TODO_PROGRESS_ALREADY_EXISTS"
        }
        .into(),
    })
}

pub async fn copy_to_case_progress(
    pool: &SqlitePool,
    id: &str,
    target_case_id: Option<String>,
) -> Result<CopyTodoResult, TodoError> {
    let mut transaction = pool.begin().await.map_err(TodoError::db)?;
    let todo =
        sqlx::query_as::<_, Todo>("SELECT * FROM case_todos WHERE id = ? AND deleted_at IS NULL")
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(TodoError::db)?
            .ok_or_else(|| TodoError::new("TODO_NOT_FOUND", "事项不存在或已删除"))?;
    let requested = trimmed_optional(target_case_id);
    let target = match (&todo.case_id, requested) {
        (Some(bound), Some(requested)) if bound != &requested => {
            return Err(TodoError::new(
                "TODO_PROGRESS_TARGET_CONFLICT",
                "已关联事项不能复制到其他案件",
            ));
        }
        (Some(bound), _) => bound.clone(),
        (None, Some(requested)) => requested,
        (None, None) => {
            return Err(TodoError::new(
                "TODO_PROGRESS_CASE_REQUIRED",
                "未关联事项必须先选择案件",
            ));
        }
    };
    ensure_case_exists(&mut transaction, &target).await?;
    if let Some(existing) = existing_copy(&mut transaction, id).await? {
        return existing_copy_result(existing, &target);
    }
    let occurred_at = todo
        .due_at
        .clone()
        .or(todo.remind_at.clone())
        .or_else(|| {
            todo.due_date
                .as_ref()
                .map(|date| format!("{date}T00:00:00+08:00"))
        })
        .unwrap_or_else(|| todo.created_at.clone());
    let work_item_id = Uuid::new_v4().to_string();
    let raw_payload = serde_json::json!({
        "source_item_id": &todo.id,
        "source": &todo.source,
        "due_at": &todo.due_at,
        "remind_at": &todo.remind_at,
        "copied_at": chrono::Utc::now().to_rfc3339(),
        "title": &todo.title,
        "content": &todo.content,
    })
    .to_string();
    sqlx::query(
        "INSERT INTO case_work_items (
            id, case_id, occurred_at, work_type, title, content, next_action, source,
            external_source, external_record_id, raw_payload_json, confirmation_status
         ) VALUES (?, ?, ?, 'todo', ?, ?, ?, ?, 'case_todo', ?, ?, 'confirmed')
         ON CONFLICT DO NOTHING",
    )
    .bind(&work_item_id)
    .bind(&target)
    .bind(occurred_at)
    .bind(&todo.title)
    .bind(&todo.content)
    .bind(&todo.next_action)
    .bind(format!("case_todo:{}", todo.source))
    .bind(id)
    .bind(raw_payload)
    .execute(&mut *transaction)
    .await
    .map_err(TodoError::db)?;
    let actual = existing_copy(&mut transaction, id)
        .await?
        .ok_or_else(|| TodoError::new("TODO_DB_ERROR", "复制写入后未找到进展"))?;
    let result = if actual.id == work_item_id {
        CopyTodoResult {
            work_item_id,
            case_id: target,
            created: true,
            outcome_code: "TODO_PROGRESS_CREATED".into(),
        }
    } else {
        existing_copy_result(actual, &target)?
    };
    transaction.commit().await.map_err(TodoError::db)?;
    Ok(result)
}

pub fn is_active_actionable(todo: &Todo) -> bool {
    matches!(todo.kind.as_str(), "todo" | "reminder")
        && ACTIVE_STATUSES.contains(&todo.status.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn fixture() -> SqlitePool {
        let pool = super::super::init_pool("sqlite::memory:")
            .await
            .expect("migrate todo fixture");
        sqlx::query(
            "INSERT INTO cases (id, name, case_type, source_folder) VALUES ('case-a', '测试案件', '诉讼', 'test://case-a')",
        )
        .execute(&pool)
        .await
        .expect("insert case fixture");
        pool
    }

    #[tokio::test]
    async fn unbound_todo_can_be_soft_deleted_and_restored() {
        let pool = fixture().await;
        let todo = add(
            &pool,
            NewTodo {
                case_id: None,
                title: "  整理证据  ".into(),
                content: Some("核对原件".into()),
                kind: None,
                priority: None,
                tags: Some(vec!["证据".into(), "证据".into()]),
                next_action: None,
                due_at: None,
                remind_at: None,
                due_date: Some("2026-08-18".into()),
            },
        )
        .await
        .expect("create global todo");
        assert_eq!(todo.title, "整理证据");
        assert_eq!(todo.case_id, None);
        assert_eq!(todo.due_at.as_deref(), Some("2026-08-18T00:00:00+08:00"));
        assert_eq!(soft_delete(&pool, &todo.id).await.unwrap(), 1);
        assert!(list_global(
            &pool,
            TodoFilter {
                state: Some("deleted".into()),
                ..Default::default()
            }
        )
        .await
        .unwrap()
        .iter()
        .any(|item| item.id == todo.id));
        assert_eq!(restore(&pool, &todo.id).await.unwrap().status, "inbox");
    }

    #[test]
    fn compatibility_date_uses_shanghai_calendar_day() {
        assert_eq!(
            projected_due_date(Some("2026-08-17T16:00:00Z"), None).as_deref(),
            Some("2026-08-18")
        );
    }

    #[tokio::test]
    async fn copy_to_progress_is_idempotent_and_case_safe() {
        let pool = fixture().await;
        let todo = add(
            &pool,
            NewTodo {
                case_id: Some("case-a".into()),
                title: "提交代理词".into(),
                content: None,
                kind: None,
                priority: None,
                tags: None,
                next_action: None,
                due_at: None,
                remind_at: None,
                due_date: None,
            },
        )
        .await
        .unwrap();
        let first = copy_to_case_progress(&pool, &todo.id, None).await.unwrap();
        let second = copy_to_case_progress(&pool, &todo.id, Some("case-a".into()))
            .await
            .unwrap();
        assert!(first.created);
        assert!(!second.created);
        assert_eq!(first.work_item_id, second.work_item_id);
    }
}
