//! 用户设置的读写。
//!
//! 设计原则(对应 CLAUDE.md 隐私铁律):
//!   - **每个用户填自己的 token**,工具不内置任何人的 key
//!   - 配置落本机 `~/Library/Application Support/CaseBoard/settings.json`
//!   - V0.1 明文存(本机用户文件保护即可);V0.2 升 macOS Keychain
//!   - 飞书反馈 webhook 不在这里(它是编译时常量,所有用户共用,
//!     接收方是作者;放在 task #8 单独处理)
//!
//! 文件结构(扁平,V0.1 简单优先):
//! ```json
//! {
//!   "mineru_api_key": "",
//!   "mineru_endpoint": "https://api.mineru.net/v1",
//!   "ollama_endpoint": "http://localhost:11434",
//!   "ollama_model": "qwen2.5:7b"
//! }
//! ```

use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use serde::{Deserialize, Serialize};

use crate::chat::mcp_bridge::McpServerConfig;
use crate::chat::mcp_credentials::McpStoredServer;
use crate::db::app_data_dir;

static SETTINGS_READ_MIGRATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn with_settings_read_migration_lock<T>(
    operation: impl FnOnce() -> Result<T, String>,
) -> Result<T, String> {
    let _guard = SETTINGS_READ_MIGRATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .map_err(|_| "SETTINGS_MIGRATION_LOCK_POISONED".to_string())?;
    operation()
}

