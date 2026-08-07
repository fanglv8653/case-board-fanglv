# 线程任务包

## 任务信息

- task_id: V083-RC-REVIEW
- title: 0.8.3 RC独立总复核
- goal: 对RC全部源码差异、版本、测试证据、release EXE和blocked_external边界做独立只读复核，给出P0/P1/P2结论
- owner_thread: worker-rc-review
- reviewer: 04-project-master
- input_path: .agent-work/33_rc_acceptance_rubric.md;.agent-work/output/V083-RC-GATE.md;.agent-work/output/V083-RC-DBSYNC-GATE.md;.agent-work/output/V083-RC-LOCAL.md;.agent-work/output/V083-RC-LOCAL-R2.md;当前git差异与测试日志
- output_path: .agent-work/output/V083-RC-REVIEW.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
