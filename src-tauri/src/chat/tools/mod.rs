//! 案件 AI 助手 V2 的工具集合(V0.2 D2-D3)。
//!
//! 27 个 tool 分 10 类(详 docs/V0.2-法律AI工作台-实施计划.md § 5):
//!   - 法规法条 5 (laws.rs)
//!   - 案例 4 (cases.rs)
//!   - 企业 6 (companies.rs)
//!   - 幻觉校验 1 (verify.rs · hall_detect,不缓存)
//!   - 案件文档 4 (docs.rs · sqlite + 案件 extracted_text_path;semantic.rs · 向量语义检索)
//!   - 本地知识库 2 (kb.rs · `~/Documents/知识库/` 整库)
//!   - 写作工具 2 (artifact.rs · save_artifact 文书生产 + edit_artifact 局部编辑,均 mutating)
//!   - 交互工具 1 (ask_user.rs · 选项式追问,agent_loop 拦截不进派发)
//!   - 文档维护 1 (reextract.rs · reextract_document,V0.3 触发后台重抽,mutating)
//!   - 入库工具 1 (save_kb.rs · save_company_report,企业报告入库 raw/companies/,P2,mutating)
//!
//! 调用方:`chat::agent_loop`(D3-D4 实施)拿到 LLM 的 function_call,
//! 用 `ToolRegistry::find(name)` 查到 tool,调 `execute(args, ctx)`,
//! 把 `ToolResult.content` 回填到 LLM 的 messages。
//!
//! 三段式(本类 15 个走 KB cache 的工具):
//!   1. 调 `LocalKb::check_cache` 看本地有无命中
//!   2. miss → 调元典 API
//!   3. API 成功 → 调 `LocalKb::save_search` / `save_detail` 写回 KB
//!
//! 不走 cache 的 6 个:`verify_legal_citations`(实时校验)、`list_case_docs`、
//! `read_case_doc`、`find_in_document`(全部案件内查 sqlite/文件)、
//! `search_local_kb`、`read_kb_file`(本身就是从 KB 读)。

pub mod artifact;
pub mod ask_user;
pub mod cases;
pub mod companies;
pub mod docs;
pub mod kb;
pub mod law_fulltext;
pub mod laws;
pub mod reextract;
pub mod save_kb;
pub mod semantic;
pub mod verify;

use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use sqlx::SqlitePool;
use thiserror::Error;

use crate::local_kb::cache::LocalKb;
use crate::settings::Settings;

/// 调 tool 时所需的运行时上下文。所有引用都是借用,生命周期 `'a` 跟 agent_loop 的
/// 一轮调度对齐。
pub struct ToolContext<'a> {
    pub pool: &'a SqlitePool,
    pub settings: &'a Settings,
    /// 当前 chat 所绑定的 case_id。某些工具(`list_case_docs` / `read_case_doc` /
    /// `find_in_document`)在 `None` 时直接报错。
    pub case_id: Option<&'a str>,
    /// V0.2 D2 的 `LocalKb` 实例,`None` = 用户没启用本地 KB,所有 KB-cache 路径跳过。
    pub local_kb: Option<&'a LocalKb>,
    /// Tauri `AppHandle`(cheap Arc clone),给需要触发后台任务并 emit 进度事件的工具用
    /// (如 `reextract_document` 走 `spawn_extraction`)。chat command 构造时传
    /// `Some(app.clone())`;单测 / 无 GUI 上下文传 `None`(此类工具会优雅报错)。
    pub app: Option<tauri::AppHandle>,
}

/// tool 执行结果。
///
/// 注:工具产生的引用统一走 `<CITATIONS>` 协议(由 agent_loop 解析 LLM 输出),
/// 不在 ToolResult 上单独挂 citations(旧 CitationSource 脚手架从未接线,已移除,D5-4)。
#[derive(Debug, Clone, Serialize)]
pub struct ToolResult {
    /// 喂回 LLM 的文本(markdown 或 JSON,LLM 自己解析)。
    pub content: String,
    /// 元典积分消耗(KB hit 计 0)。
    pub yuandian_credits_used: u32,
    /// 是否命中本地 KB(true 时反馈 MD 加 1 计数)。
    pub kb_hit: bool,
}

impl ToolResult {
    pub fn plain(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            yuandian_credits_used: 0,
            kb_hit: false,
        }
    }
}

