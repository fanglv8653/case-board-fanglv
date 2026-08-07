# V083-S1-MIG-R2：0063 身份键与迁移契约加固报告

状态：`submitted_for_review`

## 一、结论

本轮已按当前 0063 阶段基线完成迁移安全加固。生产只读预检现在会精确核验隔离记录的来源设备/序列身份、生命周期字段约束、外键语义，以及两个索引的完整定义；同名但键、列序、唯一性或 partial 条件近似的索引不能再绕过预检。

迁移定向测试为 15 passed、0 failed；项目 Windows Rust 全量脚本为 3 个测试可执行文件、340 passed、0 failed、3 ignored。未运行无关 Node 测试。

## 二、授权范围与实际修改

仅修改：

- `src-tauri/src/db/migration_safety.rs`
- `src-tauri/src/db/migration_lineage_tests.rs`
- 本报告与工作流状态记录

未修改 `src-tauri/migrations/0063_device_sync_quarantine_lifecycle.sql`、S1 device-sync 产品实现、前端、依赖或发布文件；未 commit、未 push。共享工作树中的其他 S1 变更由对应线程所有，本线程未编辑或恢复。

本轮没有独立的 `.agent-work/threads/worker-s1-mig-r2/task.md`；任务范围与验收标准以主控下发的 `V083-S1-MIG-R2` 任务正文为准，逻辑线程沿用 `worker-s1-mig`。

## 三、生产迁移安全门禁

当 `_sqlx_migrations` 声称 63 已应用时，预检新增或收紧以下契约：

- `device_sync_quarantine.source_device_id`：`TEXT NOT NULL`，无默认值；
- `device_sync_quarantine.source_sequence`：`INTEGER NOT NULL`，无默认值；
- `status`：`TEXT NOT NULL DEFAULT 'active'`，闭集为 `active/resolved/manual_review`；
- `retry_count`：`INTEGER NOT NULL DEFAULT 1`，并要求 `retry_count >= 1`；
- `last_error_code`：`TEXT NOT NULL`，无默认值；
- `device_sync_groups.auto_paused`：`INTEGER NOT NULL DEFAULT 0`；
- quarantine 表定义按规范化后的完整 DDL 精确比对，覆盖字段、默认值、CHECK 与外键；
- `group_id -> device_sync_groups(id) ON DELETE SET NULL` 按 58/63 实际版本动态核验；
- 活动隔离唯一索引必须精确为 `(COALESCE(group_id,''), source_device_id, source_sequence, reason_code) WHERE status='active'`，且必须同时满足 unique 与 partial；
- 分组状态索引必须精确为 `(group_id, status, last_seen_at DESC)`，且不得是 unique 或 partial。

任一契约缺失或被近似定义替换，均在建立生产读写/WAL 连接前返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`。

## 四、新增反例与迁移语义测试

新增/更新的关键测试：

1. 同名 active 索引使用旧式 `(COALESCE(group_id,''), COALESCE(source_path,''), reason_code)` 键时，预检必须失败且数据库物理字节与逻辑指纹不变；
2. 同名 group-status 索引将列序改为 `(status, group_id, last_seen_at DESC)` 时，预检必须失败且数据库物理字节与逻辑指纹不变；
3. 直接执行真实 0063 SQL，验证 legacy quarantine 两条重复记录按 1:1 保留为 `manual_review`，使用 `__legacy__/-1` 哨兵而不虚构来源身份；
4. 验证旧时间、路径、原因、详情、创建时间被保留，retry/status 默认与 CHECK 生效；
5. 验证缺失来源设备/序列写入失败、active 精确身份重复失败、manual_review 历史记录可与 active 身份共存；
6. 验证删除 group 后 quarantine.group_id 按外键置空，`foreign_key_check` 无异常。

迁移集合仍按完整嵌入向量比对：62 条迁移、最大版本 63、合法历史缺号 36；没有引入“版本必须连续”的错误假设。

## 五、验收结果

### 定向迁移测试

对实际 `caseboard_lib` 测试二进制嵌入项目要求的 Windows Common-Controls manifest 后执行：

```text
running 15 tests
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 288 filtered out; finished in 6.59s
```

### Windows Rust 全量门禁

执行：

```powershell
powershell.exe -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
```

最终结果：

```text
caseboard_lib:          300 passed; 0 failed; 3 ignored
caseboard:                0 passed; 0 failed; 0 ignored
device_sync_contract:    40 passed; 0 failed; 0 ignored
[ok] Windows Rust tests passed: 3 executables
```

首次启动脚本时，新 PowerShell 进程未找到 Cargo；第二次直接调用脚本受到本机执行策略阻止，两次均未进入编译或测试。随后仅显式补入现有 `C:\Users\William Feng\.cargo\bin` 到子进程 PATH，并继续使用原脚本，最终 232.6 秒通过。未修改测试脚本或项目配置。

### 静态复核

- 两个授权 Rust 文件逐文件 `rustfmt --check --config skip_children=true`：通过；
- 两个授权 Rust 文件 `git diff --check`：通过，仅有 Windows LF/CRLF 提示；
- 迁移集合：62 条、max 63、gap 36；
- `migration_lineage_tests.rs`：15 个测试属性；
- 文件行数：`migration_safety.rs` 977 行，`migration_lineage_tests.rs` 866 行。

## 六、复审重点

- 复核完整 DDL 精确比对是否与“0063 已应用即必须具备当前结构”的 fail-closed 策略一致；
- 复核 active 唯一键已完全移除 `source_path` 身份语义，并以 `source_device_id + source_sequence` 为核心；
- 复核 legacy 行只进入 `manual_review`，没有被错误激活或合并；
- 后续若 0063 继续扩展 durable export draft、outbox 单调顺序或 legacy normalization，应以扩展后的最终 0063 重新更新同一组 sentinel 与迁移语义测试；本报告仅证明当前阶段基线。
