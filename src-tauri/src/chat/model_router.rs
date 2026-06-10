//! V0.2 D4-D5.B · 模型路由(V0.3 重构:统一到 `settings.cloud_llm_model` 单一字段)。
//!
//! 把 `(TaskType, user_message, Settings)` 映射到具体 DeepSeek 模型 + 温度 + max_tokens。
//!
//! **用户在设置里只有一个选择 `cloud_llm_model`(= 三档「模型档位」)**:
//!   - `"deepseek-v4-flash"`(默认)= **全局 Flash**:所有任务都走 flash(便宜,约 pro 的 1/3 价)。
//!   - `"deepseek-v4-pro"` / `"deepseek-v4-pro-thinking"` = **全局 Pro**:所有任务都走 pro(更准更贵)。
//!   - `"auto"` = **自动挡**:简单任务走 flash、复杂任务走 pro(下面的 task 路由表)。
//!
//! 关键:**非 auto 档绝不"偷偷"把某些任务升到 pro**(老逻辑工具型 chip 强制 pro 烧钱,已废)。
//! 自动挡(auto)下才按任务复杂度分流(V0.3.3 起 6 个生成型 chip 已删):
//!   - 4 个工具/分析型(法律依据/类案/校验/模拟对抗) → pro
//!   - FreeChat → 启发式:短问/无 reasoning 关键词 = flash,否则 pro

use serde::Serialize;

use super::context::TaskType;
use crate::settings::Settings;

/// 路由结果。给 agent_loop / commands 用,代替原来硬编码的 temperature / max_tokens。
#[derive(Debug, Clone, Serialize)]
pub struct ModelChoice {
    pub model: String,
    pub temperature: f32,
    pub max_tokens: u32,
}

/// DeepSeek V4 输出长度上限(官方文档:context 1M / output 最大 384K)。
/// flash / pro 用同一上限——旧的 4096/8192 低值会把长文书拦腰截断(`finish_reason=length`,
/// 体感像「写一半就傻了」)。这是「天花板」不是「目标」:只在模型真写那么长时才计费,
/// 短问答仍会自然停(`finish_reason=stop`)。模型档位(flash/pro)由作者在 Settings 手切,本值不区分。
pub const MAX_OUTPUT_TOKENS: u32 = 384_000;

impl ModelChoice {
    /// DeepSeek V4 Flash:快速 + 便宜,适合摘要/列表/简单问答。
    pub fn flash() -> Self {
        Self {
            model: "deepseek-v4-flash".into(),
            temperature: 0.3,
            max_tokens: MAX_OUTPUT_TOKENS,
        }
    }

    /// DeepSeek V4 Pro:推理 + 工具调用更稳定,适合法律论证/工具任务。
    /// `with_reasoning=true` 时切到 `deepseek-v4-pro-thinking`(开思考链)。
    pub fn pro(with_reasoning: bool) -> Self {
        Self {
            model: if with_reasoning {
                "deepseek-v4-pro-thinking".into()
            } else {
                "deepseek-v4-pro".into()
            },
            temperature: 0.15,
            max_tokens: MAX_OUTPUT_TOKENS,
        }
    }

    /// 把用户在 Settings 强制选定的 model 字符串包装成 ModelChoice。
    /// 不识别的 model 名透传(让 DeepSeek 自己报 400)。
    pub fn from_forced(model: &str) -> Self {
        let is_pro = model.contains("pro");
        Self {
            model: model.to_string(),
            temperature: if is_pro { 0.15 } else { 0.3 },
            max_tokens: MAX_OUTPUT_TOKENS,
        }
    }
}

/// 路由主入口。统一读 `settings.cloud_llm_model` 这一个「模型档位」字段。
pub fn route_model(task: TaskType, user_message: &str, settings: &Settings) -> ModelChoice {
    // 档位:默认 flash(便宜)。空字符串也当默认。
    let mode = settings
        .cloud_llm_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("deepseek-v4-flash");

    // 全局档(非 auto):所有任务都用这个模型,不再按任务强制 pro。
    if mode != "auto" {
        return ModelChoice::from_forced(mode);
    }

    // 自动挡(auto):按任务复杂度分流。
    match task {
        // 4 个工具/分析型 → pro(不开 reasoning,保持稳定 strict mode)
        TaskType::CompileLegalBasis
        | TaskType::FindSimilarCases
        | TaskType::VerifyMyDraft
        | TaskType::SimulateOpposition => ModelChoice::pro(false),
        // 自由问 → 启发式
        TaskType::FreeChat => route_free_chat(user_message),
    }
}

