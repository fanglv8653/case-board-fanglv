use crate::chat::context::TaskType;
use crate::settings::Settings;

use super::scene_config::load_builtin_scene_configs;
use super::scene_router::{resolve_scene, ResolvedScene, SceneRouteInput};

#[derive(Debug, Clone)]
pub enum ScenePlanSource {
    Default,
    Routed { scene_id: String },
    Fallback { reason: &'static str },
}

#[derive(Debug, Clone)]
pub struct SceneExecutionPlan {
    pub effective_task_type: TaskType,
    pub prompt_preamble: Option<String>,
    pub allowed_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
    pub source: ScenePlanSource,
    pub warnings: Vec<String>,
}

impl SceneExecutionPlan {
    pub fn default(task: TaskType) -> Self {
        Self {
            effective_task_type: task,
            prompt_preamble: None,
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            source: ScenePlanSource::Default,
            warnings: Vec::new(),
        }
    }

    pub fn fallback(task: TaskType, reason: &'static str, warnings: Vec<String>) -> Self {
        Self {
            effective_task_type: task,
            prompt_preamble: None,
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            source: ScenePlanSource::Fallback { reason },
            warnings,
        }
    }

    pub fn has_tool_filters(&self) -> bool {
        !self.allowed_tools.is_empty() || !self.blocked_tools.is_empty()
    }

    pub fn strategy_label(&self) -> String {
        match &self.source {
            ScenePlanSource::Default => "agent-loop".to_string(),
            ScenePlanSource::Routed { scene_id } => format!("agent-loop/fanglv:{scene_id}"),
            ScenePlanSource::Fallback { reason } => format!("agent-loop/fallback:{reason}"),
        }
    }

    pub fn with_runtime_fallback(&self, fallback_task: TaskType, reason: &'static str) -> Self {
        Self {
            effective_task_type: fallback_task,
            prompt_preamble: None,
            allowed_tools: Vec::new(),
            blocked_tools: Vec::new(),
            source: ScenePlanSource::Fallback { reason },
            warnings: self.warnings.clone(),
        }
    }

