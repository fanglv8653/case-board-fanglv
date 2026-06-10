//! MCP-bridge:CaseBoard 当 MCP **客户端**,消费外部 MCP server 工具(治「扩展麻烦」=加能力不必改 Rust 重出 dmg)。
//!
//! 详见 `docs/adr/0008-MCP-bridge-CaseBoard当客户端消费外部工具.md`。**已落地(2026-06-02)**:
//! 手搓零依赖 stdio JSON-RPC 客户端(`McpClient`:initialize→initialized→tools/list→tools/call,
//! 按 id 匹配跳过通知 + 超时 + `kill_on_drop`)+ 配置形状(`McpServerConfig`/`McpTransport`)+
//! 转发工具(`McpForwardingTool` impl `Tool`)+ 编排(`connect_mcp_servers`,失败跳过+dlog+按名排序)。
//! 配置存 `settings.mcp_servers`(白名单,默认空 = 桥接关闭、零开销);在 `commands::case_chat_impl`
//! 起手连接,绑一次 chat 调用(registry drop → 子进程被杀)。前端配置 UI = `SettingsModal` 的
//! `McpServersCard`(增删/启用/stdio command·args·env)。
//!
//! **端到端已实测(2026-06-04)**:① python stub 协议往返(`mcp_roundtrip`,本地无网);
//! ② 真实官方 server `@modelcontextprotocol/server-everything`(`mcp_real_server`,需网络+npx);
//! ③ 真实 inputSchema(带 `$schema`/`additionalProperties`/`default`)过 `to_function_schema`
//! 后被 DeepSeek function-calling 正常接受并回 tool_call(真 key 实测,无需 schema 清洗)。
//! 两个真连测均 `#[ignore]`(离线不挂)。**HTTP 传输待实现**(connect 对 http 返回「待实现」)。
//!
//! 标 `allow(dead_code)`:`parse_server_configs` / `DiscoveredTool::to_function_schema` /
//! `McpTransport::Http` 暂留作未来/测试用,非死代码遗留。

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::process::Stdio;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::Mutex;
use tokio::time::{timeout, Duration};

use super::tools::{Tool, ToolContext, ToolError, ToolResult};

/// 外部 MCP server 的传输方式。两种传输共用此配置形状,与「rmcp 还是手搓」的实现决策无关。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum McpTransport {
    /// 本地子进程,走 stdio JSON-RPC(如 `npx -y @modelcontextprotocol/server-xxx`)。
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        /// 额外环境变量(放 token 等;**不进 git/日志**)。
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    /// 远端 HTTP/SSE endpoint。
    Http { url: String },
}

/// 一个外部 MCP server 的配置项(存 settings.json 或表,**存储无关**:从任意 JSON 反序列化)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpServerConfig {
    /// 人读名,也用作工具命名空间前缀(见 [`DiscoveredTool::namespaced_name`])。
    pub name: String,
    pub transport: McpTransport,
    /// 是否启用。白名单语义:只连 `enabled=true` 的;整个列表默认空 = 桥接关闭、行为不变。
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

impl McpServerConfig {
    /// 校验配置可用:name 非空;stdio 的 command 非空 / http 的 url 非空。
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("MCP server name 不能为空".into());
        }
        match &self.transport {
            McpTransport::Stdio { command, .. } if command.trim().is_empty() => Err(format!(
                "MCP server「{}」的 stdio command 不能为空",
                self.name
            )),
            McpTransport::Http { url } if url.trim().is_empty() => {
                Err(format!("MCP server「{}」的 http url 不能为空", self.name))
            }
            _ => Ok(()),
        }
    }
}

/// 从一段 JSON(期望是 server 配置数组,如 `settings.mcp_servers`)防御式解析出配置列表。
/// 非数组 → 空;单条反序列化失败 → 跳过该条(不整体失败)。**不**做 enabled/validate 过滤,
/// 调用方再 `.filter(|c| c.enabled && c.validate().is_ok())` 取「该连的 server」。
pub fn parse_server_configs(value: &Value) -> Vec<McpServerConfig> {
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|v| serde_json::from_value::<McpServerConfig>(v.clone()).ok())
        .collect()
}

