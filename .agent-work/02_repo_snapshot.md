# 02 仓库快照

- worktree: `D:\CodexWorkspace\008案件看板应用\case-board-v0.8.4-dev`
- branch: `feat/v0.8.4-todos-updater`
- base: `origin/main` / `c6fa8a6c7d3aa16ff4227f0d97cfda299f182cc8`
- package version: `0.8.3`
- migrations: 1—63（实际嵌入集合含历史合法缺号）
- package manager: pnpm，锁文件 `pnpm-lock.yaml`
- Rust toolchain: 1.96.0

## 关键实现

- `src/lib/updater.ts`、`UpdateAvailableDialog.tsx`、`UpdateSuccessDialog.tsx`：更新与成功提示；
- `src-tauri/src/lib.rs`：退出、数据库连接池和 Tauri 生命周期；
- `scripts/publish-release-resumable.ps1`、`release-resume-core.psm1`：可恢复发布；
- `.github/workflows/build-windows.yml`：Windows 资产构建上传；
- `src-tauri/src/db/todos.rs`、migrations 0024/0027、`TodosCard.tsx`：现有案件内待办；
- `src-tauri/src/db/feishu_sync.rs`、`feishu_entities.rs`：飞书受控预演、复核、冲突和实体同步；
- `case_work_items` / 案件时间线：复制到案件进展的目标模型。

## 基线边界

- 新工作树创建时 `git status` 干净；
- 0.8.3 已发布，`origin/main` 同时包含功能提交与公开清单提交；
- 0.8.3 旧工作树的未提交文档和验收产物未带入本工作树；
- v0.8.4 基线测试将在 N0 阶段重新执行并记录当前计数。
