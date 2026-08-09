# 线程任务包

## 任务信息

- task_id: V083-M1-COMPAT36
- title: 可信历史迁移36兼容实现
- goal: 严格绑定正式来源tuple和精确schema，允许唯一legacy version36通过写前预检并升级0063，其余未知谱系继续fail closed
- owner_thread: worker-m1-compat36
- reviewer: 04-project-master
- input_path: .agent-work/35_m1_compat36_dispatch.md;.agent-work/36_m1_compat36_acceptance.md;.agent-work/output/V083-FORMAL-DB-GATE.md;src-tauri/src/db/migration_safety.rs;src-tauri/src/db/mod.rs
- output_path: .agent-work/output/V083-M1-COMPAT36.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
