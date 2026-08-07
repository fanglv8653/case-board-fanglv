# V083-M1-REVIEW2｜第三轮阻断级独立复核

日期：2026-08-07
结论：**未发现剩余 P0/P1；建议主控在第三轮自动化门禁实跑全绿后将 M1 accepted。**
边界：本轮只读审计最新版四文件与 M1 报告；未修改源码/测试，未运行 Cargo、Node、应用或构建，未读取正式数据库或调用外部系统。

## 一、五项硬门禁结论

### 1. WAL/SHM 在首次 SQLite 连接前失败关闭：通过

关键证据：

- `src-tauri/src/db/mod.rs:218-233`：所有非内存路径先调用 `ensure_no_wal_sidecars()`，随后才判断主库是否存在并决定是否调用预检；因此主库不存在但遗留孤立 sidecar 的形状也会阻断。
- `src-tauri/src/db/migration_safety.rs:69-80`：`preflight_existing_database()` 在构造 `SqliteConnectOptions` 和 `connect_with()` 前再次检查 sidecar；只有 WAL/SHM 均不存在才使用 `create_if_missing(false) + read_only(true) + immutable(true)`。
- `src-tauri/src/db/migration_safety.rs:87-93`：预检 pool 关闭后再次检查 sidecar；如果预检期间出现 sidecar，`wal_sidecar_present_requires_recovery` 优先于先前分类结果。
- `src-tauri/src/db/migration_safety.rs:96-118`：sidecar 检查完全使用文件 API；`try_exists()` 失败按“存在或不可确认”处理，属于 fail-closed；错误创建过程不打开 SQLite。
- `src-tauri/src/db/mod.rs:235-260`：只有上述门禁和 immutable 预检全部通过后才建立 RW/WAL pool 并执行 sqlx migrate。

错误路径物理证据的测试结构已修正：

- `src-tauri/src/db/migration_lineage_tests.rs:175-223` 先在独立 live DB 上构造 WAL-only 已提交内容，再复制为 frozen 目标；SQLite 后续不再打开 frozen 目标。
- `:255-279` 对 frozen 目标的第一项操作就是文件 API 物理采样；调用 `init_pool()` 后也先再次采样，再解析结构化错误。
- `:393-405` 覆盖 DB+WAL+SHM、DB+WAL 缺 SHM、DB+SHM 缺 WAL 三种形状，且断言不创建缺失 sidecar、不改变已有文件字节。

与上一轮 P0 相比，普通 `read_only(true)` 可能创建 SHM 的反例已被消除：生产代码不再对带任一 sidecar 的主库建立 SQLite 连接；无 sidecar 才使用 immutable 只读模式。

### 2. `_sqlx_migrations` 存在但 0 行且有用户表：通过

关键证据：

- `src-tauri/src/db/migration_safety.rs:136-165`：迁移历史查询后显式判断 `history.is_empty()`；若存在除 `_sqlx_migrations`/SQLite 内部对象之外的用户 table/view/trigger/index，返回 `DB_MIGRATION_LINEAGE_INCOMPATIBLE`，reason=`migration_history_empty_for_existing_schema`；只有无其他用户对象时才按真正空 schema 放行。
- `:585-600`：用户对象查询明确排除 `sqlite_*` 和 `_sqlx_migrations` 本身，不会因迁移表自己的主键内部索引误报。
- `src-tauri/src/db/migration_lineage_tests.rs:369-390`：当前完整 schema + 合成业务行保留，只清空迁移历史；断言稳定 reason，并比较预检前后的主库/WAL/SHM 完整字节、迁移历史、schema 和业务标记。

上一轮“空迁移表绕过并进入 RW/WAL+migrate”的 P0 已关闭。

### 3. checksum 覆写、allowlist 和 CAS 已彻底移除：通过

静态全仓检索结果：

- 生产源码中无 `reconcile_migration_checksums`；
- 无 `set_ignore_missing(true)`；
- 无 `MigrationCompatibilityRule`、`MIGRATION_COMPATIBILITY_ALLOWLIST`、`ApprovedChecksumUpdate`、`MigrationPreflightPlan` 或 `apply_approved_compatibility_actions`；
- 生产源码中无 `UPDATE _sqlx_migrations SET checksum`。仅两个 `#[cfg(test)]` 夹具为构造未知 checksum 形状执行该 SQL。

