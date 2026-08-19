# 线程任务包

## 任务信息

- task_id: V084-PUBLISH
- title: v0.8.4 signed Windows release and public manifest convergence
- goal: Freeze and publish the accepted 0.8.4 release commit, obtain signed CI assets, verify the installer/update chain, publish the draft Release, and atomically converge version.json plus latest.json
- owner_thread: worker-v084-publish-local
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-RC.md;.agent-work/output/V084-R1.md;.github/workflows/build-windows.yml;scripts/publish-release-resumable.ps1;release/version.json;release/latest.json
- output_path: .agent-work/output/V084-PUBLISH.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
