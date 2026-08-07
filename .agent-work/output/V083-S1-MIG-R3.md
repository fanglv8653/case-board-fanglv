# V083-S1-MIG-R3：0063 durable export 迁移哨兵终验报告

状态：`submitted_for_review`

## 一、结论

已吸收当前 0063 的 outbox 单调捕获顺序、legacy quarantine 白名单脱敏和 durable export draft 全部结构语义，并恢复迁移定向与 Windows Rust 全量零失败。

迁移定向测试 21 passed、0 failed；Windows Rust 包装脚本共 3 个测试可执行文件，374 passed、0 failed、3 ignored。S1 报告中记载的唯一全量失败（legacy fixture 缺少 `device_sync_outbox`）已关闭。

## 二、范围

仅修改：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- 本报告与本线程工作流状态

未修改 `src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql`、`src-tauri/src/device_sync/**` 或前端产品实现；未 commit、未 push。

## 三、生产迁移安全门禁

### Outbox 捕获顺序

- `device_sync_outbox.capture_sequence` 必须为 `INTEGER NOT NULL DEFAULT 0`；
- `idx_device_sync_outbox_capture_sequence` 必须是精确的非 partial 唯一索引 `(group_id, capture_sequence)`；
- `idx_device_sync_outbox_pending_capture` 必须精确为 `(group_id, state, capture_sequence)`，且不得是 unique/partial。

### Legacy quarantine

沿用 R2 的完整 quarantine DDL、生命周期、FK 和索引精确比对。R3 迁移语义测试进一步证明旧记录：

- 1:1 保留为 `manual_review`；
- `source_path` 固定清空为 `NULL`；
- `details_json` 固定替换为白名单元数据 `legacy_record/identity/sensitive_content`；
- 不保留绝对路径、数据库错误正文或测试业务正文；
- 不虚构真实设备/序列身份。

### Durable export draft

`device_sync_export_drafts` 现在接受四层核验：表存在、15 列逐项元数据、完整 DDL、FK/索引精确定义。覆盖：

- 所有列的类型、空值属性和默认值；
- `sequence >= 1`、`key_epoch >= 1`；
- `state DEFAULT 'prepared'` 且仅允许 `prepared/finalized`；
- 复合主键 `(group_id, local_device_id, sequence)`；
- `group_id -> device_sync_groups(id) ON DELETE CASCADE`；
- 状态索引 `(group_id, local_device_id, state, sequence)`；
- 每组仅一个 prepared 草稿的唯一 partial 索引；
- 完整 DDL 精确比对会拒绝任何额外 `path`/`nas_path` 列，草稿结构不持久化 NAS 路径。

## 四、真实迁移语义夹具

legacy 夹具已补齐 0058 的真实 `device_sync_groups`、`device_sync_outbox`、原 pending 索引和 quarantine 前置结构，再直接执行仓库当前 0063 SQL。

夹具按故意打乱的插入顺序构造同毫秒记录，验证每组旧行严格按旧 planner 的 `(logical_time, operation_id)` 次序归一化为从 1 开始的 `capture_sequence`；不同组独立计数，同组重复 sequence 被唯一索引拒绝。

同一夹具还实际验证 export draft 的必填密文字段、sequence/key epoch/state CHECK、复合主键、one-prepared partial 唯一性、默认时间、无 path 列以及 group 删除后的 CASCADE。当前数据库夹具继续通过完整迁移集合创建，天然包含真实 outbox 前置结构并能安全 reopen。

## 五、失败关闭反例

新增反例均在调用生产 `init_pool()` 前采集物理字节和逻辑指纹，并断言失败返回后完全不变：

1. 同名 outbox capture 唯一索引颠倒列序；
2. 完全删除 export draft 表；
3. 删除 `operation_fingerprint` 列但恢复同名索引；
4. 将 `CHECK(sequence >= 1)` 弱化为 `>= 0`；
5. 向 draft 表加入 `nas_path` lookalike 列；
6. 将 one-prepared 索引弱化为 `(group_id, local_device_id)`。

六类均命中对应的 `M63.*` sentinel；合法 current schema 无误报。

## 六、验证结果

### 迁移定向

```text
running 21 tests
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 301 filtered out; finished in 10.04s
```

### Windows Rust 全量

执行仓库原脚本 `scripts/run-windows-rust-tests.ps1`，仅在当前进程 PATH 中显式加入本机现有 Cargo 路径：

```text
caseboard_lib:          319 passed; 0 failed; 3 ignored
caseboard:                0 passed; 0 failed; 0 ignored
device_sync_contract:    55 passed; 0 failed; 0 ignored
[ok] Windows Rust tests passed: 3 executables
```

脚本总耗时 346.1 秒，其中测试 profile 编译 3 分 46 秒；未修改脚本或测试配置。

### 静态复核

- 两个授权文件逐文件 `rustfmt --check --config skip_children=true`：通过；
- 两个授权文件 `git diff --check`：通过，仅有 Windows LF/CRLF 提示；
- 迁移集合：62 条、最大版本 63、合法缺号 36；
- 定向测试属性：21；
- 文件行数：`migration_safety.rs` 1299 行，`migration_lineage_tests.rs` 1253 行。

## 七、复审重点

- 确认完整 DDL 精确比对符合“已应用 63 必须具备当前未发布迁移的最终结构”策略；
- 确认旧 outbox 归一化严格保留旧 planner 顺序，而非采用插入顺序或 UUID 随机顺序；
- 确认 legacy quarantine 只保留白名单安全元数据；
- 确认 draft 允许 `previous_manifest_hash/finalized_at` 按迁移定义为空，其余必填与默认值均被精确核验；
- 正式双设备/NAS 验证仍属于 RC 门禁，本报告不宣称已完成真实环境验收。
