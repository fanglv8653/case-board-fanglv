# 26 V083-S1 返工 C 派发计划

## 目标

关闭最终独立审计发现的 2 项 P0、3 项 P1，不扩大设备同步业务功能。

## 必须实现

1. NAS 最终发布必须是原子 `no-replace`。`exists()` 只能用于快速同字节判断，不能作为并发正确性依据。Windows 使用不带 replace 标志且带 write-through 的原子移动；非 Windows 使用不会覆盖目标的原子链接/发布及目录 fsync。竞争失败后只允许读取赢家并逐字节比较：相同视为幂等，不同返回脱敏 Integrity，绝不覆盖。
2. 临时文件在发布前 `sync_all`；发布成功后必须有平台对应的目录项持久化保证。失败路径清理自己的临时文件，不删除目标或其他写入者的文件。
3. 从 SQLite 加载 prepared draft 后，在任何 NAS 写入或 DB finalize 前，必须：重新计算真实密文字节 hash；校验协议版本、头字段和 payload kind；用本机可信签名公钥对 event/manifest 验签并用组密钥解密；反序列化 operation 数组和 manifest；验证 operation 顺序/指纹、事件 hash、前序 hash、设备/组/序列/key epoch 全链一致。任一字节、签名、nonce、头或协议篡改均 fail closed。
4. 故障注入必须穿过生产 `export_pending` 编排。测试钩子只能在同一生产调用链的明确阶段返回错误，不得由测试手工串联 prepare/publish/finalize 代替生产恢复证明。
5. `capture_sequence` 分配前在同一 SQLite 事务内获取组级写锁，再读取 `MAX+1` 并插入；baseline 与普通 capture 共用同一分配器。补双连接并发捕获/基线竞争，证明没有重复序号、随机 UUID 不参与顺序、失败可安全重试。
6. 不修改 0063 结构或 M1 sentinel；如实现确需新增 schema，先停止并回报主控。

## 必测反例

- 两线程屏障后对同一目标发布不同字节，重复多轮：最终只可能是某一完整候选，永不出现覆盖后的第二内容或部分内容；一个赢家、一个稳定 Integrity。
- 两线程发布相同字节均成功且最终字节一致。
- 分别篡改 event/manifest 的 ciphertext、ciphertext hash、signature、nonce、protocol version、payload kind、header group/device/sequence/epoch；生产恢复在 NAS/DB 零推进下拒绝。
- 生产 `export_pending` 依次覆盖 manifest 后失败、event 后/finalize 前失败、CAS 后回滚、第二包失败并重试，断言复用原字节及完整状态。
- 两个 SQLite 连接并发请求 capture sequence；序号唯一且严格单调，业务顺序不由 UUID 决定。
- Windows 发布调用明确使用 no-replace + write-through；非 Windows 路径有 no-replace 与目录 fsync。

## 门禁

- S1 定向、设备同步契约、NAS 单元与并发测试、Windows Rust 全量、check、Clippy 全目标、Node logic/build/source、`git diff --check`。
- 只使用内存/临时数据库和临时目录；不接正式数据库、NAS、飞书或凭据。
- 只逐文件 rustfmt，不 commit、不 push。
