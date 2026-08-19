# 00 状态

## 项目概览

- project_name: 方律案件看板 v0.8.4 完整开发
- project_phase: V084-PUBLISH-inProgress
- project_health: green
- master_window: 00-master
- project_master: 04-project-master
- branch: feat/v0.8.4-todos-updater
- baseline_commit: c6fa8a6c7d3aa16ff4227f0d97cfda299f182cc8
- last_sync_at: 2026-08-19T10:20:09+08:00

## 当前结论

- 本地文件系统是唯一事实源，聊天消息不替代任务状态和验收记录。
- 用户已确认完整 v0.8.4：同时实施更新/发布链改进和“待办事项”产品功能。
- N0、U1、R1、T1、F1、RC 已全部 accepted；用户于 2026-08-19 明确授权立即进入正式发布窗口。
- 正式数据库、NAS 同步组、成员密钥和飞书正式 Base 不作为发布构建输入；安装验收必须先备份并只核对必要健康事实。
- 子 Agent 只能提交 `submitted_for_review`，最终 accepted/rejected 由主控裁决。
- 0.8.4 产品源码、0064/0065 迁移和版本源已完成；公开 `version.json` / `latest.json` 仍安全保持 0.8.3。
- `origin/main` 尚为 0.8.3 发布提交；0.8.4 tag、Release 和正式资产尚不存在，必须按签名 CI → 安装验收 → Release → 双清单顺序收敛。

## 活跃窗口

| window_id | role | status | notes |
| --- | --- | --- | --- |
| 00-master | master | active | 统一调度、冲突处理、最终验收 |
| 04-project-master | project_master | active | 任务拆分、代码审查、测试与发布门禁 |

## 进度摘要

| metric | value |
| --- | --- |
| total_tasks | 48 |
| todo_tasks | 0 |
| dispatched_tasks | 0 |
| in_progress_tasks | 1 |
| submitted_tasks | 0 |
| accepted_tasks | 40 |
| rejected_tasks | 7 |