实际生产路径位于 `src-tauri/src/db/migration_safety.rs:236-253`：任一已应用已知版本 checksum 与当前嵌入值不等，直接返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`；返回类型为 `Result<(), DbError>`，没有任何兼容更新计划可带入 RW pool。`src-tauri/src/db/mod.rs:255-260` 使用 sqlx 默认 `ignore_missing=false`。

因此上一轮“未来空/未知 sentinel 可绕过 allowlist”风险已经通过删除整个不可达写框架消除。真实旧 checksum 取得后必须另开设计和审计，不能在当前路径内临时加值。

需要明确区分“安全门禁通过”与“冻结计划全部功能完成”：当前实现是**所有 checksum mismatch 一律失败关闭**，没有任何已知旧 checksum 兼容动作。因此，冻结计划中“版本号 + 已知旧 checksum + 当前 checksum + sentinel 的窄白名单兼容”仍未实现。由于笔记本真实旧 checksum 至今没有来源核验值，N0-GATE 已允许 M1 先交付 fail-closed 分类而不得猜测白名单；所以这不是当前安全实现的 P0/P1，但属于明确未完成的产品能力。主控必须二选一并留痕：

1. 将本次 M1 验收定义为“未知/旧 checksum 安全阻断并给出恢复提示”，接受 fail-closed，不承诺笔记本旧库自动兼容；或
2. 在取得真实只读迁移元数据后另建 `M1-COMPAT`，重新设计 version/旧值/当前值/完整 sentinel 绑定和事务内复验，再完成冻结计划的兼容项。

在上述决策完成前，不得在阶段总结或发布说明中写“已兼容笔记本历史 checksum”。

### 4. sentinel 缺失与 checksum mismatch 组合优先级：通过

关键证据：

- `src-tauri/src/db/migration_safety.rs:173-213` 先处理失败行、未知应用版本和 history gap；
- `:214-234` 随后收集并立即返回 sentinel 缺失；
- `:236-253` 最后才处理 checksum mismatch。

组合顺序已经由显式代码结构冻结为：

`失败行/未知版本/history gap → sentinel 缺失 → checksum unknown`。

`src-tauri/src/db/migration_lineage_tests.rs:496-527` 同时删除迁移 49 sentinel 并写入未知 checksum，明确断言返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING` 和 `M49.table.feishu_sync_inbox`，且所有指纹不变。上一轮由循环顺序偶然决定错误码的问题已关闭。

### 5. 成功路径和后续正常升级能力：静态通过，待主控实跑确认

关键证据：

- 新库/内存库不走既有文件预检，仍由 RW pool 正常执行全部 embedded migrations：`src-tauri/src/db/mod.rs:218-260`。
- 既有无 sidecar 当前库通过 immutable 预检后进入原有 RW/WAL 和默认 sqlx migrate；没有 checksum 更新框架或 `ignore_missing` 改变正常升级语义。
- `src-tauri/src/db/migration_safety.rs:198-213` 的 history gap 只检查“小于等于已应用最大版本”的 embedded migration；未来新增的更高迁移不会被误判为 gap，会留给 sqlx 正常执行。
- `src-tauri/src/db/migration_lineage_tests.rs:282-330` 覆盖新库迁移到 61 条/max 62、当前库重开 sentinel 全通过，以及预先存在的真正空 SQLite 文件正常迁移。
- `:408-418` 覆盖当前谱系数据库重开成功且逻辑指纹不变。

本轮按只读任务要求没有运行测试。第三轮修订新增至 12 个定向夹具，必须由主控实跑确认 `12 passed / 0 failed / 0 ignored`，并重新完成 Cargo check、Clippy、Windows Rust 全量、Node 119、Vite build 和 source gate；上一轮修订前的 275/0/3 计数不能替代第三轮结果。

## 二、严重度清单

### P0

无。

### P1

无。上一轮两个 P0及 allowlist/CAS P1 均已按源码关闭。

### P2 / 可接受残余风险

