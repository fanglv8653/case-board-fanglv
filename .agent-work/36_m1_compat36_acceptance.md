# 36 V083-M1-COMPAT36 验收量表

## P0

- 修改正式 DB/迁移历史、删除 sidecar、访问凭据/NAS/飞书或发布。
- 对任意 unknown version/checksum 使用普遍放行，或仅凭版本号/表存在放行。
- 正例未走生产 `init_pool` 实际升级至 0063。

## P1

- 未同时绑定 version、description、success、固定 checksum 与精确定义 schema。
- `ignore_missing` 在完整只读预检前生效，或负例能进入写连接。
- 未证明错误 checksum/description/缺表/错 schema/额外未知版本均写前失败且文件字节不变。
- 正常当前谱系、全新库、既有 checksum mismatch/sentinel 优先级回归。

## 接受

- 独立复核 P0=0、P1=0。
- Cargo check、Clippy 和 Windows Rust 全量通过；无正式资源访问。