/// 工具执行错误。区分"参数错"和"运行时错",让 agent_loop 做不同的回退策略
/// (`InvalidArgs` 让 LLM 重写参数重试,`Runtime` 直接报给用户)。
#[derive(Debug, Error)]
pub enum ToolError {
    #[error("参数错误:{0}")]
    InvalidArgs(String),
    #[error("当前对话没绑定案件,本工具需要 case_id")]
    NoCaseBound,
    #[error("元典 API key 未配置,无法外查;请到设置里填入")]
    NoYuandianKey,
    #[error("元典调用失败:{0}")]
    Yuandian(#[from] crate::yuandian::YuandianError),
    #[error("数据库错误:{0}")]
    Sqlx(#[from] sqlx::Error),
    #[error("本地 KB 错误:{0}")]
    Kb(#[from] crate::local_kb::KbError),
    #[error("IO 错误:{0}")]
    Io(#[from] std::io::Error),
    #[error("内部错误:{0}")]
    Runtime(String),
}

impl serde::Serialize for ToolError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

/// 单个工具的 trait。dyn Tool 由 async-trait 宏支持。
#[async_trait]
pub trait Tool: Send + Sync {
    /// 工具名(英文,DeepSeek function calling 用)
    fn name(&self) -> &str;
    /// 中文 description(LLM 看到的;`include_str!` 编译期注入)
    fn description(&self) -> &str;
    /// JSON Schema 参数描述(给 DeepSeek tools 数组用)
    fn parameters_schema(&self) -> Value;
    /// 实际执行。`args` 是 LLM function_call 的 arguments(已 JSON 解析)。
    async fn execute(&self, args: &Value, ctx: &ToolContext<'_>) -> Result<ToolResult, ToolError>;

    /// 是否为 **mutating** 工具(写盘 / 改状态)。mutating 工具在 agent_loop 一轮里
    /// **串行独占**执行,read-only 工具仍并行 —— 防同轮多个改同一文书的 tool_call 在
    /// IO await 点交错导致丢更新(见 `parallel::run_parallel_subtasks`)。默认 false。
    fn is_mutating(&self) -> bool {
        false
    }

    /// DeepSeek tools 数组里的单个 entry。一般不需要 override。
    fn to_function_schema(&self) -> Value {
        serde_json::json!({
            "type": "function",
            "function": {
                "name": self.name(),
                "description": self.description(),
                "parameters": self.parameters_schema(),
            }
        })
    }
}

/// 默认注册的全部 27 个工具:V0.2 的 21 个,加 V0.3 的 save_artifact / ask_user /
/// reextract_document,V0.3.3 的 semantic_search_case_docs,ADR-0003 的 edit_artifact,
/// P2 的 save_company_report(企业报告入库)。
/// `ToolRegistry::default_v0_2()` 返回这个列表。
pub struct ToolRegistry {
    tools: Vec<Box<dyn Tool>>,
}

impl ToolRegistry {
    /// V0.2 全量注册。次序无关紧要,DeepSeek tools 数组无序。
    pub fn default_v0_2() -> Self {
        let tools: Vec<Box<dyn Tool>> = vec![
            // 法规法条 5
            Box::new(laws::SearchLaws),
            Box::new(laws::GetLawArticle),
            Box::new(laws::SearchRegulations),
            Box::new(laws::GetRegulationDetail),
            Box::new(laws::LawVectorSearch),
            // 案例 4
            Box::new(cases::SearchCasesNormal),
            Box::new(cases::SearchCasesAuthority),
            Box::new(cases::GetCaseDetail),
            Box::new(cases::CaseVectorSearch),
            // 企业 6
            Box::new(companies::EnterpriseSearch),
            Box::new(companies::EnterpriseAggregationSummary),
            Box::new(companies::EnterpriseBaseInfo),
            Box::new(companies::EnterpriseChangeInfo),
            Box::new(companies::EnterpriseWritList),
            Box::new(companies::EnterpriseAnnualReport),
            // 幻觉校验 1
            Box::new(verify::VerifyLegalCitations),
            // 案件文档 4(V0.3.3 加 semantic_search_case_docs · 语义检索本案全文)
            Box::new(docs::ListCaseDocs),
            Box::new(docs::ReadCaseDoc),
            Box::new(docs::FindInDocument),
            Box::new(semantic::SemanticSearchCaseDocs),
            // 本地 KB 2
            Box::new(kb::SearchLocalKb),
            Box::new(kb::ReadKbFile),
            // 写作工具 2(V0.3 M1 + ADR-0003):文书生产 + 局部编辑(均 mutating)
            Box::new(artifact::SaveArtifact),
            Box::new(artifact::EditArtifact),
            // 交互工具 1(V0.3):选项式追问(agent_loop 拦截,不进 parallel 派发)
            Box::new(ask_user::AskUser),
            // 文档维护 1(V0.3):触发后台重抽某文档(mutating,会重跑 OCR/LLM 烧积分)
            Box::new(reextract::ReextractDocument),
            // 入库工具 1(P2):把企业调查报告写进本地 KB raw/companies/(mutating)
            Box::new(save_kb::SaveCompanyReport),
        ];
        Self { tools }
    }

    pub fn find(&self, name: &str) -> Option<&dyn Tool> {
        self.tools
            .iter()
            .find(|t| t.name() == name)
            .map(|t| t.as_ref())
    }

    /// 工具是否 mutating(写盘/改状态)。未注册的名字按 false(只读)处理,
    /// 不影响 `run_parallel_subtasks` 的派发(未注册工具会在 `find` 那里报错)。
    pub fn is_mutating(&self, name: &str) -> bool {
        self.find(name).map(|t| t.is_mutating()).unwrap_or(false)
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Tool> {
        self.tools.iter().map(|t| t.as_ref())
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    /// V0.3.6 · 把外部 MCP server 发现的转发工具并入注册表(由 `mcp_bridge::connect_mcp_servers`
    /// 产出)。空 vec = 无变化。MCP 工具与内置工具一视同仁走 `find` / `execute` / schemas。
    pub fn with_mcp(mut self, extra: Vec<Box<dyn Tool>>) -> Self {
        self.tools.extend(extra);
        self
    }

    /// 给 DeepSeek `chat/completions` 请求体的 `tools` 数组用。
    pub fn to_function_schemas(&self) -> Vec<Value> {
        self.tools.iter().map(|t| t.to_function_schema()).collect()
    }
}

/// 工具共用工具函数:从 args 里安全拿一个必填 string。
pub(crate) fn require_str<'a>(args: &'a Value, key: &str) -> Result<&'a str, ToolError> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| ToolError::InvalidArgs(format!("缺必填字段:{}", key)))
}

/// 可选 string(空串视为 None)。
pub(crate) fn opt_str<'a>(args: &'a Value, key: &str) -> Option<&'a str> {
    args.get(key)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
}

/// 可选 u32(支持数字和数字字符串)。
pub(crate) fn opt_u32(args: &Value, key: &str) -> Option<u32> {
    args.get(key).and_then(|v| {
        v.as_u64()
            .map(|n| n as u32)
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    })
}

/// 可选 bool。
pub(crate) fn opt_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}

/// 拿元典 API key,空串 / 缺失返回 `NoYuandianKey`。
pub(crate) fn yuandian_key<'a>(ctx: &'a ToolContext<'_>) -> Result<&'a str, ToolError> {
    let k = ctx
        .settings
        .yuandian_api_key
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(ToolError::NoYuandianKey)?;
    Ok(k)
}

// =============================================================================
// 喂 LLM 的搜索结果瘦身(ADR-0003 余波 · 2026-06-01 真机暴露:法规搜索 sidecar 250-290KB,
// 整部塞进主上下文、且工具消息打不中前缀缓存=全价。原则:整部/大段只缓存本地,**进主上下文
// 的只能是定位 + 短摘要**;要某条全文 LLM 用 get_law_article(fgid+ftnum) 取单条。
// 缓存(sidecar / .md 索引)仍存完整,瘦身只作用于**返回给 LLM 的内容**。)
// =============================================================================

/// 需要对「喂 LLM 内容」瘦身的搜索类 query_type(法规法条 / 法规 / 法规向量检索)。
const SLIM_SEARCH_TYPES: &[&str] = &["rh_ft_search", "rh_fg_search", "law_vector_search"];
/// 每条结果保留的正文上限(字符)——够 LLM 判断这是不是要的条,全文走 get_law_article。
const SLIM_CONTENT_CHARS: usize = 140;
/// 喂 LLM 时砍掉的噪音/重复字段(llm_content 与 content 重复;其余是 url/日期/分类/score)。
/// ⚠️ **保留 `id`(法条直接 id,get_law_article 首选入参)和 `tid`(阿拉伯条号,可当 ftnum)**
/// —— 它们小但是模型可靠取单条的定位锚;砍了模型只能传中文 ft_num,extract_article 会失配降级。
const SLIM_DROP_FIELDS: &[&str] = &[
    "llm_content",
    "url",
    "_score",
    "fbbm",
    "fbrq",
    "ssrq",
    "fwzh",
    "xljb_1",
    "xljb_2",
    "ftmc",
    "title",
];

/// 把搜索类元典响应瘦身成「喂 LLM 的紧凑版」:每条只留定位字段(fgmc/ft_num/fgid/sxx)+
/// 截断的 content,砍掉重复 llm_content 与一堆噪音。非搜索类返回 `None`(调用方走原样)。
fn slim_search_for_llm(query_type: &str, resp: &Value) -> Option<Value> {
    if !SLIM_SEARCH_TYPES.contains(&query_type) {
        return None;
    }
    let arr = resp.get("data")?.as_array()?;
    let slimmed: Vec<Value> = arr
        .iter()
        .map(|it| match it.as_object() {
            None => it.clone(),
            Some(obj) => {
                let mut m = serde_json::Map::new();
                for (k, v) in obj {
                    if SLIM_DROP_FIELDS.contains(&k.as_str()) {
                        continue;
                    }
                    match v.as_str() {
                        Some(s) if s.chars().count() > SLIM_CONTENT_CHARS => {
                            let kept: String = s.chars().take(SLIM_CONTENT_CHARS).collect();
                            m.insert(
                                k.clone(),
                                Value::String(format!(
                                    "{kept}…〔全文用 get_law_article(fgid+ftnum) 取〕"
                                )),
                            );
                        }
                        _ => {
                            m.insert(k.clone(), v.clone());
                        }
                    }
                }
                Value::Object(m)
            }
        })
        .collect();
    let mut out = serde_json::Map::new();
    out.insert("data".into(), Value::Array(slimmed));
    out.insert(
        "_note".into(),
        Value::String(
            "结果已精简(去重复正文/噪音,正文截断)。要某条全文用 get_law_article(fgid+ftnum) 取单条,\
             不要据此截断片段直接引用原文。"
                .into(),
        ),
    );
    Some(Value::Object(out))
}

// =============================================================================
// 案例检索瘦身(2026-06-02 · 真机:case_vector_search 单次 138KB / 45 案 × 656 字正文塞进主上下文;
// get_case_detail 的 content 还带个 llm_content 重复正文)。案例响应结构跟法条不同:
//   - case_vector_search 的案在 `extra.wenshu`;ptal/qwal/case_details 在 `data.lst`。
// 列表/向量检索:每案只留定位+短摘要(LLM 据此挑案,要全文 get_case_detail 取);
// get_case_detail:留 content(本就为取全文而调),只砍重复 llm_content + 噪音。
// 缓存(sidecar / .md)仍存完整,瘦身只作用于**返回给 LLM 的内容**。
// =============================================================================

/// 列表/向量检索类(每案正文截断)。`rh_case_details` 不在此列 —— 它要留全文。
const CASE_LIST_TYPES: &[&str] = &["rh_ptal_search", "rh_qwal_search", "case_vector_search"];
/// 每案只保留这些字段(定位 + 摘要锚);其余(llm_content 重复 / url / 区划 / score / 库 等)全砍。
const CASE_KEEP_FIELDS: &[&str] = &[
    "ah",      // 案号(取全文 get_case_detail 的 case_no)
    "jbdw",    // 经办单位/法院
    "ay",      // 案由
    "anyou",   // 案由(向量库字段名)
    "cprq",    // 裁判日期
    "cj",      // 审级
    "spcx",    // 审判程序
    "title",   // 标题
    "id",      // 定位 id
    "scid",    // 向量库定位 id
    "content", // 正文(列表型截断,详情型留全)
];
/// 列表型每案正文上限(字符)——够 LLM 判断是不是要的案,全文走 get_case_detail。
const CASE_CONTENT_CHARS: usize = 160;

/// 定位案例数组(不同 query_type 路径不同)。返回 `(案例数组, 是否列表型需截断)`。
fn locate_cases<'a>(query_type: &str, resp: &'a Value) -> Option<(&'a Vec<Value>, bool)> {
    let arr = match query_type {
        "case_vector_search" => resp.get("extra")?.get("wenshu")?.as_array()?,
        "rh_ptal_search" | "rh_qwal_search" | "rh_case_details" => {
            resp.get("data")?.get("lst")?.as_array()?
        }
        _ => return None,
    };
    Some((arr, CASE_LIST_TYPES.contains(&query_type)))
}

/// 把案例类元典响应瘦身成「喂 LLM 的紧凑版」。非案例类返回 `None`(调用方走原样)。
fn slim_cases_for_llm(query_type: &str, resp: &Value) -> Option<Value> {
    let (cases, truncate) = locate_cases(query_type, resp)?;
    let slimmed: Vec<Value> = cases
        .iter()
        .map(|c| {
            let Some(obj) = c.as_object() else {
                return c.clone();
            };
            let mut m = serde_json::Map::new();
            for &k in CASE_KEEP_FIELDS {
                let Some(v) = obj.get(k) else { continue };
                if truncate && k == "content" {
                    if let Some(s) = v.as_str() {
                        if s.chars().count() > CASE_CONTENT_CHARS {
                            let kept: String = s.chars().take(CASE_CONTENT_CHARS).collect();
                            m.insert(
                                k.to_string(),
                                Value::String(format!(
                                    "{kept}…〔全文用 get_case_detail(type+case_no) 取〕"
                                )),
                            );
                            continue;
                        }
                    }
                }
                m.insert(k.to_string(), v.clone());
            }
            Value::Object(m)
        })
        .collect();
    let note = if truncate {
        "案例结果已精简(每条只留 案号/法院/案由/日期/正文摘要,正文截断)。要某案全文用 \
         get_case_detail(type+case_no) 取,不要据此截断片段直接引用裁判理由。"
    } else {
        "已去重复正文(llm_content)与噪音字段;content 为判决正文。"
    };
    let mut out = serde_json::Map::new();
    out.insert("cases".into(), Value::Array(slimmed));
    out.insert("_note".into(), Value::String(note.into()));
    Some(Value::Object(out))
}

/// 决定一次工具结果**喂给 LLM** 的字符串:法条搜索类 / 案例类瘦身,其余原样 pretty JSON。
/// 缓存里仍存完整 resp(本函数不影响落盘)。
fn content_for_llm(query_type: &str, resp: &Value) -> String {
    let v = slim_search_for_llm(query_type, resp).or_else(|| slim_cases_for_llm(query_type, resp));
    let to_print = v.as_ref().unwrap_or(resp);
    serde_json::to_string_pretty(to_print).unwrap_or_else(|_| "{}".into())
}

// =============================================================================
// P1 · 缓存分层:详情类(全文)写「可读命名全文 MD」、空结果不缓存。
// 目的:让缓存目录里的全文成品一眼可读、可治理可提升,根治「详情 .md 显示 result_count:0、
// 全文藏 .raw.json,review 时像空垃圾被误删」的结构陷阱;并堵掉空结果污染。
// 注:这是**目录卫生**,不省积分(API 已经调过)。缓存命中仍走 hash 命名的 .raw.json sidecar。
// =============================================================================

/// 详情类(全文)query_type —— 走 `save_detail`(可读命名全文 MD + 索引)而非剥 content 的 SEARCH 索引。
const DETAIL_TYPES: &[&str] = &["rh_fg_detail", "rh_ft_detail", "rh_case_details"];

/// `render_detail_md` 的产物:写可读全文 MD 所需素材。
struct DetailDoc {
    type_label: &'static str,
    obj_id: String,
    display_name: String,
    body_md: String,
}

/// 把详情类响应渲染成可读全文 MD 的素材。**按响应形状分支**(关键:三类形状不同):
/// - `rh_fg_detail` / `rh_ft_detail`:`data` 是对象,正文在 `data.content`;
/// - `rh_case_details`:是 `get_case_detail` 用 search top_k=1 顶替的实现,正文在 `data.lst[0].content`。
///
/// 取不到正文 → `None`(调用方据此退回只写 sidecar,绝不写空壳 MD)。
fn render_detail_md(query_type: &str, resp: &Value) -> Option<DetailDoc> {
    let str_at = |p: &str| resp.pointer(p).and_then(|v| v.as_str());
    match query_type {
        "rh_fg_detail" => {
            let content = str_at("/data/content")?;
            if content.trim().is_empty() {
                return None;
            }
            let name = str_at("/data/fgmc").unwrap_or("未命名法规");
            let id = str_at("/data/fgid")
                .or_else(|| str_at("/data/id"))
                .unwrap_or("na");
            Some(DetailDoc {
                type_label: "法规",
                obj_id: id.to_string(),
                display_name: name.to_string(),
                body_md: content.to_string(),
            })
        }
        "rh_ft_detail" => {
            let content = str_at("/data/content")?;
            if content.trim().is_empty() {
                return None;
            }
            let fgmc = str_at("/data/fgmc").unwrap_or("");
            let ftnum = str_at("/data/ftnum")
                .or_else(|| str_at("/data/ft_num"))
                .unwrap_or("");
            let id = str_at("/data/id").unwrap_or("na");
            let name = if ftnum.is_empty() {
                fgmc.to_string()
            } else {
                format!("{} 第{}条", fgmc, ftnum)
            };
            Some(DetailDoc {
                type_label: "法条",
                obj_id: id.to_string(),
                display_name: name,
                body_md: content.to_string(),
            })
        }
        "rh_case_details" => {
            let first = resp
                .pointer("/data/lst")
                .and_then(|v| v.as_array())?
                .first()?;
            let content = first.get("content").and_then(|v| v.as_str()).unwrap_or("");
            if content.trim().is_empty() {
                return None;
            }
            let ah = first
                .get("ah")
                .and_then(|v| v.as_str())
                .unwrap_or("未知案号");
            Some(DetailDoc {
                type_label: "案例",
                obj_id: ah.to_string(),
                display_name: ah.to_string(),
                body_md: content.to_string(),
            })
        }
        _ => None,
    }
}

/// 详情类持久化:可读全文 MD(+索引)+ 完整响应 sidecar。失败只 dlog,不致命。
/// 渲染不出正文(理论上 `response_is_empty` 已拦)→ 退回只写 sidecar,不丢数据。
pub(crate) fn persist_detail(
    kb: &LocalKb,
    query_type: &str,
    params: &Value,
    resp: &Value,
    body: &str,
) {
    if let Some(doc) = render_detail_md(query_type, resp) {
        if let Err(e) = kb.save_detail(
            query_type,
            params,
            doc.type_label,
            &doc.obj_id,
            &doc.display_name,
            &doc.body_md,
        ) {
            crate::dlog!("local_kb save_detail failed: {}", e);
        }
    } else {
        crate::dlog!("详情渲染不出正文,退回只写 sidecar: {}", query_type);
    }
    if let Err(e) = kb.save_raw_response(query_type, params, body) {
        crate::dlog!("local_kb save_raw_response failed: {}", e);
    }
}

/// 判断响应是否「空结果」—— 空就不缓存(目录卫生 + 不留 `kb_hit:true`/content 空 的迷惑信号;
/// 注:不省积分,API 已经调过)。**只对有把握判空的形状下结论**,其余一律当非空照存(保守,防误丢)。
pub(crate) fn response_is_empty(query_type: &str, resp: &Value) -> bool {
    match query_type {
        // 法规详情·对象型:正文在 data.content(已被 try_fulltext_article 佐证)。
        "rh_fg_detail" => resp
            .pointer("/data/content")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().is_empty())
            .unwrap_or(true),
        // 法条单条详情:形状未经现行代码消费佐证(仿 fg_detail 的 data.content)。稳妥:
        // data 整个缺失 / null / 空对象 → 当空;有 data 但 content 字段缺失 → **不当空、照缓存**
        // (避免假设错时静默不缓存、反而每次重查费积分);只有 content 明确为空白才算空。
        "rh_ft_detail" => match resp.pointer("/data") {
            None | Some(Value::Null) => true,
            Some(Value::Object(o)) if o.is_empty() => true,
            Some(_) => resp
                .pointer("/data/content")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().is_empty())
                .unwrap_or(false),
        },
        // 详情·案例(search 顶替):看 data.lst 是否有案
        "rh_case_details" => resp
            .pointer("/data/lst")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(true),
        // 顶层 data 数组型搜索(ft/fg/ptal/qwal):空数组 = 空结果
        "rh_ft_search" | "rh_fg_search" | "rh_ptal_search" | "rh_qwal_search" => resp
            .get("data")
            .and_then(|v| v.as_array())
            .map(|a| a.is_empty())
            .unwrap_or(false),
        // 语义检索(嵌套形状不一)/ 企业类等 → 保守当非空照存
        _ => false,
    }
}