/// 用户配置。字段全部 Option<String>,因为初始全是空的。
///
/// 这里**只放每个用户私有的配置**——不放飞书 webhook 这种"全局共享"的常量。
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// 用户的显示称呼(例:"刘律师" / "周律师"),首页问候用。
    /// 留空时显示"律师"作为兜底。2026-05-23 晚九加。
    pub user_display_name: Option<String>,

    /// 合同审查修订批注版的默认作者。为空时回退到 `user_display_name`，
    /// 仍为空则由导出命令使用产品兜底名称。
    pub contract_review_comment_author: Option<String>,

    // ===== 2026-05-23 加(作者隐私分流决策,详见 docs/产品决策与理念.md 第 2 节) =====
    /// 用户是否完成过 onboarding。
    ///
    /// **默认 false** —— 首次启动 App 检测到为 false 时,强制弹 OnboardingWizard 让用户做选择。
    /// 完成后置 true,后续启动跳过 onboarding。
    pub setup_completed: bool,

    // ===== 2026-05-23 晚六 二维独立分流(替代原 cloud_enabled) =====
    /// OCR 后端选择:`"local"` = 本机 MiniCPM-V vision / `"cloud"` = MinerU 在线
    pub ocr_provider: Option<String>,
    /// LLM 后端选择:`"local"` = 本机 MiniCPM-V chat / `"cloud"` = DeepSeek
    pub llm_provider: Option<String>,

    /// 本机模型目录(放 MiniCPM-V-4_6-Q8_0.gguf 和 mmproj-model-f16.gguf)
    ///
    /// 默认会建议 `~/.cache/caseboard/models/`,用户也可以指定其他目录(比如已经下载在
    /// `~/.lmstudio/models/openbmb/MiniCPM-V-4.6-gguf/`)
    pub local_model_dir: Option<String>,
    /// 是否允许 App 自动拉起 llama-server(默认 true,用户不用动终端)
    pub local_server_auto_start: Option<bool>,

    // ===== 旧字段:保留作向后兼容,迁移到新字段后还会用一段时间 =====
    /// [DEPRECATED] 老的"全局云端开关",2026-05-23 晚六改成 ocr_provider/llm_provider 独立
    /// 仍保留是为了不破坏老配置 — read 时如果新字段空就 fall back 到这个
    pub cloud_enabled: bool,

    /// MinerU 在线 OCR 的 API key(用户自己注册账号拿)
    #[serde(skip_serializing)]
    pub mineru_api_key: Option<String>,
    /// MinerU endpoint(一般不用改,默认值)
    pub mineru_endpoint: Option<String>,

    /// 2026-06-12:PaddleOCR VL-1.6(百度 AI Studio 星河社区)访问令牌。
    /// 申请:https://aistudio.baidu.com/account/accessToken,免费 20,000 页/天。
    /// 作者实测与 MinerU 精度打平、速度约快一倍;详 ingest/paddle_vl_http.rs 头注释。
    #[serde(skip_serializing)]
    pub paddle_vl_api_key: Option<String>,
    /// PaddleOCR key 验证通过时间(坑#11:新 cloud key 必配 verified_at,改 key 重置)
    pub paddle_vl_verified_at: Option<String>,
    /// 云端 OCR 主力选择:`"mineru"`(默认,老用户零感知)/ `"paddle-vl"`。
    /// 另一个自动成为备用:主力失败 / 超时 / 额度用完时,**备用 key 已填**才自动切换。
    pub ocr_cloud_primary: Option<String>,

    /// 本机 llama-server endpoint(默认 http://127.0.0.1:8899)
    /// 字段名是历史包袱 "ollama_*",实际用的是 llama.cpp 的 llama-server
    pub ollama_endpoint: Option<String>,
    /// 本机 LLM 模型名(默认 MiniCPM-V-4_6-Q8_0.gguf)
    pub ollama_model: Option<String>,

    /// 云端 LLM endpoint(默认推荐 DeepSeek `https://api.deepseek.com`)
    pub cloud_llm_endpoint: Option<String>,
    /// 云端 LLM 模型档位(V0.3 统一为唯一的模型选择,被 `model_router::route_model` 读取):
    ///   - `'deepseek-v4-flash'`(默认)= 全局 Flash(便宜,约 pro 的 1/3 价)
    ///   - `'deepseek-v4-pro'` = 全局 Pro(更准更贵;实测 v4-pro 本身即思考模型,无独立 -thinking 变体)
    ///   - `'auto'` = 自动挡(简单走 flash,复杂走 pro)
    ///
    /// 默认 flash;不再有"工具型任务偷偷强制 pro"的隐藏逻辑。
    pub cloud_llm_model: Option<String>,
    /// 云端 LLM API key
    #[serde(skip_serializing)]
    pub cloud_llm_api_key: Option<String>,

    /// 2026-06-15:云端 LLM 后端选择 —— `"deepseek"`(默认/缺省)/ `"minimax"`。
    /// **纯增量**:老用户(全是 DeepSeek)缺此字段 → 走 deepseek 分支,配置零改动、零重解释。
    /// 选 minimax 时改读下面一组 `minimax_*` 字段,DeepSeek 的 key/endpoint/档位完全不动。
    /// 设计见 docs/MiniMax模型接入-2026-06-15.md。
    pub cloud_llm_backend: Option<String>,
    /// MiniMax API key(独立于 DeepSeek key,切后端互不覆盖)。
    #[serde(skip_serializing)]
    pub minimax_api_key: Option<String>,
    /// MiniMax endpoint base(默认 `https://api.minimaxi.com`;聊天真实路径
    /// `/v1/text/chatcompletion_v2` 由 LlmConfig 自动补,**不是** OpenAI 兼容的 /v1/chat/completions)。
    pub minimax_endpoint: Option<String>,
    /// MiniMax 模型名(**可编辑**自由文本,默认 `MiniMax-M2`)。MiniMax 官方型号名以控制台为准,
    /// 写错会 404 —— 故做成可填而非写死下拉,「以后适配更多模型」零改代码。
    pub minimax_model: Option<String>,
    /// MiniMax key 验证通过时间(坑#11:新 cloud key 必配 verified_at,改 key 重置)。
    pub minimax_verified_at: Option<String>,

    /// 2026-06-16:通用 OpenAI 兼容云端 LLM 后端(智谱 GLM / 小米 MiMo / 自定义)。
    /// `cloud_llm_backend` 取 `"glm"` / `"mimo"` / `"custom"` 时读对应服务商的独立配置。
    /// **纯增量调和**:DeepSeek(`cloud_llm_*`+档位)/ MiniMax(`minimax_*`+v2 协议)两条老路完全不动;
    /// 这条走标准 `/v1/chat/completions`,模型名是用户**显式填的具体型号**(不套 DeepSeek 的 flash/pro 档位,
    /// 同 MiniMax 处理)。glm/mimo/custom 的 key/endpoint/model 分开保存,切换服务商不会互相覆盖。
    /// 预设默认值见 `llm::providers`。
    ///
    /// 旧版 `compat_llm_*` 作为兼容字段保留:读当前后端时,如果新字段为空,会 fallback 到旧字段,
    /// 这样用户已经填过的配置不会因升级丢失。
    pub compat_llm_endpoint: Option<String>,
    /// 通用兼容后端模型名(具体型号,如 `glm-4.6`;自由文本,以服务商控制台为准)。
    pub compat_llm_model: Option<String>,
    /// 通用兼容后端 API key(独立于 DeepSeek / MiniMax)。
    #[serde(skip_serializing)]
    pub compat_llm_api_key: Option<String>,
    /// 通用兼容后端 key 验证通过时间(坑#11)。
    pub compat_llm_verified_at: Option<String>,

    /// 智谱 GLM 独立配置(OpenAI-compatible chat completions)。
    pub glm_llm_endpoint: Option<String>,
    pub glm_llm_model: Option<String>,
    #[serde(skip_serializing)]
    pub glm_llm_api_key: Option<String>,
    pub glm_llm_verified_at: Option<String>,

    /// 小米 MiMo 独立配置(OpenAI-compatible chat completions)。
    pub mimo_llm_endpoint: Option<String>,
    pub mimo_llm_model: Option<String>,
    #[serde(skip_serializing)]
    pub mimo_llm_api_key: Option<String>,
    pub mimo_llm_verified_at: Option<String>,

    /// 自定义 OpenAI 兼容模型独立配置。
    pub custom_llm_endpoint: Option<String>,
    pub custom_llm_model: Option<String>,
    #[serde(skip_serializing)]
    pub custom_llm_api_key: Option<String>,
    pub custom_llm_verified_at: Option<String>,

    /// 2026-05-24 k:元典法律开放平台 API key — 执行案件查被执行人 / 失信 / 财产线索 用
    /// 申请:https://open.chineselaw.com/
    #[serde(skip_serializing)]
    pub yuandian_api_key: Option<String>,

    /// 2026-06-01 V0.3:快递100 实时查询 customer 编号 + 授权 key(快递查询工具用)。
    /// 申请:https://api.kuaidi100.com/(个人免费版约 50 次/天,无需企业资质)。
    /// 签名 = 大写 MD5(param + key + customer)。两者都填了才启用快递查询。
    #[serde(skip_serializing)]
    pub kuaidi100_customer: Option<String>,
    #[serde(skip_serializing)]
    pub kuaidi100_key: Option<String>,

    /// 2026-06-01 V0.3.3:Embedding 云端模型(案件文档语义检索)。OpenAI 兼容 /embeddings。
    /// 默认硅基流动 BAAI/bge-m3(免费);填了 api_key 才启用语义检索,否则回退关键词选材料。
    /// 申请:https://cloud.siliconflow.cn/me/account/ak
    pub embedding_endpoint: Option<String>,
    pub embedding_model: Option<String>,
    #[serde(skip_serializing)]
    pub embedding_api_key: Option<String>,
    /// embedding key 验证通过时间(坑#11:新 cloud key 必配 verified_at,改 key 重置)
    pub embedding_verified_at: Option<String>,

    /// 本地知识库语义向量索引「自动维护」开关(出报告 / 启动后台增量索引)。
    /// `None`/`Some(true)` = 开(默认);`Some(false)` = 关(只手动重建)。
    pub kb_semantic_auto_index: Option<bool>,

    /// 2026-05-24 e:匿名反馈识别码(UUID v4),首次启动时自动生成 + 持久化。
    /// 跟用户名/邮箱无关 — 维护者拿到用户主动发送的反馈 MD 后可关联同一安装的多次反馈。
    /// 用户能在设置里清空重生成(类比换匿名 ID)。
    pub client_id: Option<String>,

    /// 2026-05-25 V0.1.6:MinerU key 通过验证的时间(ISO 8601)。
    /// 非 null = 用户点过「验证」按钮且通过,UI 显示绿勾。
    /// 用户改 key 会被清空(前端逻辑控制)。
    pub mineru_verified_at: Option<String>,
    /// DeepSeek key 通过验证的时间(同上)。
    pub deepseek_verified_at: Option<String>,
    /// 2026-05-25 V0.1.8:元典 key 通过验证的时间(同上)。
    pub yuandian_verified_at: Option<String>,

    /// 2026-05-26 V0.1.13:首页"在办案件"卡片用户拖动排序。
    /// 数组里的 case_id 按用户拖动后的顺序排;**没在数组里的案件**
    /// 按 listCases 默认顺序追加在末尾(新建案件不会被忘记)。
    /// 删过的案件 id 留在数组里也无害(前端 filter 掉)。
    pub home_case_order: Option<Vec<String>>,

    /// 2026-06-14:首页"日程日历"功能开关(默认关闭)。
    /// 该功能与待办清单略重复且卡片较大,做成可选 —— 用户在设置里手动打开体验,
    /// 不好用可关掉,不影响其他功能。`#[serde(default)]` → 老 settings.json 缺此字段时为 false。
    pub home_calendar_enabled: bool,

    // ===== 2026-06-17 飞书日历(整合外部贡献 PR #9,gcheng-001;精简为只读日历)=====
    /// 飞书日历总开关。默认关闭;启用后复用本机 lark-cli 的登录态,不在 CaseBoard 保存飞书 token。
    /// 配好并打开后,首页显示飞书日历月历视图(替代本地"日程日历"卡片)。
    pub feishu_enabled: Option<bool>,
    /// lark-cli 可执行文件路径。`None`/空 = 按平台自动找(macOS 走 Homebrew,Windows/Linux 靠 PATH)。
    /// Windows 用户可在此填 `lark-cli.exe` 全路径(没加进 PATH 时)。
    pub feishu_lark_cli_path: Option<String>,
    /// (可选)飞书"案件池"多维表格 App Token。配了才能"点日历事件→反查并导入本地案件目录"。
    pub feishu_app_token: Option<String>,
    /// (可选)飞书"案件池"多维表格 Table ID(配合 app_token)。
    pub feishu_cases_table_id: Option<String>,
    /// v0.8.4 待办“收件箱”使用独立绑定，不复用案件总表配置。
    pub feishu_todo_inbox_app_token: Option<String>,
    pub feishu_todo_inbox_table_id: Option<String>,
    pub feishu_todo_inbox_view_id: Option<String>,
    /// 飞书自建应用 App ID。不是密钥，可保存在设置文件中；App Secret 与 OAuth token 仅存系统凭据库。
    pub feishu_oauth_app_id: Option<String>,

    // ===== 2026-06-17 辅助在线立案(整合外部贡献 PR #8,gcheng-001)=====
    /// 立案 CLI 包根目录。None = 用应用内置 standalone/court_filing_cli(打包进 resources)。
    pub court_filing_cli_path: Option<String>,
    /// Python 解释器路径。None = 用 "python3"(Windows 用户需填 "python" 或 venv 内全路径)。
    pub court_filing_python: Option<String>,
    /// 全国法院一张网账号(手机号)。仅用于旧 settings.json 一次性迁移。
    #[serde(skip_serializing)]
    pub court_filing_account: Option<String>,
    /// 全国法院一张网密码。仅用于旧 settings.json 一次性迁移。
    #[serde(skip_serializing)]
    pub court_filing_password: Option<String>,
    /// 已废弃：法院登录态不持久化。仅反序列化用于清理旧设置。
    #[serde(skip_serializing)]
    pub court_filing_cookie_dir: Option<String>,

    // ===== V0.2 D2 新增 · 本地知识库 + chat V2 budget =====
    /// 2026-05-27 V0.2:本地法律知识库根目录(支持 `~/` tilde 展开)。
    /// `None` = 不启用本地 KB,所有元典查询都走在线 + DB 临时缓存;
    /// 指向一个存在的目录 = LocalKb::auto_detect 启用,元典缓存写入
    /// `<root>/raw/yuandian-cache/`,chat 工具优先查本地。
    pub local_kb_root: Option<String>,
    /// 本地 KB 总开关(为 false 时即使 local_kb_root 有值也不启用,给用户临时停用的能力)
    pub local_kb_enabled: Option<bool>,

    /// 元典积分月度上限(整数,单位:1 次普通查询 = 1 积分;聚合查询 = 5)。
    /// `None` = 不限制。超出阈值时,chat 自动降级到 KB Stale 命中,不再发起在线调用。
    pub yuandian_monthly_credit_limit: Option<u32>,

    // V0.3:模型档位已统一到 `cloud_llm_model`(flash / pro / 'auto' 自动挡),
    // 原 `chat_default_model` 字段已废弃移除(旧 settings.json 里的该键会被 serde 忽略)。
    /// chat 总上下文 char 预算(默认 300_000,~200K token)
    pub chat_context_budget_total: Option<u32>,
    /// chat system prompt + 案件快照 + 工具 schema 段 char 预算(默认 150_000)
    pub chat_context_budget_system: Option<u32>,
    /// chat 引用文档全文段 char 预算(默认 120_000)
    pub chat_context_budget_attached: Option<u32>,
    /// chat 历史对话段 char 预算(默认 40_000,超出走 compaction)
    pub chat_context_budget_history: Option<u32>,
    /// chat agent loop 最大迭代轮数(默认 12;见 with_defaults_for_display)
    pub chat_loop_max_iters: Option<u32>,
    /// chat 单条消息最多引用文档数(默认 5)
    pub chat_max_attached: Option<u32>,
    /// 2026-06-21 方律场景路由总开关。默认 false,关闭时聊天主链保持原行为。
    pub enable_fanglv_router: bool,
    /// Static provider credentials were migrated out of settings.json.
    pub credential_migration_version: Option<u32>,
    /// V0.3.6 · 外部 MCP server 白名单(CaseBoard 当客户端消费其工具)。默认空 = 桥接关闭、零行为变化。
    /// 每项 `{name, transport:{type:"stdio",command,args,env}|{type:"http",url}, enabled}`,详 ADR-0008。
    pub mcp_servers: Vec<McpStoredServer>,

    /// 2026-06-10 团队版 Phase 1(LAN 接力同步,详 docs/提案-团队版-2026-06-10.md §6)。
    /// None = 未加入团队,团队功能整体关闭零开销。secret/配对码跟 API key 同级:只存本机不进 git。
    pub team: Option<crate::team::TeamIdentity>,
}

