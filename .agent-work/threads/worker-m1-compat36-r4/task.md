# 线程任务包

## 任务信息

- task_id: V083-M1-COMPAT36-R4
- title: v0.8.3 migration 36 compatibility remediation R4
- goal: Correct SQLite table_xinfo default normalization without changing valid DDL whitelist
- owner_thread: worker-m1-compat36-r4
- reviewer: worker_gate
- input_path: .agent-work/45_m1_compat36_r4_dispatch.md
- output_path: .agent-work/output/V083-M1-COMPAT36-R4.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
