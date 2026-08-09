# V083-M1-COMPAT36-R2 独立只读复核报告

## 结论

**拒绝验收，退回定点修复。**

- P0：0
- P1：2
- P2：1
- R2 要求 P0=0、P1=0，当前不满足。

首轮的全局 `ignore_missing` 问题已经消除；但当前“永不执行”的占位 SQL 可以被构造为成功，且新增正例/负例共用的建表 SQL在 SQLite 中语法无效。二者均会阻止本轮按量表通过。

本轮只读取源码、任务包、实现报告与本机 Cargo registry 中的 SQLx 0.8.6 源码；仅运行只读 Git 检查和 Python SQLite 纯内存探针。未修改源码、测试或迁移，未运行 Cargo/rustfmt/Node，也未访问正式数据库、WAL/SHM、凭据、NAS、飞书或发布资源。

## P1-01：version 36 占位 SQL并非无条件失败，可被替换库预置同名表后成功执行并写入伪造历史

### 证据链

1. `src-tauri/src/db/migration_safety.rs:208-216` 创建 synthetic version 36 元数据，固定 checksum 的做法正确，但其 SQL 为：

   `SELECT * FROM __caseboard_legacy_migration_36_must_already_be_applied;`

2. `src-tauri/src/db/mod.rs:259-279` 在旧文件通过预检后，把该元数据插入 migrator，并在另行打开的读写池上运行。
3. SQLx 0.8.6 `migrator.rs:168-181` 对写连接上缺少的 migration 调用 `apply`；`sqlx-sqlite/src/migrate.rs:131-162` 在 SQL执行成功后会向 `_sqlx_migrations` 插入该 migration 的 version、description 和手工提供的 checksum。
4. SQLite 3.53.1 纯内存探针证实：占位表不存在时该查询失败；预先创建同名表后查询成功并返回空结果。该名称不是不可伪造条件。

可构造如下确定性路径：安全 legacy-v36 文件先通过 immutable 预检；在预检与读写池打开之间，将路径替换成“当前 1..63（合法缺 36）的数据库 + 同名占位表”。SQLx 的 `ignore_missing=false` 会接受所有已应用版本均能在 compatible migrator 中找到，随后因 v36 缺失而执行占位查询；查询成功后 SQLx 自动写入一条 v36 历史记录，最终初始化可成功。这直接推翻实现报告“缺少 v36 必然由占位 SQL失败关闭”的结论。

### 精确修复建议

1. 将占位 SQL改为**不依赖数据库对象且在 SQLite 语义下必定报错**的语句。例如先用本项目 SQLite 版本验证 `SELECT RAISE(ABORT, 'legacy migration 36 must already be applied')` 的稳定失败行为，或使用明确不可解析的 SQL；不要再以“假设某表不存在”作为失败条件。
2. 增加直接覆盖 synthetic migrator 的测试：目标库应用了正常当前谱系但没有 v36，并预建现有占位表名称；运行 compatible migrator 必须返回错误，且不得插入 v36。若保留生产竞态钩子，再增加预检后替换的端到端用例。
3. 对缺少 v36 的更旧/空库也验证物理影响：SQLx 按版本顺序执行，若早期迁移缺失，它可能先写入 1..35 再到 v36 才失败。若“fail closed”包含失败前不写，必须在进入 migrator 前于写连接上确认 v36 仍存在，或采用能绑定预检文件身份的锁/句柄方案。

## P1-02：正例及五类 schema 负例使用 SQLite 无效 DDL，测试在进入生产 `init_pool` 前即会失败

### 证据

- 共用夹具常量位于 `src-tauri/src/db/migration_lineage_tests.rs:35-41`，使用 `sent_at TEXT NOT NULL DEFAULT datetime('now')`。
- `legacy_migration_36_fixture` 在 `migration_lineage_tests.rs:482-485` 通过 `sqlx::raw_sql` 原样执行该字符串。
- 五个新增变体位于 `migration_lineage_tests.rs:886-951`，也全部使用未加外括号的 `DEFAULT datetime('now')`。
- SQLite 3.53.1 纯内存执行同一 DDL，确定返回 `near "(": syntax error`。SQLx 0.8.6 `raw_sql.rs:117-123` 仅包装并原样返回 SQL，不会改写默认值语法。
- 项目现有迁移也统一采用合法形式 `DEFAULT (datetime('now'))`；生产谓词已经在 `migration_safety.rs:83-89、1293-1332` 接受该带括号形态。

