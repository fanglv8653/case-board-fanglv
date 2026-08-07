# V083-F1-REVIEW｜F1 最终独立安全复审

- 复审日期：2026-08-07
- 复审方式：只读源码、diff、测试与门禁复核；未修改产品代码、测试、迁移或 sentinel
- 正式资源边界：未访问正式 SQLite、NAS、飞书 Base、OAuth 凭据或真实案件数据
- 最终结论：**拒绝验收（P0=1，P1=2）**

## 一、结论摘要

F1 已经静态闭合原 Gate 的多数主路径：案件删除同事务清理、多 link 缺 inbox 整体回滚、pull orphan 隔离后以 `partial + FEISHU_ORPHAN_BINDING` 提交、历史 orphan 审计写 `previous_case_id=NULL`、unbind/rebind 失效旧候选、候选在 token/HTTP 前进行稳定码拒绝、UI 隐藏 orphan 的采用/写回动作、最新 run 的稳定码展示，以及未增加 `0064`/未修改迁移与 sentinel。

但当前实现仍有一条 P0 级跨生命周期写竞态，并且自动化反例没有覆盖该真实并发入口；另有一种由现有模型明确允许的“active orphan + 缺 inbox”状态无法通过 UI 提供的唯一恢复动作解除。因此不满足 `.agent-work/29_f1_acceptance_rubric.md` 的接受条件。

## 二、P0 缺陷

### P0-01｜设备同步可绕过 `FEISHU_WRITE_LOCK` 改写绑定，显式飞书写仍可跨生命周期提交

**证据链：**

1. F1 的案件删除、bind、unbind 以及字段/明细显式写命令在 `src-tauri/src/lib.rs:275-277`、`1579-1592`、`4699-4709`、`4790-4800` 获取 `FEISHU_WRITE_LOCK`；这是正确方向。
2. 设备同步后台由 `src-tauri/src/lib.rs:6481` 启动，手动入口也可从 `src-tauri/src/device_sync/commands.rs:169-176` 直接进入 `engine::sync_once`。该链只使用另一把独立的 `SYNC_RUN_LOCK`（`src-tauri/src/device_sync/engine.rs:310-314`），不获取 F1 的 `FEISHU_WRITE_LOCK`。
3. `feishu_sync_links` 被明确注册为可同步实体，且可同步列包含 `local_entity_id` 与 `status`（`src-tauri/src/device_sync/registry.rs:360-381`）。现有 `FEISHU_BINDING_GROUP` 只含 `app_token/table_id/record_id/slot_key`，不含 `local_entity_id/status`（`registry.rs:28`）。
4. 收到远端包后，设备同步在自身事务内调用 `apply_incoming_package`（`src-tauri/src/device_sync/engine.rs:1380-1389`）；安全字段随后直接交给通用 `apply_upsert`（`src-tauri/src/device_sync/operations.rs:857-873`），最终动态执行 `UPDATE feishu_sync_links ... WHERE id=?`（`operations.rs:975-1006`）。该链既不失效 F1 pending candidates，也不参与 F1 锁或 binding generation。

**可复现交错：**

1. link L 当前绑定 case A，pending field/entity candidate 已通过授权；显式“保留本地并写飞书”持有 `FEISHU_WRITE_LOCK` 并进入网络阶段。
2. 定时或手动设备同步持有自己的 `SYNC_RUN_LOCK`，导入对同一 L 的 `local_entity_id/status` 更新，使 L 归属 case B 或被归档；该提交不受 F1 锁阻止。
3. 旧操作继续把已取出的 A 值写到 L 对应远端记录，并在新生命周期下完成本地收尾。结果正是量表禁止的“旧案件值写入新绑定/跨绑定生命周期提交”。

**分级依据：** `.agent-work/29_f1_acceptance_rubric.md:8-10` 将“delete/bind/unbind/rebind 可与显式飞书写跨生命周期交错提交”列为 P0。这里不是理论上的锁命名问题，而是已有后台写入链可直接改动同一绑定行。

**必须修复：** 将所有会改写 `feishu_sync_links` 绑定身份或状态的设备同步导入/冲突处理纳入同一个进程级生命周期协议；可以共享同一把锁，或增加持久化 binding generation 并让设备同步更新、候选授权、HTTP 前复核及 HTTP 后收尾共同校验。仅在 Tauri F1 命令外层加锁不足。

## 三、P1 缺陷

### P1-01｜active orphan 缺 inbox 时，UI 唯一恢复动作必然失败

**证据链：**

1. bound 查询会返回所有 active case link，并用 `LEFT JOIN cases` 标记 orphan，不要求 inbox 存在（`src-tauri/src/db/feishu_sync.rs:980-1001`）。
2. UI 对任何 `is_orphaned` 项都展示“解除孤立绑定”，并直接调用 `unbindFeishuSyncCase`（`src/modules/tools/FeishuSyncPreview.tsx:353-356`）。
3. `unbind_case` 在变更 link 前强制调用 `case_link_inbox`（`src-tauri/src/db/feishu_sync.rs:1878-1908`）；该 helper 对缺 inbox 直接返回 `FEISHU_BINDING_NOT_FOUND`（`feishu_sync.rs:206-218`），事务不改变任何状态。
4. pull 的 orphan 预扫描会先 `INSERT OR IGNORE` 补一个恢复 inbox（`feishu_sync.rs:425-442`），但 UI 的直接 unbind 路径没有同等兜底。
5. CE-8 自己已经构造并认可“active link 缺 inbox”这一历史/异常状态（`src-tauri/src/db/feishu_f1_tests.rs:492-508`），所以不能以“正常数据不会出现”排除该反例。

