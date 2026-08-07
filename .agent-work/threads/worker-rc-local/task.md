# 线程任务包

## 任务信息

- task_id: V083-RC-LOCAL
- title: RC本地集成版本与隔离收敛
- goal: 完成0.8.3版本准备、0062升级夹具、双端两轮幂等与隔离恢复综合测试及全部本地发布门禁
- owner_thread: worker-rc-local
- reviewer: 04-project-master
- input_path: .agent-work/32_rc_dispatch_plan.md;.agent-work/33_rc_acceptance_rubric.md;.agent-work/34_rc_local_dispatch.md;.agent-work/output/V083-RC-GATE.md;.agent-work/output/V083-RC-DBSYNC-GATE.md
- output_path: .agent-work/output/V083-RC-LOCAL.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
