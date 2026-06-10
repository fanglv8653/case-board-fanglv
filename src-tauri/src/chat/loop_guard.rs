//! agent_loop 的 4 条 cap(V0.2 D3-D4.B,详 § 6.5)。
//!
//! 在 chat agent 多轮工具调用循环里,防止"无限调"、"反复调同一个工具"、"调用堆积太久"、
//! "thinking 模型 reasoning token 爆炸"四种失控情况。
//!
//! 每轮 LLM 请求前调 `check_iter_cap` + `check_duration_cap`;每次准备发起 tool 调用前调
//! `check_duplicate_tool_call`;LLM 返回 usage 后调 `add_reasoning_tokens`。
//!
//! 任何一个 cap 触发 → 返回 `LoopGuardViolation`,agent_loop 终止本轮并把信息塞回 LLM 让它
//! 收尾(或者直接 abort,看上层策略)。

use std::collections::HashSet;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

/// 4 条 cap 中触发哪一条。
#[derive(Debug, Clone, Serialize, Error)]
pub enum LoopGuardViolation {
    #[error("超过本会话最大轮数(max={max})")]
    IterCapExceeded { max: u32 },
    #[error("LLM 反复调同一工具 + 同参数:tool={tool},循环模式拦下")]
    DuplicateToolCall { tool: String },
    #[error("本会话总耗时超 {limit_secs}s,可能后端慢或卡死,提前 abort")]
    DurationCapExceeded { limit_secs: u64 },
    #[error("reasoning token 累计超 {limit},thinking 模型可能跑飞")]
    ReasoningTokenCapExceeded { limit: u64 },
}

pub struct LoopGuard {
    iter_count: u32,
    max_iters: u32,
    seen_tool_args: HashSet<(String, String)>,
    started_at: Instant,
    max_duration: Duration,
    reasoning_tokens: u64,
    max_reasoning_tokens: u64,
}

impl LoopGuard {
    /// 用 settings 配置 4 条 cap。settings 字段为 None 时用默认值。
    pub fn from_settings(s: &crate::settings::Settings) -> Self {
        Self {
            iter_count: 0,
            // 复杂法律任务(法规+案例+企业+校验+综合)轮数偏多,默认放到 16
            //(2026-05-31 从 12 上调:执行案法律依据曾贴满 12 轮靠 force-finish 收尾,可能漏法条)
            max_iters: s.chat_loop_max_iters.unwrap_or(16),
            seen_tool_args: HashSet::new(),
            started_at: Instant::now(),
            // 思考模型 + 多轮工具 + 写长答案,120s 墙钟太紧;放到 300s。
            // 真跑飞仍有 max_iters / max_reasoning_tokens 双重兜底。
            max_duration: Duration::from_secs(300),
            reasoning_tokens: 0,
            // thinking 模型每轮 reasoning 几千 token,多轮累积易超 8000;放到 64000。
            max_reasoning_tokens: 64_000,
        }
    }

    /// 完全用默认值(单测用)。
    #[cfg(test)]
    pub fn with_defaults() -> Self {
        Self {
            iter_count: 0,
            max_iters: 8,
            seen_tool_args: HashSet::new(),
            started_at: Instant::now(),
            max_duration: Duration::from_secs(120),
            reasoning_tokens: 0,
            max_reasoning_tokens: 8_000,
        }
    }

    /// 自定义 4 条 cap(单测用)。
    #[cfg(test)]
    pub fn with_caps(max_iters: u32, max_duration: Duration, max_reasoning_tokens: u64) -> Self {
        Self {
            iter_count: 0,
            max_iters,
            seen_tool_args: HashSet::new(),
            started_at: Instant::now(),
            max_duration,
            reasoning_tokens: 0,
            max_reasoning_tokens,
        }
    }

    pub fn iter_count(&self) -> u32 {
        self.iter_count
    }

    /// 进入新一轮(发请求前调)。失败返回 `IterCapExceeded`。
    pub fn check_iter_cap(&mut self) -> Result<(), LoopGuardViolation> {
        if self.iter_count >= self.max_iters {
            return Err(LoopGuardViolation::IterCapExceeded {
                max: self.max_iters,
            });
        }
        self.iter_count += 1;
        Ok(())
    }

    /// 派发工具前调:同一 tool + 同参数 hash 之前调过就拒绝(防 LLM 死循环)。
    /// `args` 用 canonical JSON(local_kb::hash::query_hash 不带 prefix)做 dedupe key。
    pub fn check_duplicate_tool_call(
        &mut self,
        tool: &str,
        args: &serde_json::Value,
    ) -> Result<(), LoopGuardViolation> {
        // 用同种 canonical 算法跟 KB cache 对齐,sort_keys + ensure_ascii=False
        let canonical = crate::local_kb::hash::query_hash("", args);
        let key = (tool.to_string(), canonical);
        if !self.seen_tool_args.insert(key) {
            return Err(LoopGuardViolation::DuplicateToolCall {
                tool: tool.to_string(),
            });
        }
        Ok(())
    }