/// Settings returned to the WebView. Static credential values are excluded by
/// `Settings` serialization and represented only by non-sensitive statuses.
#[derive(Serialize)]
pub struct PublicSettings {
    #[serde(flatten)]
    pub settings: Settings,
    pub credential_statuses: Vec<crate::credentials::CredentialStatus>,
}

impl PublicSettings {
    pub fn from_settings(settings: Settings) -> Self {
        Self {
            settings: settings.with_defaults_for_display(),
            credential_statuses: crate::credentials::static_statuses(),
        }
    }
}

impl Settings {
    /// 获取**真实生效**的 OCR provider。
    ///
    /// **V0.3(2026-05-31)暂时隐藏本地模型 → 强制云端(MinerU)。** 本地分支代码
    /// (`ingest/ocr.rs` 的 vision 路径)保留休眠;无论存的是 `cloud` / `local`(含老配置
    /// 残留)/ None,一律返回 `"cloud"`,顺带消化老用户 `ocr_provider="local"` 残留。
    /// **恢复本地**:把本函数改回读 `self.ocr_provider`(原逻辑见 git),同时恢复
    /// `effective_llm_provider` + `needs_local_server` 下游(pipeline 自动起 llama-server /
    /// feedback 诊断 / detect_local_readiness 引导)+ 前端 UI 入口即可。
    pub fn effective_ocr_provider(&self) -> &str {
        "cloud"
    }

    /// 云端 LLM 后端(2026-06-15;2026-06-16 加 OpenAI 兼容三档)。
    /// 取值:`"deepseek"`(默认)/ `"minimax"` / `"glm"` / `"mimo"` / `"custom"`。
    /// 缺省 / 空 / 非法值一律回落 `"deepseek"`(老用户零感知)。
    pub fn effective_cloud_llm_backend(&self) -> &str {
        match self.cloud_llm_backend.as_deref().map(str::trim) {
            Some("minimax") => "minimax",
            Some("glm") => "glm",
            Some("mimo") => "mimo",
            Some("custom") => "custom",
            _ => "deepseek",
        }
    }

    /// 是否走「通用 OpenAI 兼容」后端(glm / mimo / custom 共用 `compat_llm_*` + 标准 chat 协议)。
    pub fn cloud_llm_is_compat(&self) -> bool {
        matches!(
            self.effective_cloud_llm_backend(),
            "glm" | "mimo" | "custom"
        )
    }

    fn clean_string(value: &Option<String>) -> Option<String> {
        value
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    }

    /// 当前兼容后端的 endpoint。新字段优先,旧版 compat_llm_* 兜底。
    pub fn effective_compat_llm_endpoint(&self) -> Option<String> {
        let current = match self.effective_cloud_llm_backend() {
            "glm" => Self::clean_string(&self.glm_llm_endpoint),
            "mimo" => Self::clean_string(&self.mimo_llm_endpoint),
            "custom" => Self::clean_string(&self.custom_llm_endpoint),
            _ => None,
        };
        current.or_else(|| Self::clean_string(&self.compat_llm_endpoint))
    }

    /// 当前兼容后端的模型名。新字段优先,旧版 compat_llm_* 兜底。
    pub fn effective_compat_llm_model(&self) -> Option<String> {
        let current = match self.effective_cloud_llm_backend() {
            "glm" => Self::clean_string(&self.glm_llm_model),
            "mimo" => Self::clean_string(&self.mimo_llm_model),
            "custom" => Self::clean_string(&self.custom_llm_model),
            _ => None,
        };
        current.or_else(|| Self::clean_string(&self.compat_llm_model))
    }

    /// 一次性迁移:把旧的「共享 `compat_llm_*`」搬进**当前兼容后端**的专属字段,然后清空旧字段。
    ///
    /// 背景:旧设计 glm/mimo/custom 共用一组 `compat_llm_*`(切兼容后端会清空旧 key),所以旧值
    /// 总归属「当前激活的那个兼容后端」。整合 PR#15 后改成各家独立字段 + 旧字段兜底,但「兜底」会让
    /// 旧值跨后端串味(切到没填 key 的 MiMo 时会回落到上一个后端的旧 key/endpoint,verified 也错挂)。
    /// 迁移一次把旧值归位到专属字段并清空旧字段,此后 `effective_*` 的兜底恒为 no-op,串味消失。
    ///
    /// 幂等:旧字段已空 / 当前非兼容后端 → 不动,返回 `false`。返回 `true` 表示有改动需回写。
    pub fn migrate_legacy_compat_inplace(&mut self) -> bool {
        let le = Self::clean_string(&self.compat_llm_endpoint);
        let lm = Self::clean_string(&self.compat_llm_model);
        let lk = Self::clean_string(&self.compat_llm_api_key);
        let lv = Self::clean_string(&self.compat_llm_verified_at);
        if le.is_none() && lm.is_none() && lk.is_none() && lv.is_none() {
            return false; // 旧字段已空,迁移过了 / 从没用过
        }
        // 只在当前是兼容后端时迁移(旧值归属当前兼容后端);非兼容后端时旧值归属未知,
        // 留着不动也无害(deepseek/minimax 路径不读 compat_llm_*,不会串味)。
        let backend = self.effective_cloud_llm_backend().to_string();
        let (ep, md, key, ver) = match backend.as_str() {
            "glm" => (
                &mut self.glm_llm_endpoint,
                &mut self.glm_llm_model,
                &mut self.glm_llm_api_key,
                &mut self.glm_llm_verified_at,
            ),
            "mimo" => (
                &mut self.mimo_llm_endpoint,
                &mut self.mimo_llm_model,
                &mut self.mimo_llm_api_key,
                &mut self.mimo_llm_verified_at,
            ),
            "custom" => (
                &mut self.custom_llm_endpoint,
                &mut self.custom_llm_model,
                &mut self.custom_llm_api_key,
                &mut self.custom_llm_verified_at,
            ),
            _ => return false,
        };
        // 只填专属字段里为空的(已填的用户值优先,不覆盖)
        let fill = |dst: &mut Option<String>, src: Option<String>| {
            let dst_empty = dst
                .as_deref()
                .map(str::trim)
                .map(|x| x.is_empty())
                .unwrap_or(true);
            if dst_empty {
                if let Some(v) = src {
                    *dst = Some(v);
                }
            }
        };
        fill(ep, le);
        fill(md, lm);
        fill(key, lk);
        fill(ver, lv);
        // 清空旧共享字段:此后只认专属字段,杜绝跨后端串味
        self.compat_llm_endpoint = None;
        self.compat_llm_model = None;
        self.compat_llm_api_key = None;
        self.compat_llm_verified_at = None;
        true
    }

    /// 云端 OCR 主力偏好(2026-06-12)。这里只读取非敏感配置；
    /// token 是否可用由构造 `OcrContext` 时的凭据 resolver 和 OCR 排序过滤决定。
    pub fn effective_ocr_cloud_primary(&self) -> &str {
        match self.ocr_cloud_primary.as_deref() {
            Some("paddle-vl") => "paddle-vl",
            _ => "mineru",
        }
    }

    /// 获取**真实生效**的 LLM provider。**V0.3 暂时隐藏本地模型 → 强制云端(DeepSeek)。**
    /// 同 `effective_ocr_provider`:本地分支(`llm/mod.rs::from_settings` 的 else)保留休眠。
    pub fn effective_llm_provider(&self) -> &str {
        "cloud"
    }

    /// 任何一个 provider 用到了本机,就需要 llama-server。
    /// V0.3 隐藏本地后 `effective_*` 恒 cloud → 本函数恒 false(pipeline 不再自动起本机服务)。
    pub fn needs_local_server(&self) -> bool {
        self.effective_ocr_provider() == "local" || self.effective_llm_provider() == "local"
    }

    pub fn fanglv_router_enabled(&self) -> bool {
        self.enable_fanglv_router
    }
}

