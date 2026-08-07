# 线程任务包｜V083-N0-MIG

## 目标

建立可执行的迁移谱系失败夹具并冻结只读预检契约，不修改运行时生产行为。

## 必读

- `agent-work/tasks/V083-N0_开发前准备任务包.md`
- `.agent-work/01_project_brief.md`
- `.agent-work/02_repo_snapshot.md`
- `.agent-work/05_acceptance_gates.md`
- `D:\CodexWorkspace\008案件看板应用\笔记本v0.8.2闪退诊断与后续任务交接_20260803.md`
- `src-tauri/src/db/mod.rs`
- `src-tauri/migrations/0049_feishu_case_management_sync.sql`
- `src-tauri/migrations/0051_feishu_manual_binding.sql`

## 允许写入

- `src-tauri/src/db/` 内仅 `#[cfg(test)]` 测试模块或纯测试辅助；
- `scripts/windows-upgrade-validation/` 内纯构造库夹具；
- `.agent-work/threads/worker-migration/`；
- `.agent-work/output/V083-N0-MIG.md`。

## 禁止

- 不改 `reconcile_migration_checksums()`、`init_pool()` 等生产逻辑；
- 不读取正式数据库，不编造笔记本旧 checksum；
- 不创建 0063/0064 迁移；
- 不提交 Git，不覆盖其他 Agent 修改；
- 不运行完整 Cargo/生产构建，主控统一串行复验。

## 交付

夹具应覆盖正常谱系、未知 checksum、版本 49 success 但表缺失、未知已应用迁移、success=0；报告 sentinel、错误码、修改文件及建议定向命令。
