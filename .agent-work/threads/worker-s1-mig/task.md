# 线程任务包

## 任务信息

- task_id: V083-S1-MIG-R3
- title: S1 durable export迁移哨兵终验
- goal: 吸收0063 capture_sequence与export draft结构语义，恢复迁移及Windows Rust全量零失败
- owner_thread: worker-s1-mig
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-S1.md;.agent-work/24_s1_remediation_b_dispatch.md;src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/migration_lineage_tests.rs
- output_path: .agent-work/output/V083-S1-MIG-R3.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