impl Settings {
    /// 给前端返回时,用 sensible 默认值补全空字段(便于直接渲染表单)。
    /// 注意:**这里不返回任何 token 默认值**——key 一律保持用户输入。
    pub fn with_defaults_for_display(self) -> Self {
        // 只对「有内置默认值」的字段填默认,其余字段一律 `..self` 原样透传。
        // 用 `..self` 而非逐字段手列:以后给 Settings 加字段会自动继承原值,
        // 不会因为这里漏写一行而被静默丢成默认(B14 防漏映射)。
        Self {
            local_server_auto_start: self.local_server_auto_start.or(Some(true)),
            mineru_endpoint: self
                .mineru_endpoint
                .or_else(|| Some("https://mineru.net/api/v4".to_string())),
            ollama_endpoint: self
                .ollama_endpoint
                .or_else(|| Some("http://127.0.0.1:8899".to_string())),
            ollama_model: self
                .ollama_model
                .or_else(|| Some("MiniCPM-V-4_6-Q8_0.gguf".to_string())),
            cloud_llm_endpoint: self
                .cloud_llm_endpoint
                .or_else(|| Some("https://api.deepseek.com".to_string())),
            cloud_llm_model: self
                .cloud_llm_model
                .or_else(|| Some("deepseek-v4-flash".to_string())),
            embedding_endpoint: self
                .embedding_endpoint
                .or_else(|| Some(crate::embedding::DEFAULT_ENDPOINT.to_string())),
            embedding_model: self
                .embedding_model
                .or_else(|| Some(crate::embedding::DEFAULT_MODEL.to_string())),
            chat_context_budget_total: self.chat_context_budget_total.or(Some(300_000)),
            chat_context_budget_system: self.chat_context_budget_system.or(Some(150_000)),
            chat_context_budget_attached: self.chat_context_budget_attached.or(Some(120_000)),
            chat_context_budget_history: self.chat_context_budget_history.or(Some(40_000)),
            chat_loop_max_iters: self.chat_loop_max_iters.or(Some(16)),
            chat_max_attached: self.chat_max_attached.or(Some(5)),
            ..self
        }
    }
}

/// 拿到 settings.json 的路径(跟 caseboard.db 在同一个目录)。
pub fn settings_path() -> Result<PathBuf, String> {
    Ok(app_data_dir()
        .map_err(|e| format!("找不到数据目录: {}", e))?
        .join("settings.json"))
}

fn static_secret(
    settings: &Settings,
    slot: crate::credentials::StaticCredential,
) -> Option<&String> {
    use crate::credentials::StaticCredential::*;
    match slot {
        Mineru => settings.mineru_api_key.as_ref(),
        PaddleVl => settings.paddle_vl_api_key.as_ref(),
        Deepseek => settings.cloud_llm_api_key.as_ref(),
        Minimax => settings.minimax_api_key.as_ref(),
        Glm => settings.glm_llm_api_key.as_ref(),
        Mimo => settings.mimo_llm_api_key.as_ref(),
        Custom => settings.custom_llm_api_key.as_ref(),
        Yuandian => settings.yuandian_api_key.as_ref(),
        KuaidiCustomer => settings.kuaidi100_customer.as_ref(),
        KuaidiKey => settings.kuaidi100_key.as_ref(),
        Embedding => settings.embedding_api_key.as_ref(),
        CourtFilingAccount => settings.court_filing_account.as_ref(),
        CourtFilingPassword => settings.court_filing_password.as_ref(),
    }
}

fn set_static_secret(
    settings: &mut Settings,
    slot: crate::credentials::StaticCredential,
    value: Option<String>,
) {
    use crate::credentials::StaticCredential::*;
    match slot {
        Mineru => settings.mineru_api_key = value,
        PaddleVl => settings.paddle_vl_api_key = value,
        Deepseek => settings.cloud_llm_api_key = value,
        Minimax => settings.minimax_api_key = value,
        Glm => settings.glm_llm_api_key = value,
        Mimo => settings.mimo_llm_api_key = value,
        Custom => settings.custom_llm_api_key = value,
        Yuandian => settings.yuandian_api_key = value,
        KuaidiCustomer => settings.kuaidi100_customer = value,
        KuaidiKey => settings.kuaidi100_key = value,
        Embedding => settings.embedding_api_key = value,
        CourtFilingAccount => settings.court_filing_account = value,
        CourtFilingPassword => settings.court_filing_password = value,
    }
}

const LEGACY_STATIC_CREDENTIALS: [crate::credentials::StaticCredential; 13] = [
    crate::credentials::StaticCredential::Mineru,
    crate::credentials::StaticCredential::PaddleVl,
    crate::credentials::StaticCredential::Deepseek,
    crate::credentials::StaticCredential::Minimax,
    crate::credentials::StaticCredential::Glm,
    crate::credentials::StaticCredential::Mimo,
    crate::credentials::StaticCredential::Custom,
    crate::credentials::StaticCredential::Yuandian,
    crate::credentials::StaticCredential::KuaidiCustomer,
    crate::credentials::StaticCredential::KuaidiKey,
    crate::credentials::StaticCredential::Embedding,
    crate::credentials::StaticCredential::CourtFilingAccount,
    crate::credentials::StaticCredential::CourtFilingPassword,
];

fn clear_static_secrets(settings: &mut Settings) {
    for slot in LEGACY_STATIC_CREDENTIALS {
        set_static_secret(settings, slot, None);
    }
    settings.compat_llm_api_key = None;
    settings.court_filing_cookie_dir = None;
}

const LEGACY_STATIC_SECRET_KEYS: [&str; 15] = [
    "mineru_api_key",
    "paddle_vl_api_key",
    "cloud_llm_api_key",
    "minimax_api_key",
    "compat_llm_api_key",
    "glm_llm_api_key",
    "mimo_llm_api_key",
    "custom_llm_api_key",
    "yuandian_api_key",
    "kuaidi100_customer",
    "kuaidi100_key",
    "embedding_api_key",
    "court_filing_account",
    "court_filing_password",
    "court_filing_cookie_dir",
];

fn sanitized_json(
    settings: &Settings,
    original: Option<&serde_json::Value>,
) -> Result<serde_json::Value, String> {
    let known =
        serde_json::to_value(settings).map_err(|_| "SETTINGS_SERIALIZE_FAILED".to_string())?;
    let mut result = original
        .cloned()
        .unwrap_or_else(|| serde_json::Value::Object(Default::default()));
    let result_object = result
        .as_object_mut()
        .ok_or_else(|| "SETTINGS_ROOT_NOT_OBJECT".to_string())?;
    let known_object = known
        .as_object()
        .ok_or_else(|| "SETTINGS_SERIALIZE_FAILED".to_string())?;
    for (key, value) in known_object {
        result_object.insert(key.clone(), value.clone());
    }
    for key in LEGACY_STATIC_SECRET_KEYS {
        result_object.remove(key);
    }
    Ok(result)
}

