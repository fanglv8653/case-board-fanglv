//! V0.2 D4-D5.C · 并行子任务调度(详 § 6.3)。
//!
//! 主要 use case:agent_loop 一轮里 LLM 返回多个 tool_calls,本模块用 `futures::join_all`
//! **并发**派发(IO-bound 工具调用同时跑,显著缩短整体耗时)。允许部分失败 — 失败的
//! 在结果里标 ⚠️,不阻塞整体(对应 § 6.3 注意事项:**用 join!,不用 try_join!**)。
//!
//! 跟 agent_loop 关系:
//!   - agent_loop 决定本轮要调哪几个工具(LLM 自己规划)
//!   - 把工具调用清单包成 `Vec<Subtask>` 交给本模块
//!   - 本模块返回 `Vec<SubtaskResult>`,agent_loop 把每个结果填回 messages
//!
//! 注意:这里**不用 tokio::spawn**(跨线程)— 因为 `ToolContext` 含 `&SqlitePool` 等
//! 借用,Future 不是 `'static`。改用 `futures::future::join_all` 在同 task 里 cooperative
//! 并发,IO 等待自然并行(reqwest / sqlx 都是 async I/O)。

use serde::Serialize;
use serde_json::Value;

use super::tools::{Tool, ToolContext, ToolError, ToolRegistry};

/// 子任务定义(对应 LLM 一次 tool_call)。
#[derive(Debug, Clone)]
pub struct Subtask {
    /// LLM 给的 tool_call_id(回填 messages 时用,**必须一致**)
    pub tool_call_id: String,
    /// 工具名,从 ToolRegistry 找
    pub tool: String,
    /// 解析过的 args(LLM function_call.arguments JSON)
    pub args: Value,
}

/// 子任务结果。
#[derive(Debug, Clone, Serialize)]
pub struct SubtaskResult {
    pub tool_call_id: String,
    pub tool: String,
    pub args: Value,
    pub success: bool,
    /// 给 LLM 回填 messages 的 content(成功是工具返回 JSON,失败是 `{"error": ...}`)
    pub content: String,
    pub kb_hit: bool,
    pub credits_used: u32,
    pub error_short: Option<String>,
    pub started_at_ms: i64,
    pub finished_at_ms: i64,
}

impl SubtaskResult {
    /// 本子任务是否算"严重失败"(影响整体回答)。
    /// 我们认为:`InvalidArgs / NoCaseBound` 是软失败(LLM 调用方式问题,重试即可),
    /// `Yuandian / Sqlx / Io / Runtime` 是硬失败(基础设施异常,应该提醒用户)。
    pub fn is_hard_failure(&self) -> bool {
        if self.success {
            return false;
        }
        // 简单识别:error_short 里包含 "InvalidArgs / NoCaseBound / 未注册" 视为软失败
        let s = self.error_short.as_deref().unwrap_or("");
        !(s.contains("参数错误") || s.contains("没绑定案件") || s.contains("未注册"))
    }
}

/// 跑一组子任务,**read-only 工具并发、mutating 工具串行独占**,allow 部分失败。
///
/// 为什么 mutating 要串行:本模块用 `join_all` 在**同一 task** 内协作并发(非真线程),
/// 工具在 IO `await` 点会让出。若同一轮里有两个改同一份文书的 `edit_artifact`(read→改→write),
/// 它们会在文件 read 的 await 点交错 → 都读到原文 → 各自写回 → **后写覆盖前写(丢更新)**。
/// 故 mutating 工具(`Tool::is_mutating`)一个跑完再跑下一个;read-only 仍并发。
///
/// 返回顺序跟入参 subtasks 一致(回填 messages 要按 tool_call 顺序匹配 tool_call_id)。
pub async fn run_parallel_subtasks(
    subtasks: Vec<Subtask>,
    registry: &ToolRegistry,
    ctx: &ToolContext<'_>,
) -> Vec<SubtaskResult> {
    let n = subtasks.len();
    let mut slots: Vec<Option<SubtaskResult>> = (0..n).map(|_| None).collect();

    // 分流:read-only(并发)/ mutating(串行),都记原始下标以便按序回填
    let mut ro: Vec<(usize, Subtask)> = Vec::new();
    let mut muts: Vec<(usize, Subtask)> = Vec::new();
    for (i, st) in subtasks.into_iter().enumerate() {
        if registry.is_mutating(&st.tool) {
            muts.push((i, st));
        } else {
            ro.push((i, st));
        }
    }

    // read-only 并发
    let (ro_idx, ro_tasks): (Vec<usize>, Vec<Subtask>) = ro.into_iter().unzip();
    let ro_results = futures::future::join_all(
        ro_tasks
            .into_iter()
            .map(|st| run_one_subtask(st, registry, ctx)),
    )
    .await;
    for (i, r) in ro_idx.into_iter().zip(ro_results) {
        slots[i] = Some(r);
    }

    // mutating 串行(一个跑完再跑下一个)
    for (i, st) in muts {
        slots[i] = Some(run_one_subtask(st, registry, ctx).await);
    }

    slots
        .into_iter()
        .map(|s| s.expect("每个下标都应被填充"))
        .collect()
}

