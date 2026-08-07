# V083-S1-REVIEW-R4：S1 返工 C 最终独立复验

状态：`submitted_for_review`

## 一、结论

**建议主控验收通过。R3 的 2 项 P0、3 项 P1 已关闭；本轮未发现新的 P0/P1。**

本轮只读复验了 R3 报告、返工 C 派发/验收量表、最终 `V083-S1.md`、真实源码与测试 diff。Windows/非 Windows 原子 no-replace、草稿完整密码学验证、生产 export 编排故障注入、capture group 写锁及双连接竞争均与量表要求一致。公开错误、manual review、judge shadow、0063 迁移哨兵未见回归。

保留 2 项非阻断 P2：finalize 可进一步改为只使用 transaction 内重新加载对象的全部标量；capture 并发测试目前直接覆盖共享 allocator 的 enqueue 路径，尚未单独制造 `ensure_initial_baseline -> capture_dirty_entities` 双连接竞争。但现有生产代码两条实际 `MAX+1` 路径都在读取 revision/MAX 之前取得同一组写锁，故不构成 P1。

## 二、R3 P0 关闭复验

### P0-1 已关闭：NAS 发布已是原子 no-replace

证据：`src-tauri/src/device_sync/nas_folder.rs:356-455`。

- `exists()` 只保留为快速幂等判断；实际竞争正确性由 `publish_no_replace` 保证。
- 临时文件使用 `create_new(true)`，写完先 `file.sync_all()`。
- Windows 分支 `MoveFileExW(temp, target, MOVEFILE_WRITE_THROUGH)` 只传 `MOVEFILE_WRITE_THROUGH`，没有导入或组合 `MOVEFILE_REPLACE_EXISTING`。目标已存在时调用失败，不会覆盖赢家。
- Windows 失败后只读取目标并逐字节比较：相同则幂等成功，不同返回 `SyncError::Integrity`。外层仅删除带随机 UUID 的本次临时文件；成功移动后临时路径已不存在，不会删除目标，失败竞争也不删除赢家。
- 非 Windows 分支以同目录 `hard_link(temp, target)` 原子发布；目标已存在时 hard-link 创建失败且不会替换。成功后删除本次临时链接并对父目录执行 `sync_all()`。
- Windows `MOVEFILE_WRITE_THROUGH` 和非 Windows 父目录 fsync 分别覆盖当前平台的最终目录项持久化措施。

真实竞争测试 `nas_folder.rs:495-543` 在 barrier 后让两个线程同时进入生产 `publish_no_replace`：20 轮不同字节均严格一成功、一 `Integrity`，目标始终是一个完整候选；相同字节双方均幂等成功。barrier 仅插在生产函数发布前，发布实现没有测试替身。

### P0-2 已关闭：prepared draft 发布前执行完整密码学与业务链验证

证据：`src-tauri/src/device_sync/engine.rs:722-809, 829-897, 1089-1154`。

已有 draft 的生产恢复顺序为：

1. 在 SQLite write transaction 内重新加载 group、pending outbox 和唯一 prepared draft；
2. 先做 group/device/sequence/key epoch/previous manifest hash、operation IDs/顺序/指纹、协议版本和 envelope header 静态绑定；
3. 从 `device_sync_members` 按当前 group + local device + `status='trusted'` 读取本机签名公钥，缺失即稳定 `Integrity`；
4. 用当前 group key 和可信签名公钥分别调用 `open(event)`、`open(manifest)`；`open` 覆盖协议、Base64、nonce、真实 ciphertext SHA-256、Ed25519 签名和 AES-GCM 认证解密；
5. event 明文重新反序列化为 `Vec<SyncOperation>`，重新核对 operation ID 顺序与完整指纹；manifest 明文重新核对 group/device/sequence/event ciphertext hash/previous manifest hash；
6. 上述全部通过并提交读取 transaction 后，生产 `export_pending_inner` 才进入 manifest-first/event-last NAS 发布，随后 finalize。

新 draft 来自同一 transaction 内的 `seal` 结果；已有 draft 必经上述 `Some((group_key, signing_public_key))` 分支。finalize 虽以 `crypto_validation=None` 再读 draft，但会将两份 envelope bytes、两份 hash、operation IDs 和 fingerprint 与发布前已经完整验真的内存 `PreparedExport` 精确比较，再执行 CAS；因此发布后发生的 draft 变更不能推进数据库。

篡改矩阵 `v083_failure_tests.rs:1445-1545` 分别修改 event/manifest 的 ciphertext、ciphertext hash、signature、nonce、protocol、payload kind、group、device、sequence、epoch，共 20 个子场景；均调用生产 `prepare_or_load_export` 恢复实现并在 group sequence/outbox 零推进时失败。未放宽签名、加密、协议或 manifest 链验证。

## 三、R3 P1 关闭复验

### P1-1 已关闭：故障测试穿过生产 export 编排

证据：`engine.rs:20-55, 652-688, 1081-1190, 1201-1303`；`v083_failure_tests.rs:1547-1720`。

