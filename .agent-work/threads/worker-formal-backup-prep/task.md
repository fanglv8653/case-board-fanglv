# 线程任务包

## 任务信息

- task_id: V083-FORMAL-BACKUP-PREP
- title: 本机正式备份与隔离升级执行包
- goal: 基于两份正式Gate制定可执行的原样三文件备份、SQLite main-only备份、0.8.2恢复证明、0.8.3隔离升级与正式sidecar调和步骤，暂不执行正式写入
- owner_thread: worker-formal-backup-prep
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-FORMAL-DEVICE-GATE.md;.agent-work/output/V083-FORMAL-DB-GATE.md;scripts/windows-upgrade-validation;正式路径元数据
- output_path: .agent-work/output/V083-FORMAL-BACKUP-PREP.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
