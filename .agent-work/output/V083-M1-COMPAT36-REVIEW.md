# V083-M1-COMPAT36-REVIEW 独立验收报告

## 结论

**拒绝验收，退回修复。**

- P0：1
- P1：1
- P2：1
- 验收门槛要求 P0=0、P1=0，当前不满足。

本轮严格只读审查实现与测试；未修改源码、测试或迁移，未运行 Cargo，也未访问正式数据库、NAS、飞书或发布资源。仅运行了只读 Git 检查、源码检索和内存 SQLite 结构探针。

## P0-01：只读预检授权未绑定到后续写连接，配合全局 `ignore_missing` 可在竞态窗口放过任意 unknown version

### 证据

1. `src-tauri/src/db/migration_safety.rs:169-195` 使用独立 immutable/read-only pool 完成预检，随后关闭 pool，仅返回布尔值 `allow_missing_legacy_migration_36`。
2. `src-tauri/src/db/mod.rs:226-249` 在预检返回后另行创建 read-write/WAL pool。二者之间没有持有同一文件句柄、文件身份绑定或覆盖整个阶段的跨进程排他锁。
3. `src-tauri/src/db/mod.rs:251-266` 只要旧文件曾通过 version 36 预检，就对新写连接上的 migrator 调用 `set_ignore_missing(true)`。
4. SQLx 0.8.6 的 `sqlx-core/src/migrate/migrator.rs:28-40` 在 `ignore_missing=true` 时直接跳过整个 applied-version 缺失检查；它不是“仅忽略 version 36”的定向例外。
5. `ensure_no_wal_sidecars` 最后一次执行位于 `migration_safety.rs:192-194`。从该检查返回到 `mod.rs:245-249` 打开写 pool 之间仍存在未覆盖窗口。

因此，另一进程可在只读预检结束后、写 pool 打开前替换主库，或向同一路径数据库加入额外 unknown version/sidecar。后续写连接继承旧文件的布尔授权，而 SQLx 会普遍忽略所有 missing versions，并继续执行当前迁移。这正是验收量表所禁止的“对任意 unknown version 使用普遍放行”；静态负例只证明了无并发、文件稳定时的路径，不能封闭该竞态。

### 精确修复建议

1. **删除 `set_ignore_missing(true)` 路径。** 构造仅补入一条 version 36 元数据的兼容 migrator：version、description、migration type 与固定 checksum 明确赋值，SQL 仅作为不会被执行的占位；保留 `ignore_missing=false`。这样 SQLx 仍会拒绝 version 36 之外的任何 unknown version。
2. **把预检授权绑定到同一物理文件。** 在预检前取得并持续持有可阻止替换/并发写入的 OS 文件锁或等效跨进程锁，直至写连接完成迁移；至少同时校验文件身份，而不是只传递布尔值。锁必须覆盖“首次 sidecar 检查—immutable 预检—最终 sidecar 检查—写 pool 打开—迁移完成”的完整区间。
3. 新增确定性竞态测试：通过测试钩子/屏障在预检返回后暂停，另一执行单元替换数据库或插入额外 unknown version/创建 sidecar，再恢复 `init_pool`；必须失败，且目标主库及 sidecar 集合物理不变。

## P1-01：version 36 的“精确定义”谓词可被表级属性和额外约束绕过

### 证据

`src-tauri/src/db/migration_safety.rs:1191-1235` 只验证对象类型和 `pragma_table_xinfo` 返回的三列元数据。它没有核验：

- `PRAGMA table_list` 的 `wr` / `strict`；
- `sqlite_master.sql` 中的额外 `CHECK`、`UNIQUE`、`COLLATE` 等表级/列级约束；
- 由额外唯一约束产生的索引结构。

本轮用 Python 标准库 SQLite 的纯内存数据库验证，下列定义相对基准表均得到完全相同的 `PRAGMA table_xinfo`：

