# 线程任务包｜V083-M1

## 目标

实现数据库迁移“只读预检 → 分类 → 允许的兼容动作 → sqlx migrate”流程。任何未知或不兼容谱系必须在 checksum、schema、业务数据写入前失败关闭，并在 Tauri setup 阶段给出不依赖 WebView 的可操作提示。

## 必读

- `agent-work/output/V083-20260803_下一轮待开发计划.md` 的 V083-M1；
- `.agent-work/output/V083-N0-MIG.md`；
- `.agent-work/output/V083-N0-GATE.md`；
- `.agent-work/17_round1_master_summary_template.md`；
- `src-tauri/src/db/mod.rs`；
- `src-tauri/src/db/migration_lineage_tests.rs`；
- `src-tauri/src/lib.rs` 中 Tauri setup/`init_pool` 调用；
- `scripts/run-windows-rust-tests.ps1`。

## 必须实现

1. 只读预检现有 `_sqlx_migrations`；全新库/无迁移表允许进入正常 migrate。
2. 稳定区分：未知 checksum、未知已应用版本、`success=0`、49/51/58—62 sentinel 缺失。
3. 默认不使用 `set_ignore_missing(true)` 放过未知版本；只有明确白名单覆盖的缺失版本才允许。当前没有真实旧 checksum，白名单必须为空，禁止猜值。
4. 未知/不兼容路径执行前后 `_sqlx_migrations`、schema、业务表指纹一致。
5. 将 N0 三个缺陷行为测试改为 fail-closed 目标断言，并保持全新库与当前库回归。
6. `DbError` 提供结构化 code 和安全字段；用户文案包含错误码、数据库位置、备份建议和退出说明，不暴露业务正文/SQL 参数/凭据。
7. Tauri setup 捕获兼容错误并使用现有依赖可实现的原生 Windows 提示；若当前依赖无法安全实现，必须先报告阻断，不得新增第三方依赖或退回 WebView toast。
8. 不自动删除、覆盖、重命名、重建数据库；不创建 0063/0064；不修改版本号和发布配置。

## 允许写入

- `src-tauri/src/db/` 中迁移预检、错误结构和相关测试；
- `src-tauri/src/lib.rs` 中最小 setup 捕获/原生提示接线；
- 必要时 `scripts/windows-upgrade-validation/` 中纯构造库校验脚本；
- `.agent-work/threads/worker-m1/`；
- `.agent-work/output/V083-M1.md`。

## 禁止

- 不读取/修改正式数据库、默认应用数据目录、NAS、同步组、飞书 Base、凭据或业务正文；
- 不修改 device sync、飞书业务逻辑、迁移 SQL、Cargo/Node 依赖、版本和发布资产；
- 不提交 Git、不 push；
- 不运行全量 Cargo、Node 或生产构建；主控统一串行。允许 `rustfmt --check`、`git diff --check` 和不触发编译的静态检查。

## 验收输出

- 文件清单和实现边界；
- 四类稳定错误的触发、code、写前不变量、用户提示；
- 白名单为空及原因；
- 夹具名称和预期计数；
- 建议主控运行的最窄命令；
- 明确列出未运行项和残余风险。

完成后通过 workflow 提交为 `submitted_for_review`，不得自行 accepted。