/// 三段式辅助 step 1:查 KB cache。命中 → 返回 `Some(ToolResult)` 让调用方提前 return,
/// miss → 返回 `None`,调用方接着调 API。
pub(crate) fn try_kb_hit(
    ctx: &ToolContext<'_>,
    query_type: &str,
    cache_params: &Value,
) -> Option<ToolResult> {
    let kb = ctx.local_kb?;
    let (_hit, _fresh) = kb.check_cache(query_type, cache_params)?;
    // 只认 sidecar 完整响应(含 content 全文,字节与未命中路径一致)。缺 sidecar
    //(老缓存 / 写失败)时**不**回退残缺 .md 索引 —— 残缺结果(丢 content)会让 LLM
    // 误以为没查全,用相同参数重复调用(实测 get_law_article 被 LoopGuard 拦下丢答案)。
    // 宁可当 miss 让上层重新调 API:多花一次积分,但拿完整响应 + 重建 sidecar,打断重复调循环。
    let raw = kb.load_raw_response(query_type, cache_params)?;
    // 缓存里存的是完整响应;喂 LLM 前对搜索类瘦身(与未命中路径 save_and_wrap 同一函数 →
    // 命中/未命中喂给 LLM 的字节一致,前缀缓存照样命中)。解析失败则原样返回(兜底)。
    let content = serde_json::from_str::<Value>(&raw)
        .map(|v| content_for_llm(query_type, &v))
        .unwrap_or(raw);
    Some(ToolResult {
        content,
        yuandian_credits_used: 0,
        kb_hit: true,
    })
}

