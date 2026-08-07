# 02 仓库快照

- worktree: `D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev`
- branch: `fix/v0.8.3-data-safety`
- base: `76e4788`
- package version: `0.8.2`
- migrations: 1—62
- package manager: pnpm，锁文件 `pnpm-lock.yaml`
- Rust toolchain: 1.96.0

## 关键实现

- `src-tauri/src/db/mod.rs`：数据库连接、checksum 对齐、sqlx migrations；
- `src-tauri/src/lib.rs`：Tauri setup 与数据库初始化；
- `src-tauri/src/device_sync/engine.rs`：导出、导入、隔离与审计；
- `src-tauri/src/device_sync/registry.rs`：同步实体与字段策略；
- `src-tauri/migrations/0058_device_sync_core.sql`：设备同步核心表；
- `src-tauri/src/db/feishu_sync.rs`：飞书绑定、预演与解绑；
- `scripts/run-windows-rust-tests.ps1`：Windows Rust 测试总入口。

## 已通过基线

- Node 119/119；
- Vite build；
- cargo check；
- Clippy `-D warnings`；
- Rust 主测试 263 passed、3 ignored；
- 设备同步契约 18/18。
