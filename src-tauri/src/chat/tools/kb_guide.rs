//! 给方律原生 AI 读取的本地知识库只读说明工具。
//!
//! 工具不读取文件正文、不创建目录、不更新索引，也不生成外部 AI 配置。

use async_trait::async_trait;
use serde_json::{json, Value};

use super::{Tool, ToolContext, ToolError, ToolResult};

pub struct GetLocalKbGuide;

#[async_trait]
impl Tool for GetLocalKbGuide {
    fn name(&self) -> &str {
        "get_local_kb_guide"
    }

    fn description(&self) -> &str {
        include_str!("descriptions/get_local_kb_guide.md")
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {},
            "additionalProperties": false
        })
    }

    async fn execute(&self, _args: &Value, ctx: &ToolContext<'_>) -> Result<ToolResult, ToolError> {
        let guide = crate::local_kb::guide::build_local_kb_guide(
            ctx.local_kb
                .map(|knowledge_base| knowledge_base.root.as_path()),
        );
        let content = serde_json::to_string_pretty(&guide)
            .map_err(|error| ToolError::Runtime(format!("知识库说明序列化失败:{error}")))?;
        Ok(ToolResult {
            content,
            yuandian_credits_used: 0,
            kb_hit: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_is_read_only_and_has_no_arguments() {
        let tool = GetLocalKbGuide;
        assert_eq!(tool.name(), "get_local_kb_guide");
        assert!(!tool.is_mutating());
        assert_eq!(tool.parameters_schema()["additionalProperties"], false);
    }
}
