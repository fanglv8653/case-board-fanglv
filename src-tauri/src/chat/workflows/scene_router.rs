use crate::chat::context::TaskType;

use super::scene_config::WorkflowScene;

#[derive(Debug, Clone)]
pub struct SceneRouteInput<'a> {
    pub task: TaskType,
    pub user_message: &'a str,
    pub attached_doc_ids: &'a [String],
    pub editing_doc_id: Option<&'a str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneSource {
    WorkflowConfig,
}

#[derive(Debug, Clone)]
pub struct ResolvedScene {
    pub scene_id: String,
    pub label: String,
    pub source: SceneSource,
    pub effective_task_type: Option<TaskType>,
    pub allowed_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
    pub prompt_preamble: Option<String>,
    pub output_contract: Option<super::scene_config::WorkflowOutputContract>,
    pub ask_user_policy: Option<super::scene_config::WorkflowAskUserPolicy>,
    pub optional_tools: Vec<String>,
    pub output_modes: Vec<String>,
    pub fallback_policy: Vec<String>,
    pub preferred_task_types: Vec<String>,
    pub entry_points: Vec<String>,
}

pub fn resolve_scene(
    configs: &[WorkflowScene],
    input: &SceneRouteInput<'_>,
) -> Option<ResolvedScene> {
    let user_message_lower = input.user_message.to_lowercase();

    configs
        .iter()
        .filter_map(|scene| {
            score_scene(scene, input, &user_message_lower).map(|score| (score, scene))
        })
        .max_by(|(left_score, left_scene), (right_score, right_scene)| {
            left_score
                .cmp(right_score)
                .then_with(|| left_scene.route_priority.cmp(&right_scene.route_priority))
        })
        .map(|(_, scene)| ResolvedScene {
            scene_id: scene.scene_id.clone(),
            label: scene.label.clone(),
            source: SceneSource::WorkflowConfig,
            effective_task_type: scene.default_task_type(),
            allowed_tools: scene.allowed_tools.clone(),
            blocked_tools: scene.blocked_tools.clone(),
            prompt_preamble: scene.prompt_preamble.clone(),
            output_contract: scene.output_contract.clone(),
            ask_user_policy: scene.ask_user_policy.clone(),
            optional_tools: scene.optional_tools.clone(),
            output_modes: scene.output_modes.clone(),
            fallback_policy: scene.fallback_policy.clone(),
            preferred_task_types: scene.preferred_task_types.clone(),
            entry_points: scene.entry_points.clone(),
        })
}

fn score_scene(
    scene: &WorkflowScene,
    input: &SceneRouteInput<'_>,
    user_message_lower: &str,
) -> Option<i32> {
    let mut matched = false;
    let mut score = scene.route_priority * 10;

    if input.task != TaskType::FreeChat && scene.task_type_matches(input.task) {
        matched = true;
        score += 1_000;
    }

    if scene.editing_doc_preferred && input.editing_doc_id.is_some() {
        matched = true;
        score += 200;
    }

    if scene.attached_docs_preferred && !input.attached_doc_ids.is_empty() {
        matched = true;
        score += 80;
    }

    let intent_hits = scene
        .intent_signals
        .iter()
        .filter(|signal| {
            let normalized = signal.trim().to_lowercase();
            !normalized.is_empty() && user_message_lower.contains(&normalized)
        })
        .count() as i32;
    if intent_hits > 0 {
        matched = true;
        score += intent_hits * 40;
    }

    matched.then_some(score)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::context::TaskType;
    use crate::chat::workflows::load_builtin_scene_configs;

    #[test]
    fn contract_review_prefers_editing_mode() {
        let loaded = load_builtin_scene_configs().expect("builtin scenes should load");
        let attached = vec!["doc-1".to_string()];
        let route = SceneRouteInput {
            task: TaskType::FreeChat,
            user_message: "请帮我审一下这份合同草稿的付款和违约责任条款",
            attached_doc_ids: &attached,
            editing_doc_id: Some("draft-1"),
        };
        let resolved = resolve_scene(&loaded.scenes, &route).expect("scene should resolve");
        assert_eq!(resolved.scene_id, "contract_review_plus");
    }

    #[test]
    fn legal_research_matches_keyword_query() {
        let loaded = load_builtin_scene_configs().expect("builtin scenes should load");
        let empty: Vec<String> = Vec::new();
        let route = SceneRouteInput {
            task: TaskType::FreeChat,
            user_message: "请检索保证合同中关于保证期间的法律依据和司法解释",
            attached_doc_ids: &empty,
            editing_doc_id: None,
        };
        let resolved = resolve_scene(&loaded.scenes, &route).expect("scene should resolve");
        assert_eq!(resolved.scene_id, "legal_research");
    }

    #[test]
    fn plain_free_chat_without_other_signals_does_not_route() {
        let loaded = load_builtin_scene_configs().expect("builtin scenes should load");
        let empty: Vec<String> = Vec::new();
        let route = SceneRouteInput {
            task: TaskType::FreeChat,
            user_message: "hello there",
            attached_doc_ids: &empty,
            editing_doc_id: None,
        };
        let resolved = resolve_scene(&loaded.scenes, &route);
        assert!(resolved.is_none());
    }
}
