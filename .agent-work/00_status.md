# 00 状态

## 项目概览

- project_name: 方律案件看板 v0.8.3 数据安全热修复
- project_phase: V083-N0
- project_health: green
- master_window: 00-master
- project_master: 04-project-master
- branch: fix/v0.8.3-data-safety
- baseline_commit: 76e4788627bef621c500a3f82c5c63f6b21dcbed
- last_sync_at: 2026-08-07T15:13:42+08:00

## 当前结论

- 本地文件系统是唯一事实源，聊天消息不替代任务状态和验收记录。
- 开发顺序固定为 N0→M1→S1→F1→RC，前一阶段未 accepted 不进入下一阶段。
- 正式数据库、NAS 同步组、成员密钥、飞书正式 Base 和凭据均不属于开发测试对象。
- 子 Agent 只能提交 `submitted_for_review`，最终 accepted/rejected 由主控裁决。

## 活跃窗口

| window_id | role | status | notes |
| --- | --- | --- | --- |
| 00-master | master | active | 统一调度、冲突处理、最终验收 |
| 04-project-master | project_master | active | 任务拆分、代码审查、测试与发布门禁 |

## 进度摘要

| metric | value |
| --- | --- |
| total_tasks | 4 |
| todo_tasks | 1 |
| dispatched_tasks | 0 |
| in_progress_tasks | 0 |
| submitted_tasks | 0 |
| accepted_tasks | 3 |
| rejected_tasks | 0 |