/// 三段式辅助 step 3:元典 API 返回后,把 resp 序列化成 JSON 字符串给 LLM,
/// 顺手写回 KB(失败不致命,KB 写挂不影响本次调用)。
pub(crate) fn save_and_wrap(
    ctx: &ToolContext<'_>,
    query_type: &str,
    cache_params: &Value,
    summary: &str,
    resp: Value,
    credits: u32,
) -> ToolResult {
    // sidecar 存「完整响应 pretty JSON」(含 content 全文,供命中复用 / 人读 / 可追溯)。
    let body = serde_json::to_string_pretty(&resp).unwrap_or_else(|_| "{}".into());
    if let Some(kb) = ctx.local_kb {
        if response_is_empty(query_type, &resp) {
            // P1 · 空结果不缓存(目录卫生:不囤空壳、不留 kb_hit:true/content 空 的迷惑信号)。
            // 不省积分 —— API 已经调过;省的是后续误导与垃圾堆积。
            crate::dlog!("空结果不写缓存: {}", query_type);
        } else if DETAIL_TYPES.contains(&query_type) {
            // P1 · 详情类(全文):写可读命名全文 MD + 索引 + sidecar(替代剥 content 的 SEARCH 空壳)。
            persist_detail(kb, query_type, cache_params, &resp, &body);
        } else {
            // 搜索类:.md 索引(给 Python skill / 人读)+ sidecar(写失败只 dlog,不应让 LLM 看不到结果)。
            let empty: Vec<Value> = Vec::new();
            let results: Vec<Value> = resp
                .get("data")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or(empty);
            if let Err(e) = kb.save_search(query_type, cache_params, &results, summary) {
                crate::dlog!("local_kb save_search failed: {}", e);
            }
            if let Err(e) = kb.save_raw_response(query_type, cache_params, &body) {
                crate::dlog!("local_kb save_raw_response failed: {}", e);
            }
        }
    }
    ToolResult {
        // 喂 LLM 的内容:搜索类瘦身(只给定位+短摘要),其余原样。缓存仍是上面的完整 body。
        content: content_for_llm(query_type, &resp),
        yuandian_credits_used: credits,
        kb_hit: false,
    }
}

