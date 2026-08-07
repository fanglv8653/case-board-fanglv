# 07 决策与接管

## 当前决策

- 决策：使用 Markdown 作为人机共读的状态载体。
- 决策：主控脚本负责更新看板、线程状态和验收记录。
- 决策：V1 不做线程之间直接消息对话。

## 接管与回退记录

| timestamp | event | owner | details |
| --- | --- | --- | --- |
| pending | bootstrap | 00-master | 初始主控骨架建立完成 |
| 2026-08-07 15:07 +08:00 | V083-N0 accepted | 04-project-master | 三任务 accepted；同步与迁移各经历一次退回修正；全量门禁通过，允许进入 M1 |
| 2026-08-07 16:45 +08:00 | V083-M1 accepted | 04-project-master | fail-closed 主体经两轮退回和两次独立审计后 accepted；Windows Rust 280/0/3、设备同步 23/23、Node 119/119 及其余门禁全绿。所有 checksum mismatch 当前均阻断；历史自动兼容缺可信旧值，标记 pending_verified_input，禁止宣称已兼容旧谱系。允许进入 S1。 |
