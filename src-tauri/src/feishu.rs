//! 飞书日历读取(精简自外部贡献 PR #9,gcheng-001)。
//!
//! 只复用本机 `lark-cli --as user` 的登录态,CaseBoard **不保存飞书 token**。
//! 本模块只做两件事:
//!   1. 读飞书日历(`lark-cli calendar +agenda`)→ 首页月历展示;
//!   2. (可选)按事件标题在飞书"案件池"多维表格里反查本地案件目录 → 一键导入。
//!
//! 原 PR 的反向同步(案件→飞书表)、到期提醒推送未纳入(摘增量、避免 dead code)。
//!
//! 跨平台:lark-cli 在 macOS 走 Homebrew 路径,其他平台(Windows/Linux)靠 PATH
//! 找 `lark-cli`(Windows 会自动匹配 `lark-cli.exe`);也可在设置里填 CLI 全路径。

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::process::Command;
use tokio::time::timeout;

use crate::settings::Settings;

const LARK_CLI_TIMEOUT: Duration = Duration::from_secs(30);
const BITABLE_MAX_PAGES: usize = 50;
const BITABLE_FIELD_MAX_PAGES: usize = 5;

/// 飞书日历事件(传给前端月历)。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuCalendarEvent {
    pub event_id: String,
    pub summary: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub is_all_day: bool,
    pub description: Option<String>,
    pub location: Option<String>,
    pub app_link: Option<String>,
}

/// 案件管理预演用的飞书记录。只保留字段值和远端修改时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuRemoteCaseRecord {
    pub record_id: String,
    pub fields: Value,
    pub last_modified_time: Option<String>,
}

/// 计算 lark-cli 可执行文件:优先用设置里填的全路径,否则按平台兜底。
pub fn lark_bin(settings: &Settings) -> String {
    if let Some(p) = settings.feishu_lark_cli_path.as_deref() {
        let p = p.trim();
        if !p.is_empty() {
            return p.to_string();
        }
    }
    default_lark_bin()
}

fn default_lark_bin() -> String {
    #[cfg(target_os = "macos")]
    {
        if Path::new("/opt/homebrew/bin/lark-cli").exists() {
            return "/opt/homebrew/bin/lark-cli".to_string();
        }
        if Path::new("/usr/local/bin/lark-cli").exists() {
            return "/usr/local/bin/lark-cli".to_string();
        }
    }
    // Windows / Linux:靠系统 PATH(Windows 自动补 .exe)。
    "lark-cli".to_string()
}

/// 统一注入 lark-cli 运行环境。
///
/// PATH 注入只在 Unix 生效:macOS 下 Tauri 应用进程的 PATH 常缺 Homebrew 目录,
/// 必须补上才找得到 lark-cli。**Windows 上绝不能覆盖 PATH** —— 那会让系统找不到
/// `lark-cli.exe`(它不在这些 Unix 目录里),是致命 bug。
fn apply_lark_env(cmd: &mut Command) {
    cmd.env("LARK_CLI_NO_PROXY", "1");
    #[cfg(unix)]
    cmd.env(
        "PATH",
        "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
    );
    // Windows 下隐藏 lark-cli 控制台窗口,否则每次取飞书日历都闪一个黑色命令框。
    crate::proc_util::hide_console_window(cmd);
}

/// 调一次 lark-cli 的 `api` 子命令(复用用户登录态),返回解析后的 JSON。
async fn lark_cli_api(
    bin: &str,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> Result<Value, String> {
    let mut cmd = Command::new(bin);
    apply_lark_env(&mut cmd);
    cmd.arg("api")
        .arg(method)
        .arg(path)
        .arg("--as")
        .arg("user")
        .arg("--format")
        .arg("json");

    if let Some(body) = body {
        cmd.arg("--data")
            .arg(serde_json::to_string(&body).map_err(|e| e.to_string())?);
    }

    let output = timeout(LARK_CLI_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "lark-cli 调用超时".to_string())?
        .map_err(|e| format!("无法启动 lark-cli(确认已安装并加入 PATH): {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "lark-cli 调用失败: {}{}",
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" · {}", stdout.trim())
            }
        ));
    }

    let stdout =
        String::from_utf8(output.stdout).map_err(|e| format!("lark-cli 输出非 UTF-8: {}", e))?;
    let value: Value =
        serde_json::from_str(&stdout).map_err(|e| format!("lark-cli 输出非 JSON: {}", e))?;
    ensure_lark_ok(value)
}

