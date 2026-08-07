# 线程任务包

## 任务信息

- task_id: V083-F1-REVIEW-R2
- title: F1返工最终独立复审
- goal: 按31量表独立验证设备同步共享生命周期协议、缺inbox孤立恢复和全门禁，确认P0/P1归零
- owner_thread: worker-f1-review-r2
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-F1-R2.md;.agent-work/output/V083-F1-REVIEW.md;.agent-work/30_f1_remediation_dispatch.md;.agent-work/31_f1_remediation_acceptance.md;F1-R2全部源码diff
- output_path: .agent-work/output/V083-F1-REVIEW-R2.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
