# 09 分派看板

## 任务看板

| task_id | title | owner_thread | status | input_path | output_path | reviewer | updated_at |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V083-N0-GATE | 独立门禁与冲突审计 | worker-gate | todo | .agent-work/10_round1_dispatch_plan.md | .agent-work/output/V083-N0-GATE.md | 04-project-master | 2026-08-07T14:37:02+08:00 |
| V083-N0-MIG | 迁移谱系失败夹具与兼容契约 | worker-migration | todo | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-MIG.md | 04-project-master | 2026-08-07T14:37:00+08:00 |
| V083-N0-SYNC | 设备同步循环外键与分包失败夹具 | worker-sync | todo | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-SYNC.md | 04-project-master | 2026-08-07T14:37:01+08:00 |

## 状态规则

- `todo`：仅主控可创建。
- `dispatched`：仅主控可派发。
- `inProgress`：执行窗口已开始执行。
- `submitted_for_review`：执行窗口已提交，等待主控验收。
- `accepted`：主控验收通过。
- `rejected`：主控验收驳回。
