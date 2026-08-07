# 17 V083-N0 主控总结

## 本轮目标

以纯合成、隔离资源冻结 v0.8.3 数据安全热修复的失败形状和验收契约，不改变生产行为，为 M1、S1、F1 建立可执行基线。

## Accepted

- `V083-N0-GATE`：独立门禁、冲突和最优开发顺序审计通过。
- `V083-N0-SYNC`：5 个设备同步专项夹具通过；冻结同事务循环闭合、跨包 SQLite 787 零部分写入、contact 先到失败、重复隔离、错误成功审计及 500/501/1000/1001 边界。
- `V083-N0-MIG`：6 个迁移谱系夹具通过；冻结未知 checksum、未知已应用版本、失败迁移、缺失 schema sentinel 和当前迁移集合。

## Rejected / 修复轮次

1. SYNC 首次定向命令实际执行 15 项，而非报告的 5 项；原因是 integration test 用 `#[path]` 重引整个模块。退回后改为专用 `#[cfg(test)]` 单元模块，移除重复入口，5 项均在全量门禁通过。
2. MIG 首次将版本号 1—62 误当作连续 62 条；全量实测为 61 条、最高 62。退回后改为数据库实际版本集合与 `sqlx::migrate!` 嵌入集合逐项相等，并冻结版本 36 为合法缺号。
3. 首次 Windows Rust 全量脚本受主控 120 秒超时截断并产生 `BrokenPipe`；该次不计结果，延长有界超时后完整重跑通过。

## 已冻结契约

- 数据库四码：`DB_MIGRATION_CHECKSUM_UNKNOWN`、`DB_MIGRATION_APPLIED_VERSION_UNKNOWN`、`DB_MIGRATION_SCHEMA_SENTINEL_MISSING`、`DB_MIGRATION_LINEAGE_INCOMPATIBLE`。
- 同步三码：`SYNC_PACKAGE_DEPENDENCY_MISSING`、`SYNC_PACKAGE_QUARANTINED`、`SYNC_GROUP_AUTO_PAUSED`。
- 飞书一码：`FEISHU_ORPHAN_BINDING`。
- 历史真实 checksum 未取得前，M1 只能 fail-closed，不得猜测或加入兼容白名单。
- M1 → S1 → F1 必须串行；正式数据库、当前 NAS 同步组和飞书业务 Base 不作为开发夹具。

## 实际验证

- `git diff --check`：通过。
- `pnpm test:logic`：119/119。
- `pnpm build`：通过（仅既有 chunk size warning）。
- `pnpm validate:source`：通过；首次因 PATH 缺 Cargo 报 `spawnSync cargo ENOENT`，补入明确路径后通过。
- `cargo check --lib -j 1`：通过。
- `cargo clippy --lib -j 1 -- -D warnings`：通过。
- `scripts/run-windows-rust-tests.ps1`：3 个可执行文件通过；库测试 274 passed / 0 failed / 3 ignored，设备同步契约 23/23，二进制入口 0 tests。

## Git 状态

- 分支：`fix/v0.8.3-data-safety`。
- N0 仅新增/调整 `#[cfg(test)]` 测试模块、测试辅助、主控工作流和交付报告；生产函数体、迁移、依赖、版本与发布配置零变化。
- N0 由主控统一提交；worker 未 commit、未 push。

## 下一轮 M1 输入

- 使用已通过的 6 个迁移夹具改写为写前 fail-closed 目标断言。
- 实现只读谱系预检、49/51/58—62 schema sentinel、结构化四码和原生启动错误提示。
- 未知/不兼容路径必须验证 `_sqlx_migrations`、schema 与业务表指纹在执行前后不变。
- 真实旧 checksum 仍为 `pending_verified_input`；取得有来源的只读元数据前不做历史兼容白名单。
