# V083-M1-COMPAT36-R3 派工单

## 目标

修复 R2 独立复核的两个 P1，保持已确认的严格迁移校验和 exact-schema 设计。

## 允许修改

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/mod.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- 本任务报告/线程状态

禁止运行 Cargo/rustfmt/构建；禁止访问正式数据库、凭据、NAS、发布资源；禁止修改迁移目录。

## 必须完成

1. synthetic v36 的 SQL 必须在 SQLite 中无条件语法失败，不能通过预建任意表、视图、函数或其他 schema 对象使其成功；该 SQL只在“预检后所见数据库缺少已应用 v36”时才会被 SQLx 尝试执行。
2. 增加生产 `init_pool` 负例：预检授权后若所见迁移历史缺少 v36，占位 SQL失败，且不得插入伪造 v36；测试不得依赖可被预建对象满足的名称。
3. 所有合成 `feishu_reminder_runs` DDL 改为当前 SQLite 可执行的 `DEFAULT (datetime('now'))`；exact DDL 白名单只保留真实可执行且与正式审计一致的形态，不保留无效语法。
4. 正例与 `WITHOUT ROWID`、`STRICT`、`CHECK`、`UNIQUE`、`COLLATE NOCASE` 五类负例必须实际到达生产 `init_pool`；增加夹具建库成功的明确断言。
5. 保留 `ignore_missing=false`、第二次 sidecar 检查、完整六字段比较及四/五层 schema 元数据校验。
6. 报告 `.agent-work/output/V083-M1-COMPAT36-R3.md` 列出静态证据、未运行命令和 P2 残余竞态边界。