/// 远端 MCP server `tools/list` 返回的单个工具元数据。
///
/// 这是「能直接并进 DeepSeek tools 数组」的形态:[`Self::to_function_schema`] 跟内置
/// `Tool::to_function_schema` 同形。无论传输怎么实现,远端工具都归一到此形状。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiscoveredTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// MCP 的 `inputSchema`(JSON Schema)。
    #[serde(rename = "inputSchema", default)]
    pub input_schema: Value,
}

/// 把一段名字清洗成 DeepSeek/OpenAI function 名允许的字符集(`[A-Za-z0-9_-]`),其余 → `_`。
fn sanitize_fn_segment(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

impl DiscoveredTool {
    /// 加 server 命名空间前缀避免跟内置工具 / 其他 server 重名(`mcp__<server>__<tool>`,
    /// 与 Claude Code 的 MCP 工具命名一致)。
    ///
    /// DeepSeek/OpenAI function 名约 `^[A-Za-z0-9_-]+$`。server 名用户可填中文、远端工具名也可能
    /// 带怪字符 → 非法字符清洗成 `_`,**兜底不让整个 tools 数组被 API 拒**(一个坏名会废掉整轮 chat)。
    /// 实际调用远端用的是 `McpForwardingTool::remote_name`(原 `self.name`),不受此清洗影响。
    pub fn namespaced_name(&self, server: &str) -> String {
        format!(
            "mcp__{}__{}",
            sanitize_fn_segment(server),
            sanitize_fn_segment(&self.name)
        )
    }

    /// 转成 DeepSeek `tools` 数组单条。`tool_name` 由调用方传(一般是 namespaced)。
    pub fn to_function_schema(&self, tool_name: &str) -> Value {
        let parameters = if self.input_schema.is_null() {
            serde_json::json!({ "type": "object", "properties": {} })
        } else {
            self.input_schema.clone()
        };
        serde_json::json!({
            "type": "function",
            "function": {
                "name": tool_name,
                "description": self.description,
                "parameters": parameters,
            }
        })
    }
}

// =============================================================================
// MCP stdio JSON-RPC 客户端(手搓零依赖,见 ADR-0008 §4:对齐已知坑 #5 MinerU 客户端先例)。
// 协议:newline-delimited JSON-RPC 2.0 over stdio。握手:initialize → notifications/initialized
// → tools/list / tools/call。**真连外部 server 无法 headless 验**,有 #[ignore] 的 python stub 往返测兜底。
// =============================================================================

const MCP_PROTOCOL_VERSION: &str = "2024-11-05";
const MCP_INIT_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_LIST_TIMEOUT: Duration = Duration::from_secs(15);
const MCP_CALL_TIMEOUT: Duration = Duration::from_secs(60);

/// 一条 stdio 连接的 IO。字段按声明序 drop:先关 stdin/stdout(server 多半随之退出),
/// 再 drop child(`kill_on_drop` 兜底杀进程)。
struct McpIo {
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    _child: Child,
}

/// 已完成 initialize 握手的外部 MCP server 连接。
///
/// 单条 stdio 管道上的请求/响应必须**串行**,故内部 `Mutex` 包 IO;多 server = 多 client 互不干扰。
/// `McpClient` drop → 子进程被杀(`kill_on_drop`,生命周期绑一次 chat 调用)。
pub struct McpClient {
    io: Mutex<McpIo>,
    next_id: AtomicI64,
}

impl McpClient {
    fn next_id(&self) -> i64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// spawn 子进程 + 完成 initialize 握手。失败返回可读原因。
    pub async fn connect(cfg: &McpServerConfig) -> Result<Self, String> {
        let McpTransport::Stdio { command, args, env } = &cfg.transport else {
            return Err("暂只支持 stdio 传输(http 待实现)".into());
        };
        let mut child = Command::new(command)
            .args(args)
            .envs(env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null()) // 排空 stderr,防其缓冲填满挂死子进程
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("启动失败: {e}"))?;
        let stdin = child.stdin.take().ok_or("无法取得 stdin")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("无法取得 stdout")?);
        let mut io = McpIo {
            stdin,
            stdout,
            _child: child,
        };

        // initialize(id=0)
        let init = json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": "CaseBoard", "version": env!("CARGO_PKG_VERSION") }
        });
        rpc_request(&mut io, 0, "initialize", init, MCP_INIT_TIMEOUT).await?;
        // initialized 通知(spec 要求;缺它部分 server 拒 tools/list)
        rpc_notify(&mut io, "notifications/initialized").await?;

        Ok(Self {
            io: Mutex::new(io),
            next_id: AtomicI64::new(1),
        })
    }

    /// tools/list:发现远端工具。
    pub async fn list_tools(&self) -> Result<Vec<DiscoveredTool>, String> {
        let id = self.next_id();
        let mut io = self.io.lock().await;
        let result = rpc_request(&mut io, id, "tools/list", json!({}), MCP_LIST_TIMEOUT).await?;
        let arr = result
            .get("tools")
            .and_then(|t| t.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(arr
            .iter()
            .filter_map(|t| serde_json::from_value(t.clone()).ok())
            .collect())
    }

    /// tools/call:调远端工具,返回拼好的文本结果。
    pub async fn call_tool(&self, name: &str, arguments: &Value) -> Result<String, String> {
        let id = self.next_id();
        let mut io = self.io.lock().await;
        let params = json!({ "name": name, "arguments": arguments });
        let result = rpc_request(&mut io, id, "tools/call", params, MCP_CALL_TIMEOUT).await?;
        Ok(extract_tool_text(&result))
    }
}