#[cfg(test)]
mod tests {
    //! ToolRegistry / Tool trait 基础单测,各具体 tool 的 smoke test
    //! 放在各 tool 子模块的 `#[cfg(test)]` 里。
    use super::*;

    #[test]
    fn registry_registers_core_tools() {
        // 不再断言精确条数(加/删工具不该连带改这个测试,曾经每次都得手动 +1)。
        // 护栏 = 下限(catches 注册链路整体坏掉 / 空表)+ 关键工具存在性(catches 误删核心工具)。
        let r = ToolRegistry::default_v0_2();
        assert!(
            r.len() >= 20,
            "工具注册表异常少({}),疑似注册链路坏了",
            r.len()
        );
        for name in [
            "save_artifact",
            "edit_artifact",
            "ask_user",
            "reextract_document",
            "semantic_search_case_docs",
            "search_laws",
            "read_case_doc",
        ] {
            assert!(r.find(name).is_some(), "{name} 未注册");
        }
    }

    #[test]
    fn mutating_tools_flagged() {
        let r = ToolRegistry::default_v0_2();
        // 写盘/改状态的工具必须标 mutating(串行独占)
        for m in ["save_artifact", "edit_artifact", "reextract_document"] {
            assert!(r.is_mutating(m), "{m} 应标 mutating");
        }
        // 只读工具不串行
        for ro in ["search_laws", "read_case_doc", "list_case_docs"] {
            assert!(!r.is_mutating(ro), "{ro} 不应是 mutating");
        }
        // 未注册名按只读处理(不 panic)
        assert!(!r.is_mutating("not_a_tool"));
    }