1. **并发 TOCTOU**：预检关闭后到 RW pool 建立前仍没有跨进程文件锁；外部进程可在最后一次 sidecar 检查后创建 sidecar或修改主库。当前三次检查已显著收窄窗口，但不能替代单实例约束/文件锁。若产品允许同一数据库多实例并发，应在后续加固；当前可作为明确残余风险接受。
2. **保守阻断正常遗留 sidecar**：任一 WAL/SHM（即使可能是可恢复的正常遗留文件）都会阻断自动启动。这是刻意的 fail-closed 取舍，不是功能性误判；RC 必须验证原生提示、完整备份和隔离副本 checkpoint/恢复流程，绝不能引导用户删除 sidecar。
3. **预检连接失败仍为普通 setup error**：ACL、文件损坏或底层连接阶段错误仍映射为 `DbError::Connect`，不走四类谱系原生提示。M1 报告已如实保留该风险；不应宣称已覆盖所有数据库启动错误。
4. **最小 sentinel 不是完整 schema diff**：关键 index/trigger 仍主要按名称检查，未验证完整 SQL 语义。当前覆盖 N0 冻结集合，可接受进入后续阶段；RC/后续安全加固应继续跟踪。
5. **默认旧目录复制链未纳入本轮夹具**：`default_db_path()` 的既有 legacy 目录复制发生在 `init_pool()` 之前。它不改写源数据库/源 sidecar，但会先产生可恢复副本再由新门禁检查。正式升级前应单独验证该继承路径与提示中的数据库位置，避免把“复制出隔离目标”误报成“源库已修改”。
6. **dangling symlink 边界**：`try_exists()` 跟随符号链接；极端情况下名为 `-wal/-shm` 的悬空链接可能被视为不存在。Windows 默认正式数据目录通常不使用该形状；如需强化，可改用 `symlink_metadata()` 将任何目录项都视为 sidecar。

## 三、范围、敏感信息与报告一致性

- 实际产品源码差异仍只涉及四个授权文件：`db/mod.rs`、新 `db/migration_safety.rs`、`db/migration_lineage_tests.rs`、`lib.rs`。
- 迁移 SQL、Cargo/Node 依赖、版本、device sync、飞书逻辑和发布配置无差异。
- `git diff --check` 静态退出 0；仅有 LF/CRLF 提示。
- setup 原生提示只展示稳定错误码、数据库路径、完整备份/隔离恢复建议和退出说明；日志只含 code/version/static reason/sentinel code，不包含业务正文、SQL 参数、Token 或凭据。
- M1 报告已更新为 12 个夹具、checksum 写框架移除、sidecar 保守阻断，并明确列出未运行项及残余风险，与当前源码一致。

## 四、接受建议与剩余门禁

**建议：M1 的 fail-closed 安全实现代码审查 accepted。** 两个已知 P0均有生产修复和对应反例夹具，五项本轮安全硬门禁在静态证据上闭合，未发现新的 P0/P1。

但若主控沿用冻结计划中“已知旧 checksum 自动兼容”为 M1 的必达功能，则**不能宣称完整 M1 功能 accepted**：该项当前明确未满足。建议将验收结论写成“fail-closed 主体 accepted；历史 checksum 兼容因无来源输入 deferred/pending_verified_input”，并在进入 RC 前决定是补做 `M1-COMPAT`，还是经用户确认把 v0.8.3 的恢复边界正式改为只提示、不同谱系不自动兼容。

主控最终把任务状态改为 accepted 前，仍须完成以下非本线程门禁：

1. 第三轮 12 个定向测试及 Windows Rust 全量实跑；
2. Cargo check、Clippy、Node 119、Vite build、source gate 全绿；
3. Windows 原生对话框视觉验证，确认关闭后退出码 2、无 setup panic；
4. 使用纯合成/隔离副本验证无 sidecar 的 0.8.2 正常库可启动、未来迁移可升级，以及有 sidecar 的失败路径主库/sidecar 字节不变；
5. RC 阶段再执行 `quick_check`、`foreign_key_check` 和正式升级/恢复演练，不得直接在唯一正式库试验。

安全声明：本线程未修改任何产品/测试源码、迁移、依赖、版本或其他线程文件；未运行 Cargo/Node/构建；未读取或修改正式数据库、NAS、飞书、凭据或业务正文；未提交 Git。
