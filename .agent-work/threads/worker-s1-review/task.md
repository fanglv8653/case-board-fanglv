# 线程任务包

## 任务信息

- task_id: V083-S1-REVIEW-R4
- title: S1返工C最终独立复验
- goal: 只读复验R3审计2P0/3P1是否关闭，并确认无新高优先级缺陷
- owner_thread: worker-s1-review
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-S1.md;.agent-work/output/V083-S1-REVIEW-R3.md;.agent-work/26_s1_remediation_c_dispatch.md;.agent-work/27_s1_remediation_c_acceptance.md;src-tauri/src/device_sync;src-tauri/tests/device_sync_contract.rs
- output_path: .agent-work/output/V083-S1-REVIEW-R4.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
