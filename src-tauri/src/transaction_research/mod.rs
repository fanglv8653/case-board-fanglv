//! Transaction contract legal research MVP (FL-C2).
//!
//! The module reuses existing legal and local-KB tools, then returns a structured
//! research result without writing chat history or artifacts.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::chat::tools::{ToolContext, ToolRegistry, ToolResult};
use crate::llm::{LlmConfig, LlmError};
use crate::local_kb::cache::LocalKb;
use crate::settings::{read_settings, Settings};

const TOOL_OUTPUT_CHAR_BUDGET: usize = 3_500;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransactionLegalResearchInput {
    pub question: String,
    pub contract_name: Option<String>,
    pub contract_type: Option<String>,
    pub stance: Option<String>,
    pub risk_title: Option<String>,
    pub clause_ref: Option<String>,
    pub anchor_text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransactionResearchAuthority {
    pub authority_type: String,
    pub title: String,
    pub locator: String,
    pub snippet: String,
    pub relevance: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransactionResearchCitation {
    pub source_type: String,
    pub source_name: String,
    pub locator: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransactionResearchToolTrace {
    pub tool: String,
    pub success: bool,
    pub kb_hit: bool,
    pub credits_used: u32,
    pub error_short: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TransactionLegalResearchResponse {
    pub question: String,
    pub normalized_issue: String,
    pub scope_note: String,
    pub summary: String,
    pub authorities: Vec<TransactionResearchAuthority>,
    pub risk_analysis: Vec<String>,
    pub recommended_actions: Vec<String>,
    pub citations: Vec<TransactionResearchCitation>,
    pub tool_trace: Vec<TransactionResearchToolTrace>,
    pub follow_up_questions: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct LlmResearchSummary {
    normalized_issue: String,
    scope_note: String,
    summary: String,
    authorities: Vec<TransactionResearchAuthority>,
    risk_analysis: Vec<String>,
    recommended_actions: Vec<String>,
    citations: Vec<TransactionResearchCitation>,
    follow_up_questions: Vec<String>,
}

#[derive(Debug, Clone)]
struct EvidenceItem {
    source_type: String,
    title: String,
    locator: String,
    snippet: String,
    tool: String,
}

struct ToolRun {
    trace: TransactionResearchToolTrace,
    content: Option<String>,
    evidence: Vec<EvidenceItem>,
}

#[tauri::command]
pub async fn transaction_legal_research(
    pool: tauri::State<'_, SqlitePool>,
    input: TransactionLegalResearchInput,
) -> Result<TransactionLegalResearchResponse, String> {
    let settings = read_settings().unwrap_or_default();
    let question = build_question(&input);
    if question.trim().is_empty() {
        return Ok(TransactionLegalResearchResponse {
            question,
            normalized_issue: String::new(),
            scope_note: "当前没有可检索的问题。".to_string(),
            summary: "请先输入法律检索问题，或从某条风险点发起依据检索。".to_string(),
            authorities: Vec::new(),
            risk_analysis: Vec::new(),
            recommended_actions: Vec::new(),
            citations: Vec::new(),
            tool_trace: Vec::new(),
            follow_up_questions: vec![
                "请补充你要检索的具体条款或风险点。".to_string(),
                "如有明确合同类型，也请补充，例如：房屋租赁、股权转让、技术服务。".to_string(),
            ],
        });
    }

    if question.chars().count() < 8 && input.risk_title.as_deref().unwrap_or("").trim().is_empty() {
        return Ok(TransactionLegalResearchResponse {
            question: question.clone(),
            normalized_issue: question,
            scope_note: "问题过短，容易导致法律检索范围过宽。".to_string(),
            summary: "本轮未自动发起检索，请先补充合同类型、争议条款或我方立场。".to_string(),
            authorities: Vec::new(),
            risk_analysis: Vec::new(),
            recommended_actions: Vec::new(),
            citations: Vec::new(),
            tool_trace: Vec::new(),
            follow_up_questions: vec![
                "你想核查的是哪一类合同或哪一条风险点？".to_string(),
                "你更关心法条依据、监管规则，还是类案支持？".to_string(),
            ],
        });
    }

    let registry = ToolRegistry::default_v0_2();
    let local_kb = LocalKb::auto_detect(&settings);
    let ctx = ToolContext {
        pool: pool.inner(),
        settings: &settings,
        case_id: None,
        local_kb: local_kb.as_ref(),
        app: None,
    };

    let tool_runs = run_research_tools(&registry, &ctx, &question, &settings).await;
    let tool_trace = tool_runs.iter().map(|run| run.trace.clone()).collect::<Vec<_>>();
    let materials = tool_runs
        .iter()
        .filter_map(|run| run.content.as_ref().map(|content| (run.trace.tool.as_str(), content.as_str())))
        .map(|(tool, content)| format!("## 工具: {tool}\n{content}"))
        .collect::<Vec<_>>();

    if materials.is_empty() {
        let mut follow_up_questions = vec![
            "请确认是否已在设置页配置元典 API Key。".to_string(),
            "如果你有本地知识库，也请确认知识库目录是否已绑定并启用。".to_string(),
        ];
        if settings
            .yuandian_api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            follow_up_questions.insert(0, "当前未配置元典 API Key，是否先在设置页完成配置？".to_string());
        }
        return Ok(TransactionLegalResearchResponse {
            question: question.clone(),
            normalized_issue: question,
            scope_note: "本轮没有拿到可用检索结果。".to_string(),
            summary: "未检索到可用的法律材料。当前 MVP 不会回退到案件聊天主链，请先补足数据源配置或换一个更具体的问题。".to_string(),
            authorities: Vec::new(),
            risk_analysis: Vec::new(),
            recommended_actions: Vec::new(),
            citations: Vec::new(),
            tool_trace,
            follow_up_questions,
        });
    }

    let fact_pool = collect_evidence(&tool_runs);
    let summary = summarize_research(&settings, &input, &question, &materials).await?;
    let authorities = build_authorities(&summary, &fact_pool);
    let citations = authorities_to_citations(&authorities);

    Ok(TransactionLegalResearchResponse {
        question,
        normalized_issue: if summary.normalized_issue.trim().is_empty() {
            build_issue_title(&input)
        } else {
            summary.normalized_issue
        },
        scope_note: compose_scope_note(&summary.scope_note),
        summary: summary.summary,
        authorities,
        risk_analysis: summary.risk_analysis,
        recommended_actions: summary.recommended_actions,
        citations,
        tool_trace,
        follow_up_questions: summary.follow_up_questions,
    })
}

async fn run_research_tools(
    registry: &ToolRegistry,
    ctx: &ToolContext<'_>,
    question: &str,
    settings: &Settings,
) -> Vec<ToolRun> {
    let keyword = compact_keyword(question);
    let has_yuandian_key = settings
        .yuandian_api_key
        .as_deref()
        .map(str::trim)
        .is_some_and(|value| !value.is_empty());

    let mut runs = vec![run_tool(
        registry,
        "search_local_kb",
        json!({
            "keyword": keyword,
            "max_results": 5,
            "include_yuandian_cache": false
        }),
        ctx,
    )
    .await];

    if has_yuandian_key {
        runs.push(
            run_tool(
                registry,
                "law_vector_search",
                json!({"query": question, "top_k": 5}),
                ctx,
            )
            .await,
        );
        runs.push(
            run_tool(
                registry,
                "search_laws",
                json!({"keyword": keyword, "top_k": 5}),
                ctx,
            )
            .await,
        );
        runs.push(
            run_tool(
                registry,
                "search_regulations",
                json!({"keyword": keyword, "top_k": 5}),
                ctx,
            )
            .await,
        );
        runs.push(
            run_tool(
                registry,
                "case_vector_search",
                json!({"query": question, "top_k": 5}),
                ctx,
            )
            .await,
        );
        runs.push(
            run_tool(
                registry,
                "search_cases_authority",
                json!({"qw": keyword, "top_k": 5}),
                ctx,
            )
            .await,
        );
    }

    runs
}

async fn run_tool(
    registry: &ToolRegistry,
    name: &str,
    args: Value,
    ctx: &ToolContext<'_>,
) -> ToolRun {
    let Some(tool) = registry.find(name) else {
        return ToolRun {
            trace: TransactionResearchToolTrace {
                tool: name.to_string(),
                success: false,
                kb_hit: false,
                credits_used: 0,
                error_short: Some("工具未注册。".to_string()),
            },
            content: None,
            evidence: Vec::new(),
        };
    };

    match tool.execute(&args, ctx).await {
        Ok(ToolResult {
            content,
            yuandian_credits_used,
            kb_hit,
        }) => {
            let evidence = extract_evidence_from_tool(name, &content);
            let content = truncate_chars(&content, TOOL_OUTPUT_CHAR_BUDGET);
            ToolRun {
                trace: TransactionResearchToolTrace {
                    tool: name.to_string(),
                    success: true,
                    kb_hit,
                    credits_used: yuandian_credits_used,
                    error_short: None,
                },
                content: is_meaningful_tool_content(&content).then_some(content),
                evidence,
            }
        }
        Err(err) => ToolRun {
            trace: TransactionResearchToolTrace {
                tool: name.to_string(),
                success: false,
                kb_hit: false,
                credits_used: 0,
                error_short: Some(err.to_string()),
            },
            content: None,
            evidence: Vec::new(),
        },
    }
}

async fn summarize_research(
    settings: &Settings,
    input: &TransactionLegalResearchInput,
    question: &str,
    materials: &[String],
) -> Result<LlmResearchSummary, String> {
    let config = LlmConfig::from_settings(settings);
    if settings.effective_llm_provider() == "cloud"
        && config
            .api_key
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err("尚未配置云端 LLM API Key，请先在设置页完成配置。".to_string());
    }

    let contract_context = format!(
        "合同名称: {}\n合同类型: {}\n我方立场: {}\n风险标题: {}\n条款定位: {}\n原文片段: {}",
        input.contract_name.as_deref().unwrap_or("未提供"),
        input.contract_type.as_deref().unwrap_or("未提供"),
        normalize_stance_label(input.stance.as_deref()),
        input.risk_title.as_deref().unwrap_or("未提供"),
        input.clause_ref.as_deref().unwrap_or("未提供"),
        input.anchor_text.as_deref().unwrap_or("未提供"),
    );
    let joined_materials = materials.join("\n\n");
    let system_prompt = build_summary_system_prompt();
    let user_prompt = format!(
        "【用户问题】\n{question}\n\n【合同上下文】\n{contract_context}\n\n【检索材料】\n{joined_materials}"
    );

    let content = run_json_completion(&config, &system_prompt, &user_prompt)
        .await
        .map_err(|err| format!("合同法律检索总结失败: {err}"))?;
    let cleaned = extract_json_object(&content);
    serde_json::from_str::<LlmResearchSummary>(&cleaned)
        .map_err(|err| format!("合同法律检索结果不是有效 JSON: {err}"))
}

fn build_question(input: &TransactionLegalResearchInput) -> String {
    let explicit = normalize_whitespace(&input.question);
    if !explicit.is_empty() {
        return explicit;
    }

    let risk = normalize_whitespace(input.risk_title.as_deref().unwrap_or(""));
    let clause_ref = normalize_whitespace(input.clause_ref.as_deref().unwrap_or(""));
    let anchor_text = normalize_whitespace(input.anchor_text.as_deref().unwrap_or(""));
    let contract_type = normalize_whitespace(input.contract_type.as_deref().unwrap_or(""));
    let stance = normalize_stance_label(input.stance.as_deref());

    if !risk.is_empty() {
        let mut question = format!("请检索中国法下与“{risk}”相关的法条、监管规则和类案，并说明对{stance}的风险影响。");
        if !clause_ref.is_empty() {
            question.push_str(&format!("重点关注条款位置：{clause_ref}。"));
        }
        if !anchor_text.is_empty() {
            question.push_str(&format!("原文片段：{anchor_text}。"));
        }
        return question;
    }

    if !contract_type.is_empty() {
        return format!(
            "请围绕“{contract_type}”合同，检索中国法下常见高风险条款、可直接援引的法条和代表性类案。"
        );
    }

    String::new()
}

fn build_issue_title(input: &TransactionLegalResearchInput) -> String {
    let risk = normalize_whitespace(input.risk_title.as_deref().unwrap_or(""));
    if !risk.is_empty() {
        return risk;
    }
    let contract_type = normalize_whitespace(input.contract_type.as_deref().unwrap_or(""));
    if !contract_type.is_empty() {
        return contract_type;
    }
    normalize_whitespace(&input.question)
}

fn build_summary_system_prompt() -> String {
    r#"你是一名中国商事律师助手。现在你只能基于已给定的检索材料，输出一个 JSON 对象；不要输出 markdown 代码块，不要输出解释性前言。
输出字段必须是：
{
  "normalized_issue": "把用户问题归一成一句检索主题",
  "scope_note": "说明本轮检索覆盖到哪些材料，或有哪些边界",
  "summary": "3-5 句总结，先给结论，再讲依据边界",
  "authorities": [
    {
      "authority_type": "law | regulation | case | local_kb",
      "title": "来源标题",
      "locator": "条号/案号/文件路径/法规定位",
      "snippet": "只摘取对当前问题最相关的一小段",
      "relevance": "这条材料为什么与当前问题相关"
    }
  ],
  "risk_analysis": ["风险判断 1", "风险判断 2"],
  "recommended_actions": ["下一步建议 1", "下一步建议 2"],
  "citations": [
    {
      "source_type": "law | regulation | case | local_kb",
      "source_name": "来源名称",
      "locator": "定位"
    }
  ],
  "follow_up_questions": ["如证据不足或问题过宽，需要补问的 0-2 个问题"]
}

硬性要求：
1. 只能使用给定检索材料，不得编造不存在的法规名称、法条号、案号或裁判结论。
2. 如果材料不足，必须在 scope_note 和 summary 里明确写出边界。
3. authorities 最多 6 条，只保留最相关材料。
4. 如果没有可用材料，authorities / citations 返回空数组，recommended_actions 里明确提示补充检索。"#
        .to_string()
}

async fn run_json_completion(
    config: &LlmConfig,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String, LlmError> {
    let is_minimax = config.endpoint.contains("chatcompletion_v2");
    let mut body = serde_json::json!({
        "model": config.model,
        "messages": [
            {"role": "system", "content": system_prompt},
            {"role": "user", "content": user_prompt}
        ],
        "max_tokens": if is_minimax { 16384 } else { 8192 },
        "temperature": if is_minimax { 0.3 } else { 0.1 },
        "stream": false,
    });
    if !is_minimax {
        body["response_format"] = serde_json::json!({"type": "json_object"});
    }

    let mut req = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(config.timeout_secs * 2))
        .build()
        .map_err(|err| LlmError::Network(err.to_string()))?
        .post(&config.endpoint)
        .json(&body);
    if let Some(key) = &config.api_key {
        req = req.bearer_auth(key);
    }

    let response = req
        .send()
        .await
        .map_err(|err| LlmError::Network(err.to_string()))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(LlmError::HttpStatus(status.as_u16(), text));
    }

    let json: Value = response
        .json()
        .await
        .map_err(|err| LlmError::ResponseFormat(err.to_string()))?;
    json.get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .map(str::to_string)
        .ok_or_else(|| LlmError::ResponseFormat("missing choices[0].message.content".into()))
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
    if let Some(pos) = text.rfind("```") {
        text = text[..pos].trim();
    }
    if let (Some(start), Some(end)) = (text.find('{'), text.rfind('}')) {
        if end > start {
            return text[start..=end].to_string();
        }
    }
    text.to_string()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let count = value.chars().count();
    if count <= max_chars {
        return value.to_string();
    }
    let shortened: String = value.chars().take(max_chars).collect();
    format!("{shortened}...")
}

fn is_meaningful_tool_content(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == "[]" || trimmed == "{}" || trimmed == "null" {
        return false;
    }

    let compact: String = trimmed.chars().filter(|ch| !ch.is_whitespace()).collect();
    !matches!(
        compact.as_str(),
        "[]" | "{}" | "null" | r#"{"items":[]}"# | r#"{"data":[]}"# | r#"{"results":[]}"#
    )
}

fn compose_scope_note(raw: &str) -> String {
    let suffix = "字段边界：authorities 的 authority_type/title/locator/snippet 与 citations 仅来自已执行工具结果；authorities.relevance、summary、risk_analysis、recommended_actions、follow_up_questions 仍是基于这些材料的模型归纳。";
    let normalized = normalize_whitespace(raw);
    if normalized.is_empty() {
        suffix.to_string()
    } else if normalized.contains("字段边界：") {
        normalized
    } else {
        format!("{normalized} {suffix}")
    }
}

fn collect_evidence(runs: &[ToolRun]) -> Vec<EvidenceItem> {
    let mut items = Vec::new();
    for run in runs {
        for item in &run.evidence {
            if !items.iter().any(|existing| same_evidence(existing, item)) {
                items.push(item.clone());
            }
        }
    }
    items.sort_by_key(|item| (tool_priority(&item.tool), evidence_key(item)));
    items
}

fn build_authorities(
    summary: &LlmResearchSummary,
    evidence: &[EvidenceItem],
) -> Vec<TransactionResearchAuthority> {
    let mut authorities = Vec::new();
    let mut used_keys: Vec<String> = Vec::new();

    for wanted in &summary.authorities {
        if let Some(matched) = best_evidence_match(wanted, evidence, &used_keys) {
            used_keys.push(evidence_key(matched));
            authorities.push(fact_to_authority(
                matched,
                normalize_whitespace(&wanted.relevance),
            ));
        }
    }

    if authorities.is_empty() {
        for fact in evidence.iter().take(4) {
            used_keys.push(evidence_key(fact));
            authorities.push(fact_to_authority(fact, String::new()));
        }
    } else {
        for fact in evidence {
            let key = evidence_key(fact);
            if used_keys.iter().any(|used| used == &key) {
                continue;
            }
            used_keys.push(key);
            authorities.push(fact_to_authority(fact, String::new()));
            if authorities.len() >= 4 {
                break;
            }
        }
    }

    authorities.truncate(6);
    authorities
}

fn fact_to_authority(
    fact: &EvidenceItem,
    relevance: String,
) -> TransactionResearchAuthority {
    TransactionResearchAuthority {
        authority_type: fact.source_type.clone(),
        title: fact.title.clone(),
        locator: fact.locator.clone(),
        snippet: fact.snippet.clone(),
        relevance,
    }
}

fn authorities_to_citations(
    authorities: &[TransactionResearchAuthority],
) -> Vec<TransactionResearchCitation> {
    let mut citations: Vec<TransactionResearchCitation> = Vec::new();
    for authority in authorities {
        let citation = TransactionResearchCitation {
            source_type: authority.authority_type.clone(),
            source_name: authority.title.clone(),
            locator: authority.locator.clone(),
        };
        if !citations.iter().any(|existing| {
            normalize_source_type(&existing.source_type) == normalize_source_type(&citation.source_type)
                && normalized_key(&existing.source_name) == normalized_key(&citation.source_name)
                && normalized_key(&existing.locator) == normalized_key(&citation.locator)
        }) {
            citations.push(citation);
        }
    }
    citations
}

fn best_evidence_match<'a>(
    wanted: &TransactionResearchAuthority,
    evidence: &'a [EvidenceItem],
    used_keys: &[String],
) -> Option<&'a EvidenceItem> {
    let mut best: Option<(&EvidenceItem, i32)> = None;
    for fact in evidence {
        let key = evidence_key(fact);
        if used_keys.iter().any(|used| used == &key) {
            continue;
        }
        let score = authority_match_score(wanted, fact);
        if score < 4 {
            continue;
        }
        if best.as_ref().map(|(_, best_score)| score > *best_score).unwrap_or(true) {
            best = Some((fact, score));
        }
    }
    best.map(|(fact, _)| fact)
}