#[cfg(target_os = "windows")]
fn atomic_replace_file(temporary: &Path, target: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::Storage::FileSystem::{
        MoveFileExW, ReplaceFileW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
        REPLACEFILE_WRITE_THROUGH,
    };

    let temporary = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let target_wide = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if target.exists() {
        unsafe {
            ReplaceFileW(
                PCWSTR(target_wide.as_ptr()),
                PCWSTR(temporary.as_ptr()),
                PCWSTR::null(),
                REPLACEFILE_WRITE_THROUGH,
                None,
                None,
            )
        }
        .map_err(|_| "SETTINGS_ATOMIC_REPLACE_FAILED".to_string())
    } else {
        unsafe {
            MoveFileExW(
                PCWSTR(temporary.as_ptr()),
                PCWSTR(target_wide.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|_| "SETTINGS_ATOMIC_REPLACE_FAILED".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn atomic_replace_file(temporary: &Path, target: &Path) -> Result<(), String> {
    std::fs::rename(temporary, target).map_err(|_| "SETTINGS_ATOMIC_REPLACE_FAILED".to_string())
}

fn atomic_write_settings_with<F>(
    path: &Path,
    settings: &Settings,
    original: Option<&serde_json::Value>,
    replace: F,
) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    let value = sanitized_json(settings, original)?;
    atomic_write_json_value_with(path, &value, replace)
}

fn atomic_write_json_value_with<F>(
    path: &Path,
    value: &serde_json::Value,
    replace: F,
) -> Result<(), String>
where
    F: FnOnce(&Path, &Path) -> Result<(), String>,
{
    let parent = path
        .parent()
        .ok_or_else(|| "settings.json 路径无父目录".to_string())?;
    std::fs::create_dir_all(parent).map_err(|_| "SETTINGS_CREATE_DIR_FAILED".to_string())?;
    let bytes =
        serde_json::to_vec_pretty(value).map_err(|_| "SETTINGS_SERIALIZE_FAILED".to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|_| "SETTINGS_TEMPFILE_FAILED".to_string())?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.flush())
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|_| "SETTINGS_TEMPFILE_WRITE_FAILED".to_string())?;
    let temporary = temporary.into_temp_path();
    replace(temporary.as_ref(), path)
}

fn atomic_write_json_value(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    atomic_write_json_value_with(path, value, atomic_replace_file)
}

fn atomic_write_settings(
    path: &Path,
    settings: &Settings,
    original: Option<&serde_json::Value>,
) -> Result<(), String> {
    atomic_write_settings_with(path, settings, original, atomic_replace_file)
}

fn restore_snapshot<B: crate::credentials::CredentialBackend>(
    backend: &mut B,
    snapshot: &[(
        crate::credentials::CredentialLocator,
        Option<crate::credentials::SecretValue>,
    )],
) -> bool {
    let mut complete = true;
    for (locator, value) in snapshot {
        let restored = match value {
            Some(value) => backend.set(locator, value).is_ok(),
            None => backend.delete(locator).is_ok(),
        };
        complete &= restored;
    }
    complete
}

fn write_settings_with_backend<B: crate::credentials::CredentialBackend>(
    path: &Path,
    settings: &Settings,
    backend: &mut B,
    original: Option<&serde_json::Value>,
) -> Result<(), String> {
    let mut sanitized = settings.clone();
    clear_static_secrets(&mut sanitized);

    let mut updates = Vec::new();
    for slot in LEGACY_STATIC_CREDENTIALS {
        if let Some(value) = static_secret(settings, slot).filter(|value| !value.trim().is_empty())
        {
            let secret = crate::credentials::SecretValue::new(value.clone())
                .map_err(|error| error.code().to_string())?;
            updates.push((slot.locator(), secret));
        }
    }

    if updates.is_empty() {
        return atomic_write_settings(path, &sanitized, original);
    }

    let mut snapshot = Vec::with_capacity(updates.len());
    for (locator, _) in &updates {
        snapshot.push((
            locator.clone(),
            backend
                .get(locator)
                .map_err(|error| error.code().to_string())?,
        ));
    }
    for (locator, value) in &updates {
        if let Err(error) = crate::credentials::replace_verified_with(backend, locator, value) {
            let restored = restore_snapshot(backend, &snapshot);
            return Err(if restored {
                error.code().to_string()
            } else {
                crate::credentials::CredentialError::RollbackFailed
                    .code()
                    .to_string()
            });
        }
    }
    sanitized.credential_migration_version = Some(1);
    if let Err(error) = atomic_write_settings(path, &sanitized, original) {
        if !restore_snapshot(backend, &snapshot) {
            return Err(crate::credentials::CredentialError::RollbackFailed
                .code()
                .to_string());
        }
        return Err(error);
    }
    Ok(())
}

fn existing_settings_json(path: &Path) -> Result<Option<serde_json::Value>, String> {
    if !path.exists() {
        return Ok(None);
    }
    let bytes = std::fs::read(path).map_err(|_| "SETTINGS_EXISTING_READ_FAILED".to_string())?;
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|_| "SETTINGS_EXISTING_PARSE_FAILED".to_string())?;
    if !value.is_object() {
        return Err("SETTINGS_ROOT_NOT_OBJECT".to_string());
    }
    Ok(Some(value))
}

fn write_settings_preserving_existing_with_backend<B: crate::credentials::CredentialBackend>(
    path: &Path,
    settings: &Settings,
    backend: &mut B,
) -> Result<(), String> {
    let original = existing_settings_json(path)?;
    write_settings_with_backend(path, settings, backend, original.as_ref())
}

fn read_settings_with_backend<B: crate::credentials::CredentialBackend>(
    path: &Path,
    backend: &mut B,
) -> Result<Settings, String> {
    if !path.exists() {
        return Ok(Settings::default());
    }
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("读 settings.json 失败: {}", e))?;
    if text.trim().is_empty() {
        return Ok(Settings::default());
    }
    let original = serde_json::from_str::<serde_json::Value>(&text)
        .map_err(|e| format!("settings.json 格式错误: {}", e))?;
    let raw_mcp_servers = original.get("mcp_servers").cloned();
    let mut settings_without_mcp = original.clone();
    settings_without_mcp
        .as_object_mut()
        .ok_or_else(|| "SETTINGS_ROOT_NOT_OBJECT".to_string())?
        .remove("mcp_servers");
    let mut settings = serde_json::from_value::<Settings>(settings_without_mcp)
        .map_err(|e| format!("settings.json 格式错误: {}", e))?;
    let legacy_team = settings.team.clone().filter(|identity| {
        !identity.team_secret.trim().is_empty()
            || identity
                .pairing_code
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
    });
    let had_plaintext = LEGACY_STATIC_CREDENTIALS
        .iter()
        .any(|slot| static_secret(&settings, *slot).is_some_and(|value| !value.trim().is_empty()))
        || settings
            .compat_llm_api_key
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty());
    let compat_migrated = settings.migrate_legacy_compat_inplace();
    if settings
        .compat_llm_api_key
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err("CREDENTIAL_MIGRATION_AMBIGUOUS_COMPAT".to_string());
    }
    if let Some(identity) = legacy_team {
        let sanitized = crate::team::credentials::migrate_legacy_identity_with(
            &identity,
            backend,
            |_, sanitized| {
                let mut cleaned = original.clone();
                cleaned
                    .as_object_mut()
                    .ok_or_else(|| "SETTINGS_ROOT_NOT_OBJECT".to_string())?
                    .insert(
                        "team".to_string(),
                        serde_json::to_value(sanitized)
                            .map_err(|_| "SETTINGS_SERIALIZE_FAILED".to_string())?,
                    );
                atomic_write_json_value(path, &cleaned)
            },
        )?;
        settings.team = Some(sanitized);
    }
    let mut migrated_legacy_mcp = false;
    if let Some(raw_mcp_servers) = raw_mcp_servers.filter(|value| !value.is_null()) {
        match serde_json::from_value::<Vec<McpStoredServer>>(raw_mcp_servers.clone()) {
            Ok(stored) => settings.mcp_servers = stored,
            Err(_) => {
                let legacy = serde_json::from_value::<Vec<McpServerConfig>>(raw_mcp_servers)
                    .map_err(|_| "MCP_CONFIG_INVALID".to_string())?;
                let server_ids = vec![None; legacy.len()];
                crate::chat::mcp_credentials::migrate_legacy_servers_with(
                    backend,
                    &legacy,
                    &server_ids,
                    |backend, stored| {
                        settings.mcp_servers = stored.to_vec();
                        write_settings_with_backend(path, &settings, backend, Some(&original))
                            .map_err(|_| ())
                    },
                )
                .map_err(|error| error.code().to_string())?;
                migrated_legacy_mcp = true;
            }
        }
    }
    if !migrated_legacy_mcp && (had_plaintext || compat_migrated) {
        write_settings_with_backend(path, &settings, backend, Some(&original))?;
    }
    // 普通 Settings 对象绝不水合静态凭据。旧 settings.json 中的明文只在上面的
    // 一次性迁移局部变量中短暂存在，成功写入凭据后立即清空再返回。
    clear_static_secrets(&mut settings);
    settings.compat_llm_api_key = None;
    Ok(settings)
}

struct PreserveExistingCredentials<'a, B> {
    inner: &'a mut B,
    protected: HashSet<String>,
    shadow: HashMap<String, Option<crate::credentials::SecretValue>>,
}

impl<B: crate::credentials::CredentialBackend> crate::credentials::CredentialBackend
    for PreserveExistingCredentials<'_, B>
{
    fn set(
        &mut self,
        locator: &crate::credentials::CredentialLocator,
        secret: &crate::credentials::SecretValue,
    ) -> Result<(), crate::credentials::CredentialError> {
        if self.protected.contains(locator.id()) {
            self.shadow
                .insert(locator.id().to_string(), Some(secret.clone()));
            Ok(())
        } else {
            self.inner.set(locator, secret)
        }
    }

    fn get(
        &mut self,
        locator: &crate::credentials::CredentialLocator,
    ) -> Result<Option<crate::credentials::SecretValue>, crate::credentials::CredentialError> {
        if self.protected.contains(locator.id()) {
            if let Some(value) = self.shadow.get(locator.id()) {
                return Ok(value.clone());
            }
        }
        self.inner.get(locator)
    }

    fn delete(
        &mut self,
        locator: &crate::credentials::CredentialLocator,
    ) -> Result<(), crate::credentials::CredentialError> {
        if self.protected.contains(locator.id()) {
            self.shadow.insert(locator.id().to_string(), None);
            Ok(())
        } else {
            self.inner.delete(locator)
        }
    }
}