fn ensure_lark_ok(value: Value) -> Result<Value, String> {
    let mut value = value;
    if let Some(ok) = value.get("ok").and_then(Value::as_bool) {
        if !ok {
            let error = value.get("error").cloned().unwrap_or(Value::Null);
            let message = error
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or("未知错误");
            let code = error
                .get("code")
                .map(Value::to_string)
                .unwrap_or_else(|| "unknown".to_string());
            return Err(format!("飞书 CLI 返回错误 code={code}: {message}"));
        }
        value = value.get("data").cloned().unwrap_or(Value::Null);
    }

    if let Some(code) = value.get("code").and_then(Value::as_i64) {
        if code != 0 {
            let msg = value
                .get("msg")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            return Err(format!("飞书 API 返回 code={}: {}", code, msg));
        }
    }
    Ok(value)
}

fn response_data(value: &Value) -> &Value {
    value.get("data").unwrap_or(value)
}

fn value_as_time_string(value: Option<&Value>) -> Option<String> {
    match value {
        Some(Value::String(value)) if !value.trim().is_empty() => Some(value.clone()),
        Some(Value::Number(value)) => Some(value.to_string()),
        _ => None,
    }
}

fn parse_case_records(
    value: &Value,
) -> Result<(Vec<FeishuRemoteCaseRecord>, Option<String>), String> {
    let data = response_data(value);
    let items: &[Value] = match data.get("items") {
        Some(Value::Array(items)) => items,
        // 飞书的空结果页可能省略 `items`。只有服务端同时明确声明
        // total=0 且 has_more=false 时才按空列表处理，避免吞掉真实协议变化。
        None if data.get("total").and_then(Value::as_u64) == Some(0)
            && data.get("has_more").and_then(Value::as_bool) == Some(false) =>
        {
            &[]
        }
        _ => return Err("FEISHU_RESPONSE_INVALID: 飞书记录列表缺少有效的 data.items".to_string()),
    };

    let mut records = Vec::with_capacity(items.len());
    for item in items {
        let record_id = item
            .get("record_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let fields = item.get("fields").cloned().unwrap_or(Value::Null);
        if record_id.is_empty() || !fields.is_object() {
            return Err("FEISHU_SCHEMA_CHANGED: 飞书案件记录缺少 record_id 或 fields".to_string());
        }
        records.push(FeishuRemoteCaseRecord {
            record_id: record_id.to_string(),
            fields,
            last_modified_time: value_as_time_string(item.get("last_modified_time")),
        });
    }

    let page_token = data
        .get("page_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let has_more = data
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(page_token.is_some());
    if has_more && page_token.is_none() {
        return Err(
            "FEISHU_RESPONSE_INVALID: 飞书记录列表声明 has_more 但缺少 page_token".to_string(),
        );
    }
    Ok((records, has_more.then_some(page_token).flatten()))
}

fn validate_bitable_id(value: &str, label: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(format!("FEISHU_CONFIG_INVALID: {label} 格式无效"));
    }
    Ok(())
}

fn classify_bitable_code(code: i64) -> &'static str {
    match code {
        99991663 | 99991668 => "FEISHU_AUTH_REQUIRED",
        99991672 => "FEISHU_PERMISSION_DENIED",
        1254040 | 1254041 => "FEISHU_TABLE_NOT_FOUND",
        _ => "FEISHU_PULL_FAILED",
    }
}

