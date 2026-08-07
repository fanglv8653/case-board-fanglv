# V083-S1-REVIEW-R2｜S1 返工 A 独立只读复审

日期：2026-08-07
结论：**返工 A 关闭了旧审计 P0-1 与三项原始 P1 的主要反例，但发现 1 项新增 P0、4 项 P1；加上明确未关闭的 durable export P0-2，S1 仍不得 accepted。建议继续 returned_for_fix。**

审计边界：完整阅读旧审计 `V083-S1-REVIEW.md`、最新 `V083-S1.md`、S1 验收量表、0063 迁移、当前真实产品与测试 diff；未修改产品源码、测试、迁移或依赖，未运行 Cargo/Node/应用/构建，未访问正式数据库、NAS 或飞书，未提交 Git。`git diff --check` 静态退出 0，仅有 LF/CRLF 提示。

## 一、旧问题复核

### 已关闭或主体关闭

1. **历史 upsert 后 tombstone：已关闭原反例。** `src-tauri/src/device_sync/engine.rs:479-518` 现在按 `exported_sequence DESC, logical_time DESC, operation_id DESC` 取目标序列前最后动作，只在最后动作是 upsert 时证明依赖。对于由当前 exporter 生成的同 sequence 数组，该排序与 `pack_operation_indexes()` 的稳定键一致。
2. **pending 同实体动作跨 500 边界：主体关闭。** `src-tauri/src/device_sync/engine.rs:121-170` 把同实体全部 pending 动作并入同一 union-find 组件，并以最终动作判定依赖；组件超过 500 会在 NAS 写入前返回 `SYNC_PACKAGE_TOO_LARGE`。原来的 upsert→tombstone 被拆包问题已消除。
3. **数据库中既有 exact duplicate：主体关闭。** `src-tauri/src/device_sync/operations.rs:358-396` 在业务预检前按 operation_id 查询，并核对 source device、source sequence、payload hash；精确重复不再依赖当前业务表，不一致身份在写入前返回 `SYNC_INTEGRITY`。
4. **签名数组中的同实体顺序：主体关闭。** `src-tauri/src/device_sync/operations.rs:399-465` 保留 pending 数组中同实体的先后边，仅为 contact→case 增加拓扑边，拓扑成环会稳定返回 dependency conflict；case→judge 使用事务末尾补写，基本循环依赖仍可成立。
5. **隔离完整身份：主体关闭。** 0063 已增加非空 `source_device_id/source_sequence`，active 唯一键为 group/device/sequence/reason；`src-tauri/src/device_sync/engine.rs:998-1039` resolve 严格匹配 group/device/sequence，并同事务写 `quarantine_resolved/succeeded` 审计。跨设备同文件名不会再互相 resolve。
6. **0063 sentinel：当前真实 diff 已继续吸收。** `src-tauri/src/db/migration_safety.rs:520-602,639-672` 已校验新增列定义、完整 quarantine 表 SQL 以及两个索引的完整定义，不再只是索引 SQL 片段包含判断。

上述结论不覆盖以下新反例。

## 二、P0 阻断项

### P0-1｜确定性导出规划错误仍可绕过自动暂停，被 scheduler 无限重试

证据：

- `src-tauri/src/device_sync/engine.rs:276-283` 只有 `is_deterministic_export_error()` 返回 true 才建立隔离并自动暂停。
- `src-tauri/src/device_sync/engine.rs:821-827` 只把 `PackageDependencyMissing`、`PackageTooLarge`、`PackageDependencyConflict` 列为确定性导出错误。
- 但导出规划在写 NAS 前还会稳定返回其他错误：`src-tauri/src/device_sync/engine.rs:454-475` 对未知 action 返回 `Protocol`，对损坏的 `changed_fields_json/base_field_hashes_json` 返回 `Serialization`；`src-tauri/src/device_sync/engine.rs:81-91,94-105` 对依赖字段类型错误也返回 `Protocol`。
- 这些错误不改变 outbox，也不暂停同步组；`src-tauri/src/device_sync/scheduler.rs:47-60` 下一轮仍会选中该组并再次执行，同一损坏行会永久失败和重复写日志。

可击穿现有测试的新反例：在合成 outbox 写入合法 action、但 `changed_fields_json='{'`；调用真实 `sync_once`，第一次返回 `SYNC_SERIALIZATION` 后断言 `paused=1`、active quarantine=1、scheduler 不再选中。当前实现会保持 `paused=0`、active=0，并在每次 tick 重试。

影响：直接命中验收量表“同一确定性失败继续无限重试”的 P0。现有 `deterministic_export_planning_failure_pauses_local_sequence_once` 只直接注入 `PackageTooLarge`，没有穿过真实 planner，也无法覆盖 Serialization/Protocol。

