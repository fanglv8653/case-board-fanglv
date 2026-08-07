# V083-S1-REVIEW｜S1 独立只读安全审计

日期：2026-08-07
结论：**发现 2 项 P0、3 项 P1、3 项 P2；不建议 accepted，应退回 S1 修复并补反例测试。**

审计边界：完整阅读 `V083-S1.md`、`V083-S1-MIG.md`、`V083-N0-MIG.md`、S1 派工计划、S1 验收量表、0063 迁移及当前全部真实产品/测试 diff；未修改产品源码、测试、迁移或依赖，未运行 Cargo/Node/应用/构建，未访问正式数据库、NAS 或飞书，未提交 Git。静态门禁 `git diff --check` 退出 0，仅有工作区 LF/CRLF 提示。

## 一、P0 阻断项

### P0-1｜历史依赖证明只证明“曾经 upsert”，不证明依赖在目标序列前仍然有效，可写出必失败事件

证据：

- `src-tauri/src/device_sync/engine.rs:454-495` 的 `load_historical_dependency_proof()` 只要找到任意 `action='upsert'`、`state IN ('exported','acknowledged')` 且 `exported_sequence < next_sequence` 的记录，就把依赖视为已证明；查询没有排除同一实体在该 upsert 之后、当前序列之前已经导出的 tombstone。
- `src-tauri/src/device_sync/engine.rs:517-522` 将上述集合直接交给分包器；`src-tauri/src/device_sync/engine.rs:588-616` 随后把包和 manifest 写入 NAS。
- 接收端 `src-tauri/src/device_sync/operations.rs:342-359` 又按当前业务表是否存在依赖进行预检。因此，“序列 1 导出 contact upsert、序列 2 导出 contact tombstone、序列 3 待导出 case.judge_id 指向该 contact”会在发送端通过历史证明并写出序列 3，但按序导入序列 1、2 后，序列 3 必然得到 `SYNC_PACKAGE_DEPENDENCY_MISSING` 并自动暂停。

影响：依赖闭包仍可绕过 500 条分包安全门写出确定失败事件，直接命中 S1 验收量表的 P0 拒绝条件。该问题也使报告中“历史证明严格早于即可安全引用”的结论不成立。

建议：历史证明必须基于目标实体在当前待导出序列之前的**最后一个已持久化操作**，且最后操作必须是 upsert；或者把尚未被后续 tombstone 覆盖的实体状态建成可验证投影。增加“历史 upsert 后有 tombstone”的 3 序列反例，断言发送端在任何 NAS 写入前失败关闭。

### P0-2｜事件/manifest 先落盘、数据库后提交；失败重试无法幂等恢复，并把敏感绝对路径透传到 UI

证据链：

- `src-tauri/src/device_sync/engine.rs:572-616` 每次重试都会重新生成事件与 manifest 信封并先写文件；直到 `src-tauri/src/device_sync/engine.rs:618-644` 才推进 `next_sequence`、更新 outbox 并提交数据库。
- 如果 event/manifest 已成功落盘，而 `pool.begin()`、CAS、outbox 更新或 commit 失败，代码没有恢复/认领已落盘的同序列文件。下一次重试重新 seal；`src-tauri/src/device_sync/crypto.rs:120-130` 使用随机 nonce，header 还含当前时间，因此字节不可能稳定复现。
- `src-tauri/src/device_sync/nas_folder.rs:324-334` 遇到同名但不同字节的既有文件会返回 `Integrity`，错误正文包含 `target.display()` 的绝对路径。`src-tauri/src/device_sync/commands.rs:12-14` 原样拼入命令错误，`src/components/settings/DeviceSyncSettingsCard.tsx:153-156,188` 原样展示给 UI。

影响：一次普通的数据库提交/CAS 故障即可把同步永久卡在该序列，用户只能手工干预 NAS；同时错误 UI 会泄露 NAS 绝对目录。后者直接命中验收量表“隔离/日志/UI 泄露敏感绝对路径”的 P0 拒绝条件。多包导出时，前包已提交、后包留下半完成文件，风险更容易出现。

建议：为导出建立可恢复的两阶段状态，重试时先读取并验证同序列 event+manifest 是否与待导出 operation 集、前序 manifest hash 和当前数据库状态一致，能够安全认领则只补数据库提交；不一致则返回不含路径的稳定错误码并保留人工恢复证据。任何传给前端/审计的错误都只允许稳定 code 和脱敏字段。必须增加“event 写成后 manifest 失败”“两文件写成后 DB CAS/commit 失败”“第二个包失败后重试”的故障注入测试。

