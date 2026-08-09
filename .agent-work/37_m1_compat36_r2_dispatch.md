# V083-M1-COMPAT36-R2 派工单

## 目标

修复首轮独立验收发现的 P0/P1/P2，形成仅兼容本机已审计历史迁移 36、且不削弱 SQLx 其他迁移校验的第二版实现。

## 允许修改

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- 本任务报告及线程状态文件

禁止新增或恢复 `0036_*`、`0064_*` 迁移；禁止访问正式数据库、凭据、NAS、发布资源；禁止运行 Cargo、rustfmt 或任何构建命令，统一由主控串行执行。

## 必须完成

1. 删除 `set_ignore_missing(true)`；兼容 migrator 显式补入固定 version 36 元数据，保持 `ignore_missing=false`，其他 unknown version 必须由 SQLx 二次拒绝。
2. version 36 候选条件继续绑定版本、描述、`success == 1`、固定 SHA-384 checksum。
3. 精确结构校验覆盖：`table_xinfo`、`table_list` 的 `wr=0/strict=0/ncol=3`、规范化后的 `sqlite_master.sql` 窄白名单、`index_list/index_xinfo` 固有主键索引且无额外索引。
4. 增加 `WITHOUT ROWID`、`STRICT`、额外 `CHECK`、额外 `UNIQUE`、`COLLATE NOCASE` 五类生产 `init_pool` 负例，失败前后主库和 sidecar 物理指纹不变。
5. 正例保存并逐字段比较 version 36 完整原始记录 `(version, description, installed_on, success:i64, checksum, execution_time)`。
6. 在打开读写池前再次执行 sidecar 拒绝；说明显式 version 36 元数据如何消除首轮“全局放行”竞态。若能在不新增依赖、不扩大范围的前提下加强文件身份绑定，可实现并测试；不得用未经验证的伪锁宣称完全阻止任意外部 SQLite 工具。
7. 写报告 `.agent-work/output/V083-M1-COMPAT36-R2.md`，列出改动、静态证据、未运行命令及剩余风险。
