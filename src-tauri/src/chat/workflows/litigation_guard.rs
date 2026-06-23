use crate::chat::citations::Citation;

use super::scene_config::litigation_analysis_scene;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LitigationStructureGuard {
    #[serde(default)]
    pub scene_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub missing_structures: Vec<String>,
    #[serde(default)]
    pub missing_citation_kinds: Vec<String>,
    #[serde(default)]
    pub missing_chat_sections: Vec<String>,
    #[serde(default)]
    pub fallback_note: Option<String>,
}

impl LitigationStructureGuard {
    pub fn is_passed(&self) -> bool {
        self.status == "passed"
    }
}

pub fn validate_litigation_output(
    content: &str,
    citations: &[Citation],
) -> Option<LitigationStructureGuard> {
    let scene = litigation_analysis_scene()?;
    let contract = scene.output_contract.as_ref()?;
    let content_trimmed = content.trim();
    if content_trimmed.is_empty() {
        return Some(LitigationStructureGuard {
            scene_id: scene.scene_id.clone(),
            status: "warned".to_string(),
            missing_structures: contract.required_structures.clone(),
            missing_citation_kinds: contract.citation_kinds.clone(),
            missing_chat_sections: contract.chat_sections.clone(),
            fallback_note: Some(
                "本轮保留普通聊天结果，请补齐四图结构与引用后再沉淀为正式诉讼分析。"
                    .to_string(),
            ),
        });
    }

    let missing_structures = contract
        .required_structures
        .iter()
        .filter(|name| !structure_present(content_trimmed, name))
        .cloned()
        .collect::<Vec<_>>();
    let missing_chat_sections = contract
        .chat_sections
        .iter()
        .filter(|name| !contains_token(content_trimmed, name))
        .cloned()
        .collect::<Vec<_>>();
    let missing_citation_kinds = contract
        .citation_kinds
        .iter()
        .filter(|kind| !citation_kind_present(citations, kind))
        .cloned()
        .collect::<Vec<_>>();
    let passed = missing_structures.is_empty()
        && missing_chat_sections.is_empty()
        && missing_citation_kinds.is_empty();

    Some(LitigationStructureGuard {
        scene_id: scene.scene_id.clone(),
        status: if passed {
            "passed".to_string()
        } else {
            "warned".to_string()
        },
        missing_structures,
        missing_citation_kinds,
        missing_chat_sections,
        fallback_note: if passed {
            None
        } else {
            Some(
                "本轮不阻断聊天与文书出口，但会按诉讼结构托底提示缺口，便于下一轮补齐。"
                    .to_string(),
            )
        },
    })
}

fn citation_kind_present(citations: &[Citation], expected: &str) -> bool {
    citations
        .iter()
        .any(|citation| citation.kind.trim().eq_ignore_ascii_case(expected.trim()))
}

fn structure_present(content: &str, structure: &str) -> bool {
    if contains_token(content, structure) {
        return true;
    }
    structure_aliases(structure)
        .iter()
        .any(|alias| contains_token(content, alias))
}

fn contains_token(content: &str, token: &str) -> bool {
    let token = token.trim();
    !token.is_empty() && content.contains(token)
}

fn structure_aliases(structure: &str) -> &'static [&'static str] {
    match structure.trim() {
        "issue_map" => &["争点", "争议焦点", "核心争点"],
        "evidence_map" => &["证据", "举证", "证据链", "证明力"],
        "law_map" => &["法条", "法律依据", "法律适用", "裁判依据"],
        "case_map" => &["类案", "案例", "裁判观点", "相似案例"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn citation(kind: &str) -> Citation {
        Citation {
            ref_num: 1,
            kind: kind.to_string(),
            source: "test".to_string(),
            quote: None,
            court: None,
            verified: true,
            tool_call_id: None,
        }
    }

    #[test]
    fn guard_passes_when_all_required_parts_exist() {
        let content = r#"
## 结论摘要
## 核心争点
- issue_map
## 证据与法条支撑
- evidence_map
- law_map
- case_map
## 风险/缺口
## 下一步动作
"#;
        let guard = validate_litigation_output(
            content,
            &[citation("doc"), citation("law"), citation("case")],
        )
        .expect("guard should be available");
        assert!(guard.is_passed());
    }

    #[test]
    fn guard_warns_when_required_parts_are_missing() {
        let content = "## 结论摘要\n普通回答";
        let guard = validate_litigation_output(content, &[citation("doc")])
            .expect("guard should be available");
        assert_eq!(guard.status, "warned");
        assert!(guard.missing_structures.contains(&"issue_map".to_string()));
        assert!(guard.missing_citation_kinds.contains(&"law".to_string()));
    }
}
