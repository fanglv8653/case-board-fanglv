# 线程任务包

## 任务信息

- task_id: V083-FORMAL-DB-GATE
- title: 本设备正式数据库谱系与备份门禁
- goal: 只读定位正式数据库及sidecar，核验迁移谱系/checksum/sentinel/quick/FK并制定一致性备份与升级前后指纹，不执行写入
- owner_thread: worker-formal-db-gate
- reviewer: 04-project-master
- input_path: .agent-work/33_rc_acceptance_rubric.md;src-tauri/src/db/migration_safety.rs;本机正式数据目录
- output_path: .agent-work/output/V083-FORMAL-DB-GATE.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
