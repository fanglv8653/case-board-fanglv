# V081-YD 元典官方余额后端核心报告

日期：2026-07-29
工作树：`D:\CodexWorkspace\008案件看板应用\case-board-v0.8.1`
范围状态：后端核心完成，等待主控注册 Tauri 命令并接入前端

## 1. 完成内容

1. 新增迁移 `0055_yuandian_balance_snapshots.sql`：
   - 保存 API Key 的 SHA-256 截断指纹；
   - 保存官方积分余额、次数余额、本机累计积分、本机累计调用次数和刷新时间；
   - 按 `key_fingerprint, id DESC` 建立缓存查询索引；
   - 不保存明文 API Key。
2. 新增 `yuandian/balance.rs`：
   - 从 `Windows Credential Manager` 对应的 `StaticCredential::Yuandian` 在 Rust 内部取钥；
   - 通过现有 MCP Streamable HTTP 客户端调用免费工具 `yuandian_get_user_balance`；
   - 兼容 `dataPreview.data`、`data.data`、`data` 和 `structuredContent` 余额包装；
   - 当前 Key 指纹隔离缓存，换 Key 不读取旧账户余额；
   - 官方余额变化与 `yuandian_credits_monthly` 本机账本对账；
   - 进程内互斥刷新，避免同一实例并发重复请求；
   - 网络、鉴权、响应和数据库错误压缩为稳定安全错误码，不向前端透传服务端响应正文；
   - 刷新失败但当前 Key 存在缓存时，返回缓存、原刷新时间和安全错误说明。
3. 在 `yuandian/mod.rs` 注册 `pub mod balance`。

## 2. 安全边界

- 明文 API Key 不进入前端、SQLite、settings、日志或 argv。
- MCP Bearer 仅存在于当次 Rust 运行时 HTTP 配置。
- SQLite 中的 `key_fingerprint` 是 16 位 SHA-256 十六进制前缀，不含 Key 原文。
- 外部错误正文不会进入返回值；对外只暴露以下稳定错误码：
  - `YUANDIAN_CREDENTIAL_NOT_CONFIGURED`
  - `YUANDIAN_CREDENTIAL_UNAVAILABLE`
  - `YUANDIAN_BALANCE_AUTH_FAILED`
  - `YUANDIAN_BALANCE_NETWORK_FAILED`
  - `YUANDIAN_BALANCE_RESPONSE_INVALID`
  - `YUANDIAN_BALANCE_DATABASE_FAILED`

## 3. 定向验证

执行：

```text
cargo test -p caseboard --lib yuandian::balance::tests --locked -- --nocapture
```

结果：`6 passed; 0 failed; 233 filtered out`。

覆盖：

- 当前及旧版 MCP 余额 JSON 包装解析；
- 指纹稳定、不同 Key 隔离且不包含秘密原文；
- 官方余额与本机积分/API 调用增量对账；
- 换 Key 后不串用旧快照；
- 离线刷新失败时只回退当前 Key 的缓存并携带安全错误；
- 任意远端错误压缩为安全稳定类别。

补充说明：曾执行一次 `cargo fmt --all -- --check`，只读检查报告工作树既存换行符问题；没有执行写入式全仓格式化。后续按主控指令未再运行 fmt。定向 Clippy 在主控要求停止继续构建时主动终止，不声明其通过；最终全仓 Clippy 由主控综合门禁执行。

## 4. 主控待集成点

本任务按约束未修改 `src-tauri/src/lib.rs`、`src/lib/api.ts`、`src/lib/types.ts` 或 `SettingsModal.tsx`。主控需要：

1. 在 `lib.rs` 增加 Tauri command：
   - `refresh=false` 调用 `yuandian::balance::cached_balance(pool.inner())`；
   - `refresh=true` 调用 `yuandian::balance::refresh_balance(pool.inner())`；
   - 建议返回 `Result<Option<YuandianBalanceView>, String>`，刷新成功包装为 `Some`。
2. 在 `tauri::generate_handler!` 注册该命令。
3. 在 TypeScript 中镜像 `YuandianBalanceView` 字段。
4. 数据源页进入时调用一次刷新，另提供手动刷新；不增加定时刷新。
5. UI 明确区分“元典官方余额”和“本机估算/本地节省”，缓存状态展示 `fetched_at` 与 `refresh_error`。

## 5. 本任务实际文件

```text
src-tauri/migrations/0055_yuandian_balance_snapshots.sql
src-tauri/src/yuandian/balance.rs
src-tauri/src/yuandian/mod.rs
agent-work/output/V081-YD-backend.md
```

未提交 Git。
