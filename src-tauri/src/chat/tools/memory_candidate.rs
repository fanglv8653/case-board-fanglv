//! 原生案件助手的记忆候选提议入口。
//!
//! 本工具只能为当前案件创建 `pending` 候选，不创建、确认或启用案件记忆。

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{opt_str, require_str, Tool, ToolContext, ToolError, ToolResult};
use crate::db::case_memory::{create_memory_candidate, CreateCandidateInput};

pub struct ProposeCaseMemoryCandidate;

#[async_trait]
impl Tool for ProposeCaseMemoryCandidate {
    fn name(&self) -> &str {
        "propose_case_memory_candidate"
    }

    fn description(&self) -> &str {
        include_str!("descriptions/propose_case_memory_candidate.md")
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "type": {
                    "type": "string",
                    "enum": ["fact", "procedure", "strategy", "client_instruction", "risk_warning"],
                    "description": "候选类型：事实、程序、策略、客户指示或风险提示"
                },
                "title": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 120,
                    "description": "简短、可供律师复核的候选标题"
                },
                "content": {
                    "type": "string",
                    "minLength": 1,
                    "maxLength": 4000,
                    "description": "候选正文；应保留待核实表述，不得把推断写成已证实事实"
                },
                "source_message_id": {
                    "type": "string",
                    "description": "可选：当前案件对话中的来源消息 ID"
                }
            },
            "required": ["type", "title", "content"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> Result<ToolResult, ToolError> {
        let case_id = ctx.case_id.ok_or(ToolError::NoCaseBound)?;
        let candidate = create_memory_candidate(
            ctx.pool,
            case_id,
            CreateCandidateInput {
                proposed_type: require_str(args, "type")?.to_string(),
                proposed_title: require_str(args, "title")?.to_string(),
                proposed_content: require_str(args, "content")?.to_string(),
                proposed_by_type: "assistant".to_string(),
                source_message_id: opt_str(args, "source_message_id").map(str::to_string),
            },
        )
        .await
        .map_err(|error| ToolError::Runtime(format!("创建记忆候选失败:{error}")))?;

        let content = serde_json::to_string_pretty(&json!({
            "candidate_id": candidate.id,
            "case_id": candidate.case_id,
            "status": candidate.status,
            "active": false,
            "next_step": "请用户到“记忆”页面接受该候选；接受后仍需二次确认，才可启用为案件记忆。"
        }))
        .map_err(|error| ToolError::Runtime(format!("记忆候选结果序列化失败:{error}")))?;
        Ok(ToolResult::plain(content))
    }

    fn is_mutating(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use sqlx::SqlitePool;

    async fn pool() -> SqlitePool {
        crate::db::init_pool(":memory:").await.unwrap()
    }

    async fn seed_case(pool: &SqlitePool, id: &str) {
        sqlx::query(
            "INSERT INTO cases (
                id, name, case_type, case_status, source_folder, created_at, updated_at
             ) VALUES (
                ?1, '记忆候选测试案', '刑事', 'active', ?2, datetime('now'), datetime('now')
             )",
        )
        .bind(id)
        .bind(format!("C:/test/{id}"))
        .execute(pool)
        .await
        .unwrap();
    }

    fn context<'a>(
        pool: &'a SqlitePool,
        settings: &'a Settings,
        case_id: Option<&'a str>,
    ) -> ToolContext<'a> {
        ToolContext {
            pool,
            settings,
            case_id,
            local_kb: None,
            app: None,
        }
    }

    fn args() -> Value {
        json!({
            "type": "risk_warning",
            "title": "笔录日期待核",
            "content": "两份笔录记载日期不一致，需核对原件。"
        })
    }

    #[tokio::test]
    async fn rejects_when_chat_has_no_current_case() {
        let pool = pool().await;
        let settings = Settings::default();
        let error = ProposeCaseMemoryCandidate
            .execute(&args(), &context(&pool, &settings, None))
            .await
            .unwrap_err();
        assert!(matches!(error, ToolError::NoCaseBound));
    }

    #[tokio::test]
    async fn creates_pending_candidate_without_active_memory() {
        let pool = pool().await;
        seed_case(&pool, "case-a").await;
        let settings = Settings::default();
        let tool = ProposeCaseMemoryCandidate;
        let result = tool
            .execute(&args(), &context(&pool, &settings, Some("case-a")))
            .await
            .unwrap();
        let body: Value = serde_json::from_str(&result.content).unwrap();

        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM case_memory_candidates
             WHERE case_id = 'case-a' AND status = 'pending'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let active: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM case_memory_items
             WHERE case_id = 'case-a' AND status = 'active'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(body["status"], "pending");
        assert_eq!(body["active"], false);
        assert_eq!(pending, 1);
        assert_eq!(active, 0);
        assert!(tool.is_mutating());
    }

    #[test]
    fn schema_only_exposes_candidate_fields() {
        let schema = ProposeCaseMemoryCandidate.parameters_schema();
        let properties = schema["properties"].as_object().unwrap();
        assert_eq!(properties.len(), 4);
        for key in ["type", "title", "content", "source_message_id"] {
            assert!(properties.contains_key(key));
        }
        assert_eq!(schema["additionalProperties"], false);
    }
}