因此，正例无法构造 audited v36 fixture，五个新增负例也无法到达 schema 预检，更不能证明生产 `init_pool` 拒绝后物理字节不变。R2 的正例升级和五类绕过门槛目前均没有可执行证据。

### 精确修复建议

把测试基准 DDL及五个变体全部改成 `DEFAULT (datetime('now'))`。不要改变固定历史 checksum；该 checksum 是审计元数据，不应由测试 DDL重算。修复后由主控串行运行定向 compatibility tests，确认每个负例确实进入 `init_pool` 并命中 `M36.table.feishu_reminder_runs.exact_definition`，而不是在夹具构造阶段 panic。

## P2-01：第二次 sidecar 检查缩小了窗口，但授权仍未绑定到写连接所见状态

`src-tauri/src/db/mod.rs:245-256` 在打开读写池前新增 sidecar 检查，属于有效加强；但检查返回后到 SQLite 打开文件仍有窗口，且主文件身份没有绑定。即使将占位 SQL改为无条件失败，另一进程仍可在预检后修改一个仍含 v36 且 checksum 不变的文件：

- SQLx `list_applied_migrations` 只读取 version/checksum，不复核 description；
- `dirty_version` 只查询 `success = false`，不能替代预检中的原始整数 `success == 1` 条件；
- SQLx 不复核 v36 的精确 schema sentinel。

因此，预检后改变 description、把 success 改成非 0/1 真值，或改变表结构但保留 v36 checksum，都可能绕过之前的精确元组/schema 判断。本轮已经保留 `ignore_missing=false`，额外 unknown version 会由 SQLx 二次拒绝，所以该剩余风险不再是首轮 P0；考虑到派工明确允许不引入未经验证的 OS 锁，本报告将其列为 P2 残余边界，但不得宣称完整 TOCTOU 已关闭。

建议下一轮至少把完整预检结果与主文件身份摘要带到写前并复核；若要求对抗任意外部 SQLite 工具，则需要经验证的跨进程锁/稳定文件句柄覆盖“预检—打开—迁移”，并补确定性屏障测试。

## 已确认通过的静态项

- 全项目 Rust 源码检索不存在 `set_ignore_missing` 或 `ignore_missing: true`；compatible migrator 在 `mod.rs:270-275` 明确保留 `ignore_missing=false`。
- synthetic 元数据只增加 version 36，version、description、checksum、migration type、`no_tx` 均按冻结值构造；插入位置保持版本有序。
- 对稳定不变且已应用 v36 的数据库，SQLx 会按 version/checksum 命中并跳过占位 SQL；任意额外 unknown applied version 仍由 SQLx `validate_applied_migrations` 拒绝。
- `migration_safety.rs:1251-1383` 已联合校验 `table_xinfo`、`table_list`、规范化 DDL、`index_list`、`index_xinfo`。使用合法带括号 DDL的纯内存探针确认五种变体分别会在表属性、DDL、索引数量或 collation 层与基准不同，未再发现首轮结构绕过。
- `migration_lineage_tests.rs:704-745` 已改为升级前后比较六字段原始 v36 行，`success` 使用 `i64` 且包含 `installed_on`；断言设计正确，但当前受 P1-02 夹具语法阻断。
- `git diff -- src-tauri/migrations` 为空，未新增 `0036_*` / `0064_*`；三份任务 Rust 文件 `git diff --check` 通过，仅有既有 LF/CRLF 提示。

## 修复顺序

1. 先把 synthetic v36 SQL改成数据库内容无法使其成功的无条件失败语句，并补“预置旧占位表名称仍失败、不得插入 v36”的测试。
2. 把所有 compatibility fixture 的默认值改成合法的 `DEFAULT (datetime('now'))`，让正例和五个 schema 负例真正可运行。
3. 主控串行执行定向测试；确认夹具不是 setup panic 后，再执行全量 Rust、check、clippy 与 Windows 门禁。
4. 若版本目标要求关闭并发替换而不只是消除全局放行，再单独设计并验证跨进程锁/文件身份方案；不要把第二次 `try_exists` 检查描述为完整锁。
