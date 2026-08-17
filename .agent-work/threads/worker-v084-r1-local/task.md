# 线程任务包

## 任务信息

- task_id: V084-R1
- title: v0.8.4 atomic Windows release pipeline
- goal: Implement helper packaging, exact ASCII asset gates, draft Release convergence, and atomic paired manifest publication without publishing or bumping version
- owner_thread: worker-v084-r1-local
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-N0-UPDATER.md;.agent-work/output/V084-U1.md;.github/workflows/build-windows.yml;scripts/release-gate.mjs;scripts/publish-release-resumable.ps1;scripts/release-resume-core.psm1;scripts/test-release-resume.ps1;src-tauri/tauri.conf.json
- output_path: .agent-work/output/V084-R1.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