/// 发 JSON-RPC 请求 + 读到匹配 id 的响应(跳过通知/日志/别的 id),带超时。
async fn rpc_request(
    io: &mut McpIo,
    id: i64,
    method: &str,
    params: Value,
    to: Duration,
) -> Result<Value, String> {
    let msg = json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params });
    write_line(&mut io.stdin, &msg).await?;
    match timeout(to, read_matching(&mut io.stdout, id)).await {
        Ok(r) => r,
        Err(_) => Err(format!("MCP {method} 超时({}s)", to.as_secs())),
    }
}

/// 发 JSON-RPC 通知(无 id,不等响应)。
async fn rpc_notify(io: &mut McpIo, method: &str) -> Result<(), String> {
    let msg = json!({ "jsonrpc": "2.0", "method": method });
    write_line(&mut io.stdin, &msg).await
}

async fn write_line(stdin: &mut ChildStdin, msg: &Value) -> Result<(), String> {
    let mut line = serde_json::to_string(msg).map_err(|e| e.to_string())?;
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .await
        .map_err(|e| format!("写 MCP 请求失败: {e}"))?;
    stdin.flush().await.map_err(|e| format!("flush 失败: {e}"))
}

/// 逐行读到 id 匹配的响应;跳过通知(无 id)、日志、不同 id 的行 —— server 会在响应前穿插
/// log 通知,"读一行=我的响应"是经典 bug。泛型化以便单测。
async fn read_matching<R: tokio::io::AsyncBufRead + Unpin>(
    stdout: &mut R,
    want_id: i64,
) -> Result<Value, String> {
    loop {
        let mut line = String::new();
        let n = stdout
            .read_line(&mut line)
            .await
            .map_err(|e| format!("读 MCP 响应失败: {e}"))?;
        if n == 0 {
            return Err("MCP server 关闭了连接(EOF)".into());
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(trimmed) else {
            continue; // 非 JSON(日志噪音)→ 跳过
        };
        match v.get("id").and_then(|i| i.as_i64()) {
            Some(id) if id == want_id => {
                if let Some(err) = v.get("error") {
                    return Err(format!("MCP 返回错误: {err}"));
                }
                return Ok(v.get("result").cloned().unwrap_or(Value::Null));
            }
            _ => continue, // 通知 / 其它 id → 跳过
        }
    }
}

/// 从 tools/call 结果抽文本(`result.content = [{type:text,text},...]`);带 isError 标记。
fn extract_tool_text(result: &Value) -> String {
    let mut out = String::new();
    if let Some(blocks) = result.get("content").and_then(|c| c.as_array()) {
        for b in blocks {
            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(t);
            }
        }
    }
    if out.is_empty() {
        // 无文本块(图片/资源等少见类型)→ 整个 result 压成 JSON 兜底
        out = serde_json::to_string(result).unwrap_or_default();
    }
    if result
        .get("isError")
        .and_then(|e| e.as_bool())
        .unwrap_or(false)
    {
        format!("[MCP 工具报错] {out}")
    } else {
        out
    }
}

/// 把一个远端 MCP 工具包成本仓的一等 `Tool`,execute 转发到远端。
pub struct McpForwardingTool {
    full_name: String, // mcp__<server>__<tool>
    description: String,
    parameters: Value, // inputSchema
    remote_name: String,
    client: Arc<McpClient>,
}

