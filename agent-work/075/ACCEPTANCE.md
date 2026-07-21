# 方律案件看板 0.7.5 主控验收报告

日期：2026-07-21

基线：`origin/main@e9fb2e0`

分支：`feat/v0.7.5-four-items`

## 功能结论

- 日历：完整月历/日程卡只在 `workspace` 首页挂载；刑事、民事保留关键日期摘要。
- 飞书绑定：支持推荐、人工绑定、解除、忽略、恢复；名称日期前缀只用于匹配；操作只写本地同步关联及审计表，不写飞书、不改案件业务字段或双方名称。
- 合同审查：批注作者优先级为本次作者、已保存作者、用户姓名、产品兜底；批注及 `w:ins`/`w:del` 使用同一次后端本机时间快照。
- DOCX 时间：XML 解包测试确认 RFC3339 带时区偏移；北京时间固定样例为 `2026-07-21T18:05:06+08:00`，运行测试确认实际偏移等于本机偏移。
- 非诉：首页保持合同审查、合同起草、律师函三张主卡；工作稿带待复核标识，正式稿须通过事实、法源、执业律师三项后端门禁；不自动发送。

## 自动化门禁

- Node 逻辑/UI 契约：73/73。
- TypeScript：`pnpm exec tsc --noEmit` 通过。
- Vite：`pnpm build` 通过（仅既有大 chunk 警告）。
- Rust：`cargo test --lib --bins` 111/111。
- Rust check：`cargo check --all-targets` 通过。
- Clippy：`cargo clippy --all-targets --all-features -- -D warnings` 通过。
- 源码发布门禁：`source=0.7.5`、`published=0.7.4`，许可证哈希通过。
- Windows Release：`pnpm tauri build --no-bundle` 通过，生成 0.7.5 `caseboard.exe`。

`cargo test --all-targets` 的库测试 111/111 通过；随后尝试执行仓库签名校验 example 时被 Windows 以 `os error 740` 要求提权。该 example 已由 `cargo check --all-targets` 编译验证，正式 updater 签名在 GitHub Actions 和下载后验证阶段执行。

## 隔离数据库与启动

- 隔离目录：`D:\CodexWorkspace\008案件看板应用\cb075-acceptance-3`。
- Release EXE 隔离启动 15 秒存活，未提前崩溃。
- `PRAGMA quick_check`：`ok`。
- migration 46—51 均为 success，最高迁移号 51。
- `feishu_sync_binding_audits` 存在，`feishu_sync_inbox.auto_bind_suppressed` 存在。
- 隔离库 `cases=0`，未读取或修改正式案件库。

验收过程中发现本机增量 Release 曾复用旧 `sqlx::migrate!` 宏展开。现已在 `build.rs` 对全部 SQL 迁移内容生成编译指纹，并由数据库模块显式依赖；第三次全新隔离库确认 47—51 全部嵌入并执行。

## 发布前剩余项

- 提交并推送远端 `main`。
- GitHub Actions 生成 NSIS 安装包及 updater `.sig`。
- 远端下载后校验摘要、updater 签名和 `latest.json`。
- 备份正式库后安装 0.7.5，并核对正式库 `quick_check`、migration 51、关键业务表及应用版本。