    #[test]
    fn slim_search_drops_noise_and_truncates() {
        use serde_json::json;
        let resp = json!({"data": [{
            "fgmc": "中华人民共和国民法典",
            "ft_num": "第五百八十五条",
            "fgid": "abc123",
            "sxx": "现行有效",
            "content": "违约金".repeat(100),
            "llm_content": "重复的排版正文",
            "url": "https://x",
            "_score": 9.9,
            "id": "abc123_585",
            "tid": "585",
            "title": "标题重复"
        }]});
        let slim = slim_search_for_llm("rh_ft_search", &resp).expect("搜索类应瘦身");
        let it = &slim["data"][0];
        // 定位字段保留(含 id / tid —— 模型据此可靠调 get_law_article 取单条)
        assert_eq!(it["fgmc"], "中华人民共和国民法典");
        assert_eq!(it["ft_num"], "第五百八十五条");
        assert_eq!(it["fgid"], "abc123");
        assert_eq!(it["sxx"], "现行有效");
        assert_eq!(it["id"], "abc123_585", "id 必须保留(取单条首选锚)");
        assert_eq!(it["tid"], "585", "tid(阿拉伯条号)必须保留");
        // 噪音/重复字段砍掉
        for k in ["llm_content", "url", "_score", "title"] {
            assert!(it.get(k).is_none(), "{k} 应被砍");
        }
        // 长 content 截断 + 提示去 get_law_article 取全文
        let c = it["content"].as_str().unwrap();
        assert!(c.chars().count() < 200, "content 应截断");
        assert!(c.contains("get_law_article"), "应提示取单条");
        // 非搜索类不瘦身
        assert!(slim_search_for_llm("rh_fg_detail", &resp).is_none());
        assert!(slim_search_for_llm("get_case_detail", &resp).is_none());
    }

    #[test]
    fn slim_cases_vector_truncates_and_drops_noise() {
        use serde_json::json;
        // case_vector_search 的案在 extra.wenshu
        let resp = json!({
            "code": 0, "msg": "ok",
            "extra": { "wenshu": [{
                "ah": "(2021)苏02民终1323号",
                "jbdw": "某中级人民法院",
                "ay": ["物权确认纠纷"],
                "scid": "abc123",
                "content": "本院认为".repeat(100),
                "llm_content": "重复正文",
                "url": "https://x",
                "score": 9.9,
                "xzqh_p": "苏", "xzqh_c": "02", "db": "ptal"
            }]}
        });
        let slim = slim_cases_for_llm("case_vector_search", &resp).expect("案例向量应瘦身");
        let it = &slim["cases"][0];
        // 定位字段保留
        assert_eq!(it["ah"], "(2021)苏02民终1323号");
        assert_eq!(it["jbdw"], "某中级人民法院");
        assert_eq!(it["scid"], "abc123", "向量库定位 id 保留");
        // 重复正文 + 噪音砍掉
        for k in ["llm_content", "url", "score", "xzqh_p", "xzqh_c", "db"] {
            assert!(it.get(k).is_none(), "{k} 应被砍");
        }
        // content 截断 + 提示取全文
        let c = it["content"].as_str().unwrap();
        assert!(c.chars().count() < 200, "content 应截断");
        assert!(c.contains("get_case_detail"), "应提示取全文");
    }