fn current_preferred_locators(
    current_path: &Path,
) -> Result<Vec<crate::credentials::CredentialLocator>, String> {
    let mut locators = crate::credentials::StaticCredential::ALL
        .iter()
        .map(|slot| slot.locator())
        .collect::<Vec<_>>();
    let Some(original) = existing_settings_json(current_path)? else {
        return Ok(locators);
    };
    let raw_mcp_servers = original.get("mcp_servers").cloned();
    let mut settings_without_mcp = original;
    settings_without_mcp
        .as_object_mut()
        .ok_or_else(|| "SETTINGS_ROOT_NOT_OBJECT".to_string())?
        .remove("mcp_servers");
    let settings = serde_json::from_value::<Settings>(settings_without_mcp)
        .map_err(|_| "SETTINGS_EXISTING_PARSE_FAILED".to_string())?;
    if let Some(team) = settings.team {
        if let Ok(locator) = crate::team::credentials::secret_locator(&team.team_id) {
            locators.push(locator);
        }
        if let Ok(locator) = crate::team::credentials::pairing_locator(&team.team_id) {
            locators.push(locator);
        }
    }
    if let Some(raw) = raw_mcp_servers.filter(|value| !value.is_null()) {
        if let Ok(stored) = serde_json::from_value::<Vec<McpStoredServer>>(raw) {
            for server in stored {
                locators.extend(
                    server
                        .credential_locators()
                        .map_err(|error| error.code().to_string())?,
                );
            }
        }
    }
    Ok(locators)
}

fn migrate_legacy_settings_before_current_with_backend<B: crate::credentials::CredentialBackend>(
    current_path: &Path,
    legacy_path: Option<&Path>,
    backend: &mut B,
) -> Result<(), String> {
    let Some(legacy_path) = legacy_path else {
        return Ok(());
    };
    if current_path == legacy_path || !legacy_path.exists() {
        return Ok(());
    }
    // Existing credentials referenced by the current directory are authoritative.
    // The legacy migration may fill a missing locator, but it can only shadow an
    // existing current value while verifying its own atomic cleanup.
    let mut protected = HashSet::new();
    for locator in current_preferred_locators(current_path)? {
        if backend
            .get(&locator)
            .map_err(|error| error.code().to_string())?
            .is_some()
        {
            protected.insert(locator.id().to_string());
        }
    }
    let mut preserving = PreserveExistingCredentials {
        inner: backend,
        protected,
        shadow: HashMap::new(),
    };
    read_settings_with_backend(legacy_path, &mut preserving).map(|_| ())
}

/// 读取设置。文件不存在时返回 `Settings::default()`。
/// 静态 provider 凭据永不进入返回值；调用方必须使用 credentials resolver。
pub fn read_settings() -> Result<Settings, String> {
    with_settings_read_migration_lock(|| {
        let mut backend = crate::credentials::SystemCredentialBackend;
        if let Some((current_dir, legacy_dir)) =
            crate::db::default_data_dirs_if_unoverridden().map_err(|error| error.to_string())?
        {
            migrate_legacy_settings_before_current_with_backend(
                &current_dir.join("settings.json"),
                Some(&legacy_dir.join("settings.json")),
                &mut backend,
            )?;
        }
        // Resolve the active path only after the legacy settings cleanup. In
        // default mode this may copy the now-sanitized legacy directory when
        // the current database does not exist. Override mode never reaches the
        // branch above and therefore never touches either default directory.
        let path = settings_path()?;
        read_settings_with_backend(&path, &mut backend)
    })
}

/// 写入设置(覆盖)。会自动创建父目录。
pub fn write_settings(settings: &Settings) -> Result<(), String> {
    let path = settings_path()?;
    write_settings_preserving_existing_with_backend(
        &path,
        settings,
        &mut crate::credentials::SystemCredentialBackend,
    )
}

pub(crate) fn write_settings_using_backend<B: crate::credentials::CredentialBackend>(
    settings: &Settings,
    backend: &mut B,
) -> Result<(), String> {
    let path = settings_path()?;
    write_settings_preserving_existing_with_backend(&path, settings, backend)
}

/// 2026-05-24 e:确保 client_id 存在(给反馈通道用的匿名识别码)。
///
/// 如果 settings.client_id 为空,生成新 UUID v4 并持久化;返回最终的 client_id。
/// 跟用户名/邮箱无关,纯随机。维护者拿到用户主动发送的反馈 MD 后可关联同一安装。
pub fn ensure_client_id() -> Result<String, String> {
    let mut s = read_settings()?;
    if let Some(existing) = s.client_id.as_ref() {
        if !existing.trim().is_empty() {
            return Ok(existing.clone());
        }
    }
    let new_id = uuid::Uuid::new_v4().to_string();
    s.client_id = Some(new_id.clone());
    write_settings(&s)?;
    Ok(new_id)
}

