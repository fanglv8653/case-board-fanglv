# 线程任务包

## 任务信息

- task_id: V083-FORMAL-DEVICE-GATE
- title: 本设备正式安装与回滚门禁
- goal: 只读盘点本机0.8.2正式安装、进程、数据/设置/凭据/同步状态和可回滚备份方案，不执行安装或改数据
- owner_thread: worker-formal-device-gate
- reviewer: 04-project-master
- input_path: .agent-work/output/V083-RC-FINAL-20260808_本地验收与外部阻塞.md;本机正式安装与用户目录;项目发布脚本
- output_path: .agent-work/output/V083-FORMAL-DEVICE-GATE.md

## 允许操作

1. 只修改本线程目录及指定输出路径。
2. 更新本线程的状态、备注和交付物。
3. 向 `08_handoff.md` 写入提交通知。

## 禁止操作

1. 修改 `00_status.md`。
2. 直接写入 `09_dispatch_board.md` 的主控字段。
3. 覆盖其他线程目录。
4. 绕过主控直接宣称任务通过。
