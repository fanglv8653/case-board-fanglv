# 09 分派看板

## 任务看板

| task_id | title | owner_thread | status | input_path | output_path | reviewer | updated_at |
| --- | --- | --- | --- | --- | --- | --- | --- |
| V083-F1 | F1飞书孤立绑定本地安全修复 | worker-f1 | rejected | .agent-work/28_f1_dispatch_plan.md;.agent-work/29_f1_acceptance_rubric.md;.agent-work/output/V083-F1-GATE.md;.agent-work/output/V083-F1-MIG-SCOUT.md;agent-work/output/V083-20260803_下一轮待开发计划.md | .agent-work/output/V083-F1.md | 04-project-master | 2026-08-07T22:22:11+08:00 |
| V083-F1-GATE | F1孤立绑定只读链路审计 | worker-f1-gate | accepted | agent-work/output/V083-20260803_下一轮待开发计划.md;D:/CodexWorkspace/008案件看板应用/case-board-v0.8.2-dev/agent-work/output/V082-BUG-20260803_飞书孤立绑定字段处理失败.md;src-tauri/src/db/cases.rs;src-tauri/src/db/feishu_sync.rs;src/modules/tools/FeishuSyncPreview.tsx;src/lib/api.ts | .agent-work/output/V083-F1-GATE.md | 04-project-master | 2026-08-07T21:30:37+08:00 |
| V083-F1-MIG-SCOUT | F1是否需要0064迁移只读判定 | worker-f1-mig-scout | accepted | src-tauri/migrations/0049_feishu_case_management_sync.sql;src-tauri/migrations/0051_feishu_manual_binding.sql;src-tauri/migrations/0062_feishu_entity_change_previews.sql;src-tauri/src/db/feishu_sync.rs;src-tauri/src/db/migration_safety.rs | .agent-work/output/V083-F1-MIG-SCOUT.md | 04-project-master | 2026-08-07T21:30:12+08:00 |
| V083-F1-R2 | F1独立复审返工 | worker-f1-r2 | accepted | .agent-work/output/V083-F1-REVIEW.md;.agent-work/30_f1_remediation_dispatch.md;.agent-work/31_f1_remediation_acceptance.md;.agent-work/output/V083-F1.md | .agent-work/output/V083-F1-R2.md | 04-project-master | 2026-08-07T23:30:10+08:00 |
| V083-F1-REVIEW | F1最终独立安全复审 | worker-f1-review | accepted | .agent-work/output/V083-F1.md;.agent-work/output/V083-F1-GATE.md;.agent-work/output/V083-F1-MIG-SCOUT.md;.agent-work/28_f1_dispatch_plan.md;.agent-work/29_f1_acceptance_rubric.md;F1全部源码diff | .agent-work/output/V083-F1-REVIEW.md | 04-project-master | 2026-08-07T22:22:09+08:00 |
| V083-F1-REVIEW-R2 | F1返工最终独立复审 | worker-f1-review-r2 | accepted | .agent-work/output/V083-F1-R2.md;.agent-work/output/V083-F1-REVIEW.md;.agent-work/30_f1_remediation_dispatch.md;.agent-work/31_f1_remediation_acceptance.md;F1-R2全部源码diff | .agent-work/output/V083-F1-REVIEW-R2.md | 04-project-master | 2026-08-07T23:30:08+08:00 |
| V083-M1 | 数据库迁移谱系与启动恢复 | worker-m1 | accepted | V083-N0已验收夹具、冻结计划M1要求、现有db启动与Tauri setup代码。 | .agent-work/output/V083-M1.md | 04-project-master | 2026-08-07T16:43:38+08:00 |
| V083-M1-REVIEW | M1独立只读安全审计 | worker-m1-review | accepted | V083-M1源码差异、实现报告、N0门禁与主控实测结果。 | .agent-work/output/V083-M1-REVIEW.md | 04-project-master | 2026-08-07T16:12:55+08:00 |
| V083-M1-REVIEW2 | M1 第三轮阻断级独立复核 | worker-m1-review2 | accepted | src-tauri/src/db/mod.rs;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs;src-tauri/src/lib.rs;.agent-work/output/V083-M1.md | .agent-work/output/V083-M1-REVIEW2.md | 04-project-master | 2026-08-07T16:43:36+08:00 |
| V083-N0-GATE | 独立门禁与冲突审计 | worker-gate | accepted | .agent-work/10_round1_dispatch_plan.md | .agent-work/output/V083-N0-GATE.md | 04-project-master | 2026-08-07T14:45:16+08:00 |
| V083-N0-MIG | 迁移谱系失败夹具与兼容契约 | worker-migration | accepted | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-MIG.md | 04-project-master | 2026-08-07T15:04:35+08:00 |
| V083-N0-SYNC | 设备同步循环外键与分包失败夹具 | worker-sync | accepted | agent-work/tasks/V083-N0_开发前准备任务包.md | .agent-work/output/V083-N0-SYNC.md | 04-project-master | 2026-08-07T14:59:38+08:00 |
| V083-S1 | 设备同步循环引用、分包与隔离生命周期 | worker-s1 | accepted | .agent-work/22_s1_dispatch_plan.md;.agent-work/23_s1_acceptance_rubric.md;agent-work/output/V083-20260803_下一轮待开发计划.md;src-tauri/src/device_sync/v083_failure_tests.rs | .agent-work/output/V083-S1.md | 04-project-master | 2026-08-07T21:19:47+08:00 |
| V083-S1-MIG | S1迁移0063与M1门禁集成 | worker-s1-mig-r1 | accepted | .agent-work/output/V083-S1.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs | .agent-work/output/V083-S1-MIG.md | 04-project-master | 2026-08-07T18:34:44+08:00 |
| V083-S1-MIG-R2 | S1隔离身份迁移哨兵复核 | worker-s1-mig-r2 | accepted | .agent-work/output/V083-S1.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs | .agent-work/output/V083-S1-MIG-R2.md | 04-project-master | 2026-08-07T19:33:26+08:00 |
| V083-S1-MIG-R3 | S1 durable export迁移哨兵终验 | worker-s1-mig | accepted | .agent-work/output/V083-S1.md;.agent-work/24_s1_remediation_b_dispatch.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs | .agent-work/output/V083-S1-MIG-R3.md | 04-project-master | 2026-08-07T20:39:46+08:00 |
| V083-S1-REVIEW | S1独立只读安全审计 | worker-s1-review-r1 | accepted | .agent-work/output/V083-S1.md;.agent-work/output/V083-S1-MIG.md;.agent-work/23_s1_acceptance_rubric.md;S1全部源码diff | .agent-work/output/V083-S1-REVIEW.md | 04-project-master | 2026-08-07T18:34:42+08:00 |
| V083-S1-REVIEW-R2 | S1返工A独立安全复审 | worker-s1-review-r2 | accepted | .agent-work/output/V083-S1.md;.agent-work/output/V083-S1-REVIEW.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/device_sync;src/components/settings/DeviceSyncSettingsCard.tsx;src/lib/types.ts | .agent-work/output/V083-S1-REVIEW-R2.md | 04-project-master | 2026-08-07T19:26:31+08:00 |
| V083-S1-REVIEW-R3 | S1返工B最终独立安全审计 | worker-s1-review-r3 | accepted | .agent-work/output/V083-S1.md;.agent-work/output/V083-S1-REVIEW-R2.md;.agent-work/24_s1_remediation_b_dispatch.md;.agent-work/25_s1_remediation_b_acceptance.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/device_sync;src/components/settings/DeviceSyncSettingsCard.tsx;src/lib/types.ts | .agent-work/output/V083-S1-REVIEW-R3.md | 04-project-master | 2026-08-07T20:33:19+08:00 |
| V083-S1-REVIEW-R4 | S1返工C最终独立复验 | worker-s1-review | accepted | .agent-work/output/V083-S1.md;.agent-work/output/V083-S1-REVIEW-R3.md;.agent-work/26_s1_remediation_c_dispatch.md;.agent-work/27_s1_remediation_c_acceptance.md;src-tauri/src/device_sync;src-tauri/tests/device_sync_contract.rs | .agent-work/output/V083-S1-REVIEW-R4.md | 04-project-master | 2026-08-07T21:19:45+08:00 |

## 状态规则

- `todo`：仅主控可创建。
- `dispatched`：仅主控可派发。
- `inProgress`：执行窗口已开始执行。
- `submitted_for_review`：执行窗口已提交，等待主控验收。
- `accepted`：主控验收通过。
- `rejected`：主控验收驳回。
