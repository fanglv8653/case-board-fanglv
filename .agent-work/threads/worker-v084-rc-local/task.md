# 线程任务包

## 任务信息

- task_id: V084-RC
- title: v0.8.4 integration and local release candidate
- goal: Bump source version to 0.8.4, complete integrated gates and produce a local Windows release candidate without publishing remote manifests or changing the live Feishu Base
- owner_thread: worker-v084-rc-local
- reviewer: 04-project-master
- input_path: .agent-work/output/V084-N0-CONTRACT.md;.agent-work/output/V084-U1.md;.agent-work/output/V084-R1.md;.agent-work/output/V084-T1.md;.agent-work/output/V084-F1.md;CHANGELOG.md;package.json;src-tauri/Cargo.toml;src-tauri/tauri.conf.json
- output_path: .agent-work/output/V084-RC.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
