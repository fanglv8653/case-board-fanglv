# 24 V083-S1 返工 B 派发计划

## 目标

关闭独立审计 P0-2：任何 event、manifest 或数据库最终确认中断后，重试都能复用同一份已加密字节安全恢复；不得覆盖不同内容的同序列对象，也不得把 NAS 绝对路径传到 UI 或审计。

## 协议不变量

1. 在第一次 NAS 写入前，先在 SQLite 事务中持久化该序列的加密 event/manifest 信封、前序 manifest hash、最终 manifest hash、稳定 operation_id 列表及校验指纹。草稿只保存密文和安全元数据，不保存解密业务正文或密钥。
2. 随机 nonce、签名和生成时间只在草稿创建时生成一次。任何重试都加载并验证既有草稿，禁止重新 seal 后尝试覆盖同序列文件。
3. NAS 写入必须是逐字节幂等的；同名同字节视为已完成，不同字节返回稳定完整性错误且保留现场。发布顺序应避免接收端看见缺 manifest 的新事件；同时必须能认领升级前或故障注入形成的 event-only / manifest-only 同字节现场。
4. event 与 manifest 均确认存在后，才在一个 SQLite 事务内完成：校验草稿与 group 当前 `next_sequence`/前序 hash、一致性校验对应 outbox、CAS 推进序列及 hash、标记 outbox exported、完成或清理草稿。CAS/commit 失败不得留下部分数据库状态。
5. 多包导出逐序列执行；较早包已最终确认、后续包中断时，重试只恢复未确认序列，不重写或跳过已确认序列。
6. 并发导出只允许一个草稿获胜。冲突方必须加载获胜草稿或返回 `SYNC_BUSY`，不得生成第二份可发布字节。
7. 所有命令错误统一为稳定 code + 脱敏中文提示。`Integrity`、`NasUnavailable`、`InvalidNasPath`、数据库和序列化错误的内部正文不得进入 Tauri 返回值、UI notice 或审计 details。

## 最小结构

- 在尚未发布的 `0063_device_sync_quarantine_lifecycle.sql` 中增加 durable export draft 表及必要唯一键、CHECK、FK/索引；不得新增 0064。
- 草稿至少绑定 `group_id + local_device_id + sequence`、前序 hash、两个加密信封、两个密文 hash、operation_id 列表/指纹、状态和时间。
- 产品实现范围仍限 `src-tauri/src/device_sync/**`、0063 及设备同步设置页所需最小类型/展示。
- 不修改 M1 lineage/sentinel；完成产品实现后由迁移线程串行吸收。

## 返工 A 复审追加项

以下缺陷与 durable export 同批关闭，不得留到 F1：

1. `capture.rs` 不得再用“秒级 logical_time + 随机 UUID”裁决同实体动作先后。为每组本机 outbox 建立事务内严格递增顺序；0063 对旧同秒记录按旧版实际使用的 `(logical_time, operation_id)` 顺序无歧义归一化，并加数据库唯一约束。planner、历史证明和签名数组统一使用该顺序。
2. 在任何业务预检前拒绝签名事件数组内重复的 `operation_id`，无论两项正文是否相同；返回稳定完整性错误且零业务写入。
3. 多个 case judge 操作必须按签名/拓扑顺序维护真实的临时 judge 状态。前一项成功、后一项 conflict 时保留前一项；后一项的冲突判断也必须基于前一项成功后的状态。不得把全部 judge 先置空后只看静态最后意图。补 judge→judge conflict、judge→tombstone conflict 及连续成功反例，并校验最终值、revision/hash、dirty 与 conflict。
4. 本地 export quarantine 在对应 `group + local_device + sequence` 成功最终确认时必须严格 resolve 并同事务审计，否则恢复后仍会再次自动暂停。
5. `manual_review` 必须有只返回脱敏字段的列表和显式“确认归档/保留待核”动作，动作需审计。0063 迁移旧记录时不得原样复制可能含绝对路径/底层错误正文的 `source_path/details_json`。
6. 真实 planner 因持久化 outbox 损坏产生的 Serialization/Protocol/Integrity 等可稳定复现错误必须只隔离一次、自动暂停并退出 scheduler；瞬时 Database/NAS 错误不得被粗暴归为确定性错误。用损坏 JSON、非法依赖字段类型和非法 action 穿透真实 `sync_once` 验证。

## 必测故障

1. 第一个 NAS 对象成功、第二个写失败；重试复用相同草稿字节并完成。
2. event 与 manifest 均存在，但 CAS 后、commit 前注入失败并回滚；重试认领原字节并一次性最终确认。
3. 两个以上包：第一包完成，第二包部分写入失败；重试不改变第一包，恢复第二包并继续后续包。
4. 同序列 NAS 已有不同字节：不覆盖、不推进数据库，错误正文和 UI 均不含临时目录或绝对路径。
5. 数据库已有草稿但 NAS 全空、仅 event、仅 manifest、两者齐全四种状态均有确定结果。
6. 草稿绑定的 operation 集、前序 hash、设备或序列与当前数据库不一致时 fail closed。
7. 竞争创建同一序列草稿不会产生两份随机信封；最终只有一组 outbox/CAS 结果。
8. 草稿持久化内容断言不含测试业务正文、密钥或完整路径。
9. 同秒且 UUID 逆序的 upsert/tombstone 仍按捕获先后得到正确最终态；旧行归一化保持旧 planner 的既有次序。
10. 同一事件内重复 operation_id、连续 judge 成功/冲突组合均在零部分写入或正确最终状态下结束。
11. 本地规划失败经修复、显式恢复并成功导出后，原 active 隔离变 resolved，随后才允许记成功。
12. legacy manual_review 列表不泄露旧绝对路径/错误正文，确认归档与保留动作均有审计。

## 验证与边界

- S1 定向测试、设备同步契约、Windows Rust 包装脚本、`cargo check`、Clippy `-D warnings`、Node logic/build/source gate、`git diff --check`。
- 仅使用内存/临时合成数据库与临时目录；不访问正式数据库、正式 NAS、飞书或真实凭据。
- 不 commit、不 push；报告必须区分已自动验证与延期到 RC 的真实双设备/NAS 验证。