    pub fn apply_system_prompt(&self, base_prompt: String) -> String {
        match self.prompt_preamble.as_deref().map(str::trim) {
            Some(extra) if !extra.is_empty() => {
                format!("{base_prompt}\n\n【方律场景约束】\n{extra}")
            }
            _ => base_prompt,
        }
    }
}

pub fn build_scene_execution_plan(
    settings: &Settings,
    route_input: SceneRouteInput<'_>,
) -> SceneExecutionPlan {
    if !settings.fanglv_router_enabled() {
        return SceneExecutionPlan::default(route_input.task);
    }

    let loaded = match load_builtin_scene_configs() {
        Ok(loaded) => loaded,
        Err(err) => {
            return SceneExecutionPlan::fallback(
                route_input.task,
                "fanglv-config",
                vec![format!("方律场景配置加载失败: {err}")],
            );
        }
    };

    if let Some(resolved) = resolve_scene(&loaded.scenes, &route_input) {
        let mut plan = plan_from_scene(route_input.task, resolved);
        plan.warnings = loaded.warnings.clone();
        return plan;
    }

    let mut plan = SceneExecutionPlan::default(route_input.task);
    plan.warnings = loaded.warnings.clone();
    plan
}

fn plan_from_scene(original_task: TaskType, scene: ResolvedScene) -> SceneExecutionPlan {
    let effective_task_type = scene.effective_task_type.unwrap_or(original_task);
    let prompt_preamble = build_prompt_preamble(&scene, effective_task_type);
    let allowed_tools = merge_tool_names(&scene.allowed_tools, &scene.optional_tools);

    SceneExecutionPlan {
        effective_task_type,
        prompt_preamble,
        allowed_tools,
        blocked_tools: scene.blocked_tools,
        source: ScenePlanSource::Routed {
            scene_id: scene.scene_id,
        },
        warnings: Vec::new(),
    }
}

fn build_prompt_preamble(scene: &ResolvedScene, task: TaskType) -> Option<String> {
    let mut blocks = Vec::new();

    if let Some(extra) = scene.prompt_preamble.as_deref().map(str::trim) {
        if !extra.is_empty() {
            blocks.push(extra.to_string());
        }
    }

    if let Some(contract) = render_scene_contract(scene, task) {
        blocks.push(contract);
    }

    if blocks.is_empty() {
        None
    } else {
        Some(blocks.join("\n\n"))
    }
}

fn render_scene_contract(scene: &ResolvedScene, task: TaskType) -> Option<String> {
    let output_contract = scene.output_contract.as_ref()?;
    let ask_user_policy = scene.ask_user_policy.as_ref();
    let structure_lines = render_bullets(&output_contract.required_structures, |name| {
        format!("- `{name}`")
    });
    let chat_sections = render_ordered(&output_contract.chat_sections);
    let artifact_sections = render_ordered(&output_contract.artifact_sections);
    let allowed_tools = render_bullets(&scene.allowed_tools, |name| format!("- `{name}`"));
    let optional_tools = render_bullets(&scene.optional_tools, |name| format!("- `{name}`"));
    let blocked_tools = render_bullets(&scene.blocked_tools, |name| format!("- `{name}`"));
    let fallback_rules = render_bullets(&scene.fallback_policy, |rule| format!("- {rule}"));
    let preferred_tasks = render_inline_code_list(&scene.preferred_task_types);
    let entry_points = render_inline_code_list(&scene.entry_points);
    let output_modes = render_inline_code_list(&scene.output_modes);
    let citation_kinds = render_inline_code_list(&output_contract.citation_kinds);
    let pause_rules = ask_user_policy
        .map(|policy| render_bullets(&policy.must_pause_when, |rule| format!("- {rule}")))
        .filter(|text| !text.is_empty())
        .unwrap_or_else(|| "- 无额外追问约束".to_string());
    let max_rounds = ask_user_policy
        .map(|policy| policy.max_rounds)
        .filter(|value| *value > 0)
        .unwrap_or(1);

    Some(format!(
        "【场景约束｜{label}｜{scene_id}】\n\
         - 场景来源: {:?}\n\
         - 入口来源: {entry_points}\n\
         - 既有任务入口: {preferred_tasks}\n\
         - 允许输出模式: {output_modes}\n\
         \n\
         【本任务重点】\n\
         {task_focus}\n\
         \n\
         【结构化输出硬约束】\n\
         {structure_lines}\n\
         - `issue_map` 是主索引: 每个争点都要回挂证据、法条、类案或明确缺口。\n\
         - `evidence_map` 要说明证据事实、定位片段、支持争点和证明力风险。\n\
         - `law_map` 要说明法条名称、条号、用途和对应争点。\n\
         - `case_map` 要说明案号、法院、支持度判断、相似点和差异点。\n\
         \n\
         【引用约束】\n\
         - 本场景允许的引用类型: {citation_kinds}\n\
         - 事实结论要挂 `doc` 引用, 法律结论要挂 `law` 引用, 类案判断要挂 `case` 引用。\n\
         - 若只是分析判断,必须显式标注“分析判断”,不要伪装成已查证事实。\n\
         \n\
         【工具使用边界】\n\
         优先工具:\n\
         {allowed_tools}\n\
         可选补充:\n\
         {optional_tools}\n\
         当前场景禁用:\n\
         {blocked_tools}\n\
         \n\
         【追问边界】\n\
         {pause_rules}\n\
         - 最多追问 {max_rounds} 轮; 若已能形成阶段性结论,应直接输出结构化结果。\n\
         \n\
         【聊天输出顺序】\n\
         {chat_sections}\n\
         \n\
         【文书出口】\n\
         - 继续复用现有 `save_artifact` / `edit_artifact` 出口。\n\
         - 若需要落报告,标题优先使用《{artifact_title}》。\n\
         - 报告目录建议:\n\
         {artifact_sections}\n\
         \n\
         【回退规则】\n\
         {fallback_rules}",
        scene.source,
        label = scene.label.as_str(),
        scene_id = scene.scene_id.as_str(),
        entry_points = entry_points,
        preferred_tasks = preferred_tasks,
        output_modes = output_modes,
        task_focus = task_focus_hint(task),
        structure_lines = structure_lines,
        citation_kinds = citation_kinds,
        allowed_tools = allowed_tools,
        optional_tools = optional_tools,
        blocked_tools = blocked_tools,
        pause_rules = pause_rules,
        max_rounds = max_rounds,
        chat_sections = chat_sections,
        artifact_title = output_contract.artifact_title.as_str(),
        artifact_sections = artifact_sections,
        fallback_rules = fallback_rules,
    ))
}

fn render_bullets<T>(items: &[T], map: impl Fn(&T) -> String) -> String {
    let rows: Vec<String> = items.iter().map(map).collect();
    if rows.is_empty() {
        "- 无".to_string()
    } else {
        rows.join("\n")
    }
}

fn render_ordered(items: &[String]) -> String {
    if items.is_empty() {
        "1. 无".to_string()
    } else {
        items
            .iter()
            .enumerate()
            .map(|(idx, item)| format!("{}. {}", idx + 1, item))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn render_inline_code_list(items: &[String]) -> String {
    if items.is_empty() {
        "无".to_string()
    } else {
        items
            .iter()
            .map(|item| format!("`{item}`"))
            .collect::<Vec<_>>()
            .join(" / ")
    }
}

fn merge_tool_names(primary: &[String], extra: &[String]) -> Vec<String> {
    let mut merged = Vec::with_capacity(primary.len() + extra.len());
    for name in primary.iter().chain(extra.iter()) {
        let trimmed = name.trim();
        if trimmed.is_empty() || merged.iter().any(|existing| existing == trimmed) {
            continue;
        }
        merged.push(trimmed.to_string());
    }
    merged
}

fn task_focus_hint(task: TaskType) -> &'static str {
    match task {
        TaskType::CompileLegalBasis => {
            "- 当前入口偏向法律依据整理,但仍要把法条结论回挂到争点和证据缺口。"
        }
        TaskType::FindSimilarCases => {
            "- 当前入口偏向类案支持度分析,不能只给类案列表,必须说明支持点和差异点。"
        }
        TaskType::VerifyMyDraft => {
            "- 当前入口偏向草稿核校,优先指出风险条款、引用瑕疵和建议改法。"
        }
        TaskType::SimulateOpposition => {
            "- 当前入口偏向攻防推演,每个争点都要同时写对方主张和我方回应。"
        }
        TaskType::DeepAnalysis | TaskType::CriminalDeepAnalysis => {
            "- 当前入口偏向完整分析,应尽量把结构表和阶段性结论展开到可继续编辑。"
        }
        TaskType::FreeChat => "- 当前入口来自自由聊天,但已命中方律场景,输出仍需遵守场景约束。",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn litigation_scene_builds_prompt_contract() {
        let empty: Vec<String> = Vec::new();
        let route_input = SceneRouteInput {
            task: TaskType::DeepAnalysis,
            user_message: "请做一份完整诉讼分析",
            attached_doc_ids: &empty,
            editing_doc_id: None,
        };
        let plan = build_scene_execution_plan(
            &Settings {
                enable_fanglv_router: true,
                ..Default::default()
            },
            route_input,
        );
        assert!(matches!(plan.source, ScenePlanSource::Routed { .. }));
        let prompt = plan
            .prompt_preamble
            .expect("litigation plan should inject prompt");
        assert!(prompt.contains("issue_map"));
        assert!(prompt.contains("save_artifact"));
    }

    #[test]
    fn optional_tools_are_exposed_to_runtime_filter() {
        let empty: Vec<String> = Vec::new();
        let route_input = SceneRouteInput {
            task: TaskType::DeepAnalysis,
            user_message: "请做一份完整诉讼分析",
            attached_doc_ids: &empty,
            editing_doc_id: None,
        };
        let plan = build_scene_execution_plan(
            &Settings {
                enable_fanglv_router: true,
                ..Default::default()
            },
            route_input,
        );
        assert!(plan.allowed_tools.contains(&"search_local_kb".to_string()));
        assert!(plan.allowed_tools.contains(&"semantic_search_local_kb".to_string()));
    }
}
