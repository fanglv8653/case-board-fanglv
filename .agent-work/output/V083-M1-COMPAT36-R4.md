# V083-M1-COMPAT36-R4 实现报告

## 结论

已按 R4 派工完成唯一必要修正：`pragma_table_xinfo.dflt_value` 对 `sent_at` 的期望值由 `"(datetime('now'))"` 改为 SQLite 实际返回的 `"datetime('now')"`。

## 表达形式分离

- `sqlite_master.sql` 的合法 DDL 白名单仍只接受：`DEFAULT (datetime('now'))`。
- `pragma_table_xinfo` 的表达式元数据只期望：`datetime('now')`。
- 未恢复无括号的无效 CREATE TABLE DDL，未改变固定 checksum、表/索引谓词或兼容 migrator。

静态位置：

- `src-tauri/src/db/migration_safety.rs:76-82`：合法括号 DDL保持不变。
- `src-tauri/src/db/migration_safety.rs:1268-1274`：`table_xinfo` 默认值期望改为无外括号形式。

## 范围

R4 源码改动仅为 `src-tauri/src/db/migration_safety.rs` 中上述一个字符串字面量。测试文件无需修改，因此未改；迁移目录无差异。

保留 R3 已确认项，包括：

- `SELECT FROM;` 无条件语法失败占位；
- `ignore_missing=false` 及显式固定 v36 元数据；
- 写池前第二次 sidecar 检查；
- after-preflight 缺 v36 不伪造历史测试；
- 五类合法 DDL schema 变体；
- v36 六字段原始记录完整比较。

## 静态核对与未运行命令

- 已确认 DDL 白名单仍为 `DEFAULT (datetime('now'))`。
- 已确认 `table_xinfo` 期望为 `datetime('now')`。
- `git diff --check -- src-tauri/src/db/migration_safety.rs` 通过，仅有既有 LF/CRLF 提示。
- `git diff -- src-tauri/migrations` 为空。

按派工禁令，本轮未运行 Cargo、rustfmt、Node、测试、构建、打包或发布命令。定向测试、check、clippy、Windows Rust 全量及独立复核均待主控串行执行，当前不得标记最终通过。

## 资源与剩余边界

本轮未访问正式数据库、正式 WAL/SHM、凭据、NAS、飞书或发布资源。R3 已披露的 preflight 与写连接物理文件未完全绑定的 P2 TOCTOU 边界不变；本轮不扩大范围处理，也不宣称已由第二次 sidecar 检查完全关闭。
