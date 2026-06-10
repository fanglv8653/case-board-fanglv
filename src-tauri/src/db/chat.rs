//! 案件 AI 助手聊天记录(`chat_messages`)表的 CRUD。
//!
//! V0.1.13+ 案件详情页右侧聊天面板的持久化层。
//!
//! 设计要点:
//!   - 流式输出**完成后**才写一条 assistant 消息(中途不写,避免半句留库)
//!   - prompt_tokens / completion_tokens / latency_ms 由 chat 模块读 DeepSeek
//!     usage 段写入,用于反馈 MD 性能埋点(content **不**进反馈)
//!   - based_on 是 JSON 数组,记录这次回答引用的 document.id;前端 markdown
//!     可以渲染成"来源:民事诉状.docx"之类的引用
//!   - artifact_doc_id 指向 documents 表里的 chat artifact(若本次输出落了 MD)
//!   - error_short:assistant 出错时填脱敏错误(真错透传,见 CLAUDE.md 坑 #9)
//!
//! 隐私铁律 #3:chat_messages.content **永远不进反馈 MD**,
//! feedback::tests 加 regression 测试。

use serde::{Deserialize, Serialize};
use sqlx::{FromRow, SqlitePool};

/// 聊天消息表行。
///
/// 一条 user 消息 + 一条 assistant 消息 = 一对 turn。
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ChatMessage {
    pub id: String,
    pub case_id: String,
    /// 'user' / 'assistant'
    pub role: String,
    pub content: String,
    /// NULL=自由问;否则枚举 generate_case_overview / generate_evidence_list /
    /// generate_timeline / generate_client_update / find_payment / list_missing
    pub task_type: Option<String>,
    pub model: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    /// JSON 数组,例如 `["doc-uuid-1","doc-uuid-2"]`
    pub based_on: Option<String>,
    /// 若 assistant 输出落了 artifact,指向 documents.id
    pub artifact_doc_id: Option<String>,
    /// assistant 出错时填脱敏错误
    pub error_short: Option<String>,
    pub created_at: String,
    /// V0.2 D6.5 · 本轮用户在 AttachmentPicker 引用的 doc.id 列表(JSON 数组字符串)。
    /// user 消息上写;assistant 消息可冗余写一份方便回放(也可保持 null,按 task_id link)。
    pub attached_doc_ids: Option<String>,
    /// V0.2 D6.5 · `<CITATIONS>` 协议解析后落库的 JSON 数组(`Vec<Citation>` 序列化)。
    /// 前端 history 重放时 deserialize 给 CitationsCard 渲染。仅 assistant 消息有值。
    pub citations_json: Option<String>,
    /// V0.2 D6.5 · 关联到 chat_tasks.id(若本条消息走了 agent_loop 路径)。
    /// chat_tasks 行落业务级数据(plan/subtask/tool_calls/credits 等);本字段只是 link。
    /// 前端 history 重放 ToolCallTrace 时按 task_id 反查 chat_tasks.tool_calls_json。
    pub task_id: Option<String>,
}

/// 新建一条聊天消息的入参。
#[derive(Debug, Clone)]
pub struct NewChatMessage<'a> {
    pub id: &'a str,
    pub case_id: &'a str,
    pub role: &'a str,
    pub content: &'a str,
    pub task_type: Option<&'a str>,
    pub model: Option<&'a str>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub latency_ms: Option<i64>,
    pub based_on: Option<&'a str>,
    pub artifact_doc_id: Option<&'a str>,
    pub error_short: Option<&'a str>,
    /// V0.2 D6.5 · 引用的 doc.id JSON 数组(`["uuid1","uuid2"]`)
    pub attached_doc_ids: Option<&'a str>,
    /// V0.2 D6.5 · `<CITATIONS>` 解析结果 JSON
    pub citations_json: Option<&'a str>,
    /// V0.2 D6.5 · chat_tasks.id;tool_calls 通过本 link 反查 chat_tasks 表
    pub task_id: Option<&'a str>,
}

