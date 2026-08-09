# V083-M1-COMPAT36 实现报告

## 结论

已在限定的迁移安全、数据库初始化和迁移谱系测试文件中实现 version 36 的单点兼容。实现不新增或恢复 0036、不新增 0064、不改写既有 `_sqlx_migrations`；SQLx 的 `ignore_missing` 仅在不可变只读预检完整通过并返回窄授权后启用。

本工作线程未访问正式数据库文件、正式 WAL/SHM、凭据、NAS、飞书接口或发布状态，也未删除任何 sidecar。仅阅读了正式门槛的脱敏审计报告作为固定兼容依据。

## 兼容谓词

只有同时满足下列全部条件，`MigrationPreflight` 才返回 `allow_missing_legacy_migration_36=true`：

1. 历史记录 version 严格等于 `36`；
2. description 严格等于 `feishu reminder runs`；
3. success 原始整数严格等于 `1`，不把其他非零整数当作成功；
4. stored SQLx SHA-384 严格等于 `84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480`；
5. `pragma_table_xinfo('feishu_reminder_runs')` 恰好只有三列，且 cid、名称、声明类型、NOT NULL、默认值、主键序号和 hidden 均精确匹配：
   - `sent_date TEXT PRIMARY KEY NOT NULL`；
   - `sent_at TEXT NOT NULL DEFAULT datetime('now')`；
   - `item_count INTEGER NOT NULL DEFAULT 0`；
6. 其余已应用迁移全部存在于当前嵌入集合且 checksum 一致；
7. 没有已应用历史缺口、失败记录或其他 unknown version；
8. 现有全部适用 schema sentinel 通过。

生产 `init_pool` 在只读预检返回上述窄授权之前不创建读写/WAL 连接；仅授权分支克隆当前嵌入 migrator 并开启 `ignore_missing=true`，非授权分支继续使用 SQLx 默认 `ignore_missing=false`。

## 失败优先级

保持现有稳定优先级，并将 version 36 纳入相同失败关闭链路：

1. WAL/SHM sidecar：`DB_MIGRATION_LINEAGE_INCOMPATIBLE`；
2. success 不严格等于 1：`DB_MIGRATION_LINEAGE_INCOMPATIBLE / failed_history_row`；
3. description 不匹配或其他未知版本：`DB_MIGRATION_APPLIED_VERSION_UNKNOWN / applied_version_not_embedded`；
4. 已应用历史缺口：`DB_MIGRATION_LINEAGE_INCOMPATIBLE / applied_history_gap`；
5. 缺表或任一列定义不精确：`DB_MIGRATION_SCHEMA_SENTINEL_MISSING / applied_migration_schema_missing`；
6. schema 完整后再判断 checksum；错误 version 36 checksum：`DB_MIGRATION_CHECKSUM_UNKNOWN / checksum_not_allowlisted`。

因此 schema 缺陷仍优先于 checksum mismatch，其他既有组合优先级未被放宽。

## 测试覆盖

迁移谱系模块现有 31 个测试入口，其中 29 个 Tokio 测试、2 个普通测试，2 个普通测试为父进程夹具调用的 ignored child。此次新增 7 个父测试和 1 个 ignored child：

- 正例：从真实嵌入迁移 0001-0062（当前集合不含 36）构造合成正式形状，插入固定 version 36 tuple 和精确表结构；父测试通过独立子进程调用生产 `init_pool`，要求实际升级到 0063，成功历史为 63 行、max=63、36 和 63 各一行，并证明 version 36 的 description、success、checksum、execution_time 原样保留，同时执行 `quick_check` 和 `foreign_key_check`。
- 负例：错误 checksum、错误 description、success=2、缺表、额外列、并存额外 unknown version 均调用生产 `init_pool`，并在失败前后比较主 DB、WAL、SHM 的物理字节及逻辑指纹。
- 回归：现有全新库、当前谱系重开、pre-0063 正常升级、unknown checksum/version、schema sentinel 与组合优先级测试保持在同一模块。

## 门禁执行事实

- `cargo check --workspace --locked` 曾得到明确 exit 0，但发生在最后一次 success 严格化和 clippy 修正之前，不能作为最终源码门禁结论。
- 最后一次 `cargo clippy --workspace --locked --all-targets -- -D warnings` 明确失败 1 项：`legacy_migration_36_schema_matches` 的七元组触发 `type_complexity`。随后已静态改为具名 `TableColumnDefinition`，但按主控“禁止再启动构建命令”的指令未自行重跑。
- 两次共享仓库并发误启如实记录：第一次连续启动两个 `cargo test --no-run`，确认均属本线程后停止后发进程并保留先发；第二次本线程 clippy 与共享槽位中的后发 check 重叠，主控告警后两进程均退出。此后本线程停止所有构建活动。
- `git diff --check` 对三份限定 Rust 文件通过；最终 Cargo check、Clippy、最窄测试、全部迁移安全测试和 Windows Rust 全量均交由主控在共享仓库中独立串行重跑。本报告不把未重跑门禁写成通过。

## 修改范围

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- `.agent-work/output/V083-M1-COMPAT36.md`

未修改迁移 SQL、正式数据、业务模块或发布文件。
