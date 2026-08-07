# V083-N0-MIG｜迁移谱系失败夹具与只读预检契约

状态：主控已验收为 `accepted`

## 一、结论

已在纯 `#[cfg(test)]` 范围建立 6 个可执行迁移谱系夹具，覆盖：全新库、当前嵌入迁移谱系正常库、同编号不同 SQL/未知 checksum、版本 49 `success=1` 但关键表缺失、未知已应用迁移、`success=0`。当前真实谱系为 61 个迁移文件、最大版本 62，版本 36 是合法间隙；夹具按嵌入版本集合逐项比对，不再假设版本 1—62 连续。测试侧同时冻结迁移 49、51、58—62 的 schema sentinel。

本线程未改 `reconcile_migration_checksums()`、`init_pool()`、`set_ignore_missing(true)` 或任何运行时生产行为；未创建 0063/0064；未读取正式数据库、默认应用数据目录、NAS、飞书数据、凭据或业务正文。

## 二、修改文件

1. `src-tauri/src/db/mod.rs`
   - 仅新增 `#[cfg(test)] mod migration_lineage_tests;`。
2. `src-tauri/src/db/migration_lineage_tests.rs`
   - 新增 504 行纯测试模块；所有文件数据库均由 `tempfile::TempDir` 隔离创建，且显式传入合成路径。
3. `.agent-work/output/V083-N0-MIG.md`
   - 本报告。

## 三、可执行夹具矩阵

| 测试名 | 合成形状 | v0.8.2 当前行为/断言 | M1 应替换成的目标 |
|---|---|---|---|
| `fresh_database_reaches_current_lineage_and_all_frozen_sentinels` | 临时空文件库 | 实际已应用版本集合与 `sqlx::migrate!` 嵌入集合完全一致：61 条、最大版本 62、合法缺号 36、失败 0，sentinel 无缺失 | 保持通过 |
| `current_database_reopen_keeps_migration_history_unchanged` | 当前嵌入谱系正常库再次启动 | checksum 历史不变 | 保持通过 |
| `unknown_checksum_fixture_documents_unconditional_preflight_write` | 将版本 49 checksum 替换为合成“同编号不同 SQL”的 SHA-384；schema 仍为当前结构 | 直接调用现有 reconciliation 后，未知 checksum 被无条件改回当前值；测试以此证明预检前写入风险 | 未知值应在任何写入前返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`，历史行不变 |
| `migration_49_success_without_inbox_reaches_migration_51_failure` | 当前库删除 51+ 历史行，保留版本 49/50 为成功，再删除 `feishu_sync_inbox` | 现有启动继续走到迁移 51，返回包含 `51` 和 `feishu_sync_inbox` 的通用迁移错误 | 迁移前返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`，checksum/schema/业务表不写入 |
| `unknown_applied_version_fixture_documents_ignore_missing_behavior` | 当前库插入版本 9999、`success=1` 的合成迁移行 | `set_ignore_missing(true)` 接受启动，未知行保留；测试以此证明 fail-open | 写入前返回 `DB_MIGRATION_APPLIED_VERSION_UNKNOWN` |
| `failed_migration_row_fixture_is_rejected_by_sqlx` | 当前库把版本 62 标为 `success=0` | sqlx 拒绝启动，但仍只有通用 `DbError::Migrate` | 预检阶段稳定分类为 `DB_MIGRATION_LINEAGE_INCOMPATIBLE`，reason=`failed_history_row` |

说明：合成“同编号不同 SQL”使用固定测试 SQL 文本计算 SHA-384，仅用于证明未知 checksum 行为，不代表笔记本历史 checksum，也不得加入兼容白名单。

## 四、当前 checksum 证据

以下值由本工作树迁移文件以 `Get-FileHash -Algorithm SHA384` 重新核验；它们只是当前发布谱系，不是历史兼容白名单。

| 版本 | 当前 SHA-384 |
|---|---|
| 47 | `7c859e982563e09ec8400d4475985f5c5c682a97eeea367b8db4a89fc4aecafdd03424bd22607d7316e1b33807c0b465` |
| 48 | `560f92033b9f5cf14af743d1f8c63505141c02f379b7bfc0c51157d8ed5ba1353e04aed4572c89efbdbd5df169e5b513` |
| 49 | `5391dee7120f0715473530f80c0d3c336778bd29cde4c51cf7f367e4c3a5c85cc1a66483103e84850783eeee2641497c` |
| 50 | `2104cb8aa3e5523816994504876a45b23df166267e5cbfa6e60191c967f405e07339aee45805d0a126a9402415dd9d88` |
| 51 | `480cba0c5b59775e7ca65ff30be83e7c57b53f221085229bb75173f7a5f0c687b6297aae7b7e1f3e16a047eb4458b144` |
| 52 | `0b2515cf4e1d8bfb8e7e090cf8127011ede92ba70b246773b871d0c064d17c6b9ef4be505fb186874a67eb254082fe16` |