fn authority_match_score(wanted: &TransactionResearchAuthority, fact: &EvidenceItem) -> i32 {
    let wanted_type = normalize_source_type(&wanted.authority_type);
    let fact_type = normalize_source_type(&fact.source_type);
    let mut score = 0;

    if !wanted_type.is_empty() {
        if wanted_type == fact_type {
            score += 2;
        } else {
            score -= 1;
        }
    }

    let wanted_title = normalized_key(&wanted.title);
    let fact_title = normalized_key(&fact.title);
    if !wanted_title.is_empty() && !fact_title.is_empty() {
        if wanted_title == fact_title {
            score += 5;
        } else if contains_significant_fragment(&wanted_title, &fact_title) {
            score += 4;
        }
    }

    let wanted_locator = normalized_key(&wanted.locator);
    let fact_locator = normalized_key(&fact.locator);
    if !wanted_locator.is_empty() && !fact_locator.is_empty() {
        if wanted_locator == fact_locator {
            score += 4;
        } else if contains_significant_fragment(&wanted_locator, &fact_locator) {
            score += 3;
        }
    }

    let wanted_snippet = normalized_key(&wanted.snippet);
    let fact_snippet = normalized_key(&fact.snippet);
    if !wanted_snippet.is_empty()
        && !fact_snippet.is_empty()
        && contains_significant_fragment(&wanted_snippet, &fact_snippet)
    {
        score += 2;
    }

    score
}

