# V083-S1-REVIEW-R3：S1 返工 B 最终独立安全审计

状态：`submitted_for_review`

## 一、结论

**不建议验收通过，结论为 rejected。**

R2 指出的主要业务缺陷大多已实质关闭：草稿先于 NAS 持久化、manifest-first/event-last、SQLite CAS 收尾、多包逐序列推进、outbox 指纹绑定、事件内重复 `operation_id` 拒绝、judge 连续操作影子状态、本地 export quarantine 延迟到最终提交后解除、legacy manual-review 脱敏与人工入口、公开错误统一脱敏，静态实现均已形成闭环。

但最终反例审计仍发现 **2 项 P0、3 项 P1、2 项 P2**。其中 NAS 发布并不具备原子 create-new 语义，且持久化草稿恢复没有验证加密信封真实字节、签名和协议版本；两者都能造成 SQLite 已推进而 NAS 上出现错误或被覆盖的事件，直接违反返工 B 的核心安全不变量。现有 durable-export 测试又主要手工串联 `*_for_test` helper，没有证明生产 `export_pending -> publish_prepared_export -> finalize_prepared_export` 编排路径。

## 二、P0 阻断项

### P0-1：NAS “同名不同字节不覆盖”仍存在 TOCTOU，`rename` 不是原子 no-replace

证据：`src-tauri/src/device_sync/nas_folder.rs:356-390`。

当前 `atomic_write` 先执行 `target.exists()`，随后写入随机临时文件，最后直接 `fs::rename(&temp, target)`。存在性检查与 rename 之间没有原子排他条件，也没有 no-replace API、目标占位锁或 rename 失败后的目标字节复核。

可击穿反例：

1. 写入者 A、B 同时检查，均观察到目标不存在；
2. A、B 分别写完并 `sync_all` 自己的临时文件；
3. A 先 rename 成功；B 随后仍直接 rename 到同一目标；
4. 在允许 rename 替换目标的平台或 SMB 实现上，B 可覆盖 A；即使某些实现返回“已存在”，当前代码也没有在失败后读取目标并把“同字节”认领为成功。

这与报告宣称的“create-new；同字节幂等、不同字节保留现场”不一致。现有测试只覆盖“调用前目标已经存在”，没有覆盖两个写入者都通过 `exists()` 的竞争窗口。

整改要求：使用真正的原子 no-replace 发布原语；若跨平台只能分支实现，必须把 `AlreadyExists` 转换为“读取目标并逐字节比较”，相同才成功、不同返回脱敏完整性错误。补充两个独立写入者对同一目标的真实竞争测试，并断言不同字节绝不覆盖。

### P0-2：草稿恢复只比较信封“自报 hash”，没有验证真实密文字节、签名和协议版本

证据：

- `src-tauri/src/device_sync/engine.rs:675-693` 仅反序列化信封，并比较 header 的 group/device/sequence/key_epoch/payload_kind 以及 `envelope.ciphertext_sha256 == draft.*_ciphertext_sha256`；
- 该处没有校验 `header.protocol_version`，没有 Base64 解码后重算 SHA-256，也没有验证签名；
- 完整验证实际只存在于 `src-tauri/src/device_sync/crypto.rs:137-169` 的 `open()`；
- `src-tauri/src/device_sync/engine.rs:970-1003` 在发现已有草稿时刻意不加载密钥，随后直接发布并收尾。

可击穿反例：prepare 已提交后进程崩溃；本地 SQLite 中 `event_envelope_bytes` 的 `ciphertext_b64`、`signature_b64` 或 `header.protocol_version` 发生可解析篡改/位损坏，而信封内 `ciphertext_sha256` 与草稿列保持原值。恢复校验会通过，错误字节被写入 NAS，`finalize_prepared_export` 随后推进 `next_sequence`、manifest hash 并标记 outbox exported；接收端才在 `open()` 失败并隔离。发送端已经不可逆地越过该序列。

