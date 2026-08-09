# V083-M1-COMPAT36-R4 独立只读复核报告

## 结论

**独立静态复核通过，可进入主控串行门禁。**

- P0：0
- P1：0
- P2：1（既有、已披露的残余 TOCTOU 边界）

R4 唯一必要修正正确：`pragma_table_xinfo.dflt_value` 期望值改为无外括号的 `datetime('now')`；`sqlite_master.sql` 白名单仍只接受 SQLite 可执行的 `DEFAULT (datetime('now'))` DDL。未发现其他逻辑回退。

本轮为只读复核，未修改源码、测试或迁移，未运行 Cargo/rustfmt/构建，未访问正式数据库、WAL/SHM、凭据、NAS、飞书或发布资源。最终接受仍以主控串行定向测试、check、clippy 和 Windows 全量 Rust 门禁为前提。

## R4 修正确认

### PRAGMA 与 DDL 表达形式已正确分离

- `src-tauri/src/db/migration_safety.rs:76-82` 的唯一合法 DDL 仍为 `DEFAULT (datetime('now'))`。
- `migration_safety.rs:1249-1287` 的 `table_xinfo` 列谓词现要求 `Some("datetime('now')")`。
- `migration_safety.rs:1308-1322` 仍仅把 `sqlite_master.sql` 与上述合法括号 DDL 比较；没有恢复无括号的无效 CREATE TABLE 白名单。

SQLite 3.53.1 纯内存复核结果：

- 合法括号 DDL 建表成功；
- `pragma_table_xinfo.dflt_value` 返回 `datetime('now')`；
- `sqlite_master.sql` 保留 `DEFAULT (datetime('now'))`。

当前实现与 SQLite 的两种实际表示完全一致，R3 唯一 P1 已消除。

## R3/R2 已确认项无回退

- `migration_safety.rs:83-85` 仍使用 `SELECT FROM;`。纯内存复核继续返回 `near "FROM": syntax error`，不依赖任何数据库对象。
- 全项目 Rust 源码检索不存在 `set_ignore_missing` 或 `ignore_missing: true`；compatible migrator 仍只补固定 v36 metadata，并保持 `ignore_missing=false`。
- v36 候选继续绑定 version、description、原始整数 `success == 1`、固定 SHA-384、精确 schema；其他 embedded checksum 和全部适用 sentinel 仍在放行前完成。
- `mod.rs` 的测试 hook 仍位于 immutable preflight 成功之后、第二次 sidecar 检查和读写 pool 打开之前；按目标路径一次性执行。
- after-preflight 替换测试仍调用同一次生产 `init_pool`，把目标替换为已到 0063但缺 v36 的 main-only 数据库，要求 syntax migration error，并断言 `(v36 count, max version) == (0,63)`。
- 基准及五类变体均使用有效 `DEFAULT (datetime('now'))`；修正 PRAGMA 期望后，公共列谓词可通过，`WITHOUT ROWID`、`STRICT`、`CHECK`、`UNIQUE`、`COLLATE NOCASE` 将分别到达表属性、DDL、索引或 collation 目标拒绝层。
- exact schema 仍联合校验 `table_xinfo`、`table_list`、单一 DDL、`index_list`、`index_xinfo`。
- 正例仍由生产子进程升级到 0063，并以 `(version, description, installed_on, success:i64, checksum, execution_time)` 六字段整行比较 v36 前后不变。
- 错误 checksum/description/success、缺表、额外列、额外 unknown version及五类结构负例仍经生产 `init_pool`；preflight 负例 helper 仍比较 main/WAL/SHM 物理指纹。
- preflight 前和读写 pool 前的两次 sidecar 拒绝均保留。

## 范围与静态门禁

- R4 报告所述变更与 R3 复核问题一致，仅修正 `migration_safety.rs` 中一个 default-value 字符串字面量；当前 `mod.rs` 和测试逻辑与 R3 已复核内容一致。
- `git diff -- src-tauri/migrations` 为空，不存在 0036/0064 迁移变更。
- 三个 Rust 文件 `git diff --check` 通过，仅有工作区 LF/CRLF 提示。

## P2 残余边界

immutable preflight 结果仍未通过 OS 文件锁或稳定文件句柄与后续写连接绑定。第二次 sidecar 检查、`ignore_missing=false` 和无条件失败的 synthetic v36 已消除首轮的普遍放行及“缺 v36 被伪造”问题，但无法阻止外部进程在极小窗口替换成“仍含固定 v36 checksum、其他 tuple/schema 已改变”的文件。该风险按 R3 范围继续记为 P2，不影响本轮 P0/P1 清零，也不得对外描述为完全关闭 TOCTOU。

## 验收建议

独立静态复核已满足 P0=0、P1=0。主控应串行运行 compat36 定向测试，重点确认：

1. legacy-v36 正例升级至 0063且六字段不变；
2. after-preflight 缺 v36 返回 syntax migration error且不插入 v36；
3. 五类有效 DDL 变体分别失败并保持物理不写；
4. check、clippy、Windows 全量 Rust 门禁通过。

上述动态门禁全部通过后，可接受 V083-M1-COMPAT36-R4。
