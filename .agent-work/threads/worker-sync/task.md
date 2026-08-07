# 线程任务包｜V083-N0-SYNC

## 目标

建立设备同步循环外键、整包回滚、重复隔离和 501+ 分包边界的确定性失败夹具，不修改运行时生产行为。

## 必读

- `agent-work/tasks/V083-N0_开发前准备任务包.md`
- `.agent-work/01_project_brief.md`
- `.agent-work/02_repo_snapshot.md`
- `.agent-work/05_acceptance_gates.md`
- `D:\CodexWorkspace\008案件看板应用\case-board-v0.8.2-dev\agent-work\output\V082-BUG-20260803_设备同步基线外键隔离.md`
- `src-tauri/src/device_sync/engine.rs`
- `src-tauri/src/device_sync/registry.rs`
- `src-tauri/src/device_sync/capture.rs`
- `src-tauri/migrations/0058_device_sync_core.sql`

## 允许写入

- `src-tauri/src/device_sync/` 内仅 `#[cfg(test)]` 测试模块或纯测试辅助；
- `src-tauri/tests/` 内 v0.8.3 专项测试；
- `.agent-work/threads/worker-sync/`；
- `.agent-work/output/V083-N0-SYNC.md`。

## 禁止

- 不改导出、导入、隔离、审计等生产路径；
- 不关闭外键，不读取/解密正式事件；
- 不写当前 NAS、同步组或正式数据库；
- 不创建迁移、不提交 Git、不覆盖其他 Agent 修改；
- 不运行完整 Cargo/生产构建，主控统一串行复验。

## 交付

夹具与报告必须明确：case/contact 循环引用、失败零部分写入、同包重复隔离、500/501/1000+ 边界、当前 audit succeeded 语义缺口和后续实现不变量。