async fn run_one_subtask(
    st: Subtask,
    registry: &ToolRegistry,
    ctx: &ToolContext<'_>,
) -> SubtaskResult {
    let started = chrono::Local::now().timestamp_millis();
    let exec = run_tool_inner(&st.tool, &st.args, registry, ctx).await;
    let finished = chrono::Local::now().timestamp_millis();
    match exec {
        Ok(r) => SubtaskResult {
            tool_call_id: st.tool_call_id,
            tool: st.tool,
            args: st.args,
            success: true,
            content: r.content,
            kb_hit: r.kb_hit,
            credits_used: r.yuandian_credits_used,
            error_short: None,
            started_at_ms: started,
            finished_at_ms: finished,
        },
        Err(e) => {
            let err_str = e.to_string();
            // sanitize 路径(避免泄漏 /Users/xxx 这种)— 走反馈 MD 同一套
            let safe = crate::feedback::sanitize_paths(&err_str);
            let content = serde_json::to_string(&serde_json::json!({"error": &safe}))
                .unwrap_or_else(|_| format!("{{\"error\":\"{}\"}}", safe.replace('"', "'")));
            SubtaskResult {
                tool_call_id: st.tool_call_id,
                tool: st.tool,
                args: st.args,
                success: false,
                content,
                kb_hit: false,
                credits_used: 0,
                error_short: Some(safe),
                started_at_ms: started,
                finished_at_ms: finished,
            }
        }
    }
}

