# 08 交接与通知

## 协议

1. 主控派发任务并写入线程任务包。
2. 执行窗口先落盘，再通知主控。
3. 通知只做提醒，不承载最终事实。
4. 主控只依据本地文件和验收标准裁决。

## 交接记录

| timestamp | source | target | task_id | type | message | paths |
| --- | --- | --- | --- | --- | --- | --- |
| pending | 00-master | 04-project-master | - | bootstrap | 已建立初始交接协议 | `.agent-work/08_handoff.md` |
| 2026-08-07 | 04-project-master | workers | V083-N0 | bootstrap | v0.8.3 主控事实源已建立，等待正式派发 | `.agent-work/10_round1_dispatch_plan.md` |