fn evidence_key(fact: &EvidenceItem) -> String {
    format!(
        "{}|{}|{}",
        normalize_source_type(&fact.source_type),
        normalized_key(&fact.title),
        normalized_key(&fact.locator)
    )
}

fn same_evidence(left: &EvidenceItem, right: &EvidenceItem) -> bool {
    evidence_key(left) == evidence_key(right)
}

fn tool_priority(tool: &str) -> u8 {
    match tool {
        "search_laws" => 0,
        "search_regulations" => 1,
        "search_cases_authority" => 2,
        "search_local_kb" => 3,
        "law_vector_search" => 4,
        "case_vector_search" => 5,
        _ => 9,
    }
}

fn extract_evidence_from_tool(tool: &str, content: &str) -> Vec<EvidenceItem> {
    let Ok(value) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };

    match tool {
        "search_local_kb" => extract_local_kb_evidence(&value, tool),
        "search_laws" | "law_vector_search" => extract_law_evidence(&value, tool),
        "search_regulations" => extract_regulation_evidence(&value, tool),
        "search_cases_authority" | "case_vector_search" => extract_case_evidence(&value, tool),
        _ => Vec::new(),
    }
}

fn extract_local_kb_evidence(value: &Value, tool: &str) -> Vec<EvidenceItem> {
    let Some(items) = value.as_array() else {
        return Vec::new();
    };

    items
        .iter()
        .take(3)
        .filter_map(|item| {
            let path = pick_first_str(item, &["relative_path"])?;
            Some(EvidenceItem {
                source_type: "local_kb".to_string(),
                title: path.to_string(),
                locator: path.to_string(),
                snippet: normalize_whitespace(pick_first_str(item, &["snippet"]).unwrap_or("")),
                tool: tool.to_string(),
            })
        })
        .collect()
}