- 追加 `WITHOUT ROWID`；
- 追加 `STRICT`；
- 追加 `CHECK (item_count >= 0)`；
- 追加 `UNIQUE (sent_at)`；
- 给主键列追加 `COLLATE NOCASE`。

其中 `table_list` 能区分 `WITHOUT ROWID` 与 `STRICT`，而其余差异需要结合规范化后的 `sqlite_master.sql` 和索引元数据识别。当前这些伪造结构会被当作 `M36.table.feishu_reminder_runs.exact_definition` 成功，从而取得兼容授权，不满足“固定 checksum + 精确定义 schema”这一联合条件。

### 精确修复建议

1. 在现有 `table_xinfo` 精确列检查之外，校验 `PRAGMA table_list`：`type='table'`、`ncol=3`、`wr=0`、`strict=0`。
2. 对规范化后的 `sqlite_master.sql` 使用窄白名单，拒绝任何额外 token/约束；规范化只应容忍已确认无语义差别的空白和 SQLite 对默认值括号的标准化。
3. 校验 `PRAGMA index_list/index_xinfo`，只允许该定义固有的主键索引及其精确列序、排序、collation，不允许额外 UNIQUE/业务索引。
4. 至少新增上述五个变体的生产 `init_pool` 负例，并沿用物理指纹先后比对，证明均在写连接前失败且文件字节不变。

## P2-01：正例对“version 36 原记录不变”的断言不完整

`src-tauri/src/db/migration_lineage_tests.rs:692-707` 把 `success` 解码为 `bool`，并且没有读取 `installed_on`。因此该断言不能证明成功标志仍是原始整数 `1`，也不能证明整条原记录保持不变。运行时代码已在 `migration_safety.rs:238-288` 用 `i64` 严格检查 `success == 1`，且存在 `success=2` 负例，所以此项定为 P2 测试证据缺口，而不是额外 P1。

精确建议：升级前保存 version 36 的完整原始行 `(version, description, installed_on, success: i64, checksum, execution_time)`，生产 `init_pool` 返回后以相同原始类型读取并逐字段相等比较；不要把 `success` 解码成 `bool`。

## 已确认符合的部分

- `migration_safety.rs:275-305` 先以原始整数严格拒绝 `success != 1`，随后仅允许 `version=36 + description='feishu reminder runs'` 进入候选路径；其他 unknown version 立即失败。
- `migration_safety.rs:307-343` 在 checksum 放行前完成历史缺口和全部适用 sentinel 检查；`migration_safety.rs:345-376` 再核验固定 version 36 checksum 与其他 embedded checksum。
- 正例由 `migration_lineage_tests.rs:674-745` 的父/子进程夹具实际调用生产 `init_pool`，升级到 0063 并检查业务 marker、`quick_check`、`foreign_key_check`。
- 错误 checksum、错误 description、`success=2`、缺表、额外列、额外 unknown version 均通过生产 `init_pool` 负例；`migration_lineage_tests.rs:394-419` 在失败前后先比较主库/sidecar 物理指纹，再进行逻辑核验。
- 实质代码差异限于 `migration_safety.rs`、`mod.rs`、`migration_lineage_tests.rs`；`git diff -- src-tauri/migrations` 为空，且不存在新增 `0036_*` / `0064_*` 迁移。
- 三个任务文件执行 `git diff --check` 通过；仅出现工作区既有 LF/CRLF 提示。

## 建议修复顺序

1. 先消除 P0：移除全局 `ignore_missing`，改为只补 version 36 的显式迁移元数据，并封闭文件身份/锁的竞态窗口。
2. 再修 P1：扩展 exact-schema 谓词并补齐五类绕过负例。
3. 最后修 P2：以原始类型逐字段证明 version 36 整行不变。
4. 修复后由独立验收重新静态复核；再由主控按串行 Cargo 规则执行定向测试、全量 Rust 测试、check、clippy 与 Windows Rust 门禁。
