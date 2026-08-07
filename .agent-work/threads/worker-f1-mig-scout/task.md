# 线程任务包

## 任务信息

- task_id: V083-F1-MIG-SCOUT
- title: F1是否需要0064迁移只读判定
- goal: 核对现有飞书表约束、审计FK和可用状态，判定F1能否不新增schema；如需0064给出最小DDL与sentinel
- owner_thread: worker-f1-mig-scout
- reviewer: 04-project-master
- input_path: src-tauri/migrations/0049_feishu_case_management_sync.sql;src-tauri/migrations/0051_feishu_manual_binding.sql;src-tauri/migrations/0062_feishu_entity_change_previews.sql;src-tauri/src/db/feishu_sync.rs;src-tauri/src/db/migration_safety.rs
- output_path: .agent-work/output/V083-F1-MIG-SCOUT.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