## 二、P1 阻断项

### P1-1｜重复操作仍参与依赖预检，已成功包的重放并不幂等

- `src-tauri/src/device_sync/operations.rs:320-335` 会把已存在于 `device_sync_applied_operations` 的 upsert 排除出 `package_upserts`；但 `src-tauri/src/device_sync/operations.rs:338-359` 第二轮没有跳过同一批 duplicate，仍按**当前**接收端实体状态检查其历史依赖。
- `src-tauri/src/device_sync/operations.rs:514-526` 原本能够把该操作直接返回为 duplicate，但这一分支位于包预检之后，无法执行。

反例：某 case+judge 包已经完整应用，之后 judge contact 被合法 tombstone；同一旧包重放时，本应全部 duplicate，却会因 judge 当前不存在而返回 `SYNC_PACKAGE_DEPENDENCY_MISSING` 并自动暂停。该行为违反接收端重放幂等和验收量表“接收端已有依赖与包内依赖一致判定”。

建议：包预检先建立可信的 duplicate 集并对 duplicate 校验原始 `source_device_id/source_sequence/payload_hash`；完全一致的 duplicate 不再参与业务依赖判断，不一致的同 operation_id 重用应以稳定完整性错误失败关闭。补“依赖后来删除后的整包重放”和“同 operation_id 不同 payload”反例。

### P1-2｜包预检忽略 tombstone，执行器又统一把 upsert 提前，正常多操作包可变成确定性失败

- `src-tauri/src/device_sync/operations.rs:320-360` 只收集/验证 upsert，不计算同包 tombstone 后的最终依赖状态。
- `src-tauri/src/device_sync/operations.rs:379-389` 无条件把 case upsert、contact upsert 排在其他操作之前；tombstone 进入最后一档，不再保持认证包中的完整逻辑顺序。
- `src-tauri/src/device_sync/operations.rs:392-448` 先执行全部操作，随后才补写 case.judge_id。若包内同时含依赖 contact 的 upsert 与 tombstone，预检会因 upsert 通过，contact 随后被删除，最后 judge 补写留下 deferred FK 在 commit 时失败；整个包虽会回滚，但会被当作确定性数据库错误隔离并自动暂停。

影响：同一组合法 outbox 操作是否成功取决于 500 条装箱边界；分成前后两个事件可能成功，恰好装入同一事件则失败。当前“完整包预检后才写入”的注释和报告结论因此过强。

建议：闭包规划与接收预检都必须纳入同实体多操作顺序和 tombstone 最终态；拒绝矛盾包应在任何业务写入前给出专用稳定错误码，或者严格按认证逻辑顺序构造可提交的最终依赖图。补 upsert+tombstone、tombstone+upsert、同实体多 op、恰跨 500 边界四组测试。

### P1-3｜隔离唯一键丢失设备身份；不同成员的同序号事件会碰撞，成功重放可错误 resolve 另一设备的隔离

- `src-tauri/src/device_sync/engine.rs:771-777` 将来源路径缩减为文件名；设备事件文件名只含序列号。
- `src-tauri/src/device_sync/engine.rs:805-829` 虽把 `device_id` 放入 details，但 upsert 键实际只有 `group_id + safe_source + reason_code`；`src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql:93-97` 的唯一索引同样没有设备身份。
- `src-tauri/src/device_sync/engine.rs:944-962` 成功导入时只按 `group_id + source_path` resolve，连 reason_code 和 device_id 都不校验。

反例：同一同步组的设备 A、B 都有 `00000000000000000001.cbe`，发生相同错误会被折叠为一条 active，后一次还会覆盖 details 中的设备身份；任一同名文件成功后会 resolve 这条共享记录。在成员被撤销/跳过等组合下，可丢失原失败归属并为未真正重放的失败留下成功路径。

建议：隔离表增加并强制 `source_device_id`、`source_sequence`，active 唯一键使用 `(group_id, source_device_id, source_sequence, reason_code)`；列表级错误另设 package key。resolve 必须匹配完整身份，并写入明确的 resolved audit。迁移需收敛旧行且保留无法判定身份的 `manual_review`/legacy 标识。

## 三、P2 / 应补强项