笔记本旧 checksum 当前仍未取得。本线程没有读取正式库或编造旧值；M1 白名单必须等待有来源的只读副本元数据。

## 五、冻结的 schema sentinel

测试辅助 `missing_schema_sentinels()` 只查询 `sqlite_master`、`PRAGMA table_info` 和 `PRAGMA foreign_key_list`，不查询业务字段值。

### 迁移 49

- 表：`feishu_sync_links`、`feishu_sync_inbox`；
- 列：links 的 `entity_type/local_entity_id/status`，inbox 的 `status/bound_case_id`；
- 索引：`idx_feishu_sync_inbox_status`；
- 外键：`feishu_sync_inbox.bound_case_id → cases.id ON DELETE SET NULL`。

### 迁移 51

- 表：`feishu_sync_binding_audits`；
- 列：`feishu_sync_inbox.auto_bind_suppressed`；
- 外键：binding audit 的 `inbox_id → feishu_sync_inbox.id CASCADE`、`previous_case_id → cases.id SET NULL`。

### 迁移 58

- 表：`device_sync_groups/members/outbox/dirty_entities/applied_operations/entity_revisions/conflicts/receipts/snapshots/quarantine/audits`；
- 索引：`idx_device_sync_outbox_pending`；
- 触发器：`device_sync_cases_insert`、`device_sync_contacts_insert`；
- 外键：member 的 `group_id → groups.id CASCADE`，quarantine 的 `group_id → groups.id SET NULL`。

### 迁移 59

- 表及列：`legal_skill_binding_suppressions.id/legal_domain/task_type`；
- 触发器：`device_sync_skill_binding_suppressions_insert/update/delete`。

### 迁移 60

- 表、索引：`case_domain_status_migration_audits`、`idx_case_domain_status_migration_audits_case`；
- 触发器：`case_stage_items_domain_guard_insert/update`。

### 迁移 61

- 列：`feishu_sync_field_previews.review_status/resolution_value_json/resolved_at`；
- 表、索引：`feishu_sync_operation_audits`、`idx_feishu_sync_operation_audits_preview`；
- 外键：operation audit 的 `preview_id → feishu_sync_field_previews.id SET NULL`。

### 迁移 62

- 表、列、索引：`feishu_sync_entity_previews`、其 `review_status`、`idx_feishu_sync_entity_previews_pending`；
- 外键：entity preview 的 `case_id → cases.id CASCADE`。

## 六、稳定错误码契约

| 错误码 | 触发条件 | 可重试 | 自动暂停 | 用户文案 | 安全日志字段 | 禁止展示/记录 |
|---|---|---|---|---|---|---|
| `DB_MIGRATION_CHECKSUM_UNKNOWN` | 已应用的已知版本，其 checksum 既非当前值也非有来源白名单值 | 否；取得兼容证明或受控恢复后再试 | 启动失败关闭 | 检测到无法验证的数据库迁移记录。原数据库未修改，请先备份并联系支持。 | code、db 路径、version、description、stored/current checksum、sentinel 结果、app version | 业务表内容、案件正文、密钥、Token |
| `DB_MIGRATION_APPLIED_VERSION_UNKNOWN` | `_sqlx_migrations` 存在当前二进制未解析的成功版本 | 否；换回兼容版本或完成谱系审计后再试 | 启动失败关闭 | 数据库来自当前版本无法识别的迁移谱系。原数据库未修改。 | code、db 路径、未知 version/description/success、当前最大版本 | 未知迁移涉及的业务行、凭据 |
| `DB_MIGRATION_SCHEMA_SENTINEL_MISSING` | 迁移标记成功，但对应关键表/列/索引/触发器/外键缺失 | 否；隔离副本修复后再试 | 启动失败关闭 | 数据库结构与迁移记录不一致。原数据库未修改，请先备份。 | code、version、缺失 sentinel code 列表、db 路径、app version | 表内值、SQL dump、案件正文 |
| `DB_MIGRATION_LINEAGE_INCOMPATIBLE` | `success=0`、白名单动作的预期结构不匹配、多个谱系信号冲突 | 否；人工审计后再试 | 启动失败关闭 | 数据库迁移谱系不兼容，应用已停止启动且未修改原库。 | code、reason、version、success、sentinel 摘要、db 路径 | 业务行、秘密信息、完整数据库内容 |
| `SYNC_PACKAGE_DEPENDENCY_MISSING` | 包的依赖闭包和接收端均不存在被引用实体 | 禁止自动重试；修复依赖后由用户明确重试 | 是 | 同步包缺少必要依赖，已整包回滚并暂停该同步组。 | code、group/device、sequence、source filename、entity type/id 的安全摘要 | 解密载荷、字段值、密钥、案件正文 |
| `SYNC_PACKAGE_QUARANTINED` | 确定性包错误已登记为活动隔离；同键只更新重试计数 | 否；解决原因后由用户明确重试 | 是 | 同步包已隔离，未写入部分数据。 | code、quarantine id、group/device、sequence、reason、retry_count、first/last_seen | 解密载荷、业务字段、成员密钥 |
| `SYNC_GROUP_AUTO_PAUSED` | 确定性失败首次隔离后，同步组被熔断 | 否；用户检查并明确恢复 | 是 | 检测到确定性同步错误，该同步组已自动暂停。 | code、group id、trigger reason、quarantine id、paused_at | 凭据、业务正文、明文事件 |
| `FEISHU_ORPHAN_BINDING` | active link 指向不存在的本地案件 | 否；本地解除/重绑后重试 | 不暂停设备同步；单条飞书记录隔离 | 本地案件已不存在，请解除绑定后重新绑定。 | code、link/inbox/run id、entity type、remote record id 的安全标识 | Token、飞书字段正文、其他案件内容 |

