# 线程任务包

## 任务信息

- task_id: V083-RC-LOCAL-R2
- title: RC本地release executable补验
- goal: 补跑0.8.3无bundle release executable构建，核验版本/隔离启动边界并更新RC本地报告
- owner_thread: worker-rc-local-r2
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-RC-LOCAL.md;.agent-work/34_rc_local_dispatch.md;当前RC源码与门禁结果
- output_path: .agent-work/output/V083-RC-LOCAL-R2.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
