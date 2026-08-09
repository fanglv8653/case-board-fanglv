# V083-M1-COMPAT36-R3 实现报告

## 结论

已完成 R2 独立复核指出的两个 P1 窄修复：synthetic version 36 占位 SQL 改为不依赖任何数据库对象的固定 SQLite 非法语法；全部兼容正/负夹具 DDL 改为当前 SQLite 可执行的括号默认值，并增加夹具建表成功断言。R2 已确认的 `ignore_missing=false`、第二次 sidecar 检查、固定 tuple/checksum、完整六字段比较和多层 exact-schema 校验均保留。

本轮未运行 Cargo、rustfmt、Node 或任何构建/测试命令；最终结论须以主控串行门禁及独立复核为准。

## P1-01：占位 SQL 无条件失败

synthetic version 36 的 SQL 已从可被同名表满足的查询改为：

```sql
SELECT FROM;
```

该语句在 SQLite 语法层缺少 SELECT 结果表达式且错误使用 FROM，解析不依赖表、视图、函数或其他 schema 对象；预建任意数据库对象不能使其成为合法 SQL。固定 version、description、migration type、checksum 和 `no_tx` 未改变。

新增测试专用、按目标数据库路径匹配的一次性 after-preflight hook：

1. 先由真实 legacy-v36 合成库通过生产 `init_pool` 内的 immutable preflight；
2. preflight 返回后，hook 把目标替换为合法当前谱系数据库，该数据库已应用到 0063但没有 version 36；
3. 同一次生产 `init_pool` 继续执行第二次 sidecar 检查、打开写池并运行 compatible migrator；
4. `ignore_missing=false` 下 SQLx 只会尝试缺少的 synthetic version 36，固定非法 SQL必须产生 migration syntax error；
5. 失败后读取 `_sqlx_migrations`，断言 `(version36_count, max_version) == (0, 63)`，证明未插入伪造 v36。

该测试不创建、查询或依赖旧占位表名称；生产非测试构建不包含 hook。

## P1-02：兼容夹具 DDL 可执行

以下位置已统一使用 SQLite 可执行语法：

`sent_at TEXT NOT NULL DEFAULT (datetime('now'))`

- 生产 exact DDL 白名单只保留这一真实可执行形态；删除无外括号的无效白名单。
- `pragma_table_xinfo` 的默认值期望同步固定为 `(datetime('now'))`。
- 正例共用 `LEGACY_MIGRATION_36_TABLE_SQL` 已修正。
- `WITHOUT ROWID`、`STRICT`、额外 `CHECK`、额外 `UNIQUE`、`COLLATE NOCASE` 五个变体全部修正。
- 基准建表和每次变体替换后，都明确查询 `sqlite_master` 并断言 `feishu_reminder_runs` 表计数为 1；只有断言成功后才调用生产 `init_pool`，避免 setup panic 被误认作兼容拒绝。

## 保留的 R2 已确认项

- 全局不存在 `set_ignore_missing(true)` 或 `ignore_missing: true`。
- compatible migrator 只按序补入固定 version 36 元数据，并保持 `ignore_missing=false`；其他 unknown version 继续由 SQLx 二次拒绝。
- preflight 前和打开读写 pool 前均执行 sidecar 拒绝。
- version 36 候选仍严格绑定 version、description、`success == 1`、固定 SHA-384。
- exact schema 仍联合校验 `table_xinfo`、`table_list`、单一规范化 DDL、`index_list`、`index_xinfo`。
- 正例仍通过生产子进程升级到 0063，并比较 `(version, description, installed_on, success:i64, checksum, execution_time)` 完整原始行。
- 错误 checksum/description/success、缺表、额外列、额外 unknown version及五类 schema 变体仍使用生产 `init_pool` 失败路径；既有 preflight 负例保持主库和 sidecar 物理指纹不变证明。

## 静态证据

- `src-tauri/src/db/migration_safety.rs:76-85`：唯一可执行 exact DDL及固定非法占位 SQL。
- `src-tauri/src/db/migration_safety.rs:1253-1373`：只接受括号默认值及单一规范化 DDL，表/索引联合谓词保留。
- `src-tauri/src/db/mod.rs:23-72`：仅测试构建存在的按路径一次性 after-preflight hook。
- `src-tauri/src/db/mod.rs:286-294`：hook 位于 immutable preflight 之后、第二次 sidecar 检查与写池打开之前。
- `src-tauri/src/db/migration_lineage_tests.rs:465-501`：基准 DDL 执行并显式断言表已创建。
- `src-tauri/src/db/migration_lineage_tests.rs:520-543`：变体 DDL 执行并显式断言表已创建。
- `src-tauri/src/db/migration_lineage_tests.rs:765-803`：preflight 后替换、生产初始化语法失败、v36 未伪造。
- `src-tauri/src/db/migration_lineage_tests.rs:945-1009`：五类合法 DDL 变体。
- 文本检索确认任务源码中不存在无括号 `DEFAULT datetime('now')`，不存在旧占位对象名；`git diff --check` 通过；迁移目录无差异。

迁移谱系模块静态计数为 35 个 Tokio 测试和 2 个普通测试，其中 2 个普通测试为父夹具调用的 ignored child。本轮新增 1 个确定性 after-preflight 生产路径父测试。

## 未运行命令

按 R3 派工禁令，本轮明确未运行：

- Cargo check、clippy、test；
- Windows Rust 全量脚本；
- rustfmt；
- Node、pnpm、npm；
- 任何构建、打包或发布命令。

上述门禁全部为“待主控串行执行”，不得表述为已通过。

## P2 残余竞态边界

第二次 sidecar 检查及 synthetic v36 无条件失败只能收窄和约束 TOCTOU 窗口，不能把 immutable preflight 结果绑定到后续写连接的同一物理文件。若外部进程在 preflight 后替换为仍含固定 v36 checksum、但 description、success 原始值或 schema 已改变的数据库，SQLx 自身不会复核全部预检谓词。

本轮未引入 OS 级文件锁、稳定文件句柄或文件身份摘要，也不宣称完全关闭任意外部 SQLite 工具并发修改。若发布门槛要求消除该 P2，应另立任务设计跨进程锁/文件身份复核并提供确定性屏障测试，不能以第二次 `try_exists` 代替锁。

## 范围与资源声明

本轮仅修改：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- `.agent-work/output/V083-M1-COMPAT36-R3.md`
- 工作流线程状态文件

未修改迁移目录，未新增/恢复 0036 或 0064。未访问正式数据库、正式 WAL/SHM、凭据、NAS、飞书或发布资源。
