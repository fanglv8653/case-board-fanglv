# 31 V083-F1 返工验收量表

## P0 拒绝

- 设备同步后台或手动入口仍可绕过共享协议改写 `feishu_sync_links`，与显式飞书网络动作跨生命周期交错。
- 并发测试只验证锁对象本身，未穿过设备同步与显式动作的生产协调入口。
- 通过扩大数据库 schema、放宽候选授权/外键或删除既有设备同步能力规避竞态。

## P1 拒绝

- active orphan 缺 inbox 时 UI 仍提供必然失败的解绑，或后端为了兜底让正常非 orphan 缺 inbox 静默通过。
- 缺 inbox 恢复事务未同时设置 pending/null/suppressed、归档 link、失效候选/冲突和写 NULL-FK 审计。
- 新路径有任何飞书读写调用，或稳定错误不脱敏/不可重试。
- 原 CE-1 至 CE-8、稳定码三分、partial run 或 UI 门禁出现回归。

## 接受

- 真实并发反例证明设备同步、生命周期与显式飞书写不能跨绑定提交。
- 缺 inbox active orphan 可由 UI 唯一本地动作成功恢复，`foreign_key_check` 为空，HTTP read/write 均为 0。
- 全量门禁通过，独立复审无 P0/P1。
