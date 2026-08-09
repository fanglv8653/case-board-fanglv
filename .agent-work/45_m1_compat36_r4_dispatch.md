# V083-M1-COMPAT36-R4 派工单

## 目标

只修复 R3 唯一 P1：区分合法 DDL 文本与 SQLite `table_xinfo.dflt_value` 的规范化表示。

## 范围与要求

- 只允许修改 `src-tauri/src/db/migration_safety.rs` 和任务报告/线程状态；如测试无需改动则不得改。
- `sqlite_master.sql` 白名单继续只接受合法 `DEFAULT (datetime('now'))` DDL。
- `pragma_table_xinfo` 的 `dflt_value` 期望改为 SQLite 实际返回的 `datetime('now')`（无外括号）。
- 禁止其他逻辑变化、迁移变化、Cargo/rustfmt/构建、正式数据访问。
- 报告 `.agent-work/output/V083-M1-COMPAT36-R4.md`。