建议：把“规划阶段、任何 NAS 写入之前、由当前持久化输入稳定重现”的错误统一映射为可审计的安全 code 并自动暂停；不要简单把所有 Database/NAS 错误都算确定性。增加真实损坏 JSON、非法依赖类型及非法协议值反例。

### P0-2｜durable export 仍完全未关闭，且会同时触发无限重试和绝对路径日志/UI 泄漏

该项是旧审计明确 P0，本次报告也承认延期；当前代码没有变化：

- `src-tauri/src/device_sync/engine.rs:590-639` 先随机 seal 并写 event/manifest，`src-tauri/src/device_sync/engine.rs:641-667` 后开数据库事务推进 sequence/outbox。
- 文件已写而 DB CAS/commit 失败后，重试会因随机 nonce/时间生成不同字节；`src-tauri/src/device_sync/nas_folder.rs:324-334` 拒绝覆盖，并把 `target.display()` 绝对路径放入 `SYNC_INTEGRITY` 正文。
- 该 Integrity 不在 export 自动暂停集合；`src-tauri/src/device_sync/scheduler.rs:53-59` 会持续把完整错误写 stderr，`src-tauri/src/device_sync/commands.rs:12-14` 与 `DeviceSyncSettingsCard.tsx:153-156,188` 也会把原文透传到 UI。

因此 P0-2 不只是“未来耐久性增强”，而是当前验收量表的双重 P0：半完成 I/O 无法恢复并无限重试，同时日志/UI 泄露敏感绝对路径。S1 在该项关闭前不得 accepted。

## 三、P1 阻断项

### P1-1｜同实体真实因果顺序仍由“秒级时间＋随机 UUID”裁决，可把最终动作反转

- `src-tauri/src/device_sync/capture.rs:165-180` 用 `strftime('%s','now') * 1000` 产生 logical_time，实际只有 1 秒分辨率；operation_id 是随机 UUID。
- `src-tauri/src/device_sync/engine.rs:140-153` 以 `(logical_time, operation_id)` 排同实体动作并决定最终 action；`src-tauri/src/device_sync/engine.rs:505-506` 的同 sequence 历史最后动作也使用相同随机 tie-break。

反例：同一实体在同一秒内先 upsert（base revision r），后 tombstone（base revision r+1），但 tombstone UUID 字典序更小。planner 会把 tombstone 排在前、upsert 判为最终动作并按该顺序签名，接收端会复活发送端已经删除的实体；反向 UUID 则可能把真实最终 upsert 误判为 tombstone 并自动暂停依赖包。

现有 `pending_entity_action_order_is_atomic_at_the_500_boundary` 和历史同 sequence 测试都人为设置不同 logical_time，没有覆盖同秒碰撞；测试反转输入后仍要求相同结果，反而无法证明真实捕获顺序。

建议：为 outbox 增加单调、事务内确定的本设备序号，或至少对同实体使用可证明单调的 revision/capture sequence；随机 operation_id 只能做最终唯一 tie-break，不能决定业务先后。补“同 logical_time、UUID 逆序、base revision 单调”的反例。

### P1-2｜同一个签名事件内复用 operation_id 不会被分类器识别，第二个不同操作会被静默当 duplicate

- `src-tauri/src/device_sync/operations.rs:365-395` 分类阶段只查询已提交表，没有在当前 operations 数组内建立 operation_id 唯一集合。若数据库尚无该 ID，同一数组内的两项都会进入 pending。
- 第一项在 `src-tauri/src/device_sync/operations.rs:828-838` 写入 applied operation；执行第二项时，`src-tauri/src/device_sync/operations.rs:656-675` 只看到相同 device/sequence/payload hash，直接返回 duplicate，不比较第二项的 entity/action/fields。

反例：同一认证数组包含两个 operation_id 相同、但 entity_id 不同的独立 calendar upsert。包会成功提交第一项、静默跳过第二项、推进成员 sequence，且不会产生完整性错误。已有“同 ID 不同 payload”测试只覆盖数据库中已存在 ID 后换 envelope 的情形，未覆盖同一 payload 内 ID 重用。

建议：在任何业务预检前要求事件内 operation_id 唯一；重复即 `SYNC_INTEGRITY`，无论两项正文是否相同。补同事件相同 ID/不同实体、相同 ID/不同 action 两个零写入反例。

### P1-3｜`desired_judges` 预先按最后签名动作覆盖，未根据实际 conflict 结果回退，会丢失前序可应用的 judge 变更

- `src-tauri/src/device_sync/operations.rs:500-519` 在执行前按签名顺序只保留每个 case 最后一个 judge/tombstone 意图。
- 所有 judge upsert 在 `src-tauri/src/device_sync/operations.rs:521-560` 都先以 `judge_id=null` 应用；真正补写只在 `src-tauri/src/device_sync/operations.rs:564-587` 检查**最后意图对应操作**的 outcome。
- 如果同 case 的前一 judge 变更可应用、后一 judge 变更或 tombstone 因 receiver divergence 发生 conflict，最后 outcome 不补写，但前一意图已经被 map 覆盖，最终 null 占位会提交。