/// 插入一条 chat 消息。流式完成后一次性写,中途不写半句。
pub async fn insert_chat_message(
    pool: &SqlitePool,
    msg: NewChatMessage<'_>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO chat_messages \
         (id, case_id, role, content, task_type, model, \
          prompt_tokens, completion_tokens, latency_ms, \
          based_on, artifact_doc_id, error_short, \
          attached_doc_ids, citations_json, task_id) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(msg.id)
    .bind(msg.case_id)
    .bind(msg.role)
    .bind(msg.content)
    .bind(msg.task_type)
    .bind(msg.model)
    .bind(msg.prompt_tokens)
    .bind(msg.completion_tokens)
    .bind(msg.latency_ms)
    .bind(msg.based_on)
    .bind(msg.artifact_doc_id)
    .bind(msg.error_short)
    .bind(msg.attached_doc_ids)
    .bind(msg.citations_json)
    .bind(msg.task_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// 列出某案件下所有聊天记录,按 created_at 升序(老的在前)。
///
/// 前端拿到后直接按顺序渲染。limit=None 表示不限,常见 limit=200 防意外暴涨。
pub async fn list_chat_messages(
    pool: &SqlitePool,
    case_id: &str,
    limit: Option<i64>,
) -> Result<Vec<ChatMessage>, sqlx::Error> {
    match limit {
        Some(n) => {
            // 取最近 n 条然后正序返回(用子查询)
            sqlx::query_as::<_, ChatMessage>(
                "SELECT * FROM ( \
                   SELECT * FROM chat_messages \
                   WHERE case_id = ? \
                   ORDER BY created_at DESC \
                   LIMIT ? \
                 ) ORDER BY created_at ASC",
            )
            .bind(case_id)
            .bind(n)
            .fetch_all(pool)
            .await
        }
        None => {
            sqlx::query_as::<_, ChatMessage>(
                "SELECT * FROM chat_messages WHERE case_id = ? ORDER BY created_at ASC",
            )
            .bind(case_id)
            .fetch_all(pool)
            .await
        }
    }
}

/// 删除某案件下全部聊天记录(用户主动清空 / 案件删除级联)。
///
/// 案件删除时 ON DELETE CASCADE 已经自动清理,这个函数留给用户手动清空。
pub async fn delete_chat_history_for_case(
    pool: &SqlitePool,
    case_id: &str,
) -> Result<u64, sqlx::Error> {
    let res = sqlx::query("DELETE FROM chat_messages WHERE case_id = ?")
        .bind(case_id)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::init_pool;

    async fn make_case(pool: &SqlitePool) -> String {
        let case_id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO cases (id, name, case_type, source_folder) VALUES (?, ?, ?, ?)")
            .bind(&case_id)
            .bind("测试案件")
            .bind("诉讼")
            .bind(format!("/tmp/chat_test/{}", case_id))
            .execute(pool)
            .await
            .unwrap();
        case_id
    }

    #[tokio::test]
    async fn insert_and_list_round_trip() {
        let pool = init_pool(":memory:").await.unwrap();
        let case_id = make_case(&pool).await;

        let user_id = uuid::Uuid::new_v4().to_string();
        insert_chat_message(
            &pool,
            NewChatMessage {
                id: &user_id,
                case_id: &case_id,
                role: "user",
                content: "这个案子的争议焦点是什么?",
                task_type: None,
                model: None,
                prompt_tokens: None,
                completion_tokens: None,
                latency_ms: None,
                based_on: None,
                artifact_doc_id: None,
                error_short: None,
                attached_doc_ids: None,
                citations_json: None,
                task_id: None,
            },
        )
        .await
        .expect("insert user msg");

        let assistant_id = uuid::Uuid::new_v4().to_string();
        insert_chat_message(
            &pool,
            NewChatMessage {
                id: &assistant_id,
                case_id: &case_id,
                role: "assistant",
                content: "本案争议焦点是 ...",
                task_type: None,
                model: Some("deepseek-v4-flash"),
                prompt_tokens: Some(3200),
                completion_tokens: Some(180),
                latency_ms: Some(4200),
                based_on: Some(r#"["doc-1","doc-2"]"#),
                artifact_doc_id: None,
                error_short: None,
                attached_doc_ids: None,
                citations_json: None,
                task_id: None,
            },
        )
        .await
        .expect("insert assistant msg");

        let msgs = list_chat_messages(&pool, &case_id, None).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].prompt_tokens, Some(3200));
        assert_eq!(msgs[1].based_on.as_deref(), Some(r#"["doc-1","doc-2"]"#));
    }

    #[tokio::test]
    async fn limit_returns_most_recent_in_ascending_order() {
        let pool = init_pool(":memory:").await.unwrap();
        let case_id = make_case(&pool).await;

        // 写 5 条 user 消息,内容用序号区分;sleep 1ms 让 created_at 不撞
        for i in 0..5 {
            insert_chat_message(
                &pool,
                NewChatMessage {
                    id: &uuid::Uuid::new_v4().to_string(),
                    case_id: &case_id,
                    role: "user",
                    content: &format!("消息 {}", i),
                    task_type: None,
                    model: None,
                    prompt_tokens: None,
                    completion_tokens: None,
                    latency_ms: None,
                    based_on: None,
                    artifact_doc_id: None,
                    error_short: None,
                    attached_doc_ids: None,
                    citations_json: None,
                    task_id: None,
                },
            )
            .await
            .unwrap();
            tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        }

        // 取最近 3 条 → 应该是消息 2,3,4(升序)
        let recent = list_chat_messages(&pool, &case_id, Some(3)).await.unwrap();
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].content, "消息 2");
        assert_eq!(recent[1].content, "消息 3");
        assert_eq!(recent[2].content, "消息 4");
    }

    #[tokio::test]
    async fn cascade_delete_when_case_removed() {
        let pool = init_pool(":memory:").await.unwrap();
        let case_id = make_case(&pool).await;

        insert_chat_message(
            &pool,
            NewChatMessage {
                id: &uuid::Uuid::new_v4().to_string(),
                case_id: &case_id,
                role: "user",
                content: "测试级联",
                task_type: None,
                model: None,
                prompt_tokens: None,
                completion_tokens: None,
                latency_ms: None,
                based_on: None,
                artifact_doc_id: None,
                error_short: None,
                attached_doc_ids: None,
                citations_json: None,
                task_id: None,
            },
        )
        .await
        .unwrap();

        sqlx::query("DELETE FROM cases WHERE id = ?")
            .bind(&case_id)
            .execute(&pool)
            .await
            .unwrap();

        let (count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM chat_messages WHERE case_id = ?")
                .bind(&case_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count, 0, "删案件应级联删除聊天记录");
    }

    #[tokio::test]
    async fn documents_source_column_backfill_works() {
        let pool = init_pool(":memory:").await.unwrap();
        let case_id = make_case(&pool).await;

        // 插一个普通扫描文件 + 一个 is_ai_artifact=1 的 AI 报告
        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, is_ai_artifact) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("scan-1")
        .bind(&case_id)
        .bind("/tmp/scan/起诉状.docx")
        .bind("起诉状.docx")
        .bind(false)
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, is_ai_artifact) \
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind("ai-1")
        .bind(&case_id)
        .bind("/tmp/scan/案件总览.md")
        .bind("案件总览.md")
        .bind(true)
        .execute(&pool)
        .await
        .unwrap();

        // 新插的 row 应该都拿到 DEFAULT 'scan'(backfill 只跑了一次,影响 migration 时已有的)
        // 但 migration 已经跑完,我们模拟"新 install 的 DB"。
        // 验证 source 列存在且默认值生效
        let rows: Vec<(String, bool, String)> = sqlx::query_as(
            "SELECT id, is_ai_artifact, source FROM documents WHERE case_id = ? ORDER BY id",
        )
        .bind(&case_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        // 两个都默认 'scan'(因为是 migration 之后插入的)
        assert_eq!(rows[0].2, "scan");
        assert_eq!(rows[1].2, "scan");

        // 显式插一个 source='chat' 的 artifact,验证非默认值能写入
        sqlx::query(
            "INSERT INTO documents (id, case_id, source_path, filename, is_ai_artifact, source) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind("chat-1")
        .bind(&case_id)
        .bind("/tmp/scan/chat_总览.md")
        .bind("chat_总览.md")
        .bind(true)
        .bind("chat")
        .execute(&pool)
        .await
        .unwrap();

        let (chat_count,): (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM documents WHERE source = 'chat'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(chat_count, 1);
    }
}