fn ensure_bitable_success(value: &Value) -> Result<(), String> {
    let code = value
        .get("code")
        .and_then(Value::as_i64)
        .ok_or_else(|| "FEISHU_RESPONSE_INVALID: 飞书响应缺少整数业务 code".to_string())?;
    if code == 0 {
        return Ok(());
    }
    let stable = classify_bitable_code(code);
    Err(format!("{stable}: 飞书接口拒绝本次读取（code={code}）"))
}

async fn read_bitable_response(response: reqwest::Response) -> Result<Value, String> {
    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err("FEISHU_AUTH_REQUIRED: 飞书授权已失效".to_string());
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return Err("FEISHU_PERMISSION_DENIED: 应用没有多维表格只读权限".to_string());
    }
    let bytes = response
        .bytes()
        .await
        .map_err(|_| "FEISHU_RESPONSE_INVALID: 飞书响应不可读取".to_string())?;
    if bytes.len() > 8 * 1024 * 1024 {
        return Err("FEISHU_RESPONSE_INVALID: 飞书单页响应超过安全上限".to_string());
    }
    if !status.is_success() {
        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
            ensure_bitable_success(&value)?;
        }
        return Err(format!(
            "FEISHU_PULL_FAILED: 飞书服务返回 HTTP {}",
            status.as_u16()
        ));
    }
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|_| "FEISHU_RESPONSE_INVALID: 飞书响应不是有效 JSON".to_string())?;
    ensure_bitable_success(&value)?;
    Ok(value)
}

fn parse_field_names(value: &Value) -> Result<(Vec<String>, Option<String>), String> {
    let data = response_data(value);
    let items = data
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "FEISHU_RESPONSE_INVALID: 飞书字段列表缺少 data.items".to_string())?;
    let mut names = Vec::with_capacity(items.len());
    for item in items {
        let name = item
            .get("field_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                "FEISHU_RESPONSE_INVALID: 飞书字段列表包含无效 field_name".to_string()
            })?;
        names.push(name.to_string());
    }
    let page_token = data
        .get("page_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let has_more = data
        .get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(page_token.is_some());
    if has_more && page_token.is_none() {
        return Err(
            "FEISHU_RESPONSE_INVALID: 飞书字段列表声明 has_more 但缺少 page_token".to_string(),
        );
    }
    Ok((names, has_more.then_some(page_token).flatten()))
}

fn validate_case_table_fields(field_names: &[String]) -> Result<String, String> {
    let has_case_name = field_names.iter().any(|name| name == "案件名称");
    let status_field = field_names
        .iter()
        .find(|name| matches!(name.as_str(), "☑状态" | "☑️状态"));
    if has_case_name {
        if let Some(status_field) = status_field {
            return Ok(status_field.clone());
        }
    }
    Err(
        "FEISHU_TABLE_SCHEMA_MISMATCH: 目标数据表不是案件总表（缺少“案件名称”或“☑状态”字段）"
            .to_string(),
    )
}

fn active_case_filter(status_field: &str) -> Result<String, String> {
    if !matches!(status_field, "☑状态" | "☑️状态") {
        return Err("FEISHU_TABLE_SCHEMA_MISMATCH: 案件状态字段名称不受支持".to_string());
    }
    Ok(format!(r#"AND(CurrentValue.[{status_field}]="在办")"#))
}

async fn validate_case_table_schema(
    client: &reqwest::Client,
    access_token: &str,
    app_token: &str,
    table_id: &str,
) -> Result<String, String> {
    let endpoint = format!(
        "https://open.feishu.cn/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/fields"
    );
    let mut page_token: Option<String> = None;
    let mut field_names = Vec::new();
    for _ in 0..BITABLE_FIELD_MAX_PAGES {
        let mut query = vec![("page_size", "100".to_string())];
        if let Some(token) = page_token.as_ref() {
            query.push(("page_token", token.clone()));
        }
        let response = client
            .get(&endpoint)
            .bearer_auth(access_token)
            .query(&query)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    "FEISHU_NETWORK_TIMEOUT: 读取飞书字段结构超时".to_string()
                } else {
                    "FEISHU_NETWORK_ERROR: 无法连接飞书开放平台".to_string()
                }
            })?;
        let value = read_bitable_response(response).await?;
        let (mut names, next_page_token) = parse_field_names(&value)?;
        field_names.append(&mut names);
        if let Ok(status_field) = validate_case_table_fields(&field_names) {
            return Ok(status_field);
        }
        page_token = next_page_token;
        if page_token.is_none() {
            return validate_case_table_fields(&field_names);
        }
    }
    Err("FEISHU_RESPONSE_INVALID: 飞书字段列表分页超过安全上限".to_string())
}