反例：签名数组依次为 case.judge=A（base hash 与接收端一致，可应用）、case.judge=B（base hash 与接收端分支不一致，产生 judge conflict）。正确结果应保留 A 并登记 B 冲突；当前实现会因最后 B 冲突而不补任何 judge，case 最终为 null，同时整个包仍成功。相同问题也发生在“前序 judge 可应用＋后续 tombstone conflict”。

建议：judge 延迟执行必须按拓扑/签名顺序维护实际可见的临时状态，后续 conflict 以该状态计算；不能先把所有 judge 改为 null、最后只看静态最终意图。增加多 judge＋第二项 conflict、judge＋tombstone conflict 的最终值/revision/hash/dirty 断言。

### P1-4｜新隔离状态仍没有完整恢复闭环：本地导出 active 永远不能自动 resolve，manual_review 也无人工处置入口

- 本地规划错误通过 `src-tauri/src/device_sync/engine.rs:830-844` 以 local device + next_sequence 创建 active。
- 全项目生产路径只有导入远端事件成功时在 `src-tauri/src/device_sync/engine.rs:776` 调用 `resolve_active_quarantine()`；本机不会作为 member 导入自己的事件。因此用户修复依赖/outbox 后恢复同步，即使导出成功，旧的本地 active 仍存在，`src-tauri/src/device_sync/engine.rs:909-953` 会在本轮末再次自动暂停，永远不能到达真实成功。
- 0063 把旧记录全部置为 manual_review，但 `src-tauri/src/device_sync/engine.rs:425-450` 和 UI 只展示计数；commands/queries 没有列出、确认、驳回或解决 manual_review 的入口。`record_sync_success()` 只检查 active，manual_review 可以永久悬空同时继续记 succeeded。

影响：导出规划失败没有“修复→显式恢复→重试→resolved→成功”的闭环；manual_review 名称也不对应任何可执行人工动作。命中验收量表 resolved 生命周期/明确恢复动作的 P1。

建议：为本地 export quarantine 建立与成功导出同事务或可验证相邻事务的严格 resolve，并写 resolved audit；为 manual_review 提供只读详情、脱敏证据和明确的确认/保留动作，动作必须审计。补真实 planning failure 修复后的 resume/re-export/resolved/success 测试。

## 四、P2 / 报告与安全边界问题

1. **旧隔离迁移仍原样复制敏感字段。** `0063_device_sync_quarantine_lifecycle.sql:43-57` 将 legacy `source_path`、`details_json` 原样写入 manual_review。v0.8.2 生产代码曾把完整 `path.to_string_lossy()` 和 `error.to_string()` 存入这两列，因此最新报告“隔离与审计不保存完整路径”的绝对表述不成立。当前 UI 只显示数量，尚未直接外泄，但新增人工复核详情前必须先做 basename/安全 code 迁移或明确受控本地证据边界。
2. **最新报告的全量验证不是全绿证据。** 报告自身记录 Windows Rust 仍有 3 个 M63 sentinel 失败；当前共享工作区的 migration safety 源码随后已出现进一步吸收，但没有与当前最终 diff 对应的重新实跑结果。主控不得把“295 passed、3 failed”写成全量通过。
3. **错误分类过宽/过窄并存。** import 端 `is_deterministic_event_error()` 把所有 Database/Crypto/Serialization 都视为确定性并永久自动暂停，其中可能包含瞬时 SQLite busy/I/O；export 端却漏掉由持久化 planner 输入导致的稳定 Serialization/Protocol。后续应按阶段和可重现性分类，而不是只按大枚举类型。

## 五、验收建议

**建议：V083-S1-REVIEW-R2 = returned_for_fix；不得 accepted。**

最低复验条件：

1. 关闭新增 P0-1，并用真实 planner 损坏输入证明一次隔离、自动暂停、scheduler 停止；
2. 修复同秒顺序、事件内重复 ID、judge 多操作冲突和本地 export quarantine 恢复闭环四项 P1；
3. 完成并验证 durable export P0-2，包括 event 成功/manifest 失败、两文件成功/DB CAS 或 commit 失败、第二包失败后的幂等认领与无路径错误；
4. 对最终稳定 diff 重新实跑 S1 定向测试、migration lineage、Cargo check/clippy、Windows Rust 全量、Node logic/build/source gate、`git diff --check`；
5. 正式双设备 NAS 两轮收敛仍延后 RC，不得在 S1 报告中提前宣称通过。

安全声明：本线程仅新增本复审报告并更新自己的工作流状态/提交通知；未修改任何产品源码、测试、迁移、依赖、版本或其他线程文件，未读取或修改正式数据库、NAS、飞书、凭据或业务正文，未提交 Git。