**最小复现：** 在临时库创建一条指向不存在 case 的 active link，且不创建相同 app/table/record 的 inbox；`get_preview` 返回 `is_orphaned=true` 并呈现解绑按钮；调用 `unbind_case(link_id)` 得到 `FEISHU_BINDING_NOT_FOUND`，link 仍为 active。该恢复动作没有飞书读写，但也无法清除孤立状态。

**必须修复：** 推荐在 orphan unbind 的同一事务内按 pull 相同规则合成受抑制的恢复 inbox，再归档 link、失效候选并写 NULL-FK 审计；或在缺 inbox 时不展示“可成功”的动作并提供另一条真正可执行的本地修复入口。需新增“active orphan + missing inbox + 直接 unbind”数据库及 UI 自动化反例。

### P1-02｜CE-6 自动化只验证同一 helper 的 try-lock，没有验证真实并发写入口

`src-tauri/src/lib.rs:4683-4694` 的锁测试只是先持有 `acquire_feishu_write_lock()`，再断言第二次获取返回 busy；它没有让显式飞书写与任一生命周期事务并发，也没有覆盖设备同步导入。`src-tauri/src/db/feishu_f1_tests.rs:511-531` 的“zero HTTP”测试则直接调用 DB 函数，未经过 Tauri 编排层，更不是并发测试。

量表把“并发锁缺少自动化反例”单列为 P1（`.agent-work/29_f1_acceptance_rubric.md:18`）。应使用 barrier/受控 future：在显式网络闭包授权后暂停，同时触发生命周期或设备同步绑定更新，证明后者必须等待，或证明 generation 变化会使前者在 HTTP 写前/提交前 fail closed。

## 四、测试与门禁复核

| 项目 | 本次结果 | 说明 |
|---|---|---|
| Node 定向测试 | 通过，8/8 | `node --test src/modules/tools/FeishuSyncPreview.test.mjs src/modules/tools/feishuOrphanBindingUi.test.mjs`；这些是源码/映射门禁，未覆盖缺 inbox 的真实解绑结果 |
| Rust F1 定向复跑 | 未启动成功 | 编译完成后测试二进制退出 `0xc0000139 / STATUS_ENTRYPOINT_NOT_FOUND`；因此本次不能把实现报告中的旧成功记录重新确认为当前环境通过 |
| `git diff --check` | 通过 | 仅有 Windows 工作区 LF→CRLF 提示，无 whitespace error |
| 迁移/sentinel | 通过 | `git diff --name-only -- src-tauri/migrations src-tauri/src/db/migration_safety.rs` 为空；无 `0064*` |
| 正式资源 | 未访问 | 未读取正式 DB/NAS/飞书/凭据，未发起真实 HTTP |

Rust 环境失败本身不是上述 P0/P1 的根因，但在修复后必须恢复可执行测试环境并重跑量表要求的定向、Windows Rust 全量、Node logic、`cargo check`、全目标 Clippy `-D warnings`、Vite build、source gate 与 diff check。

## 五、已经静态闭合的 CE 项

- CE-1/CE-8：`delete_case` 与绑定隔离处于同一 SQLite 事务，多 link 任一缺 inbox 整体回滚。
- CE-2：pull 先隔离 orphan，再继续有效记录，run 以 partial 和稳定码提交。
- CE-3：历史 orphan 在 inbox 存在时解绑审计使用 `previous_case_id=NULL`，避免失效 FK。
- CE-4/CE-5：标准 unbind/rebind 会 supersede/dismiss 旧候选，resolver 在网络闭包前按稳定码拒绝。
- CE-7：active orphan 显示“本地案件已删除”与 `FEISHU_ORPHAN_BINDING`；字段/明细候选不返回，UI 不提供采用/写回动作；pull 后 archived orphan 不再显示为 bound。
- 最新 run 使用确定性倒序，partial 的 error code/message 可见；前端按稳定码而非中文文本分类。

这些闭合项不能抵销 P0-01、P1-01 和 P1-02，因此当前总体仍为拒绝。

## 六、建议修复顺序与复验入口

1. 先统一设备同步与 F1 的绑定生命周期并发协议，增加真实 barrier 并发反例；这是 P0。
2. 再补齐缺 inbox orphan 的直接本地解绑事务和自动化反例。
3. 恢复 Rust 测试二进制运行环境，执行 CE-1 至 CE-8 加上述两个新反例。
4. 最后重跑全量量表；独立复审确认 P0/P1 为 0 后，才可由主控验收。本文仅提交复审意见，不自行宣布通过。