    #[test]
    fn slim_case_detail_keeps_content_drops_dup() {
        use serde_json::json;
        // get_case_detail(rh_case_details)的案在 data.lst,且 content 本就为取全文而调 → 留全
        let full = "本院认为，".to_string() + &"裁判理由".repeat(500);
        let resp = json!({
            "code": 0, "status": "success",
            "data": { "lst": [{
                "ah": "(2019)豫02民终986号",
                "content": full.clone(),
                "llm_content": "重复的 llm 正文",
                "url": "https://y",
                "score": 8.0
            }], "total": 1 }
        });
        let slim = slim_cases_for_llm("rh_case_details", &resp).expect("案例详情应瘦身");
        let it = &slim["cases"][0];
        // 详情型:正文留全(不截断)
        assert_eq!(
            it["content"].as_str().unwrap().chars().count(),
            full.chars().count(),
            "详情型正文必须留全,不截断"
        );
        // 重复 llm_content + 噪音砍掉
        assert!(it.get("llm_content").is_none(), "重复 llm_content 应砍");
        assert!(it.get("url").is_none());
        assert!(it.get("score").is_none());
    }

    #[test]
    fn slim_cases_ignores_non_case_types() {
        use serde_json::json;
        // 法条搜索/详情类不被案例瘦身接管(各走各的)
        assert!(slim_cases_for_llm("rh_fg_search", &json!({"data": []})).is_none());
        assert!(slim_cases_for_llm("get_law_article", &json!({"x": 1})).is_none());
        // 缺数组路径(空/错误响应)→ 原样放行
        assert!(slim_cases_for_llm("case_vector_search", &json!({"code": 0})).is_none());
    }

    #[test]
    fn response_is_empty_detects_empty_shapes_conservatively() {
        use serde_json::json;
        // 详情·对象型:无 content / content 空白 → 空
        assert!(response_is_empty(
            "rh_fg_detail",
            &json!({"data": {"fgmc": "x"}})
        ));
        assert!(response_is_empty(
            "rh_ft_detail",
            &json!({"data": {"content": "   "}})
        ));
        // ft_detail 稳妥:有 data 但缺 content 字段 → **不当空、照缓存**(形状未佐证,宁缓存不漏)
        assert!(!response_is_empty(
            "rh_ft_detail",
            &json!({"data": {"id": "x", "fgmc": "民法典"}})
        ));
        // ft_detail:data 缺失 / null / 空对象 → 当空
        assert!(response_is_empty("rh_ft_detail", &json!({})));
        assert!(response_is_empty("rh_ft_detail", &json!({"data": null})));
        assert!(response_is_empty("rh_ft_detail", &json!({"data": {}})));
        assert!(!response_is_empty(
            "rh_fg_detail",
            &json!({"data": {"content": "第一条 …"}})
        ));
        // 详情·案例(search 顶替):data.lst 空 → 空
        assert!(response_is_empty(
            "rh_case_details",
            &json!({"data": {"lst": []}})
        ));
        assert!(!response_is_empty(
            "rh_case_details",
            &json!({"data": {"lst": [{"content": "本院认为"}]}})
        ));
        // 顶层 data 数组搜索:空数组 → 空
        assert!(response_is_empty("rh_ft_search", &json!({"data": []})));
        assert!(!response_is_empty(
            "rh_ft_search",
            &json!({"data": [{"id": "1"}]})
        ));
        // 语义检索 / 企业类:嵌套或形状不一 → 保守当非空(防误丢)
        assert!(!response_is_empty(
            "law_vector_search",
            &json!({"data": []})
        ));
        assert!(!response_is_empty(
            "case_vector_search",
            &json!({"extra": {"wenshu": []}})
        ));
        assert!(!response_is_empty(
            "rh_enterpriseBaseInfo",
            &json!({"data": {}})
        ));
    }

    #[test]
    fn render_detail_md_branches_by_response_shape() {
        use serde_json::json;
        // 法规:data.content 对象型 → 取 fgid/fgmc/全文
        let fg = render_detail_md(
            "rh_fg_detail",
            &json!({"data": {"fgmc": "中华人民共和国民法典", "fgid": "f9", "content": "第一条 …全文…"}}),
        )
        .expect("法规应渲染出全文");
        assert_eq!(fg.type_label, "法规");
        assert_eq!(fg.obj_id, "f9");
        assert_eq!(fg.display_name, "中华人民共和国民法典");
        assert!(fg.body_md.contains("第一条"));
        // 案例:data.lst[0](search 形状)→ 取案号/全文
        let case = render_detail_md(
            "rh_case_details",
            &json!({"data": {"lst": [{"ah": "(2021)苏02民终1号", "content": "本院认为……"}]}}),
        )
        .expect("案例应渲染出全文");
        assert_eq!(case.type_label, "案例");
        assert_eq!(case.obj_id, "(2021)苏02民终1号");
        assert!(case.body_md.contains("本院认为"));
        // 空 content → None(不写空壳)
        assert!(render_detail_md("rh_fg_detail", &json!({"data": {"content": ""}})).is_none());
        // 非详情类 → None(走搜索分支)
        assert!(render_detail_md("rh_ft_search", &json!({"data": []})).is_none());
    }