fn extract_law_evidence(value: &Value, tool: &str) -> Vec<EvidenceItem> {
    let Some(items) = value.get("data").and_then(|data| data.as_array()) else {
        return Vec::new();
    };

    items
        .iter()
        .take(3)
        .filter_map(|item| {
            let title = build_law_title(item)?;
            Some(EvidenceItem {
                source_type: "law".to_string(),
                title,
                locator: build_law_locator(item),
                snippet: normalize_whitespace(pick_first_str(item, &["content", "snippet"]).unwrap_or("")),
                tool: tool.to_string(),
            })
        })
        .collect()
}

fn extract_regulation_evidence(value: &Value, tool: &str) -> Vec<EvidenceItem> {
    let Some(items) = value.get("data").and_then(|data| data.as_array()) else {
        return Vec::new();
    };

    items
        .iter()
        .take(3)
        .filter_map(|item| {
            let title = pick_first_str(item, &["fgmc", "title"])?;
            Some(EvidenceItem {
                source_type: "regulation".to_string(),
                title: title.to_string(),
                locator: build_regulation_locator(item),
                snippet: normalize_whitespace(pick_first_str(item, &["content", "snippet"]).unwrap_or("")),
                tool: tool.to_string(),
            })
        })
        .collect()
}

fn extract_case_evidence(value: &Value, tool: &str) -> Vec<EvidenceItem> {
    let Some(items) = value.get("cases").and_then(|cases| cases.as_array()) else {
        return Vec::new();
    };

    items
        .iter()
        .take(3)
        .filter_map(|item| {
            let title = pick_first_str(item, &["title", "ah"])?;
            Some(EvidenceItem {
                source_type: "case".to_string(),
                title: title.to_string(),
                locator: build_case_locator(item),
                snippet: normalize_whitespace(pick_first_str(item, &["content", "snippet"]).unwrap_or("")),
                tool: tool.to_string(),
            })
        })
        .collect()
}