/// 通过飞书开放平台原生 HTTP API 拉取“状态=在办”的案件。
///
/// access token 仅存在于 Rust 内存和 Windows 凭据管理器，绝不进入命令行、SQLite 或日志。
pub async fn fetch_active_case_records(
    access_token: &str,
    app_token: &str,
    table_id: &str,
) -> Result<Vec<FeishuRemoteCaseRecord>, String> {
    validate_bitable_id(app_token, "App Token")?;
    validate_bitable_id(table_id, "Table ID")?;
    if access_token.trim().is_empty() {
        return Err("FEISHU_AUTH_REQUIRED: 飞书授权已失效".to_string());
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|_| "FEISHU_NETWORK_ERROR: 无法初始化网络客户端".to_string())?;
    let endpoint = format!(
        "https://open.feishu.cn/open-apis/bitable/v1/apps/{app_token}/tables/{table_id}/records"
    );
    let status_field =
        validate_case_table_schema(&client, access_token, app_token, table_id).await?;
    let filter = active_case_filter(&status_field)?;
    let mut page_token: Option<String> = None;
    let mut records = Vec::new();

    for _ in 0..BITABLE_MAX_PAGES {
        let mut query = vec![
            ("page_size", "500".to_string()),
            ("automatic_fields", "true".to_string()),
            ("filter", filter.clone()),
        ];
        if let Some(token) = page_token.as_ref() {
            query.push(("page_token", token.clone()));
        }

        let response = client
            .get(&endpoint)
            .bearer_auth(access_token)
            .query(&query)
            .send()
            .await
            .map_err(|error| {
                if error.is_timeout() {
                    "FEISHU_NETWORK_TIMEOUT: 读取飞书超时".to_string()
                } else {
                    "FEISHU_NETWORK_ERROR: 无法连接飞书开放平台".to_string()
                }
            })?;
        let value = read_bitable_response(response).await?;
        let (mut page_records, next_page_token) = parse_case_records(&value)?;
        records.append(&mut page_records);
        page_token = next_page_token;
        if page_token.is_none() {
            return Ok(records);
        }
    }

    Err("FEISHU_RESPONSE_INVALID: 飞书记录分页超过安全上限".to_string())
}

fn clean_required(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|s| !s.is_empty())
}