- 生产 `export_pending` 与测试入口共同调用唯一的 `export_pending_inner`。
- 测试只注入临时密钥材料及 `AfterManifest/AfterEvent/AfterCas(sequence)` 枚举；prepare、草稿恢复、manifest/event 发布顺序、finalize CAS、outbox 更新和多包循环均为生产实现。
- 故障判断点直接位于生产 `publish_prepared_export` 和 `finalize_prepared_export_inner`，不再由测试手工拼接流程。
- 三个单包故障阶段均通过再次调用同一编排恢复；501 项用例在 sequence 2 的 manifest 后中断，断言第一包 500 项已完成、第二包 1 项保持 pending，重试只恢复第二包并推进到 sequence 3。

旧 `prepare/publish/finalize *_for_test` helper 仍用于其他局部单元测试，但返工 C 的生产恢复证明已经不依赖它们。

### P1-2 已关闭：capture sequence 在 `MAX+1` 前取得组写锁

证据：`capture.rs:9-22, 71-100, 185-231`；`operations.rs:175-186, 240-279`。

- 共享 allocator 边界为 `lock_capture_sequence_group`，通过同 transaction 内 `UPDATE device_sync_groups SET updated_at=updated_at WHERE id=?` 取得 SQLite 写锁并验证组存在。
- `capture_dirty_entities` 在读取 entity revision、当前实体、`MAX(capture_sequence)+1` 和插入 outbox 之前先取得该锁。
- `enqueue_operation` 同样在 sanitize、revision 与 MAX 查询之前调用相同锁。
- `ensure_initial_baseline` 本身只建立 dirty marker，不直接分配 outbox sequence；后续 baseline 实体仍全部进入 `capture_dirty_entities` 的同一 allocator，不存在独立的 UUID/时间排序入口。

双连接测试 `v083_failure_tests.rs:3008-3046` 使用文件 SQLite、pool `max_connections(2)` 和 10 秒 busy timeout，同时开启两个 transaction 并调用真实 `enqueue_operation`；两次均提交，最终序列严格为 `[1, 2]`。唯一索引不是正常并发控制手段，写锁在 MAX 前已经串行化分配。

### P1-3 已关闭：目录项持久化已有平台措施

- Windows 使用 `MOVEFILE_WRITE_THROUGH` 完成最终原子移动。
- 非 Windows 在 hard-link 发布及临时链接清理后打开父目录并 `sync_all()`。
- 临时文件在发布前已 `sync_all()`；失败清理仅针对本次 UUID 临时文件。

## 四、回归复查

- 公开错误：`SyncError::public_message()`、serde、Tauri `command_error` 和 scheduler stderr 仍只输出稳定 code + 脱敏文案；UI 的 `errorText` 接收的是已脱敏命令错误。
- quarantine/manual review：legacy 仍以 `__legacy__/-1` 进入 `manual_review`，路径清空、details 固定白名单；列表 DTO 不返回 source/details；retain/archive 仍按 group + id + status 限定并写安全审计。
- judge：事件内重复 operation ID 仍在业务写前拒绝；连续 judge shadow、后项 judge conflict、后项 tombstone conflict 逻辑和用例仍在，最终实体值与 revision hashes 同步。
- migration：返工 C 未修改 0063、migration sentinel 或 Cargo 依赖。当前 0063 SHA-256 为 `8ED128EEFE866FB1FDF50ECEF57298AF42C5A6AD746B87E365998370325925DD`，与最终报告一致。MIG-R3 报告记录迁移定向 21/0、Windows Rust 374/0/3 ignored，最终返工 C 报告记录当前全量 325/0/3 ignored 与 contract 59/0。
- `src-tauri/Cargo.toml` 无当前差异，既有 `windows 0.61.3` 已启用 `Win32_Storage_FileSystem`，Windows API 调用具备编译依赖。
- 本轮执行 `git diff --check`，exit 0；输出仅为 Windows LF/CRLF 提示。

## 五、非阻断 P2

### P2-1：finalize 可进一步收紧为完整标量比较

`engine.rs:1231-1239` 仍只显式比较 envelope bytes/hash、operation IDs 和 fingerprint，没有逐项比较 `PreparedExport` 的 key epoch、previous manifest hash 等标量。当前 production prepared 必由同一 `prepare_or_load_export` 产生，draft 查询又以 prepared group/device/sequence 定位，group CAS 继续约束 epoch/sequence/previous hash，未形成可击穿生产反例。建议未来改为完整结构比较，或只使用 transaction 内的 `current` 执行 CAS，降低未来重构风险。

### P2-2：baseline 双连接场景可增加显式测试

当前双连接用例直接覆盖共享 allocator 的 `enqueue_operation`，而不是并发运行 `ensure_initial_baseline/capture_dirty_entities`。源码已确认 baseline 不自行分配 sequence，实际 outbox 生成仍进入带同一组写锁的 `capture_dirty_entities`，所以不影响本轮验收；可补一个 baseline marker 与普通 dirty capture 同时竞争的回归用例，使量表证据更直观。

## 六、验证边界

- 本轮仅做静态复验，没有重新执行会创建临时数据库/目录的测试；测试计数引用最终 S1 与 MIG-R3 本地报告，并核对了对应测试源码与生产调用链。
- 未修改产品、测试、迁移或配置；仅写本报告及后续工作流提交记录。
- 未访问正式数据库、正式 NAS、真实凭据、飞书或业务正文。
- 未 commit、未 push、未 merge；最终 accepted/rejected 仍由主控写入。