fn build_law_title(item: &Value) -> Option<String> {
    let law_name = pick_first_str(item, &["fgmc", "title"])?;
    let article = pick_first_str(item, &["ftnum", "ft_num", "tid", "sxx"])
        .map(article_label)
        .unwrap_or_default();
    if article.is_empty() {
        Some(law_name.to_string())
    } else {
        Some(format!("{law_name} {article}"))
    }
}

fn build_law_locator(item: &Value) -> String {
    let article = pick_first_str(item, &["ftnum", "ft_num", "tid", "sxx"])
        .map(article_label)
        .unwrap_or_default();
    let fgid = pick_first_str(item, &["fgid", "id"]).unwrap_or("");
    match (article.is_empty(), fgid.is_empty()) {
        (false, false) => format!("{article}; fgid={fgid}"),
        (false, true) => article,
        (true, false) => format!("fgid={fgid}"),
        (true, true) => String::new(),
    }
}

fn build_regulation_locator(item: &Value) -> String {
    if let Some(id) = pick_first_str(item, &["fgid", "id"]) {
        return format!("fgid={id}");
    }
    let effect_level = pick_first_str(item, &["effect_level"]).unwrap_or("");
    let implement_date = pick_first_str(item, &["ssrq", "implement_date"]).unwrap_or("");
    normalize_whitespace(&format!("{} {}", effect_level, implement_date))
}