现有 `durable_export_draft_binding_mismatches_fail_closed` 只变更 outbox、前序 hash、device、sequence、epoch，没有变更两份信封 BLOB、协议版本、密文、nonce 或签名。

整改要求：恢复和 finalize 前对两份信封执行完整的本机可信校验，至少包括协议版本、Base64、真实密文字节 hash、签名；如不希望解密 manifest/event，也必须使用本机受信公钥验证签名并重算 hash。新增持久化草稿逐字段损坏反例，断言 NAS/组序列/outbox 均不推进。

## 三、P1 高优先级项

### P1-1：durable-export 关键测试绕过生产编排路径

证据：`src-tauri/src/device_sync/v083_failure_tests.rs:852-1343`。

empty / manifest-only / event-only / both、CAS 回滚、不同字节、多包中断、并发草稿等测试，均手工串联：

- `prepare_next_export_for_test`；
- `publish_prepared_export_for_test`；
- `finalize_prepared_export_for_test`。

其中 `publish_prepared_export_for_test` 在 `engine.rs:913-948` 复制了一套发布编排，而不是从生产 `publish_prepared_export` 注入故障；多包测试也由测试代码自行选择第一包、第二包，没有执行生产 `export_pending` 的循环恢复。生产入口 `export_pending_for_test` 只在包过大、历史依赖等规划测试中出现，未覆盖已有 draft 的四种 NAS 现场和第二包恢复。

风险：helper 与生产编排发生漂移时，35/35 仍可通过；P0-1 就没有真实竞争测试，P0-2 也没有真实草稿字节损坏测试。

整改要求：把故障注入点放入生产 `export_pending/publish_prepared_export/finalize` 控制流，仅在 `cfg(test)` 下选择故障阶段；至少增加一个从真实 `export_pending_for_test` 进入的 manifest 后崩溃恢复、CAS 后回滚恢复和 501 项第二包中断用例。

### P1-2：`capture_sequence` 使用 `MAX+1`，没有并发分配器或重试闭环

证据：

- `src-tauri/src/device_sync/capture.rs:169-176`；
- `src-tauri/src/device_sync/operations.rs:249-255`；
- 唯一索引仅在 `0063_device_sync_quarantine_lifecycle.sql:34-35` 负责拒绝重复；
- 测试 `v083_failure_tests.rs:2643-2709` 只在同一 transaction 中顺序调用两次 enqueue。

两个 deferred transaction 可同时读取相同 MAX，随后一个成功，另一个得到 busy/唯一约束错误。唯一索引能防止重复编号，但不能提供“并发调用均获得严格递增序列”的分配语义，也没有针对跨连接/跨进程竞争的有限重试。`capture_dirty_entities` 受当前进程 `SYNC_RUN_LOCK` 间接保护，但锁不是跨进程锁，且 `enqueue_operation` 本身没有写锁前置。

整改要求：把下一个 capture sequence 放入组级计数器，使用单条 `UPDATE ... RETURNING` 或先取得组写锁后分配；补充两个独立连接并发捕获/入队测试，断言两次都成功、序列唯一且严格递增。

### P1-3：文件内容已 flush，但 rename 后目录项没有持久化证明

证据：`src-tauri/src/device_sync/nas_folder.rs:379-390`。

临时文件执行了 `file.sync_all()`，但 rename 后没有对父目录做可行的持久化确认。崩溃窗口为：event rename 返回成功 -> SQLite finalize 提交 -> 掉电/网络存储元数据尚未稳定，重启后事件目录项丢失，而本地 sequence/outbox 已推进。对 Windows/SMB 的真实耐久语义当前既无实现说明，也无故障测试。

整改要求：明确 Windows 本地卷与 SMB 的承诺边界，采用平台可支持的目录/句柄 flush 或存储侧 durable-write 策略；无法保证时不得把 rename 返回等同于“已持久化”，并应在收尾前做重新打开、字节校验及可审计的降级说明。

## 四、P2 完整性与证据项

### P2-1：finalize 的 prepared 对象标量字段没有与重新加载结果逐项比较

