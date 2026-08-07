# 线程任务包

## 任务信息

- task_id: V083-F1-REVIEW
- title: F1最终独立安全复审
- goal: 按29量表和CE-1至CE-8独立复核F1源码、测试、事务/网络/UI门禁，报告P0/P1及证据
- owner_thread: worker-f1-review
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-F1.md;.agent-work/output/V083-F1-GATE.md;.agent-work/output/V083-F1-MIG-SCOUT.md;.agent-work/28_f1_dispatch_plan.md;.agent-work/29_f1_acceptance_rubric.md;F1全部源码diff
- output_path: .agent-work/output/V083-F1-REVIEW.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
