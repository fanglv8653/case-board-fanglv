# V083-S1-MIG｜0063 与迁移安全门禁集成报告

状态：`submitted_for_review`

## 一、结论

S1 的 0063 迁移已纳入 M1 迁移谱系门禁。当前嵌入迁移集合按完整版本向量比对，固定事实为 62 条、最大版本 63、合法缺号 36；没有引入“版本必须连续”的脆弱假设。

0063 的组状态字段、隔离生命周期字段和索引语义已增加生产 sentinel。声称已应用 63、但活动隔离索引只是同名普通索引的合成库，会在任何 RW/WAL 连接前返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`。

迁移谱系定向测试最终为 13 passed、0 failed、0 ignored、281 filtered。

## 二、修改范围

仅修改：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- `.agent-work/output/V083-S1-MIG.md`
- `.agent-work/threads/worker-s1-mig/` 状态文件

未修改 `0063_device_sync_quarantine_lifecycle.sql`、S1 device-sync 实现、前端、依赖、版本或发布文件；未 commit、push。

共享工作树中已有 S1 交付的 7 个 device-sync 文件和 2 个前端文件差异，本线程只读核对，未编辑或恢复这些文件。0063 为 S1 已提供的输入迁移，本线程同样未改动。

## 三、迁移集合断言

`fresh_database_reaches_current_lineage_and_all_frozen_sentinels` 继续先比较：

```rust
assert_eq!(actual_versions, embedded_versions);
```

并冻结当前集合事实：

- 迁移总数：62；
- 最大版本：63；
- 36 不在嵌入集合中，是合法历史缺号；
- `success=0` 行数为 0。

预先存在的空数据库迁移后同样断言 `_sqlx_migrations` 为 62 行。测试不使用 `1..=63` 等连续集合推导。

## 四、0063 schema sentinel

当版本 63 已应用时，生产只读预检新增以下 sentinel：

### `device_sync_groups`

- `last_attempt_at`
- `last_success_at`
- `auto_paused`
- `pause_reason_code`

### `device_sync_quarantine`

- `status`
- `first_seen_at`
- `last_seen_at`
- `retry_count`
- `resolved_at`
- `last_error_code`

### 索引

- `idx_device_sync_quarantine_group_status` 必须存在；
- `idx_device_sync_quarantine_active_key` 不只检查名称，还必须：
  - 属于 `device_sync_quarantine`；
  - `unique=1`；
  - `partial=1`；
  - 定义包含 `COALESCE(group_id,'')`、`COALESCE(source_path,'')`、`reason_code`；
  - partial 条件包含 `WHERE status='active'`。

索引 SQL 比较只做大小写和空白归一化，不读取业务字段。`sqlite_master`、`PRAGMA index_list` 的查询/解码失败沿用 `schema_metadata_unreadable` 结构化失败路径。

0063 重建后的 `group_id → device_sync_groups(id) ON DELETE SET NULL` 继续由既有 M58 外键 sentinel 覆盖。

## 五、确定性 main-only 夹具

`migrated_fixture` 先在纯合成 `staging.db` 上运行当前迁移并插入固定合成案件标记，再执行参数化 `VACUUM INTO` 生成独立 `caseboard.db`。该目标从未作为 WAL 源打开，生成后明确断言不存在 `-wal/-shm`，作为确定性的 checkpointed main-only 输入。

`current_database_reopen_keeps_all_fingerprints_unchanged`：

1. 对 main-only 输入建立 immutable 只读逻辑指纹；
2. 调用生产 `init_pool()` 重开；
3. 直接从成功返回的生产 pool 读取迁移历史、schema 和合成业务行指纹；
4. 比较一致后关闭 pool。

没有删除被测 sidecar，没有在失败路径 checkpoint，也没有弱化生产“发现 WAL/SHM 即失败关闭”的边界。成功连接建立 WAL 属正常生产行为，测试不再在关闭后用另一个 immutable helper 误判该行为。

## 六、新增 0063 失败夹具

`migration_63_requires_semantic_active_quarantine_index_before_write`：

1. 从 current main-only 合成库开始；
2. 删除 0063 活动唯一索引；
3. 创建同名但非唯一、非 partial、无 COALESCE 表达式的普通索引；
4. 在调用 `init_pool()` 前先采完整 DB/WAL/SHM 物理字节，再采逻辑指纹；
5. 断言返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`、`version=63`，缺失码包含 `M63.index.idx_device_sync_quarantine_active_key`；
6. 失败返回后先比较物理字节，再比较完整迁移历史、schema 和合成业务行，全部不变。

同时将失败迁移行夹具从版本 62 更新为当前最新版本 63。

## 七、验证结果

### 静态检查

- 两个授权 Rust 文件逐文件 `rustfmt --check --config skip_children=true`：通过；
- 两文件 `git diff --check`：通过，仅有 Windows LF/CRLF 提示；
- 迁移目录静态版本解析：62 条、max 63、缺号 36；
- 测试计数：13；
- 0063 的 12 个新增 sentinel code/语义 helper：均存在；
- 文件行数：`migration_safety.rs` 783 行，`migration_lineage_tests.rs` 619 行。

### 定向测试

首次直接执行 `cargo test` 已完成编译，但 Windows 测试二进制在任何测试运行前以 `0xc0000139 STATUS_ENTRYPOINT_NOT_FOUND` 退出。原因是没有执行仓库 Windows 测试脚本中的 Common-Controls manifest 嵌入步骤，因此该次没有测试计数。

按 `scripts/run-windows-rust-tests.ps1` 的既有方式，用 Windows SDK `mt.exe` 为同一测试二进制嵌入 manifest 后，首轮真实结果为 12 passed、1 failed。唯一失败是 current reopen 成功建立 WAL 后，旧 fingerprint helper 在 pool 关闭后仍要求 sidecar 不存在；0063 sentinel 与迁移集合断言均已通过。

修正 current reopen 的验证顺序后，先执行 `cargo test --lib --no-run -j 1` 成功重新生成测试二进制，再嵌入同一 manifest，并只运行迁移过滤器：

```text
running 13 tests
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 281 filtered out; finished in 5.50s
```

本线程未运行全量 Rust、Node、Vite、S1 device-sync 专项、Cargo check 或 Clippy；这些由主控按总体验收矩阵执行。

## 八、安全声明与复验重点

- 所有数据库均为 `TempDir` 下合成库，没有读取默认应用目录、正式数据库、NAS、飞书或凭据；
- 未删除或 checkpoint 被测失败路径的 sidecar；
- 未修改迁移 SQL或 S1 实现；
- 请主控复验 13 项定向测试，并在全量 Windows Rust 门禁中确认原先 61/62 的唯一失败已消失；
- 请继续保留 M1 的生产 sidecar 失败关闭策略，不把成功测试夹具的 main-only 生成逻辑引入生产路径。