/// 从飞书日历获取指定日期范围内的事件。
///
/// 使用 `lark-cli calendar +agenda --as user` 获取(复用本机登录态)。
pub async fn fetch_calendar_events(
    bin: &str,
    start: &str,
    end: &str,
) -> Result<Vec<FeishuCalendarEvent>, String> {
    let mut cmd = Command::new(bin);
    apply_lark_env(&mut cmd);
    cmd.arg("calendar")
        .arg("+agenda")
        .arg("--as")
        .arg("user")
        .arg("--start")
        .arg(start)
        .arg("--end")
        .arg(end)
        .arg("--format")
        .arg("json");

    let output = timeout(LARK_CLI_TIMEOUT, cmd.output())
        .await
        .map_err(|_| "lark-cli 日历查询超时".to_string())?
        .map_err(|e| format!("无法启动 lark-cli(确认已安装并加入 PATH): {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "飞书日历查询失败: {}{}",
            stderr.trim(),
            if stdout.trim().is_empty() {
                String::new()
            } else {
                format!(" · {}", stdout.trim())
            }
        ));
    }

    let stdout =
        String::from_utf8(output.stdout).map_err(|e| format!("lark-cli 输出非 UTF-8: {}", e))?;
    let value: Value =
        serde_json::from_str(&stdout).map_err(|e| format!("lark-cli 输出非 JSON: {}", e))?;

    let events = value
        .pointer("/data")
        .and_then(Value::as_array)
        .ok_or_else(|| "飞书日历响应缺少 data".to_string())?;

    let mut result = Vec::new();
    for event in events {
        let event_id = event
            .get("event_id")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let summary = event
            .get("summary")
            .and_then(Value::as_str)
            .unwrap_or("(无标题)")
            .to_string();

        // 解析开始时间(date=全天 / datetime=带时刻)
        let start_time = event.get("start_time");
        let (start_date, is_all_day) = if let Some(st) = start_time {
            if let Some(date) = st.get("date").and_then(Value::as_str) {
                (date.to_string(), true)
            } else if let Some(datetime) = st.get("datetime").and_then(Value::as_str) {
                let date = datetime.split('T').next().unwrap_or(datetime);
                (date.to_string(), false)
            } else {
                continue;
            }
        } else {
            continue;
        };

        let end_date = event.get("end_time").and_then(|et| {
            et.get("date")
                .or_else(|| et.get("datetime"))
                .and_then(Value::as_str)
                .map(|s| {
                    if s.contains('T') {
                        s.split('T').next().unwrap_or(s).to_string()
                    } else {
                        s.to_string()
                    }
                })
        });

        let description = event
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string);
        let location = event
            .get("location")
            .and_then(|l| l.get("name").or_else(|| l.get("address")))
            .and_then(Value::as_str)
            .map(str::to_string);
        let app_link = event
            .get("app_link")
            .and_then(Value::as_str)
            .map(str::to_string);

        result.push(FeishuCalendarEvent {
            event_id,
            summary,
            start_date,
            end_date,
            is_all_day,
            description,
            location,
            app_link,
        });
    }

    Ok(result)
}

/// 去掉事件标题里的常见后缀,得到用于匹配的"干净"案件名片段。
fn clean_event_summary(summary: &str) -> String {
    summary
        .trim()
        .trim_end_matches("案件开庭")
        .trim_end_matches("开庭")
        .trim_end_matches("案件")
        .trim_end_matches("续封")
        .trim_end_matches("到期")
        .trim()
        .to_string()
}

