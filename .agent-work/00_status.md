# 00 状态

## 项目概览

- project_name: 方律案件看板 v0.8.4 完整开发
- project_phase: V084-N0-accepted-awaiting-implementation
- project_health: green
- master_window: 00-master
- project_master: 04-project-master
- branch: feat/v0.8.4-todos-updater
- baseline_commit: c6fa8a6c7d3aa16ff4227f0d97cfda299f182cc8
- last_sync_at: 2026-08-17T14:45:59+08:00

## 当前结论

- 本地文件系统是唯一事实源，聊天消息不替代任务状态和验收记录。
- 用户已确认完整 v0.8.4：同时实施更新/发布链改进和“待办事项”产品功能。
- 开发顺序固定为 N0 契约冻结 → U1/R1/T1 → F1 → RC，依赖未 accepted 不进入下游。
- 正式数据库、NAS 同步组、成员密钥、飞书正式 Base 和凭据均不属于开发测试对象。
- 子 Agent 只能提交 `submitted_for_review`，最终 accepted/rejected 由主控裁决。
- 现有 `case_todos.case_id` 非空，不能直接满足未关联案件、同步版本、冲突和来源追踪要求。
- N0 三项只读契约均已主控 accepted；总契约见 `.agent-work/output/V084-N0-CONTRACT.md`。
- 产品源码、迁移、版本号和公开发布状态尚未修改。

## 活跃窗口

| window_id | role | status | notes |
| --- | --- | --- | --- |
| 00-master | master | active | 统一调度、冲突处理、最终验收 |
| 04-project-master | project_master | active | 任务拆分、代码审查、测试与发布门禁 |

## 进度摘要

| metric | value |
| --- | --- |
| total_tasks | 42 |
| todo_tasks | 0 |
| dispatched_tasks | 0 |
| in_progress_tasks | 0 |
| submitted_tasks | 0 |
| accepted_tasks | 35 |
| rejected_tasks | 7 |
