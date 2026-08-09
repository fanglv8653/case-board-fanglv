# V083-M1-COMPAT36-R2 实现报告

## 结论

已按首轮独立复核的 P0/P1/P2 完成第二版定点修正。R2 不再对 SQLx 使用任何全局 `ignore_missing` 放行；兼容 migrator 只显式补入固定 version 36 元数据，并保持 `ignore_missing=false`。精确 schema 谓词已扩展到列、表属性、DDL 和索引四层；正例改为生产初始化前后比较 version 36 完整六字段原始记录。

本轮没有运行 Cargo、rustfmt、Node 或任何构建/测试命令。所有动态门禁均留给主控串行执行，在此之前不得视为最终通过。

## P0 修正：消除全局放行

1. 删除 `set_ignore_missing(true)`，源码中不存在该调用或等效全局放行。
2. `migration_safety::legacy_migration_36_metadata()` 构造唯一兼容元数据：
   - version `36`；
   - description `feishu reminder runs`；
   - `MigrationType::Simple`；
   - 固定 48 字节 SQLx SHA-384；
   - `no_tx=false`；
   - SQL 为只读不存在表查询 `SELECT * FROM __caseboard_legacy_migration_36_must_already_be_applied`，仅作永不应执行的占位。
3. `init_pool` 将该元数据按 version 顺序插入当前嵌入 migrations，兼容 migrator 明确保持 `ignore_missing=false`。因此即使只读预检后出现额外 unknown applied version，SQLx 的二次 applied-version 校验仍会拒绝，而不会像首轮一样跳过整个 missing 检查。
4. 如果预检后目标被替换为没有已应用 version 36 的数据库，SQLx 会尝试执行上述占位 SQL并因不存在专用占位表而失败，不能静默伪造一条 version 36 历史记录。
5. 在读写 pool 的 `connect_with` 之前再次执行 `ensure_no_wal_sidecars`；预检前和写连接前各有一次拒绝检查。

本轮没有引入未经验证的文件锁，也不宣称能完全阻止任意外部 SQLite 工具在最后一次检查后的极小窗口并发修改。剩余风险见下文。

## P1 修正：精确 schema 四层谓词

version 36 仍须先满足 version、description、`success == 1` 和固定 checksum；其 `feishu_reminder_runs` 还必须同时通过：

1. `pragma_table_xinfo`：恰好三列，逐项匹配 cid、名称、声明类型、NOT NULL、默认值、主键序号、hidden；只窄范围容忍 `datetime('now')` 与 `(datetime('now'))` 两种默认值括号形态。
2. `pragma_table_list`：`type='table'`、`ncol=3`、`wr=0`、`strict=0`。
3. `sqlite_master.sql`：去除空白并转小写后，只允许基准 DDL及默认值带外括号的等价 DDL，任何额外 `CHECK`、`UNIQUE`、`COLLATE` 或表选项都会失败。
4. `pragma_index_list`：只允许 `sqlite_autoindex_feishu_reminder_runs_1`，且 `seq=0`、`unique=1`、`origin='pk'`、`partial=0`。
5. `pragma_index_xinfo`：主键键列必须是 `sent_date`、升序、`BINARY` collation；只允许一个固有 rowid 辅助项，不允许额外索引或变更 collation。

新增五个独立生产 `init_pool` 负例：`WITHOUT ROWID`、`STRICT`、额外 `CHECK`、额外 `UNIQUE`、`COLLATE NOCASE`。每个负例均通过既有 helper 在调用前后比较主 DB、WAL、SHM 的物理指纹，并核对逻辑指纹。

## P2 修正：完整历史行不变

正例在调用独立子进程的生产 `init_pool` 前保存：

`(version, description, installed_on, success:i64, checksum, execution_time)`

升级到 0063 后按相同原始类型读取并整行相等比较；同时逐字段核对合成审计基线，避免把 success 解码为 bool 或漏掉 installed_on。原有业务 marker、`quick_check`、`foreign_key_check` 和 0063 到达断言保留。

## 静态证据

- `src-tauri/src/db/migration_safety.rs:210`：固定 version 36 `Migration` 元数据。
- `src-tauri/src/db/migration_safety.rs:410`：stored checksum 与固定 48 字节数组直接比较。
- `src-tauri/src/db/migration_safety.rs:1253-1385`：table_xinfo、table_list、DDL、index_list、index_xinfo 联合谓词。
- `src-tauri/src/db/mod.rs:256-260`：写连接前第二次 sidecar 拒绝。
- `src-tauri/src/db/mod.rs:269-290`：只插入 version 36 元数据，`ignore_missing=false`。
- `src-tauri/src/db/migration_lineage_tests.rs:704-745`：完整原始历史行升级前后比较。
- `src-tauri/src/db/migration_lineage_tests.rs:886-951`：五类结构绕过负例。
- 文本检索确认不存在 `set_ignore_missing`；`git diff --check` 对三份 Rust 文件通过；`git diff -- src-tauri/migrations` 为空。

迁移谱系模块静态计数为 34 个 Tokio 测试和 2 个普通测试，其中 2 个普通测试是父夹具调用的 ignored child。本轮在首版基础上新增 5 个结构绕过父测试。

## 未运行命令

按派工禁令，本轮明确未运行：

- Cargo check；
- Cargo clippy；
- 任何 Cargo test；
- Windows Rust 全量脚本；
- rustfmt；
- Node/pnpm/npm；
- 任何构建、打包或发布命令。

以上门禁状态均为“待主控串行执行”，不是“通过”。

## 剩余风险与边界

1. 第二次 sidecar 检查缩小但不能从原理上消除检查返回到 SQLite 打开文件之间的并发窗口。
2. 未增加 OS 级跨进程排他锁或持久文件身份句柄；因此不宣称阻止任意外部工具替换或原地修改数据库。
3. 该剩余风险不再造成首轮的“任意 unknown version 被全局忽略”：显式 version 36 元数据配合 `ignore_missing=false` 保留 SQLx 的其他 unknown version 二次拒绝；替换为缺少 36 的文件则由不可执行占位 SQL失败关闭。
4. 源码尚未经本轮格式化、编译和动态验证，需主控门禁与后续独立只读复核确认 SQLx/SQLite 运行时细节。

## 资源与范围声明

本轮仅修改：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- `.agent-work/output/V083-M1-COMPAT36-R2.md`
- 工作流自身线程状态文件

未新增/恢复 0036 或 0064，未修改任何迁移 SQL。未访问正式数据库、正式 WAL/SHM、凭据、NAS、飞书或发布资源。

工作流首次 start 时任务尚未登记并返回 `missing task`；主控完成任务包 dispatch 后，已使用 actor `worker-m1-compat36-r2` 成功 start。
