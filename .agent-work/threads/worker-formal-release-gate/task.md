# 线程任务包

## 任务信息

- task_id: V083-FORMAL-RELEASE-GATE
- title: 0.8.3远端签名发布只读门禁
- goal: 只读核验origin/main/tag/CI workflow/updater secret存在性和发布脚本，给出先本机后远端的安全顺序，不push/tag/release
- owner_thread: worker-formal-release-gate
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-RC-GATE.md;release脚本;Git远端与GitHub Actions
- output_path: .agent-work/output/V083-FORMAL-RELEASE-GATE.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