#[async_trait]
impl Tool for McpForwardingTool {
    fn name(&self) -> &str {
        &self.full_name
    }
    fn description(&self) -> &str {
        &self.description
    }
    fn parameters_schema(&self) -> Value {
        self.parameters.clone()
    }
    async fn execute(&self, args: &Value, _ctx: &ToolContext<'_>) -> Result<ToolResult, ToolError> {
        match self.client.call_tool(&self.remote_name, args).await {
            Ok(text) => Ok(ToolResult::plain(text)),
            Err(e) => Err(ToolError::Runtime(format!(
                "MCP 工具 {} 调用失败: {e}",
                self.full_name
            ))),
        }
    }
}

/// 连接所有 enabled 的 MCP server、发现工具、包成转发工具。失败(配置非法/连不上/列不出)
/// 的 server **跳过 + dlog**,绝不拖垮 chat。结果按工具名**确定性排序**(保前缀缓存稳定)。
/// **隐私**:只 dlog server 名 + 工具数,绝不记 tool-call 参数(含案件内容)。
pub async fn connect_mcp_servers(configs: &[McpServerConfig]) -> Vec<Box<dyn Tool>> {
    let mut tools: Vec<Box<dyn Tool>> = Vec::new();
    for cfg in configs.iter().filter(|c| c.enabled) {
        if let Err(e) = cfg.validate() {
            crate::dlog!("MCP server「{}」配置无效,跳过: {}", cfg.name, e);
            continue;
        }
        let client = match McpClient::connect(cfg).await {
            Ok(c) => Arc::new(c),
            Err(e) => {
                crate::dlog!("MCP server「{}」连接失败,跳过: {}", cfg.name, e);
                continue;
            }
        };
        let discovered = match client.list_tools().await {
            Ok(d) => d,
            Err(e) => {
                crate::dlog!("MCP server「{}」列工具失败,跳过: {}", cfg.name, e);
                continue;
            }
        };
        crate::dlog!(
            "MCP server「{}」已连,发现 {} 个工具",
            cfg.name,
            discovered.len()
        );
        for dt in discovered {
            let parameters = if dt.input_schema.is_null() {
                json!({ "type": "object", "properties": {} })
            } else {
                dt.input_schema.clone()
            };
            tools.push(Box::new(McpForwardingTool {
                full_name: dt.namespaced_name(&cfg.name),
                description: dt.description.clone(),
                parameters,
                remote_name: dt.name.clone(),
                client: client.clone(),
            }));
        }
    }
    // 确定性顺序 → 前缀缓存稳定(prefix_cache 观测 tools 指纹漂移)
    tools.sort_by(|a, b| a.name().cmp(b.name()));
    tools
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_text_concats_text_blocks() {
        let r =
            json!({"content": [{"type":"text","text":"hello"}, {"type":"text","text":"world"}]});
        assert_eq!(extract_tool_text(&r), "hello\nworld");
    }

    #[test]
    fn extract_text_flags_error() {
        let r = json!({"content": [{"type":"text","text":"boom"}], "isError": true});
        assert!(extract_tool_text(&r).contains("报错"));
    }

    #[tokio::test]
    async fn read_matching_skips_notifications_and_matches_id() {
        // server 先吐 log 通知(无 id)、再吐别的 id,最后才是要的 id=7 —— 必须跳过前两条
        let data: &[u8] = b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/message\"}\n{\"jsonrpc\":\"2.0\",\"id\":99,\"result\":{\"x\":1}}\n{\"jsonrpc\":\"2.0\",\"id\":7,\"result\":{\"ok\":true}}\n";
        let mut r = BufReader::new(data);
        let v = read_matching(&mut r, 7).await.unwrap();
        assert_eq!(v["ok"], true);
    }

    #[tokio::test]
    async fn read_matching_propagates_rpc_error() {
        let data: &[u8] =
            b"{\"jsonrpc\":\"2.0\",\"id\":3,\"error\":{\"code\":-32601,\"message\":\"no\"}}\n";
        let mut r = BufReader::new(data);
        assert!(read_matching(&mut r, 3).await.is_err());
    }

    #[tokio::test]
    async fn read_matching_eof_errors() {
        let data: &[u8] = b"";
        let mut r = BufReader::new(data);
        assert!(read_matching(&mut r, 1).await.is_err());
    }

    #[tokio::test]
    #[ignore = "需 python3,手动验真子进程往返: cargo test mcp_roundtrip -- --ignored"]
    async fn mcp_roundtrip_against_python_stub() {
        let cfg = McpServerConfig {
            name: "stub".into(),
            transport: McpTransport::Stdio {
                command: "python3".into(),
                args: vec![
                    concat!(env!("CARGO_MANIFEST_DIR"), "/../scripts/mcp_stub_server.py").into(),
                ],
                env: BTreeMap::new(),
            },
            enabled: true,
        };
        let client = McpClient::connect(&cfg).await.expect("connect+handshake");
        let tools = client.list_tools().await.expect("list_tools");
        assert!(tools.iter().any(|t| t.name == "echo"), "stub 应暴露 echo");
        let out = client
            .call_tool("echo", &json!({"msg": "hi"}))
            .await
            .expect("call echo");
        assert!(out.contains("hi"), "echo 应回显 hi");
    }

    /// **真实外部 server 端到端**(2026-06-04 已实测通过)。连官方参考 server
    /// `@modelcontextprotocol/server-everything`,验 connect→list→call 全链路 +
    /// 把真实 `inputSchema` 过 `to_function_schema` 看 DeepSeek function-calling 收不收
    /// (这是真 server 相对 python stub 唯一新增的高价值信息 —— stub 的 schema 是我们写的,
    /// 真 server 可能带 `$schema`/`$ref`/`additionalProperties`/`format` 等 DeepSeek 可能挑剔的关键字)。
    /// **网络 + npx 依赖**,故 `#[ignore]`(不进默认 `cargo test`,离线也不会挂)。
    /// 跑前先预热:`npx -y @modelcontextprotocol/server-everything </dev/null`(首次下载可能超 15s 握手超时)。
    /// 运行:`cargo test mcp_real_server -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "需网络+npx,手动验真实 MCP server: cargo test mcp_real_server -- --ignored --nocapture"]
    async fn mcp_real_server_everything_roundtrip() {
        let cfg = McpServerConfig {
            name: "everything".into(),
            transport: McpTransport::Stdio {
                command: "npx".into(),
                args: vec![
                    "-y".into(),
                    "@modelcontextprotocol/server-everything".into(),
                ],
                env: BTreeMap::new(),
            },
            enabled: true,
        };
        let client = McpClient::connect(&cfg).await.expect("connect+handshake");
        let tools = client.list_tools().await.expect("list_tools");
        assert!(!tools.is_empty(), "真实 server 应暴露工具");

        // dump 每个工具的真实 inputSchema 过 to_function_schema 后的形状,人工检视 DeepSeek 兼容性
        for t in &tools {
            let schema = t.to_function_schema(&t.namespaced_name(&cfg.name));
            println!(
                "[MCP-real] {} ::\n{}",
                t.name,
                serde_json::to_string_pretty(&schema).unwrap()
            );
        }

        // echo 是 server-everything 的稳定工具;参数名是 message
        let echo = tools
            .iter()
            .find(|t| t.name == "echo")
            .expect("有 echo 工具");
        let out = client
            .call_tool(&echo.name, &json!({"message": "hi-from-caseboard"}))
            .await
            .expect("call echo");
        assert!(
            out.contains("hi-from-caseboard"),
            "echo 应回显输入,实得: {out}"
        );
    }

    #[test]
    fn parse_stdio_config() {
        let v = json!([{
            "name": "filesystem",
            "transport": { "type": "stdio", "command": "npx", "args": ["-y", "server-fs"] }
        }]);
        let cfgs = parse_server_configs(&v);
        assert_eq!(cfgs.len(), 1);
        assert_eq!(cfgs[0].name, "filesystem");
        assert!(cfgs[0].enabled, "enabled 缺省应为 true");
        assert!(cfgs[0].validate().is_ok());
        match &cfgs[0].transport {
            McpTransport::Stdio { command, args, .. } => {
                assert_eq!(command, "npx");
                assert_eq!(args, &vec!["-y".to_string(), "server-fs".to_string()]);
            }
            _ => panic!("应是 stdio"),
        }
    }

    #[test]
    fn parse_http_config_disabled() {
        let v = json!([{
            "name": "remote",
            "transport": { "type": "http", "url": "http://localhost:3000/mcp" },
            "enabled": false
        }]);
        let cfgs = parse_server_configs(&v);
        assert_eq!(cfgs.len(), 1);
        assert!(!cfgs[0].enabled);
        assert!(matches!(cfgs[0].transport, McpTransport::Http { .. }));
    }

    #[test]
    fn parse_skips_malformed_entries_keeps_good() {
        let v = json!([
            { "name": "ok", "transport": { "type": "stdio", "command": "x" } },
            { "name": "bad", "transport": { "type": "nonsense" } },
            "not even an object"
        ]);
        let cfgs = parse_server_configs(&v);
        assert_eq!(cfgs.len(), 1, "坏条目应被跳过,好的保留");
        assert_eq!(cfgs[0].name, "ok");
    }

    #[test]
    fn parse_non_array_is_empty() {
        assert!(parse_server_configs(&json!({"x": 1})).is_empty());
        assert!(parse_server_configs(&json!(null)).is_empty());
    }

    #[test]
    fn validate_rejects_empty_name_and_command() {
        let empty_name = McpServerConfig {
            name: "  ".into(),
            transport: McpTransport::Stdio {
                command: "x".into(),
                args: vec![],
                env: BTreeMap::new(),
            },
            enabled: true,
        };
        assert!(empty_name.validate().is_err());

        let empty_cmd = McpServerConfig {
            name: "s".into(),
            transport: McpTransport::Stdio {
                command: "".into(),
                args: vec![],
                env: BTreeMap::new(),
            },
            enabled: true,
        };
        assert!(empty_cmd.validate().is_err());
    }

    #[test]
    fn enabled_and_valid_filter() {
        let v = json!([
            { "name": "a", "transport": { "type": "stdio", "command": "x" }, "enabled": true },
            { "name": "b", "transport": { "type": "stdio", "command": "y" }, "enabled": false },
            { "name": "c", "transport": { "type": "stdio", "command": "" }, "enabled": true }
        ]);
        let active: Vec<_> = parse_server_configs(&v)
            .into_iter()
            .filter(|c| c.enabled && c.validate().is_ok())
            .collect();
        assert_eq!(active.len(), 1, "只 a 该连(b 禁用,c command 空)");
        assert_eq!(active[0].name, "a");
    }

    #[test]
    fn discovered_tool_to_function_schema_matches_deepseek_shape() {
        let dt = DiscoveredTool {
            name: "read_file".into(),
            description: "读文件".into(),
            input_schema: json!({"type": "object", "properties": {"path": {"type": "string"}}}),
        };
        let s = dt.to_function_schema(&dt.namespaced_name("fs"));
        assert_eq!(s["type"], "function");
        assert_eq!(s["function"]["name"], "mcp__fs__read_file");
        assert_eq!(s["function"]["description"], "读文件");
        assert_eq!(
            s["function"]["parameters"]["properties"]["path"]["type"],
            "string"
        );
    }

    #[test]
    fn namespaced_name_sanitizes_to_valid_function_name() {
        // 用户填中文 server 名 + 远端工具名带怪字符 → 清洗后仍是合法 function 名,不致整轮被拒
        let dt = DiscoveredTool {
            name: "read.file!".into(),
            description: String::new(),
            input_schema: Value::Null,
        };
        let full = dt.namespaced_name("文件系统");
        assert!(
            full.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "清洗后只剩 [A-Za-z0-9_-],实得: {full}"
        );
        assert!(full.starts_with("mcp__"));
        // ASCII 名不被改动(回归:已 probe 过的 everything/echo 形状不变)
        let ascii = DiscoveredTool {
            name: "echo".into(),
            description: String::new(),
            input_schema: Value::Null,
        };
        assert_eq!(ascii.namespaced_name("everything"), "mcp__everything__echo");
    }

    #[test]
    fn discovered_tool_null_schema_defaults_to_object() {
        let dt = DiscoveredTool {
            name: "ping".into(),
            description: String::new(),
            input_schema: Value::Null,
        };
        let s = dt.to_function_schema("ping");
        assert_eq!(s["function"]["parameters"]["type"], "object");
    }

    #[test]
    fn discovered_tool_parses_mcp_inputschema_field() {
        // MCP 协议字段名是 camelCase 的 inputSchema
        let dt: DiscoveredTool = serde_json::from_value(json!({
            "name": "t",
            "description": "d",
            "inputSchema": {"type": "object"}
        }))
        .unwrap();
        assert_eq!(dt.input_schema["type"], "object");
    }
}