fn build_case_locator(item: &Value) -> String {
    if let Some(case_no) = pick_first_str(item, &["ah"]) {
        return case_no.to_string();
    }
    normalize_whitespace(&format!(
        "{} {}",
        pick_first_str(item, &["jbdw"]).unwrap_or(""),
        pick_first_str(item, &["cprq"]).unwrap_or("")
    ))
}

fn normalize_source_type(value: &str) -> String {
    match value.trim().to_lowercase().as_str() {
        "law" => "law".to_string(),
        "regulation" => "regulation".to_string(),
        "case" => "case".to_string(),
        "local_kb" => "local_kb".to_string(),
        other => other.to_string(),
    }
}

fn pick_first_str<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(|field| field.as_str()))
        .map(str::trim)
        .filter(|field| !field.is_empty())
}

fn normalized_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || ('一' <= *ch && *ch <= '龥'))
        .collect::<String>()
        .to_lowercase()
}

fn contains_significant_fragment(left: &str, right: &str) -> bool {
    if left.is_empty() || right.is_empty() {
        return false;
    }
    let (shorter, longer) = if left.chars().count() <= right.chars().count() {
        (left, right)
    } else {
        (right, left)
    };
    shorter.chars().count() >= 4 && longer.contains(shorter)
}

fn article_label(raw: &str) -> String {
    let normalized = normalize_whitespace(raw);
    if normalized.is_empty() {
        String::new()
    } else if normalized.contains('条') {
        normalized
    } else {
        format!("第{normalized}条")
    }
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").trim().to_string()
}

fn compact_keyword(question: &str) -> String {
    let normalized = normalize_whitespace(question);
    let cleaned: String = normalized
        .chars()
        .filter(|ch| {
            !matches!(
                ch,
                '\n'
                    | '\r'
                    | '\t'
                    | '，'
                    | ','
                    | '、'
                    | '；'
                    | ';'
                    | '：'
                    | ':'
                    | '？'
                    | '?'
                    | '！'
                    | '!'
                    | '。'
                    | '('
                    | ')'
                    | '“'
                    | '”'
                    | '"'
                    | '\''
                    | '《'
                    | '》'
            )
        })
        .collect();
    let candidate = if cleaned.trim().is_empty() {
        normalized
    } else {
        cleaned
    };
    candidate.chars().take(28).collect()
}

fn normalize_stance_label(value: Option<&str>) -> String {
    match value.unwrap_or("").trim() {
        "party_a" | "甲方" => "甲方".to_string(),
        "party_b" | "乙方" => "乙方".to_string(),
        "neutral" | "中立" => "中立".to_string(),
        _ => "中立".to_string(),
    }
}
