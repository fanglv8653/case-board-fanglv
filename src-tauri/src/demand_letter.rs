//! 非诉律师函的精简原生工作流：核心录入 → AI 工作稿 → 律师复核 → DOCX。
//! 只负责起草和导出，不发送、不写飞书，也不承载民事/刑事诉讼文书。

use serde::{Deserialize, Serialize};

use crate::llm::LlmConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandLetterInput {
    pub letter_type: String,
    pub sender: String,
    pub recipient: String,
    pub relationship: String,
    pub facts: String,
    pub demands: String,
    pub deadline: String,
    pub tone: String,
    #[serde(default)]
    pub evidence_note: String,
    #[serde(default)]
    pub legal_basis_note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DemandLetterDraft {
    pub title: String,
    pub draft_md: String,
    #[serde(default)]
    pub missing_items: Vec<String>,
    #[serde(default)]
    pub risk_notes: Vec<String>,
    /// 后端固定返回 draft；只有通过导出门禁才能形成正式稿。
    #[serde(default = "draft_status")]
    pub review_status: String,
}

fn draft_status() -> String {
    "draft".into()
}

fn validate_input(input: &DemandLetterInput) -> Result<(), String> {
    let required = [
        ("发函方", input.sender.trim()),
        ("收函方", input.recipient.trim()),
        ("基本事实", input.facts.trim()),
        ("具体要求", input.demands.trim()),
    ];
    let missing: Vec<&str> = required
        .into_iter()
        .filter_map(|(label, value)| value.is_empty().then_some(label))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!("请先补充：{}", missing.join("、")))
    }
}

#[tauri::command]
pub async fn generate_demand_letter(input: DemandLetterInput) -> Result<DemandLetterDraft, String> {
    validate_input(&input)?;
    let settings = crate::settings::read_settings().unwrap_or_default();
    let config = LlmConfig::from_settings(&settings);
    if settings.effective_llm_provider() == "cloud"
        && config
            .api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err("尚未配置云端 LLM API Key，请先在设置中完成配置。".into());
    }

    let system = r#"你是一名中国大陆执业律师的文书辅助工具。请根据用户提供的信息起草一份律师函工作稿，并且只输出一个 JSON 对象，不要输出 markdown 代码围栏。
JSON 字段必须是：
{"title":"律师函标题","draft_md":"律师函正文 Markdown","missing_items":["仍需核实的事实或材料"],"risk_notes":["发出前需律师注意的风险"]}

硬性规则：
1. 只处理非诉函件，不生成起诉状、刑事法律意见书或其他诉讼文书。
2. 严格区分用户陈述、材料可证事实、法律依据和分析判断；缺失信息使用【待补充】或列入 missing_items，不得编造。
3. 不得编造法条、案号、金额、日期、主体或送达事实。未经用户明确提供或核验的法源标记【法源待核验】。
4. 正文保持克制、专业、可执行，写清事实、要求、履行期限和逾期后果；不得使用侮辱、威胁或确定性定罪措辞。
5. 不要写“已发送”。不得把工作稿描述为律师已审核或可直接发出。
6. 若内容涉及承认债务、和解让步、时效、管辖/仲裁、证据保全或重大不可逆后果，必须写入 risk_notes。"#;
    let user = format!(
        "函件类型：{}\n发函方：{}\n收函方：{}\n双方关系：{}\n基本事实：{}\n具体要求：{}\n履行期限：{}\n语气：{}\n证据/附件：{}\n用户提供的法源说明：{}",
        input.letter_type,
        input.sender,
        input.recipient,
        input.relationship,
        input.facts,
        input.demands,
        input.deadline,
        input.tone,
        input.evidence_note,
        input.legal_basis_note,
    );
    let content = run_json_completion(&config, system, &user).await?;
    let cleaned = extract_json_object(&content);
    let mut draft: DemandLetterDraft = serde_json::from_str(&cleaned)
        .map_err(|error| format!("律师函工作稿不是有效 JSON：{}", error))?;
    draft.review_status = "draft".into();
    Ok(draft)
}

#[tauri::command]
pub async fn export_demand_letter_docx(
    title: String,
    draft_md: String,
    document_status: String,
    facts_confirmed: bool,
    sources_verified: bool,
    lawyer_confirmed: bool,
    save_path: String,
) -> Result<String, String> {
    let final_doc = document_status.trim() == "final";
    if final_doc && !(facts_confirmed && sources_verified && lawyer_confirmed) {
        return Err("正式稿导出被拦截：请先完成事实、法源和执业律师三项复核。".into());
    }
    let body = if final_doc {
        draft_md
    } else {
        format!(
            "> **工作稿：待执业律师复核，不得直接对外发送。**\n\n{}",
            draft_md
        )
    };
    let safe_title = if title.trim().is_empty() {
        "律师函"
    } else {
        title.trim()
    };
    let bytes = crate::docx_filing::build_filing_docx_bytes(safe_title, &body)?;
    std::fs::write(&save_path, bytes).map_err(|error| format!("写律师函 DOCX 失败：{}", error))?;
    Ok(save_path)
}

async fn run_json_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, String> {
    let is_minimax = config.endpoint.contains("chatcompletion_v2");
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt},
        ],
        "max_tokens": if is_minimax { 16384 } else { 8192 },
        "temperature": config.temperature,
        "stream": false,
    });
    if !is_minimax {
        body["response_format"] = serde_json::json!({"type": "json_object"});
    }
    let mut request = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs * 3))
        .build()
        .map_err(|error| format!("创建律师函请求失败：{}", error))?
        .post(&config.endpoint)
        .json(&body);
    if let Some(key) = &config.api_key {
        request = request.bearer_auth(key);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("律师函起草网络失败：{}", error))?;
    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("律师函起草服务返回 HTTP {}：{}", status, detail));
    }
    let json: serde_json::Value = response
        .json()
        .await
        .map_err(|error| format!("律师函起草响应格式错误：{}", error))?;
    json.get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::to_string)
        .ok_or_else(|| "律师函起草响应缺少 choices[0].message.content".into())
}

fn extract_json_object(content: &str) -> String {
    let mut text = content.trim();
    if let Some(end) = text.find("</think>") {
        text = text[end + "</think>".len()..].trim();
    }
    if let Some(rest) = text.strip_prefix("```json") {
        text = rest.trim();
    } else if let Some(rest) = text.strip_prefix("```") {
        text = rest.trim();
    }
    if let Some(end) = text.rfind("```") {
        text = text[..end].trim();
    }
    match (text.find('{'), text.rfind('}')) {
        (Some(start), Some(end)) if end > start => text[start..=end].to_string(),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_intake_is_required_but_optional_fields_are_not() {
        let mut input = DemandLetterInput {
            letter_type: "履行催告".into(),
            sender: "甲公司".into(),
            recipient: "乙公司".into(),
            relationship: String::new(),
            facts: "双方签订合同".into(),
            demands: "限期履行".into(),
            deadline: String::new(),
            tone: "克制".into(),
            evidence_note: String::new(),
            legal_basis_note: String::new(),
        };
        assert!(validate_input(&input).is_ok());
        input.facts.clear();
        assert!(validate_input(&input).unwrap_err().contains("基本事实"));
    }
}