`engine.rs:1078-1087` 只比较两份 envelope bytes、两份 hash、operation IDs 和 fingerprint，没有显式比较 `key_epoch`、`previous_manifest_hash` 等 `PreparedExport` 标量。生产当前对象来自同一加载路径，直接利用难度低；但作为安全收尾边界，建议统一比较完整结构或仅使用 transaction 内重新加载的 `current` 执行 CAS，避免测试钩子/未来重构引入内存对象漂移。

### P2-2：最终报告与当前迁移扩展后的全量证明未形成同一快照

`V083-S1.md` 仍记录 Windows Rust “312 通过 / 1 失败”，失败原因是旧迁移 fixture 缺少 outbox；`V083-S1-MIG-R2.md` 虽记录其当时 300/0、40/0，但报告末尾明确提示后续若把 durable draft、capture sequence、legacy normalization 继续并入 0063，需要重新扩展 sentinel 和迁移语义测试。当前源码已经完成这些扩展，但尚无一份与最终产品 diff 同快照的合并门禁证据。本轮按任务要求未执行测试，因此只能确认 `git diff --check` exit 0。

## 五、已确认关闭的 R2 项

- draft 在 NAS 前提交；恢复优先认领唯一 prepared draft；finalize 的 group CAS、逐条 outbox 更新、quarantine resolve、draft 删除位于同一 SQLite transaction。
- manifest-first、event-last；已有同字节的非竞争场景可幂等，已有不同字节的非竞争场景会拒绝且不推进 DB。
- 多包循环每轮重新加载 group；第一包完成、第二包中断后不会重新规划第一包。
- operation ID 集合、顺序、数量和 outbox 指纹绑定；group/device/sequence/epoch/previous hash/header payload kind 均有静态校验。
- 接收端在任何业务写前拒绝同事件重复 operation ID。
- judge shadow 能保留前项成功值，并在最终批量 patch 后重算 revision hashes、清理 dirty marker；现有连续成功/后项 judge conflict/后项 tombstone conflict 用例覆盖核心路径。
- legacy quarantine 迁移不复制原绝对路径和 details 正文；manual-review DTO 为白名单；retain/archive 均按 group + id + manual_review 状态限定并写入脱敏审计。
- 本地 export quarantine 只在对应 local device/sequence 的 finalize transaction 内解除；其他设备同序列不受影响。
- `SyncError::public_message()` 已覆盖命令、serde 和 scheduler stderr；UI 仅展示命令返回的稳定 code + 脱敏文案；审计与 quarantine 新写入仅保存稳定代码、设备、序列和文件名。
- 0063 对旧 outbox 按旧 planner 的 `(logical_time, operation_id)` 顺序一次性归一化，并建立 group-wide capture-sequence 唯一索引；历史依赖证明使用 `(exported_sequence, capture_sequence)`。
- 迁移 sentinel 已静态覆盖新增表、关键列和索引定义；`git diff --check` 通过，仅有工作树 LF/CRLF 提示。

## 六、建议的最小返工顺序

1. 先修 P0-1：实现真正原子 no-replace，并建立真实竞争测试。
2. 再修 P0-2：草稿恢复执行完整信封校验，增加 BLOB/header/hash/signature 损坏矩阵。
3. 将故障注入下沉到生产编排，重写 durable-export 三类关键测试，避免 helper 自行拼流程。
4. 将 `capture_sequence` 改为事务型组级分配器并增加双连接并发测试。
5. 明确 rename 后耐久边界，补齐平台实现/测试；随后在同一最终 diff 上重新跑迁移定向、S1 定向、contract、Windows Rust 全量、check、clippy、Node、build、source validation 与 diff check。

## 七、本轮边界

- 仅静态审计当前真实 diff；未修改产品、测试或迁移代码。
- 未访问正式数据库、正式 NAS、真实凭据、飞书或业务正文。
- 未运行会创建临时数据库/NAS 文件的测试；仅执行只读检索、源码逐段核验及 `git diff --check`。
- 未 commit、未 push、未 merge。