#[cfg(test)]
mod secure_settings_tests {
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex as TestMutex};
    use std::time::Duration;

    use super::*;
    use crate::credentials::{
        CredentialBackend, CredentialError, CredentialLocator, SecretValue, StaticCredential,
    };

    #[derive(Default)]
    struct MemoryBackend {
        values: HashMap<String, String>,
    }

    impl CredentialBackend for MemoryBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            self.values
                .insert(locator.id().to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.values
                .get(locator.id())
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.values.remove(locator.id());
            Ok(())
        }
    }

    #[derive(Clone)]
    struct ConcurrentBackend {
        values: Arc<TestMutex<HashMap<String, String>>>,
        fail_next_set: Arc<AtomicBool>,
    }

    impl CredentialBackend for ConcurrentBackend {
        fn set(
            &mut self,
            locator: &CredentialLocator,
            secret: &SecretValue,
        ) -> Result<(), CredentialError> {
            if self.fail_next_set.swap(false, Ordering::SeqCst) {
                return Err(CredentialError::SecureStore);
            }
            self.values
                .lock()
                .expect("store")
                .insert(locator.id().to_string(), secret.expose().to_string());
            Ok(())
        }

        fn get(
            &mut self,
            locator: &CredentialLocator,
        ) -> Result<Option<SecretValue>, CredentialError> {
            self.values
                .lock()
                .expect("store")
                .get(locator.id())
                .cloned()
                .map(SecretValue::new)
                .transpose()
        }

        fn delete(&mut self, locator: &CredentialLocator) -> Result<(), CredentialError> {
            self.values.lock().expect("store").remove(locator.id());
            Ok(())
        }
    }

    fn assert_static_secrets_absent(settings: &Settings) {
        for slot in LEGACY_STATIC_CREDENTIALS {
            assert!(
                static_secret(settings, slot).is_none(),
                "{} must not be hydrated into Settings",
                slot.locator().id()
            );
        }
        assert!(settings.compat_llm_api_key.is_none());
    }

    #[test]
    fn read_settings_migrates_legacy_secret_without_hydrating_settings() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let legacy_marker = "legacy-mineru-secret";
        let court_account = "legacy-court-account";
        let court_password = "legacy-court-password";
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "mineru_api_key": legacy_marker,
                "ocr_cloud_primary": "paddle-vl",
                "court_filing_account": court_account,
                "court_filing_password": court_password,
                "court_filing_cookie_dir": "C:/legacy/plaintext-cookies"
            }))
            .expect("json"),
        )
        .expect("write");
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &StaticCredential::PaddleVl.locator(),
                &SecretValue::new("saved-paddle-secret".to_string()).expect("secret"),
            )
            .expect("seed");

        let settings =
            read_settings_with_backend(&path, &mut backend).expect("read and migrate settings");

        assert_static_secrets_absent(&settings);
        assert_eq!(settings.effective_ocr_cloud_primary(), "paddle-vl");
        assert!(
            crate::credentials::status_with(&mut backend, &StaticCredential::Mineru.locator())
                .configured
        );
        assert!(
            crate::credentials::status_with(&mut backend, &StaticCredential::PaddleVl.locator())
                .configured
        );
        assert_eq!(
            backend
                .get(&StaticCredential::CourtFilingAccount.locator())
                .expect("vault")
                .expect("court account")
                .expose(),
            court_account
        );
        assert_eq!(
            backend
                .get(&StaticCredential::CourtFilingPassword.locator())
                .expect("vault")
                .expect("court password")
                .expose(),
            court_password
        );
        let disk = std::fs::read_to_string(path).expect("settings file");
        assert!(!disk.contains(legacy_marker));
        assert!(!disk.contains("mineru_api_key"));
        assert!(!disk.contains(court_account));
        assert!(!disk.contains(court_password));
        assert!(!disk.contains("court_filing_account"));
        assert!(!disk.contains("court_filing_password"));
        assert!(!disk.contains("court_filing_cookie_dir"));
    }

    #[test]
    fn read_settings_migrates_team_credentials_and_cleans_plaintext() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let team_id = "11111111-2222-3333-4444-555555555555";
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "team": {
                    "team_id": team_id,
                    "team_name": "Test Team",
                    "team_secret": "legacy-team-secret",
                    "member_id": "member-1",
                    "my_name": "Alice",
                    "role": "leader",
                    "pairing_code": "654321"
                }
            }))
            .expect("json"),
        )
        .expect("write");
        let mut backend = MemoryBackend::default();

        let settings = read_settings_with_backend(&path, &mut backend).expect("migrate");
        let identity = settings.team.expect("team");
        assert!(identity.team_secret.is_empty());
        assert!(identity.pairing_code.is_none());
        let disk = std::fs::read_to_string(path).expect("settings");
        assert!(!disk.contains("legacy-team-secret"));
        assert!(!disk.contains("654321"));
        assert!(!disk.contains("team_secret"));
        assert!(!disk.contains("pairing_code"));
        assert_eq!(
            backend
                .get(&crate::team::credentials::secret_locator(team_id).expect("locator"))
                .expect("vault")
                .expect("secret")
                .expose(),
            "legacy-team-secret"
        );
        assert_eq!(
            backend
                .get(&crate::team::credentials::pairing_locator(team_id).expect("locator"))
                .expect("vault")
                .expect("code")
                .expose(),
            "654321"
        );
    }

    #[test]
    fn settings_and_public_settings_never_serialize_team_credentials() {
        let settings = Settings {
            team: Some(crate::team::TeamIdentity {
                team_id: "team-1".into(),
                team_name: "Test Team".into(),
                team_secret: "team-secret-marker".into(),
                member_id: "member-1".into(),
                my_name: "Alice".into(),
                role: "leader".into(),
                pairing_code: Some("123456".into()),
            }),
            ..Settings::default()
        };
        for json in [
            serde_json::to_string(&settings).expect("settings"),
            serde_json::to_string(&PublicSettings::from_settings(settings)).expect("public"),
        ] {
            assert!(!json.contains("team-secret-marker"));
            assert!(!json.contains("123456"));
            assert!(!json.contains("team_secret"));
            assert!(!json.contains("pairing_code"));
        }
    }

    #[test]
    fn read_settings_migrates_legacy_mcp_without_webview_or_disk_secret() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let marker = "mcp-legacy-header-marker";
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&serde_json::json!({
                "mcp_servers": [{
                    "name": "legal-data",
                    "enabled": true,
                    "transport": {
                        "type": "http",
                        "url": "https://example.invalid/mcp",
                        "headers": {"Authorization": marker}
                    }
                }]
            }))
            .expect("json"),
        )
        .expect("write");
        let mut backend = MemoryBackend::default();

        let settings = read_settings_with_backend(&path, &mut backend).expect("migrate MCP");

        assert_eq!(settings.mcp_servers.len(), 1);
        assert!(settings.mcp_servers[0].complete.configured);
        let public_json = serde_json::to_string(&PublicSettings {
            settings: settings.clone(),
            credential_statuses: Vec::new(),
        })
        .expect("WebView projection");
        let disk = std::fs::read_to_string(path).expect("settings file");
        assert!(!public_json.contains(marker));
        assert!(!disk.contains(marker));
        assert!(backend.values.values().any(|value| value == marker));
    }

    #[test]
    fn legacy_mcp_secret_argv_keeps_original_settings_and_existing_vault_entries() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let original = br#"{"mcp_servers":[{"name":"unsafe","enabled":true,"transport":{"type":"stdio","command":"node","args":["server.js","--token","legacy-argv-secret"],"env":{}}}],"keep":"byte-for-byte"}"#;
        std::fs::write(&path, original).expect("seed");
        let existing_locator = CredentialLocator::new(
            "mcp",
            "11111111-2222-3333-4444-555555555555",
            "stdio-env-api-key",
        )
        .expect("locator");
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &existing_locator,
                &SecretValue::new("existing-vault-value".to_string()).expect("secret"),
            )
            .expect("seed vault");
        let before = backend.values.clone();

        let result = read_settings_with_backend(&path, &mut backend);

        assert_eq!(
            result.err().expect("migration must fail"),
            "MCP_STDIO_SECRET_ARG_FORBIDDEN_USE_SECRET_ENV"
        );
        assert_eq!(std::fs::read(path).expect("original"), original);
        assert_eq!(backend.values, before);
    }

    #[test]
    fn legacy_cleanup_preserves_existing_current_credential() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current = temp.path().join("current-settings.json");
        let legacy = temp.path().join("legacy-settings.json");
        std::fs::write(&current, b"{}").expect("current");
        std::fs::write(&legacy, br#"{"mineru_api_key":"legacy-mineru-value"}"#).expect("legacy");
        let locator = StaticCredential::Mineru.locator();
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &locator,
                &SecretValue::new("current-vault-value".to_string()).expect("secret"),
            )
            .expect("seed");

        migrate_legacy_settings_before_current_with_backend(&current, Some(&legacy), &mut backend)
            .expect("legacy cleanup");

        assert_eq!(
            backend
                .get(&locator)
                .expect("vault")
                .expect("credential")
                .expose(),
            "current-vault-value"
        );
        assert!(!std::fs::read_to_string(legacy)
            .expect("legacy")
            .contains("mineru_api_key"));
    }

    #[test]
    fn current_plaintext_migration_wins_after_legacy_cleanup() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current = temp.path().join("current-settings.json");
        let legacy = temp.path().join("legacy-settings.json");
        std::fs::write(&current, br#"{"mineru_api_key":"current-plaintext-value"}"#)
            .expect("current");
        std::fs::write(&legacy, br#"{"mineru_api_key":"legacy-plaintext-value"}"#).expect("legacy");
        let locator = StaticCredential::Mineru.locator();
        let mut backend = MemoryBackend::default();

        migrate_legacy_settings_before_current_with_backend(&current, Some(&legacy), &mut backend)
            .expect("legacy cleanup");
        read_settings_with_backend(&current, &mut backend).expect("current migration");

        assert_eq!(
            backend
                .get(&locator)
                .expect("vault")
                .expect("credential")
                .expose(),
            "current-plaintext-value"
        );
        for path in [&current, &legacy] {
            assert!(!std::fs::read_to_string(path)
                .expect("settings")
                .contains("mineru_api_key"));
        }
    }

    #[test]
    fn override_mode_skips_legacy_settings_entirely() {
        let temp = tempfile::tempdir().expect("tempdir");
        let active = temp.path().join("override-settings.json");
        let legacy = temp.path().join("default-legacy-settings.json");
        std::fs::write(&active, b"{}").expect("active");
        let original = br#"{"mineru_api_key":"must-remain-untouched"}"#;
        std::fs::write(&legacy, original).expect("legacy");
        let mut backend = MemoryBackend::default();

        migrate_legacy_settings_before_current_with_backend(&active, None, &mut backend)
            .expect("override must skip");

        assert_eq!(std::fs::read(legacy).expect("legacy"), original);
        assert!(backend.values.is_empty());
    }

    #[test]
    fn legacy_migration_failure_keeps_file_and_vault_unchanged() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current = temp.path().join("current-settings.json");
        let legacy = temp.path().join("legacy-settings.json");
        std::fs::write(&current, b"{}").expect("current");
        let original = br#"{"mcp_servers":[{"name":"unsafe","enabled":true,"transport":{"type":"stdio","command":"node","args":["--token","legacy-secret"],"env":{}}}]}"#;
        std::fs::write(&legacy, original).expect("legacy");
        let locator = StaticCredential::Mineru.locator();
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &locator,
                &SecretValue::new("current-vault-value".to_string()).expect("secret"),
            )
            .expect("seed");
        let before = backend.values.clone();

        let result = migrate_legacy_settings_before_current_with_backend(
            &current,
            Some(&legacy),
            &mut backend,
        );

        assert_eq!(
            result.expect_err("must fail"),
            "MCP_STDIO_SECRET_ARG_FORBIDDEN_USE_SECRET_ENV"
        );
        assert_eq!(std::fs::read(legacy).expect("legacy"), original);
        assert_eq!(backend.values, before);
    }

    #[test]
    fn concurrent_double_directory_migration_is_serial_and_recovers_after_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let current = temp.path().join("current-settings.json");
        let legacy = temp.path().join("legacy-settings.json");
        let current_values = [
            ("mineru_api_key", "current-mineru"),
            ("paddle_vl_api_key", "current-paddle"),
            ("cloud_llm_api_key", "current-deepseek"),
            ("yuandian_api_key", "current-yuandian"),
            ("kuaidi100_customer", "current-kuaidi-customer"),
            ("kuaidi100_key", "current-kuaidi-key"),
        ];
        let current_json = serde_json::Value::Object(
            current_values
                .iter()
                .map(|(key, value)| {
                    (
                        (*key).to_string(),
                        serde_json::Value::String((*value).to_string()),
                    )
                })
                .collect(),
        );
        std::fs::write(
            &current,
            serde_json::to_vec(&current_json).expect("current json"),
        )
        .expect("current");
        std::fs::write(
            &legacy,
            serde_json::to_vec(&serde_json::json!({
                "mineru_api_key": "legacy-mineru",
                "paddle_vl_api_key": "legacy-paddle",
                "cloud_llm_api_key": "legacy-deepseek"
            }))
            .expect("legacy json"),
        )
        .expect("legacy");

        let values = Arc::new(TestMutex::new(HashMap::new()));
        let fail_next_set = Arc::new(AtomicBool::new(true));
        let start = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let current = current.clone();
            let legacy = legacy.clone();
            let values = values.clone();
            let fail_next_set = fail_next_set.clone();
            let start = start.clone();
            let active = active.clone();
            let max_active = max_active.clone();
            handles.push(std::thread::spawn(move || {
                start.wait();
                with_settings_read_migration_lock(|| {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    max_active.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    let mut backend = ConcurrentBackend {
                        values,
                        fail_next_set,
                    };
                    let result = migrate_legacy_settings_before_current_with_backend(
                        &current,
                        Some(&legacy),
                        &mut backend,
                    )
                    .and_then(|_| read_settings_with_backend(&current, &mut backend));
                    active.fetch_sub(1, Ordering::SeqCst);
                    result
                })
            }));
        }
        let results = handles
            .into_iter()
            .map(|handle| handle.join().expect("thread"))
            .collect::<Vec<_>>();

        assert_eq!(max_active.load(Ordering::SeqCst), 1);
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(results.iter().filter(|result| result.is_err()).count(), 1);
        let stored = values.lock().expect("store");
        for (slot, expected) in [
            (StaticCredential::Mineru, "current-mineru"),
            (StaticCredential::PaddleVl, "current-paddle"),
            (StaticCredential::Deepseek, "current-deepseek"),
            (StaticCredential::Yuandian, "current-yuandian"),
            (StaticCredential::KuaidiCustomer, "current-kuaidi-customer"),
            (StaticCredential::KuaidiKey, "current-kuaidi-key"),
        ] {
            assert_eq!(
                stored.get(slot.locator().id()).map(String::as_str),
                Some(expected)
            );
        }
        drop(stored);
        for path in [&current, &legacy] {
            let disk = std::fs::read_to_string(path).expect("settings");
            for (key, _) in &current_values {
                assert!(!disk.contains(key), "{key} must be scrubbed");
            }
        }
    }

    #[test]
    fn mcp_migration_persist_failure_restores_vault_and_original_bytes() {
        use crate::chat::mcp_bridge::{McpServerConfig, McpTransport};
        use std::collections::BTreeMap;

        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let original = br#"{"mcp_servers":[],"keep":"byte-for-byte"}"#;
        std::fs::write(&path, original).expect("seed");
        let marker = "mcp-rollback-marker";
        let legacy = McpServerConfig {
            name: "rollback".to_string(),
            enabled: true,
            transport: McpTransport::Http {
                url: "https://example.invalid/mcp".to_string(),
                headers: BTreeMap::from([("Authorization".to_string(), marker.to_string())]),
            },
        };
        let mut backend = MemoryBackend::default();
        let mut settings = Settings::default();

        let result = crate::chat::mcp_credentials::migrate_legacy_servers_with(
            &mut backend,
            &[legacy],
            &[None],
            |_, stored| {
                settings.mcp_servers = stored.to_vec();
                atomic_write_settings_with(&path, &settings, None, |_, _| {
                    Err("FORCED_REPLACE_FAILURE".to_string())
                })
                .map_err(|_| ())
            },
        );

        assert_eq!(
            result.unwrap_err().code(),
            "MCP_CONFIG_ATOMIC_PERSIST_FAILED"
        );
        assert!(backend.values.is_empty());
        assert_eq!(std::fs::read(path).expect("original"), original);
    }

    #[test]
    fn public_settings_serialization_never_contains_static_secrets() {
        let marker = "must-not-cross-public-boundary";
        let mut settings = Settings {
            mineru_api_key: Some(marker.to_string()),
            paddle_vl_api_key: Some(marker.to_string()),
            cloud_llm_api_key: Some(marker.to_string()),
            minimax_api_key: Some(marker.to_string()),
            glm_llm_api_key: Some(marker.to_string()),
            mimo_llm_api_key: Some(marker.to_string()),
            custom_llm_api_key: Some(marker.to_string()),
            yuandian_api_key: Some(marker.to_string()),
            kuaidi100_customer: Some(marker.to_string()),
            kuaidi100_key: Some(marker.to_string()),
            embedding_api_key: Some(marker.to_string()),
            ..Settings::default()
        };
        settings.compat_llm_api_key = Some(marker.to_string());
        let public = PublicSettings {
            settings,
            credential_statuses: vec![crate::credentials::CredentialStatus {
                locator: StaticCredential::Mineru.locator().id().to_string(),
                configured: true,
                backend: "windows_credential_manager",
                error_code: None,
            }],
        };

        let json = serde_json::to_string(&public).expect("serialize");

        assert!(!json.contains(marker));
        for key in LEGACY_STATIC_SECRET_KEYS {
            assert!(!json.contains(key), "{key} leaked into PublicSettings");
        }
    }

    #[test]
    fn settings_json_excludes_static_provider_fields_and_values() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let marker = "settings-secret-marker";
        let settings = Settings {
            mineru_api_key: Some(marker.to_string()),
            ..Settings::default()
        };
        let mut backend = MemoryBackend::default();
        let original = serde_json::json!({
            "mineru_api_key": marker,
            "future_non_secret_setting": {"enabled": true}
        });

        write_settings_with_backend(&path, &settings, &mut backend, Some(&original))
            .expect("secure write");
        let disk = std::fs::read_to_string(path).expect("settings file");
        assert!(!disk.contains(marker));
        assert!(!disk.contains("mineru_api_key"));
        assert!(disk.contains("future_non_secret_setting"));
        assert!(disk.contains("\"credential_migration_version\": 1"));
        assert_eq!(
            backend
                .get(&StaticCredential::Mineru.locator())
                .expect("vault")
                .expect("stored")
                .expose(),
            marker
        );
    }

    #[test]
    fn failed_atomic_replace_restores_previous_credential() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        std::fs::create_dir(&path).expect("force persist failure");
        let locator = StaticCredential::Mineru.locator();
        let mut backend = MemoryBackend::default();
        backend
            .set(
                &locator,
                &SecretValue::new("old-marker".to_string()).expect("old"),
            )
            .expect("seed");
        let settings = Settings {
            mineru_api_key: Some("new-marker".to_string()),
            ..Settings::default()
        };

        assert_eq!(
            write_settings_with_backend(&path, &settings, &mut backend, None),
            Err("SETTINGS_ATOMIC_REPLACE_FAILED".to_string())
        );
        assert_eq!(
            backend
                .get(&locator)
                .expect("vault")
                .expect("restored")
                .expose(),
            "old-marker"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_atomic_replace_replaces_an_existing_file() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        std::fs::write(&path, br#"{"old":true}"#).expect("seed");
        let settings = Settings {
            setup_completed: true,
            ..Settings::default()
        };

        atomic_write_settings(&path, &settings, None).expect("replace existing file");
        let replaced = std::fs::read_to_string(path).expect("replaced file");
        assert!(replaced.contains("\"setup_completed\": true"));
        assert!(!replaced.contains("\"old\": true"));
    }

    #[test]
    fn forced_atomic_replace_failure_keeps_original_bytes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let original = br#"{"original":"byte-for-byte"}"#;
        std::fs::write(&path, original).expect("seed");
        let settings = Settings {
            setup_completed: true,
            ..Settings::default()
        };

        let result = atomic_write_settings_with(&path, &settings, None, |_, _| {
            Err("FORCED_REPLACE_FAILURE".to_string())
        });
        assert_eq!(result, Err("FORCED_REPLACE_FAILURE".to_string()));
        assert_eq!(std::fs::read(path).expect("original"), original);
    }

    #[test]
    fn daily_write_preserves_unknown_non_secret_keys() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        std::fs::write(
            &path,
            br#"{"future_non_secret":{"enabled":true},"setup_completed":false}"#,
        )
        .expect("seed");
        let settings = Settings {
            setup_completed: true,
            ..Settings::default()
        };
        let mut backend = MemoryBackend::default();

        write_settings_preserving_existing_with_backend(&path, &settings, &mut backend)
            .expect("daily write");
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(path).expect("written")).expect("json");
        assert_eq!(value["future_non_secret"]["enabled"], true);
        assert_eq!(value["setup_completed"], true);
    }

    #[test]
    fn daily_write_refuses_to_overwrite_unreadable_json() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.json");
        let original = b"{not-json";
        std::fs::write(&path, original).expect("seed");
        let mut backend = MemoryBackend::default();

        assert_eq!(
            write_settings_preserving_existing_with_backend(
                &path,
                &Settings::default(),
                &mut backend,
            ),
            Err("SETTINGS_EXISTING_PARSE_FAILED".to_string())
        );
        assert_eq!(std::fs::read(path).expect("original"), original);
    }
}

// ============================================================================
// 测试
// ============================================================================
