use std::sync::OnceLock;

use serde::Deserialize;

use crate::chat::context::TaskType;

const LITIGATION_ANALYSIS_JSON: &str = include_str!("litigation_analysis.json");
const LEGAL_RESEARCH_JSON: &str = include_str!("legal_research.json");
const CONTRACT_REVIEW_PLUS_JSON: &str = include_str!("contract_review_plus.json");

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WorkflowOutputContract {
    pub required_structures: Vec<String>,
    pub citation_kinds: Vec<String>,
    pub chat_sections: Vec<String>,
    pub artifact_title: String,
    pub artifact_sections: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WorkflowAskUserPolicy {
    pub must_pause_when: Vec<String>,
    pub max_rounds: usize,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct WorkflowScene {
    pub scene_id: String,
    pub label: String,
    pub preferred_task_types: Vec<String>,
    pub entry_points: Vec<String>,
    pub intent_signals: Vec<String>,
    pub route_priority: i32,
    pub default_task_type: Option<String>,
    pub editing_doc_preferred: bool,
    pub attached_docs_preferred: bool,
    pub allowed_tools: Vec<String>,
    pub optional_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
    pub output_modes: Vec<String>,
    pub output_contract: Option<WorkflowOutputContract>,
    pub ask_user_policy: Option<WorkflowAskUserPolicy>,
    pub fallback_policy: Vec<String>,
    pub prompt_preamble: Option<String>,
}

impl WorkflowScene {
    pub fn task_type_matches(&self, task: TaskType) -> bool {
        let Some(task_key) = task_key(task) else {
            return self
                .preferred_task_types
                .iter()
                .any(|item| item.trim() == "free_chat");
        };
        self.preferred_task_types
            .iter()
            .any(|item| item.trim() == task_key)
    }

    pub fn default_task_type(&self) -> Option<TaskType> {
        self.default_task_type
            .as_deref()
            .and_then(parse_task_type_strict)
    }

    pub fn is_valid(&self) -> bool {
        !self.scene_id.trim().is_empty() && !self.label.trim().is_empty()
    }
}

#[derive(Debug)]
pub struct LoadedScenes {
    pub scenes: Vec<WorkflowScene>,
    pub warnings: Vec<String>,
}

static BUILTIN_SCENES: OnceLock<Result<LoadedScenes, String>> = OnceLock::new();

pub fn load_builtin_scene_configs() -> Result<&'static LoadedScenes, String> {
    match BUILTIN_SCENES.get_or_init(parse_builtin_scene_configs) {
        Ok(loaded) => Ok(loaded),
        Err(err) => Err(err.clone()),
    }
}

pub fn litigation_analysis_scene() -> Option<&'static WorkflowScene> {
    load_builtin_scene_configs().ok().and_then(|loaded| {
        loaded
            .scenes
            .iter()
            .find(|scene| scene.scene_id == "litigation_analysis")
    })
}

fn parse_builtin_scene_configs() -> Result<LoadedScenes, String> {
    let mut scenes = Vec::new();
    let mut warnings = Vec::new();

    for (file_name, raw) in [
        ("litigation_analysis.json", LITIGATION_ANALYSIS_JSON),
        ("legal_research.json", LEGAL_RESEARCH_JSON),
        ("contract_review_plus.json", CONTRACT_REVIEW_PLUS_JSON),
    ] {
        match serde_json::from_str::<WorkflowScene>(raw) {
            Ok(scene) if scene.is_valid() => scenes.push(scene),
            Ok(_) => warnings.push(format!("{} 缺少 scene_id 或 label，已跳过", file_name)),
            Err(err) => warnings.push(format!("{} 解析失败: {}", file_name, err)),
        }
    }

    if scenes.is_empty() {
        return Err(format!(
            "方律场景配置全部不可用: {}",
            warnings.join(" | ")
        ));
    }

    scenes.sort_by(|left, right| {
        right
            .route_priority
            .cmp(&left.route_priority)
            .then_with(|| left.scene_id.cmp(&right.scene_id))
    });

    Ok(LoadedScenes { scenes, warnings })
}

pub fn parse_task_type_strict(raw: &str) -> Option<TaskType> {
    match raw.trim() {
        "free_chat" => Some(TaskType::FreeChat),
        "compile_legal_basis" => Some(TaskType::CompileLegalBasis),
        "find_similar_cases" => Some(TaskType::FindSimilarCases),
        "verify_my_draft" => Some(TaskType::VerifyMyDraft),
        "simulate_opposition" => Some(TaskType::SimulateOpposition),
        "deep_analysis" => Some(TaskType::DeepAnalysis),
        "criminal_deep_analysis" => Some(TaskType::CriminalDeepAnalysis),
        _ => None,
    }
}

pub fn task_key(task: TaskType) -> Option<&'static str> {
    task.as_db_str()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_scenes_load() {
        let loaded = load_builtin_scene_configs().expect("builtin scenes should load");
        assert!(loaded.scenes.len() >= 3);
        assert!(loaded
            .scenes
            .iter()
            .any(|scene| scene.scene_id == "litigation_analysis"));
        assert!(loaded
            .scenes
            .iter()
            .any(|scene| scene.scene_id == "legal_research"));
        assert!(loaded
            .scenes
            .iter()
            .any(|scene| scene.scene_id == "contract_review_plus"));
    }

    #[test]
    fn strict_task_type_parser_rejects_unknown_values() {
        assert_eq!(
            parse_task_type_strict("compile_legal_basis"),
            Some(TaskType::CompileLegalBasis)
        );
        assert_eq!(parse_task_type_strict("free_chat"), Some(TaskType::FreeChat));
        assert_eq!(parse_task_type_strict("unknown"), None);
    }
}