    #[test]
    fn registry_tool_names_unique() {
        let r = ToolRegistry::default_v0_2();
        let mut names: Vec<&str> = r.iter().map(|t| t.name()).collect();
        names.sort();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total, "tool 名重复:{:?}", names);
    }

    #[test]
    fn registry_find_matches_by_name() {
        let r = ToolRegistry::default_v0_2();
        assert!(r.find("search_laws").is_some());
        assert!(r.find("not_a_real_tool").is_none());
    }

    #[test]
    fn registry_function_schemas_match_deepseek_shape() {
        // DeepSeek 期望:{ type: "function", function: { name, description, parameters } }
        let r = ToolRegistry::default_v0_2();
        let schemas = r.to_function_schemas();
        // 不锁精确条数;真正的不变量是「每个工具恰好产一个 schema」。
        assert_eq!(schemas.len(), r.len(), "每个工具应产且仅产一个 schema");
        for s in schemas {
            assert_eq!(s["type"], "function");
            assert!(s["function"]["name"].is_string());
            assert!(s["function"]["description"].is_string());
            assert!(s["function"]["parameters"].is_object());
        }
    }

    #[test]
    fn registry_descriptions_are_substantial() {
        // 每个 description 至少 400 字(中文),§ 19.7 要求 30-50 行详细描述。
        // D2-D3.F 阶段写完 21 个 descriptions/*.md 后必须过。
        let r = ToolRegistry::default_v0_2();
        let mut weak: Vec<(&str, usize)> = Vec::new();
        for t in r.iter() {
            let chars = t.description().chars().count();
            if chars < 400 {
                weak.push((t.name(), chars));
            }
        }
        assert!(
            weak.is_empty(),
            "以下 tool description 不足 400 字(目标 30-50 行):{:?}",
            weak
        );
    }

    /// 真元典 key 端到端冒烟:走真实 tool 链路 → 写真实 KB 缓存 → 读出来核对。
    /// 验证:① 法规全文路径(get_law_article fgid+ftnum)生成可读 `法规-{id}_{名}.md` + 索引
    /// ② 同法规复取 0 积分命中 ③ rh_ft_detail/rh_case_details 真实形状下详情可读 MD 正常。
    /// 默认 `#[ignore]`(要真 key + 联网 + 烧少量积分)。手动跑:
    ///   `YUANDIAN_API_KEY=$(python3 -c "...") cargo test smoke_real_yuandian_cache -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "需真元典 key + 联网 + 烧少量积分;手动 --ignored --nocapture 跑"]
    async fn smoke_real_yuandian_cache() {
        use crate::local_kb::cache::LocalKb;
        use crate::settings::Settings;
        use serde_json::json;

        let key = std::env::var("YUANDIAN_API_KEY").unwrap_or_default();
        if key.trim().is_empty() {
            eprintln!("⏭  跳过:未设置 YUANDIAN_API_KEY");
            return;
        }
        let tmp = tempfile::TempDir::new().unwrap();
        let settings = Settings {
            yuandian_api_key: Some(key),
            local_kb_root: Some(tmp.path().to_string_lossy().into_owned()),
            local_kb_enabled: Some(true),
            ..Default::default()
        };
        let kb = LocalKb::auto_detect(&settings).expect("auto_detect KB");
        let pool = crate::db::init_pool(":memory:").await.unwrap();
        let ctx = ToolContext {
            pool: &pool,
            settings: &settings,
            case_id: None,
            local_kb: Some(&kb),
            app: None,
        };

        // 1) 法条检索,挑一个有「第X条」结构的法规(民法典)演示全文路径
        let sr = laws::SearchLaws
            .execute(&json!({"keyword":"违约金","top_k":10}), &ctx)
            .await
            .expect("search_laws");
        let v: Value = serde_json::from_str(&sr.content).unwrap();
        let items = v["data"].as_array().expect("data 数组");
        let item = items
            .iter()
            .find(|it| it["fgmc"].as_str().unwrap_or("").contains("民法典"))
            .unwrap_or(&items[0]);
        let fgid = item["fgid"].as_str().expect("fgid").to_string();
        let ftnum = item["tid"]
            .as_str()
            .or_else(|| item["ft_num"].as_str())
            .expect("tid/ft_num")
            .to_string();
        eprintln!(
            "\n[1] search_laws → fgmc={:?} fgid={} ftnum={}",
            item["fgmc"].as_str().unwrap_or(""),
            fgid,
            ftnum
        );

        // 2) get_law_article 全文路径(首次:拉整部法规缓存,1 积分)
        let a1 = laws::GetLawArticle
            .execute(&json!({"fgid":fgid,"ftnum":ftnum}), &ctx)
            .await
            .expect("get_law_article #1");
        eprintln!(
            "[2] get_law_article 首次 credits={} kb_hit={}",
            a1.yuandian_credits_used, a1.kb_hit
        );

        // 3) 同法规复取(应 0 积分命中本地全文)
        let a2 = laws::GetLawArticle
            .execute(&json!({"fgid":fgid,"ftnum":ftnum}), &ctx)
            .await
            .expect("get_law_article #2");
        eprintln!(
            "[3] get_law_article 复取 credits={} kb_hit={}  ← 期望 0 / true",
            a2.yuandian_credits_used, a2.kb_hit
        );

        // 4) 读真实缓存目录
        eprintln!(
            "\n===== 缓存目录文件清单 {} =====",
            kb.yuandian_cache_dir.display()
        );
        let mut names: Vec<String> = std::fs::read_dir(&kb.yuandian_cache_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for n in &names {
            eprintln!("  · {}", n);
        }
        // 打印可读全文 MD(法规-/法条-/案例-)
        for n in &names {
            if (n.starts_with("法规-") || n.starts_with("法条-")) && n.ends_with(".md") {
                let body = std::fs::read_to_string(kb.yuandian_cache_dir.join(n)).unwrap();
                let head: String = body.chars().take(500).collect();
                eprintln!("\n----- {} (前500字) -----\n{}", n, head);
            }
        }
        let idx = std::fs::read_to_string(&kb.index_path).unwrap_or_default();
        eprintln!("\n----- index.json -----\n{}", idx);

        // 断言核心省积分行为
        assert!(
            a1.yuandian_credits_used >= 1 || a1.kb_hit,
            "首次取应消耗积分或命中(取决于是否已缓存)"
        );
        assert_eq!(a2.yuandian_credits_used, 0, "同法规复取应 0 积分");
        assert!(a2.kb_hit, "同法规复取应 kb_hit=true");
        // 应生成至少一个可读命名详情 MD(法规-/法条-)
        assert!(
            names
                .iter()
                .any(|n| (n.starts_with("法规-") || n.starts_with("法条-")) && n.ends_with(".md")),
            "应生成可读命名全文 MD(法规-/法条-),实际:{:?}",
            names
        );
    }
}