    /// 每次工具调用 / LLM 调用后调一次。超过 2 分钟就 abort。
    pub fn check_duration_cap(&self) -> Result<(), LoopGuardViolation> {
        if self.started_at.elapsed() > self.max_duration {
            return Err(LoopGuardViolation::DurationCapExceeded {
                limit_secs: self.max_duration.as_secs(),
            });
        }
        Ok(())
    }

    /// LLM 返回 usage 时累计 reasoning_tokens(thinking 模型 usage.reasoning_tokens)。
    pub fn add_reasoning_tokens(&mut self, n: u64) -> Result<(), LoopGuardViolation> {
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(n);
        if self.reasoning_tokens > self.max_reasoning_tokens {
            return Err(LoopGuardViolation::ReasoningTokenCapExceeded {
                limit: self.max_reasoning_tokens,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn iter_cap_triggers_at_max() {
        let mut g = LoopGuard::with_caps(3, Duration::from_secs(120), 8_000);
        assert!(g.check_iter_cap().is_ok()); // 1
        assert!(g.check_iter_cap().is_ok()); // 2
        assert!(g.check_iter_cap().is_ok()); // 3
                                             // 第 4 次拒
        let r = g.check_iter_cap();
        assert!(matches!(
            r,
            Err(LoopGuardViolation::IterCapExceeded { max: 3 })
        ));
    }

    #[test]
    fn duplicate_tool_call_detected() {
        let mut g = LoopGuard::with_defaults();
        let args = json!({"keyword": "合同解除", "top_k": 10});
        assert!(g.check_duplicate_tool_call("search_laws", &args).is_ok());
        // 同 tool 同参再调拒
        let r = g.check_duplicate_tool_call("search_laws", &args);
        assert!(matches!(
            r,
            Err(LoopGuardViolation::DuplicateToolCall { .. })
        ));
    }

    #[test]
    fn duplicate_detects_param_order_invariant() {
        let mut g = LoopGuard::with_defaults();
        let a = json!({"a": 1, "b": 2});
        let b = json!({"b": 2, "a": 1}); // 同语义,key 顺序不同
        assert!(g.check_duplicate_tool_call("x", &a).is_ok());
        let r = g.check_duplicate_tool_call("x", &b);
        assert!(matches!(
            r,
            Err(LoopGuardViolation::DuplicateToolCall { .. })
        ));
    }

    #[test]
    fn different_tool_or_args_allowed() {
        let mut g = LoopGuard::with_defaults();
        let args1 = json!({"keyword": "A"});
        let args2 = json!({"keyword": "B"}); // 同 tool 不同参 OK
        assert!(g.check_duplicate_tool_call("search_laws", &args1).is_ok());
        assert!(g.check_duplicate_tool_call("search_laws", &args2).is_ok());
        // 不同 tool 同参也 OK
        assert!(g
            .check_duplicate_tool_call("get_law_article", &args1)
            .is_ok());
    }

    #[test]
    fn duration_cap_zero_triggers_immediately() {
        let g = LoopGuard::with_caps(8, Duration::from_secs(0), 8_000);
        std::thread::sleep(Duration::from_millis(10));
        let r = g.check_duration_cap();
        assert!(matches!(
            r,
            Err(LoopGuardViolation::DurationCapExceeded { .. })
        ));
    }

    #[test]
    fn reasoning_token_cap_accumulates() {
        let mut g = LoopGuard::with_caps(8, Duration::from_secs(120), 100);
        assert!(g.add_reasoning_tokens(50).is_ok());
        assert!(g.add_reasoning_tokens(40).is_ok());
        let r = g.add_reasoning_tokens(20);
        assert!(matches!(
            r,
            Err(LoopGuardViolation::ReasoningTokenCapExceeded { limit: 100 })
        ));
    }

    #[test]
    fn from_settings_uses_relaxed_caps() {
        // 放宽后的生产默认值(2026-05-29;2026-05-31 max_iters 12→16):
        // 避免 thinking+工具长会话误触上限,复杂执行案不漏法条
        let g = LoopGuard::from_settings(&crate::settings::Settings::default());
        assert_eq!(g.max_iters, 16);
        assert_eq!(g.max_duration, Duration::from_secs(300));
        assert_eq!(g.max_reasoning_tokens, 64_000);
    }

    #[test]
    fn from_settings_reads_chat_loop_max_iters() {
        let s = crate::settings::Settings {
            chat_loop_max_iters: Some(3),
            ..Default::default()
        };
        let g = LoopGuard::from_settings(&s);
        assert_eq!(g.max_iters, 3);
    }
}