程序分类只能依赖 `code` 和结构化字段，不得解析中文文案。

## 七、实际静态验证

本线程遵从主控的并发指令，没有运行 Cargo；同步线程仍在共享工作树工作，定向 Rust 测试留给主控串行执行。

首次提交后，主控通过 Windows 清单脚本实际运行 277 个测试，本任务 6 个测试中 5 个通过、1 个失败；失败仅为正常谱系夹具错误断言 `count=62`，实际为 `count=61/max_version=62`。本次已改为逐项对比实际应用版本与 `sqlx::migrate!` 嵌入版本集合，并明确断言 61 条、最大版本 62、版本 36 为合法间隙。修订后按指令未再次运行 Cargo，仍待主控串行复验。

| 命令 | 退出码 | 结果 |
|---|---:|---|
| `rustfmt.exe --edition 2021 --check src-tauri/src/db/migration_lineage_tests.rs` | 0 | 格式通过 |
| `git diff --check -- src-tauri/src/db/mod.rs` | 0 | 无空白错误；仅出现 Windows LF/CRLF 提示，不是失败 |
| `Get-FileHash -Algorithm SHA384`（迁移 47—52） | 0 | 与冻结计划中的 6 个当前值一致 |
| 测试文件敏感路径静态检索 | 0 | 未出现 `default_db_path`、`app_data_dir()`、`CASEBOARD_DATA_DIR`、`APPDATA`、`settings.json` 或正式数据库路径 |

建议主控串行执行：

```powershell
$env:PATH = 'C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL = '0'
Set-Location 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev\src-tauri'
cargo test --lib db::migration_lineage_tests -- --test-threads=1 --nocapture
```

预期发现 6 个测试，目标为 `6 passed / 0 failed / 0 ignored`。这些测试中的“通过”包含对 v0.8.2 缺陷行为的确定性记录；进入 M1 后，应将三个缺陷证明测试改写为 fail-closed 目标断言。

## 八、Git 与遗留风险

- 本线程未提交、未推送 Git。
- 本线程源码差异仅为 `src-tauri/src/db/mod.rs` 的 3 行 test module 声明和新测试文件；共享看板、其他线程报告/状态变化不属于本线程，未覆盖。
- 由于主控禁止并发 Cargo，当前报告没有编译/运行计数；若定向命令出现 Rust 类型错误或 SQLite 行为差异，应退回本线程修正，不得直接视为 accepted。
- 笔记本历史 checksum 仍未知，兼容白名单仍为空；这是有意的 fail-closed 边界，不是遗漏。
- 当前 sentinel 是 v0.8.3 M1 的最小关键集合，不等价于完整 schema diff；M1 仍需对不兼容路径做“执行前后迁移表、schema、业务表指纹一致”验证。
- 未修改正式数据库、NAS、同步组、飞书 Base、凭据或业务数据；没有生成持久化 `.db` 夹具，测试执行后的临时目录由 `TempDir` 清理。