/// 根据事件标题在飞书"案件池"多维表格里查找匹配的本地案件目录。
///
/// 匹配规则:事件标题包含案件名(如"张三案件开庭"匹配"张三"),或案件名与清理后的
/// 标题互相包含。返回第一个匹配且本地目录真实存在的记录路径。
///
/// 需要配置 `feishu_app_token` + `feishu_cases_table_id`(案件池表),且表里有
/// "案件名称""本地路径"两列;未配置则返回 None(不报错)。
pub async fn find_case_local_path(
    settings: &Settings,
    event_summary: &str,
) -> Result<Option<String>, String> {
    if !settings.feishu_enabled.unwrap_or(false) {
        return Ok(None);
    }

    let Some(app_token) = clean_required(settings.feishu_app_token.as_deref()) else {
        return Ok(None);
    };
    let Some(table_id) = clean_required(settings.feishu_cases_table_id.as_deref()) else {
        return Ok(None);
    };

    let bin = lark_bin(settings);
    let path = format!(
        "/open-apis/bitable/v1/apps/{}/tables/{}/records?page_size=500&field_names=true",
        app_token, table_id
    );
    let value = lark_cli_api(&bin, "GET", &path, None).await?;

    let items = response_data(&value)
        .get("items")
        .and_then(Value::as_array)
        .ok_or_else(|| "飞书记录列表响应缺少 data.items".to_string())?;

    let clean_summary = clean_event_summary(event_summary);

    for item in items {
        let Some(fields) = item.get("fields") else {
            continue;
        };

        let case_name = fields.get("案件名称").and_then(Value::as_str).unwrap_or("");
        if case_name.is_empty() {
            continue;
        }

        let matches = event_summary.contains(case_name)
            || case_name.contains(&clean_summary)
            || clean_summary.contains(case_name);
        if !matches {
            continue;
        }

        let local_path = fields
            .get("本地路径")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty());

        if let Some(p) = local_path {
            if Path::new(p).exists() {
                return Ok(Some(p.to_string()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_raw_and_cli_wrapped_record_responses() {
        let raw = serde_json::json!({
            "code": 0,
            "data": {"items": [{"record_id": "rec1", "fields": {"案件名称": "测试案"}, "last_modified_time": 1784518994000_i64}]}
        });
        let wrapped = serde_json::json!({"ok": true, "data": raw});

        for value in [
            ensure_lark_ok(raw).unwrap(),
            ensure_lark_ok(wrapped).unwrap(),
        ] {
            let (records, next) = parse_case_records(&value).unwrap();
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].record_id, "rec1");
            assert_eq!(
                records[0].last_modified_time.as_deref(),
                Some("1784518994000")
            );
            assert!(next.is_none());
        }
    }

    #[test]
    fn query_filter_is_utf8_percent_encoded_and_readonly() {
        assert!(validate_bitable_id("bascn123_ABC-9", "App Token").is_ok());
        assert!(validate_bitable_id("../bad", "App Token").is_err());
    }

    #[test]
    fn accepts_explicit_empty_record_page_when_items_is_omitted() {
        let response = serde_json::json!({
            "code": 0,
            "data": {"has_more": false, "total": 0}
        });

        ensure_bitable_success(&response).unwrap();
        let (records, next) = parse_case_records(&response).unwrap();
        assert!(records.is_empty());
        assert!(next.is_none());
    }

    #[test]
    fn rejects_missing_items_when_response_is_not_an_explicit_empty_page() {
        let response = serde_json::json!({
            "code": 0,
            "data": {"has_more": false, "total": 1}
        });

        let error = parse_case_records(&response).unwrap_err();
        assert!(error.starts_with("FEISHU_RESPONSE_INVALID:"));
    }

    #[test]
    fn rejects_incomplete_pagination_contract() {
        let response = serde_json::json!({
            "code": 0,
            "data": {"items": [], "has_more": true, "total": 1}
        });

        let error = parse_case_records(&response).unwrap_err();
        assert!(error.contains("page_token"));
    }

    #[test]
    fn classifies_bitable_business_error_before_parsing_data() {
        let response = serde_json::json!({"code": 99991672, "msg": "forbidden"});

        let error = ensure_bitable_success(&response).unwrap_err();
        assert!(error.starts_with("FEISHU_PERMISSION_DENIED:"));
        assert!(!error.contains("forbidden"));
    }

    #[test]
    fn validates_case_table_schema_with_supported_status_variants() {
        for status_name in ["☑状态", "☑️状态"] {
            let names = vec!["案件名称".to_string(), status_name.to_string()];
            let selected = validate_case_table_fields(&names).unwrap();
            assert_eq!(selected, status_name);
            assert_eq!(
                active_case_filter(&selected).unwrap(),
                format!(r#"AND(CurrentValue.[{status_name}]="在办")"#)
            );
        }
    }

    #[test]
    fn rejects_non_case_table_before_an_empty_filtered_result_can_be_accepted() {
        let wrong_table_fields = vec!["事项名称".to_string(), "进度".to_string()];

        let error = validate_case_table_fields(&wrong_table_fields).unwrap_err();
        assert!(error.starts_with("FEISHU_TABLE_SCHEMA_MISMATCH:"));
    }

    #[test]
    fn parses_paginated_field_names_without_field_values() {
        let response = serde_json::json!({
            "code": 0,
            "data": {
                "items": [
                    {"field_id": "fld1", "field_name": "案件名称", "type": 1},
                    {"field_id": "fld2", "field_name": "☑状态", "type": 3}
                ],
                "has_more": true,
                "page_token": "next-page"
            }
        });

        let (names, next) = parse_field_names(&response).unwrap();
        assert_eq!(names, ["案件名称", "☑状态"]);
        assert_eq!(next.as_deref(), Some("next-page"));
    }
}
