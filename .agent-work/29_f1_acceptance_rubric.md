# 29 V083-F1 验收量表

## P0 拒绝

- 案件删除后仍可能留下 active case link、bound inbox、可执行 pending candidate/conflict，或清理与删除不在同一事务。
- 一个 orphan 仍能使同批有效案件预演回滚，或 run 被错误标记为 succeeded/failed 而不是可见的 partial。
- 历史 orphan 解绑仍可能因 binding audit 外键失败。
- unbind/rebind 后旧字段或明细候选可跨案件执行，或失效候选在 token/HTTP 之后才被拒绝。
- delete/bind/unbind/rebind 可与显式飞书写跨生命周期交错提交，造成旧案件值写入新绑定。
- 本地修复、删除、解绑、重绑产生任何飞书写调用，或测试连接正式资源。
- 为 F1 新增 `0064`、修改既有迁移/sentinel，或放宽外键/事务门禁。

## P1 拒绝

- UI 继续把 orphan 显示为 UUID，或仍提供“采用飞书/写回飞书”动作。
- 前端依赖中文错误文本分类；`FEISHU_ORPHAN_BINDING`、`FEISHU_REVIEW_NOT_FOUND` 等稳定码未明确映射。
- partial run 的稳定码/说明不可见，用户仍被误导为权限故障。
- 删除、孤立拉取、孤立解绑、重绑旧候选、多 link 回滚、网络 spy、并发锁任一缺少自动化反例。
- `foreign_key_check` 非空，或审计无法通过 inbox/link/动作/时间追踪。
- 修改无关案件字段、HomeView、团队、聊天、AI、MCP 或其他非 F1 模块。

## 接受条件

- CE-1 至 CE-8 全部通过；本地生命周期动作读写网络均为 0，pull 仅允许既定只读拉取且写网络为 0。
- 删除/隔离/解绑/重绑终态、回滚原子性、候选授权与稳定错误码均有数据库和 UI 自动化证据。
- 定向测试、Windows Rust 全量、Node logic、`cargo check`、全目标 Clippy `-D warnings`、Vite build、source gate、`git diff --check` 全部通过。
- 独立只读复审无未关闭 P0/P1；工作流 audit 通过，且正式数据库、NAS、飞书 Base 和凭据未被访问。
