# 27 V083-S1 返工 C 验收量表

## P0 拒绝

- 仍以 `exists()` 后普通 rename 作为 no-replace，或竞态可覆盖不同字节。
- prepared draft 未经 `open()` 等等价完整密码学校验即可发布/finalize。
- 篡改信封可推进 NAS、outbox、group sequence 或 manifest hash。
- 故障恢复测试未穿过生产 export 编排。

## P1 拒绝

- `capture_sequence` 仍可能被两个连接同时分配相同值，唯一索引错误被当作正常并发控制。
- 临时文件已同步但最终目录项没有平台持久化措施。
- 只测不同字节目标预先存在，没有真实并发竞态。
- 为通过测试放宽签名、加密、协议或 manifest 链校验。

## 接受

- 上述反例全部实际通过，最终独立审计无 P0/P1。
- R3/R4 迁移门禁、Windows Rust 与全部项目门禁零失败；仅计划内 ignored 项可保留。
