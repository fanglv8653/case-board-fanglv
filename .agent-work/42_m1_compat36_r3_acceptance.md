# V083-M1-COMPAT36-R3 验收标准

- P0=0、P1=0。
- 占位 SQL 无条件失败，不能由数据库对象预置绕过。
- 所有兼容正/负夹具 DDL 在当前 SQLite 实际可执行，测试到达生产 `init_pool`。
- 其他 unknown version、错误 tuple、五类 schema 绕过均失败关闭且有物理不写证明。
- 正例升级至 0063且 v36 六字段原始行不变。
- 定向测试、check、clippy、Windows 全量 Rust 门禁及独立复核全部通过后方可接受。