1. **0063 sentinel 仍不是充分语义校验。** `src-tauri/src/db/migration_safety.rs:514-533` 只对 active 唯一索引做片段匹配；`src-tauri/src/db/migration_safety.rs:703-751` 只确认 unique/partial 标志并检查 SQL 包含若干字符串。它没有验证 0063 的 `status`/`retry_count` CHECK、`last_error_code` NOT NULL、默认值、quarantine 外键及 group-status 索引列/顺序，且 `WHERE status='active' OR ...` 一类 lookalike 也可能包含所需片段。迁移 SQL 本身可在报告构造的真实旧 schema 上执行、NULL/重复旧键也能收敛，但“迁移 63 已应用”的防漂移证明仍不完整。
2. **暂停/恢复缺少显式审计。** `src-tauri/src/device_sync/queries.rs:114-143` 直接清除或覆盖 auto-pause 元数据，没有记录谁/何时手工暂停或恢复。隔离行保留 `resolved_at`、成功时间只在无 active 隔离时推进，这两点是正确的；但恢复动作和隔离 resolve 没有独立 audit，事后只能间接推断。
3. **现有定向测试没有覆盖上述反例。** `v083_failure_tests.rs` 覆盖了基本循环依赖、缺依赖回滚、重复隔离 upsert、暂停/恢复、0063 重复迁移、500/501/1001 和简单“严格早于”证明；未覆盖“早期 upsert 后 tombstone”“duplicate 依赖后来删除”“同包 tombstone”“多设备同文件名隔离碰撞”“部分 I/O 后重试”。UI 合同测试主要是源字符串顺序断言，不能证明错误脱敏。

## 四、已确认正确或基本闭合的部分

- `src-tauri/src/device_sync/engine.rs:725-754` 把整包业务写、revision/applied/conflict/dirty、成员序列推进和同包隔离 resolve 放在同一 SQLite 事务中；`apply_incoming_package` 返回错误时显式 rollback。未发现“单包失败留下部分数据库业务写入”的直接路径。
- `src-tauri/src/device_sync/operations.rs:448-497` 在 judge 最终补写后重新 fetch 实体、重算完整 field hash、更新既有 revision、清 dirty 并确认 applied operation；该成功路径的 revision/hash 与最终实体一致。
- `src-tauri/src/device_sync/engine.rs:108-190` 的 union-find 能传递合并依赖边与同实体 atomic group，组件超过 500 会在 NAS 写入前返回 `SYNC_PACKAGE_TOO_LARGE`；除 tombstone/历史状态问题外，基本依赖闭包与多包 manifest 前序 hash 传递成立。
- `src-tauri/src/device_sync/engine.rs:812-851` 的 quarantine upsert、自动暂停与 paused audit 位于同一事务；`src-tauri/src/device_sync/scheduler.rs:8-12,47-54` 只调度 `paused=0`；`src-tauri/src/device_sync/engine.rs:902-940` 在 active 隔离存在时不会推进 `last_success_at` 或写 succeeded audit。
- 0063 的旧重复行收敛使用 NULL-safe 的 `IS` 比较，并在创建 active 唯一索引前保留一条 active、其余转 resolved；针对报告所建 v0.8.2 旧表形状，迁移路径合理。
- 后端新增错误均有稳定 code，UI 能展示自动/手动暂停、尝试/成功时间、活动隔离，并在失败后刷新状态；但 P0-2 的原始错误正文透传必须先修复。

## 五、验收建议

**建议：V083-S1 rejected / returned_for_fix，不得 accepted。**

最低复验条件：

1. 关闭 P0-1 的最新历史状态证明漏洞，并证明不会在 NAS 产生必失败事件；
2. 建立 event+manifest+DB 的可恢复幂等导出协议，所有 UI/审计错误只输出稳定码和脱敏字段；
3. 修复 duplicate、tombstone/多 op 与 quarantine 完整身份三个 P1；
4. 为以上每项补可失败的反例测试，并由主控重新实跑 S1 定向测试、migration lineage、Cargo check/clippy、Windows Rust 全量、Node logic/build/source gate、`git diff --check`；
5. 本轮未执行正式 NAS/双设备验证，仍只能延后至 RC，修复报告不得提前宣称通过。

安全声明：本线程仅新增本审计报告并更新自己的工作流状态/提交通知；未修改任何产品源码、测试、迁移、依赖、版本或其他线程文件，未读取或修改正式数据库、NAS、飞书、凭据或业务正文，未提交 Git。
