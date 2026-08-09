# V083-M1-COMPAT36-R3 独立只读复核报告

## 结论

**拒绝验收，退回一项定点修复。**

- P0：0
- P1：1
- P2：1（延续已披露的残余 TOCTOU）
- R3 要求 P0=0、P1=0，当前不满足。

R2 的两个原始问题中，占位 SQL已正确改为不可由 schema 对象满足的语法错误，所有测试 DDL 也已改成 SQLite 可执行形式；但 R3 把“DDL 源文本的括号”错误地等同于 `pragma_table_xinfo.dflt_value` 的返回形式，导致合法 v36 表仍被 exact-schema 谓词拒绝。

本轮只读检查 R3 任务包、报告、R2 复核、三个 Rust 文件与 SQLx 执行路径；仅运行 Python SQLite 纯内存语法/元数据探针。未修改源码、测试或迁移，未运行 Cargo/rustfmt/构建，也未访问正式数据库、WAL/SHM、凭据、NAS、飞书或发布资源。

## P1-01：合法括号 DDL 的 PRAGMA 默认值会去掉外括号，当前 exact-schema 期望值错误

### 证据

1. 生产及测试 DDL 已正确改成：

   `sent_at TEXT NOT NULL DEFAULT (datetime('now'))`

   见 `migration_safety.rs:76-82`、`migration_lineage_tests.rs:35-41` 及五个变体 `945-1009`。
2. `migration_safety.rs:1249-1287` 读取 `pragma_table_xinfo.dflt_value` 后，要求 `sent_at.default_value == Some("(datetime('now'))")`。
3. SQLite 3.53.1 纯内存实证：基准、`WITHOUT ROWID`、`STRICT`、额外 `CHECK`、额外 `UNIQUE`、`COLLATE NOCASE` 六种合法 DDL 均成功建表，但 `pragma_table_xinfo` 对 `sent_at` 全部返回 `datetime('now')`，不带外层括号。
4. SQLite 的 `sqlite_master.sql` 仍保留 `DEFAULT (datetime('now'))`；因此 DDL 白名单只保留括号形态是正确的，错误仅在于把同一文本形式用于 PRAGMA 元数据比较。

### 实际影响

- 正例 `audited_legacy_migration_36_upgrades_through_production_init_and_preserves_history` 会在生产 preflight 的 `M36.table.feishu_reminder_runs.exact_definition` 处失败，无法升级到 0063，也无法证明完整六字段原始行不变。
- `authorized_preflight_then_missing_v36_fails_without_forging_history` 的初始 legacy-v36 文件同样无法通过 preflight；`mod.rs:293-296` 的 after-preflight hook 不会运行，测试将得到 compatibility sentinel error，而不是预期的 migration syntax error。
- 五类 schema 负例虽已能成功建表并调用生产 `init_pool`，但都会先命中共同的默认值错配，无法证明 `table_list`、DDL、索引或 collation 各自真的拦截了目标变体。
- 错误 checksum/description 等组合负例也可能因既有“schema 优先于 checksum”顺序改报 sentinel，造成多项定向测试失败。

### 精确修复建议

1. `table_xinfo` 的 `expected_columns[1].default_value` 改回 `Some("datetime('now')")`；不要恢复无效的无括号 CREATE TABLE 白名单。
2. 继续让 `sqlite_master.sql` 只匹配合法的 `DEFAULT (datetime('now'))` 规范化 DDL。DDL 源文本和 PRAGMA 表达式元数据应分别冻结。
3. 在基准 fixture 和每个变体 fixture 的“建表成功”断言中，不只查 table count，还显式读取并断言 `pragma_table_xinfo` 返回的默认值为 `datetime('now')`，防止再次混淆两种表示。
4. after-preflight 测试增加 hook-fired 原子标记或一次性通道断言，明确证明 hook 确实在生产 preflight 成功后执行，而不是仅依赖最终错误类型间接推断。

## 已确认修复有效的部分

### `SELECT FROM;` 是无条件 SQLite 语法失败

- `migration_safety.rs:83-85` 的占位语句不包含可由库内对象解析的名称。
- SQLite 3.53.1 纯内存分别在空库、预建名为 `FROM` 的表、列、视图及注册同名函数后执行 `SELECT FROM;`，全部返回 `near "FROM": syntax error`；schema 对象不能改变解析结果。
- SQLx 只在写连接所见历史缺少 synthetic v36 时进入 `apply`；若 v36 已存在且固定 checksum 匹配，按正常路径跳过该 SQL。

### after-preflight 测试结构真实经过生产 `init_pool`

- hook 仅在 `cfg(test)` 下存在，按精确数据库路径一次性取出，位置为 `mod.rs:287-296`：生产 immutable preflight 返回之后、第二次 sidecar 检查和读写池打开之前。
- 测试 `migration_lineage_tests.rs:765-803` 先构造真实 legacy-v36 授权库及当前 0063/缺 v36 的 main-only 替换库，再在同一次生产 `init_pool` 中替换目标；失败后查询 `(v36 count, max version) == (0,63)`，检查方向正确。
- 一旦修复上述 PRAGMA 默认值，SQLx 将只尝试缺失的 synthetic v36，语法错误事务不会插入 v36。当前测试需要主控 Cargo 门禁最终动态确认。

### R2 其他确认项未见回退

- 全项目 Rust 检索仍不存在 `set_ignore_missing` 或 `ignore_missing:true`；compatible migrator 只补固定 v36 且 `ignore_missing=false`。
- version、description、原始 `success == 1`、固定 checksum、其他 embedded checksum、全部适用 sentinel 的预检顺序保持。
- preflight 前及写 pool 前的两次 sidecar 拒绝保持。
- exact schema 的 `table_list`、单一合法 DDL、`index_list`、`index_xinfo` 层仍在；五类变体 DDL均使用有效的括号默认值。
- 正例仍以 `(version, description, installed_on, success:i64, checksum, execution_time)` 六字段升级前后整行比较。
- 既有错误 tuple、缺表、额外列、额外 unknown version及五类 schema 负例仍调用生产 `init_pool`，静态 preflight 失败 helper 仍比较主库/WAL/SHM 物理指纹。
- `git diff -- src-tauri/migrations` 为空，无 0036/0064；三个 Rust 文件 `git diff --check` 通过，仅有 LF/CRLF 提示。

## P2-01：预检结果仍未与写连接所见物理文件完全绑定

R3 正确保留了第二次 sidecar 检查，并使“替换后缺少 v36”必然失败；但若外部在 preflight 后替换为仍含固定 v36 checksum、而 description、非规范 success 或 schema 已改变的数据库，SQLx 仍只复核 version/checksum。该边界没有升级成首轮的全局 unknown-version 放行，按 R2/R3 派工约定继续列为 P2，不宣称完整关闭 TOCTOU。

## 后续顺序

1. 仅修正 `table_xinfo` 默认值的期望表示，并补 PRAGMA/hook-fired 断言；不要改动固定 checksum 或恢复无效 DDL。
2. 主控串行运行 compat36 定向测试，确认正例、after-preflight、错误 tuple及五类变体都命中各自预期路径。
3. 再执行全量 Rust、check、clippy、Windows 门禁并派独立复核；P1 清零前不得接受。
