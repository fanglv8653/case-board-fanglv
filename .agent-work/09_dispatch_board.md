# 09 分派看板

## 任务看板

| task_id | title | owner_thread | status | input_path | output_path | reviewer | updated_at |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V083-M1 | 数据库迁移谱系与启动恢复 | worker-m1 | accepted | V083-N0已验收夹具、冻结计划M1要求、现有db启动与Tauri setup代码。 | .agent-work/output/V083-M1.md | 04-project-master | 2026-08-07T16:43:38+08:00 |
| V083-M1-REVIEW | M1独立只读安全审计 | worker-m1-review | accepted | V083-M1源码差异、实现报告、N0门禁与主控实测结果。 | .agent-work/output/V083-M1-REVIEW.md | 04-project-master | 2026-08-07T16:12:55+08:00 |
| V083-M1-REVIEW2 | M1 第三轮阻断级独立复核 | worker-m1-review2 | accepted | src-tauri/src/db/mod.rs;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs;src-tauri/src/lib.rs;.agent-work/output/V083-M1.md | .agent-work/output/V083-M1-REVIEW2.md | 04-project-master | 2026-08-07T16:43:36+08:00 |
| V083-N0-GATE | 独立门禁与冲突审计 | worker-gate | accepted | .agent-work/10_round1_dispatch_plan.md | .agent-work/output/V083-N0-GATE.md | 04-project-master | 2026-08-07T14:45:16+08:00 |
| V083-N0-MIG | 迁移谱系失败夹具与兼容契约 | worker-migration | accepted | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-MIG.md | 04-project-master | 2026-08-07T15:04:35+08:00 |
| V083-N0-SYNC | 设备同步循环外键与分包失败夹具 | worker-sync | accepted | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-SYNC.md | 04-project-master | 2026-08-07T14:59:38+08:00 |

## 状态规则

- `todo`：仅主控可创建。
- `dispatched`：仅主控可派发。
- `inProgress`：执行窗口已开始执行。
- `submitted_for_review`：执行窗口已提交，等待主控验收。
- `accepted`：主控验收通过。
- `rejected`：主控验收驳回。
