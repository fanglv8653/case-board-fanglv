# 线程任务包

## 任务信息

- task_id: V084-N0-UPDATER
- title: v0.8.4更新生命周期与原子发布契约
- goal: 只读审计现有更新、退出、发布与CI链路，形成可实施状态机、原子发布和ASCII资产契约
- owner_thread: worker-v084-updater
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-更新与发布流程待办.md;.agent-work/53_v084_n0_dispatch_plan.md;.agent-work/54_v084_n0_acceptance_rubric.md;src/lib/updater.ts;src/components/UpdateAvailableDialog.tsx;src/components/UpdateSuccessDialog.tsx;src-tauri/src/lib.rs;scripts/publish-release-resumable.ps1;scripts/release-resume-core.psm1;.github/workflows/build-windows.yml
- output_path: .agent-work/output/V084-N0-UPDATER.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
