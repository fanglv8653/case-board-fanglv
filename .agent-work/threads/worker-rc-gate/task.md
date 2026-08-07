# 线程任务包

## 任务信息

- task_id: V083-RC-GATE
- title: RC版本签名发布只读门禁
- goal: 只读盘点0.8.3版本源、release gate、Windows bundle/updater/signing链与外部资源缺口
- owner_thread: worker-rc-gate
- reviewer: 04-project-master
- input_path: .agent-work/32_rc_dispatch_plan.md;.agent-work/33_rc_acceptance_rubric.md;package.json;src-tauri/Cargo.toml;src-tauri/tauri.conf.json;发布脚本与CI配置
- output_path: .agent-work/output/V083-RC-GATE.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