/// 启发式:短问(<30 字)或不带"推理类"关键词 → flash;否则 pro。
fn route_free_chat(msg: &str) -> ModelChoice {
    let chars = msg.chars().count();
    if chars < 30 {
        return ModelChoice::flash();
    }
    const REASONING_KEYWORDS: &[&str] = &[
        "建议",
        "分析",
        "为什么",
        "怎么办",
        "如何",
        "拒执",
        "风险",
        "怎么处理",
        "策略",
        "对比",
        "评估",
        "推理",
    ];
    if REASONING_KEYWORDS.iter().any(|k| msg.contains(k)) {
        ModelChoice::pro(false)
    } else {
        ModelChoice::flash()
    }
}

#[cfg(test)]
mod tests {
    //! 8+ 表驱动单测,覆盖路由表所有分支。
    use super::*;

    /// 设 `cloud_llm_model` 档位(None = 不配置,等同默认 flash)。
    fn make_settings(mode: Option<&str>) -> Settings {
        Settings {
            cloud_llm_model: mode.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn case_1_global_pro_all_tasks_pro() {
        let s = make_settings(Some("deepseek-v4-pro"));
        // 全局 Pro:连短问 FreeChat(默认会走 flash)也走 pro
        let c = route_model(TaskType::FreeChat, "随便问", &s);
        assert_eq!(c.model, "deepseek-v4-pro");
    }

    #[test]
    fn case_2_global_thinking_all_tasks_thinking() {
        let s = make_settings(Some("deepseek-v4-pro-thinking"));
        let c = route_model(TaskType::FreeChat, "", &s);
        assert_eq!(c.model, "deepseek-v4-pro-thinking");
    }

    #[test]
    fn case_3_default_and_global_flash_never_force_pro() {
        // 默认(None)= 全局 flash:**工具型任务也走 flash,绝不偷偷升 pro**(老板核心诉求:省钱)
        for s in [
            make_settings(None),
            make_settings(Some("deepseek-v4-flash")),
        ] {
            for task in [
                TaskType::CompileLegalBasis, // 工具型,以前强制 pro
                TaskType::SimulateOpposition,
                TaskType::FreeChat,
            ] {
                let c = route_model(task, "对方主张违约金过高怎么办有没有策略建议分析", &s);
                assert_eq!(
                    c.model, "deepseek-v4-flash",
                    "全局flash下 {:?} 必须 flash",
                    task
                );
            }
        }
    }

    #[test]
    fn case_5_auto_tool_tasks_route_to_pro() {
        let s = make_settings(Some("auto"));
        for task in [
            TaskType::CompileLegalBasis,
            TaskType::FindSimilarCases,
            TaskType::VerifyMyDraft,
        ] {
            let c = route_model(task, "", &s);
            assert_eq!(c.model, "deepseek-v4-pro", "auto挡 {:?} 应走 pro", task);
            assert_eq!(c.temperature, 0.15);
            assert_eq!(c.max_tokens, MAX_OUTPUT_TOKENS);
        }
    }

    #[test]
    fn case_6_auto_free_chat_short_message_goes_flash() {
        let s = make_settings(Some("auto"));
        // < 30 字
        let c = route_model(TaskType::FreeChat, "案号是多少", &s);
        assert_eq!(c.model, "deepseek-v4-flash");
    }

    #[test]
    fn case_7_auto_free_chat_long_with_reasoning_keyword_goes_pro() {
        let s = make_settings(Some("auto"));
        let msg = "这个案件对方主张违约金过高,我们应该怎么办?有没有应对策略?给一些建议";
        assert!(msg.chars().count() >= 30);
        let c = route_model(TaskType::FreeChat, msg, &s);
        assert_eq!(c.model, "deepseek-v4-pro");
        assert_eq!(c.temperature, 0.15);
    }

    #[test]
    fn case_8_auto_free_chat_long_without_reasoning_keyword_stays_flash() {
        let s = make_settings(Some("auto"));
        // 长但都是事实型描述,不含"建议/分析"等推理关键词
        let msg = "请把这个案件里张三和李四之间签订的合同内容列出来,我想看看具体条款约定了什么内容";
        assert!(msg.chars().count() >= 30);
        let c = route_model(TaskType::FreeChat, msg, &s);
        assert_eq!(c.model, "deepseek-v4-flash");
    }

    #[test]
    fn case_9_auto_free_chat_anti_enforcement_keyword_triggers_pro() {
        let s = make_settings(Some("auto"));
        // 拒执关键词最重要 — V0.2 老板的核心 use case
        let msg = "对方公司在立案后突击转移股权,涉嫌拒执罪,我们能不能追加刑事责任?";
        let c = route_model(TaskType::FreeChat, msg, &s);
        assert_eq!(c.model, "deepseek-v4-pro");
    }

    #[test]
    fn case_10_forced_unknown_model_passes_through() {
        let s = make_settings(Some("my-custom-model"));
        let c = route_model(TaskType::FreeChat, "", &s);
        assert_eq!(c.model, "my-custom-model");
    }
}
