# 线程任务包

## 任务信息

- task_id: V084-F1
- title: v0.8.4 Feishu inbox todo synchronization
- goal: Implement accepted R2 read-only-first Feishu inbox binding, local sync ledger, preview and explicit resolution flows without changing the live Base
- owner_thread: worker-v084-f1-local
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-N0-CONTRACT.md;.agent-work/output/V084-N0-FEISHU.md;.agent-work/output/V084-FEISHU-INBOX-USAGE-AUDIT.md;.agent-work/output/V084-T1.md;src-tauri/src/feishu.rs;src-tauri/src/settings.rs;src-tauri/src/feishu_oauth.rs;src-tauri/src/db/todos.rs;src/components/TodoBoard.tsx
- output_path: .agent-work/output/V084-F1.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
