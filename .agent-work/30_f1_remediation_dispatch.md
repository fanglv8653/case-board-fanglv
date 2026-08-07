# 30 V083-F1 返工派发

## 目标

关闭独立复审的 1 项 P0、2 项 P1，不扩大 F1 业务范围，不新增迁移。

## 必须修复

1. 设备同步所有可能改写 `feishu_sync_links` 绑定身份或状态的生产导入路径，必须与 delete/bind/unbind/ignore/restore、pull 和显式飞书字段/明细动作共享同一个进程级生命周期协议。不得仅在 Tauri 命令外层加锁；后台与手动 `device_sync::engine::sync_once` 生产入口也必须受控。
2. 允许采用同一 `try_lock` fail-fast 语义或经过证明的 generation 协议。若用共享锁，应抽到双方可访问的单一实现，避免重复锁、自锁或锁顺序死锁；设备同步遇到占用时必须返回稳定、脱敏、可重试错误，不能继续 apply。
3. 增加真实 barrier/受控 future 并发反例：显式飞书网络动作持锁并暂停时，设备同步绑定导入及生命周期动作不得进入改写；反向持锁时显式动作也不得进入。测试必须穿过各自生产锁入口，不能只连续调用两次锁 helper。
4. 修复 `active orphan + missing inbox`：直接本地解绑在同一事务内建立受抑制的恢复 inbox，再归档 link、失效候选/冲突并写 `previous_case_id=NULL` 审计；不得发起飞书网络调用。正常非 orphan link 缺 inbox 仍应 fail closed，避免掩盖一致性破坏。
5. UI 只有在后端动作真实可成功时才显示“解除孤立绑定”。新增缺 inbox active orphan 的数据库、DTO/UI、FK 与 HTTP 0/0 回归。
6. 恢复 Windows Rust 测试运行；独立审计出现的 `STATUS_ENTRYPOINT_NOT_FOUND` 需通过既有 manifest 运行方式规避/验证，不得删测试或放宽门禁。

## 禁止

- 不新增 `0064`，不修改迁移或 sentinel。
- 不访问正式数据库、NAS、飞书 Base、OAuth 凭据或真实案件数据。
- 不修改无关模块；设备同步仅允许改共享协调入口和对应反例。
- 不 commit、不 push、不 merge。

## 验收

- 原 F1 Rust 8/8、共享锁测试、Node 122/122 全部保持通过。
- 新增设备同步真实并发反例与缺 inbox orphan 直接解绑反例全部通过。
- Windows Rust 全量、device sync contract、cargo check、全目标 Clippy `-D warnings`、Vite build、source gate、diff check 全通过。
- 新一轮独立只读复审 P0=0、P1=0。
