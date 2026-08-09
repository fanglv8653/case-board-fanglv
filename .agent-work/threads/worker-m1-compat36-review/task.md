# 线程任务包

## 任务信息

- task_id: V083-M1-COMPAT36-REVIEW
- title: 可信历史迁移36独立安全复核
- goal: 独立只读审查唯一legacy36兼容谓词、SQLx ignore_missing边界、正反例和正式资源零访问，给出P0/P1/P2
- owner_thread: worker-m1-compat36-review
- reviewer: 04-project-master
- input_path: .agent-work/35_m1_compat36_dispatch.md;.agent-work/36_m1_compat36_acceptance.md;.agent-work/output/V083-M1-COMPAT36.md;COMPAT36全部源码diff
- output_path: .agent-work/output/V083-M1-COMPAT36-REVIEW.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