async fn run_tool_inner(
    name: &str,
    args: &Value,
    registry: &ToolRegistry,
    ctx: &ToolContext<'_>,
) -> Result<super::tools::ToolResult, ToolError> {
    let tool: &dyn Tool = registry
        .find(name)
        .ok_or_else(|| ToolError::InvalidArgs(format!("未注册的工具:{}", name)))?;
    tool.execute(args, ctx).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use sqlx::SqlitePool;

    async fn make_pool() -> SqlitePool {
        crate::db::init_pool(":memory:").await.unwrap()
    }

    fn empty_settings() -> crate::settings::Settings {
        crate::settings::Settings::default()
    }

    #[tokio::test]
    async fn empty_subtasks_returns_empty_results() {
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None,
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(vec![], &registry, &ctx).await;
        assert!(r.is_empty());
    }

    #[tokio::test]
    async fn unknown_tool_is_soft_failure() {
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None,
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(
            vec![Subtask {
                tool_call_id: "c1".into(),
                tool: "fake_tool_xyz".into(),
                args: json!({}),
            }],
            &registry,
            &ctx,
        )
        .await;
        assert_eq!(r.len(), 1);
        assert!(!r[0].success);
        assert!(r[0].error_short.is_some());
        // 未注册 → 软失败
        assert!(!r[0].is_hard_failure());
    }

    #[tokio::test]
    async fn list_case_docs_without_case_id_fails_soft() {
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None, // 关键:没 case_id,list_case_docs 应该报 NoCaseBound
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(
            vec![Subtask {
                tool_call_id: "c1".into(),
                tool: "list_case_docs".into(),
                args: json!({}),
            }],
            &registry,
            &ctx,
        )
        .await;
        assert_eq!(r.len(), 1);
        assert!(!r[0].success);
        // NoCaseBound 错误属于软失败(LLM 调用方式问题)
        assert!(!r[0].is_hard_failure());
    }

    #[tokio::test]
    async fn multiple_subtasks_preserve_input_order() {
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None,
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(
            vec![
                Subtask {
                    tool_call_id: "c1".into(),
                    tool: "fake_a".into(),
                    args: json!({}),
                },
                Subtask {
                    tool_call_id: "c2".into(),
                    tool: "fake_b".into(),
                    args: json!({}),
                },
                Subtask {
                    tool_call_id: "c3".into(),
                    tool: "fake_c".into(),
                    args: json!({}),
                },
            ],
            &registry,
            &ctx,
        )
        .await;
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].tool_call_id, "c1");
        assert_eq!(r[1].tool_call_id, "c2");
        assert_eq!(r[2].tool_call_id, "c3");
    }

    #[tokio::test]
    async fn mutating_in_middle_keeps_input_order() {
        // read-only 并发 / mutating 串行分流后,仍要按原 tool_call 顺序回填(中间夹一个 mutating)。
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None, // save_artifact 会报 NoCaseBound(软失败),但仍返回 result —— 只验顺序
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(
            vec![
                Subtask {
                    tool_call_id: "c1".into(),
                    tool: "list_case_docs".into(), // read-only
                    args: json!({}),
                },
                Subtask {
                    tool_call_id: "c2".into(),
                    tool: "save_artifact".into(), // mutating(串行)
                    args: json!({}),
                },
                Subtask {
                    tool_call_id: "c3".into(),
                    tool: "list_case_docs".into(), // read-only
                    args: json!({}),
                },
            ],
            &registry,
            &ctx,
        )
        .await;
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].tool_call_id, "c1");
        assert_eq!(r[1].tool_call_id, "c2");
        assert_eq!(r[1].tool, "save_artifact", "mutating 工具回填到原始下标 1");
        assert_eq!(r[2].tool_call_id, "c3");
    }

    #[tokio::test]
    async fn partial_failure_does_not_block_others() {
        // 三个任务,其中一个会失败(假工具名),其他两个也会失败但都返回 result,
        // 不会因为某个 task 失败就中止整体(对应 join_all 而非 try_join_all 的语义)。
        let pool = make_pool().await;
        let s = empty_settings();
        let ctx = ToolContext {
            pool: &pool,
            settings: &s,
            case_id: None,
            local_kb: None,
            app: None,
        };
        let registry = ToolRegistry::default_v0_2();
        let r = run_parallel_subtasks(
            vec![
                Subtask {
                    tool_call_id: "1".into(),
                    tool: "fake_1".into(),
                    args: json!({}),
                },
                Subtask {
                    tool_call_id: "2".into(),
                    tool: "fake_2".into(),
                    args: json!({}),
                },
            ],
            &registry,
            &ctx,
        )
        .await;
        assert_eq!(r.len(), 2);
        for x in &r {
            assert!(!x.success);
            // 每个失败都有 content,可以塞回 LLM
            assert!(!x.content.is_empty());
        }
    }

    #[test]
    fn is_hard_failure_classification() {
        let mk = |success: bool, err: Option<&str>| SubtaskResult {
            tool_call_id: "x".into(),
            tool: "x".into(),
            args: json!({}),
            success,
            content: "".into(),
            kb_hit: false,
            credits_used: 0,
            error_short: err.map(String::from),
            started_at_ms: 0,
            finished_at_ms: 0,
        };
        // 成功 → 不是 hard
        assert!(!mk(true, None).is_hard_failure());
        // 软失败(参数错误)
        assert!(!mk(false, Some("参数错误:缺 keyword")).is_hard_failure());
        // 软失败(NoCaseBound)
        assert!(!mk(false, Some("当前对话没绑定案件,本工具需要 case_id")).is_hard_failure());
        // 软失败(未注册)
        assert!(!mk(false, Some("未注册的工具:foo")).is_hard_failure());
        // 硬失败(元典 500)
        assert!(mk(false, Some("元典 HTTP 500:服务异常")).is_hard_failure());
        // 硬失败(IO)
        assert!(mk(false, Some("IO 错误:Permission denied")).is_hard_failure());
    }
}
