# V083-RC-DBSYNC-GATE｜数据库升级与双端同步 RC 夹具只读盘点

- 逻辑线程：`worker-rc-dbsync-gate`
- 审计方式：源码、脚本、测试与阶段报告静态审计
- 本轮状态：`submitted_for_review`
- 安全边界：未修改源码/测试/迁移，未运行 Cargo 或构建，未访问正式数据库、NAS、同步组或凭据

## 一、结论

当前夹具可以作为 RC 的基础，但尚未满足 `32_rc_dispatch_plan.md` 与 `33_rc_acceptance_rubric.md` 的全部本地门禁。最小阻断缺口有四项：

1. 没有“仅应用到 0062 的真实 0.8.2 正常形状 → 当前 0063”的独立升级夹具；现有 `current_database_reopen_keeps_all_fingerprints_unchanged` 是当前最大版本 63 的重开，不是 0.8.2 升级。
2. 没有“已知历史 checksum → 窄兼容 → 正常升级”的正向夹具；M1 最终实现已删除 allowlist 和所有 checksum 改写能力，任何 mismatch 都返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`。真实旧 checksum 仍为 `pending_verified_input`，所以该项不只是缺测试，当前生产能力也未实现。
3. 现有双池生产测试只证明 A→NAS→B、冲突生成和隔离 restore preview；没有完整证明“B→NAS→A”的第二轮收敛、无变更重复同步幂等，以及两端最终业务投影一致、pending/conflict/active quarantine 为 0、两端 quick/FK 均健康。
4. 隔离生命周期已有强单点测试，但“真实 `sync_once` 产生隔离 → 用户恢复 → 修复包重放 → 同一双端环境再次收敛”的完整 RC 编排尚不存在。现有恢复测试使用部分 `*_for_test` helper，不能单独替代 RC 生产入口证明。

按当前验收量表，以上任一项缺失都属于 P1；因此 RC-LOCAL 在补齐前不能标记本地接受。历史 checksum 若仍无来源核验输入，必须保持 `blocked_external/pending_verified_input`，或由主控/用户明确修改 RC 验收范围；不能用当前 checksum 相等的正常重开冒充历史兼容。

## 二、数据库夹具覆盖矩阵

| RC 要求 | 现有证据 | 结论 | 最小缺口 |
| --- | --- | --- | --- |
| 全新库 0→当前 | `fresh_database_reaches_current_lineage_and_all_frozen_sentinels`：不存在库和预先存在的空 SQLite 文件均到 62 条迁移、最大 63，并重开走全部 sentinel | 基本覆盖 | 在 RC 专项断言中补 `PRAGMA quick_check='ok'` 与空 `foreign_key_check`，记录最终版本/条数 |
| 0.8.2 正常形状升级 | 无；`current_database_reopen...` 的 fixture 已执行全部当前迁移 | 未覆盖，P1 | 新建只执行仓库 0001—0062 的临时 fixture，写少量合成业务标记，再调用生产 `init_pool()` 升到 0063；比较标记、迁移行，断言 quick/FK |
| 已知 checksum 兼容 | 仅有 `unknown_checksum_fails_closed...`；生产代码 `checksum_not_allowlisted` 对任何 mismatch 失败 | 未覆盖且生产能力不存在，P1 | 先取得来源核验的真实旧 checksum；另建经审计的 `M1-COMPAT` 窄规则，再增加正向兼容与 sentinel/lookalike/错误 checksum 反例。不得猜值或只改测试 |
| 不兼容谱系 | 21 项 migration lineage/sentinel 测试覆盖未知 checksum、未知版本、失败行、缺/空历史、49/63 sentinel、组合优先级、DB/WAL/SHM 连接前阻断；失败前后比较迁移/schema/业务与物理字节 | 强覆盖 | RC 脚本集中运行并输出按错误码汇总；无需复制二进制业务库 |
| 失败前后指纹 | `physical_fingerprint`、`database_fingerprint`、`existing_schema_fingerprint` 已覆盖 DB/WAL/SHM 与逻辑内容 | 已覆盖 | RC 汇总保留每类 fixture 名、错误码和 unchanged 结果 |
| Windows 启动/升级自动化 | `run-windows-rust-tests.ps1` 会编译全部测试、嵌入 Common Controls manifest 并运行 3 个测试 executable | 部分覆盖 | 没有 RC 数据库专项入口/结构化汇总，也没有 0.8.2/兼容正向 fixture；原生 setup 对话框仍需单独视觉/退出码验证 |

### 0.8.2 fixture 的安全构造建议

- 只在 `TempDir` 内生成，不提交含业务数据的 SQLite 二进制。
- 从仓库迁移目录复制/加载版本 `<=62` 的 SQL，由 sqlx migrator 真实执行，不能先建当前库再手工删除 0063 对象。
- 插入固定、脱敏的合成标记，关闭连接并确认无 sidecar 后，采集迁移/schema/业务指纹。
- 调用生产 `db::init_pool()` 完成 0063，断言旧标记保留、0063 仅出现一次、62 条成功迁移/max 63、`quick_check=ok`、FK 空；第二次调用应无变化。
- 若要声称“发布版 0.8.2 形状”，还应把 0001—0062 文件哈希与发布基线/tag 核对；当前工作树内同名 SQL 只能证明“当前仓库的 pre-0063 形状”。

### checksum 兼容的必要前置

M1 已刻意移除 `MIGRATION_COMPATIBILITY_ALLOWLIST`、CAS 更新计划和 `_sqlx_migrations` checksum 写路径。RC 若仍要求自动兼容，必须先取得以下最小、来源可追溯输入：

- 只读一致性副本中的 `_sqlx_migrations`：`version/description/success/hex(checksum)`；
- 对应发布版本或安装包的迁移 SQL 哈希；
- 该版本对应的完整 sentinel 结果；
- 副本来源、采集时间和文件 SHA-256。

之后需另开实现/复审：兼容规则必须绑定“版本号 + 已核验旧 checksum + 当前 checksum + 完整 sentinel”，并在单一事务内复验后才能做最小更新。来源值未取得前，只能验证 fail-closed，不能完成验收量表中的正向兼容项。

## 三、设备同步夹具覆盖矩阵

| RC 要求 | 现有证据 | 结论 | 最小缺口 |
| --- | --- | --- | --- |
| 两个临时端 + 临时 NAS | Windows test `pairing_two_pool_sync_conflict_revocation_and_isolated_restore_contract` 使用两个内存库和 `tempdir` NAS，经过真实配对与 `engine::sync_once` | 部分覆盖 | 改为两个临时文件库，便于重启/文件指纹/quick/FK；异常也要自动清理测试凭据 |
| 第一轮 A→NAS→B | 上述测试创建案件，A `sync_once` 后 B `sync_once` 并验证案件 | 已覆盖 | RC 新夹具保留生产入口证明 |
| 第二轮 B→NAS→A | 上述测试后续只验证冲突在 B 生成，未让 B 的独立变更回传并在 A 收敛 | 未覆盖，P1 | B 写不同实体/字段后 `B sync_once -> A sync_once`，断言双方最终一致 |
| 重复幂等 | `different_field_remote_change_merges_and_duplicate_is_idempotent` 覆盖重复 incoming package；多项 durable export 测试覆盖草稿/文件重放 | 局部覆盖 | 在双端已收敛后再各执行一次无变更 `sync_once`，断言业务/序列/manifest/outbox/审计不产生错误推进 |
| 失败整包回滚 | `package_midway_failure_rolls_back_all_sync_and_business_rows` 等 | 已覆盖 | 纳入 RC 专项过滤与汇总即可 |
| 隔离和恢复 | `auto_pause_retry_resume_replay_resolves_and_records_only_real_success` 覆盖暂停、retry_count、明确恢复、重放、resolved 历史、quick/FK；`real_sync_once_quarantines...` 单独覆盖真实 `sync_once` 隔离；pairing 测试覆盖 `prepare_isolated_restore` 不改正式库 | 分段覆盖 | 增加同一临时双端环境的生产入口组合：真实坏包触发 B 隔离/暂停，A 不受污染；修复后显式 resume，真实 `sync_once` 重放并 resolved，随后两端重新收敛 |
| 收敛终态 | 主计划要求冲突 0、active quarantine 0、FK 0、quick ok | 未在双端综合测试中断言 | 两端都断言 canonical 业务投影相等、pending outbox 0、pending conflict 0、active quarantine 0、quick/FK 健康 |

### 建议新增的最小 RC 双端测试

建议在 `src-tauri/tests/device_sync_contract.rs` 新增一个明确前缀的 Windows integration test，例如：

`rc_dbsync_two_file_endpoints_converge_twice_repeat_idempotently_and_recover_quarantine`

测试只使用两个 `TempDir` 文件数据库、一个新 `TempDir` mounted-folder、固定合成业务数据和随机测试组：

1. 建组、配对 A/B；
2. A 写记录，`A sync_once → B sync_once`；
3. B 写不同的非冲突记录/字段，`B sync_once → A sync_once`；
4. 双方各再执行一次无变更同步，记录 sequence/manifest/outbox/业务指纹不变；
5. 向新临时目录放入确定性坏包，让 B 的真实 `sync_once` 隔离并 auto-pause，确认 A 与两端业务指纹未被污染；
6. 用测试控制点修复同一序列包，显式 resume 后再次走真实 `sync_once`，确认活动隔离转为 resolved；
7. 再执行 A/B 收敛轮，最终比较双方允许同步实体的 canonical 投影，且两端 `quick_check=ok`、FK 空、pending outbox/conflict 和 active quarantine 均为 0。

现有配对测试会写入 Windows Credential Manager 的随机测试记录，并仅在函数末尾显式删除；中途 panic 可能留下条目。RC 新夹具至少应使用 RAII cleanup guard，在成功、失败和 unwind 时按随机 group/device 精确删除；更稳妥的是为测试注入内存/临时凭据后端。不得读取、枚举或覆盖现有正式凭据。

## 四、现有脚本与可复用命令

### 可直接复用

Windows Rust 测试必须继续使用仓库清单脚本，不能用裸 `cargo test` 的成功编译代替实际执行：

```powershell
$env:PATH = 'C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL = '0'
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
```

该脚本已经具备：`cargo test --workspace --locked --no-run --message-format=json`、自动发现测试 executable、嵌入 Windows manifest、逐 executable 实际运行、任一失败即停止。新增 Rust fixture 后会自动进入全量门禁。

RC-LOCAL 仍可复用以下仓库门禁：

```powershell
pnpm test:logic
pnpm build
cargo check --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
pnpm validate:source
git diff --check
```

### 建议的最小脚本改动

不建议复制 `run-windows-rust-tests.ps1` 的 manifest 嵌入逻辑。最小方案是在现有脚本增加可选的测试过滤参数（默认空，保持全量行为），让每个 executable 接收 `rc_dbsync_ --test-threads=1 --nocapture`；RC 专项脚本只做薄封装、串行调用并保存退出码/计数：

- `scripts/run-windows-rc-dbsync-tests.ps1`
  - 检查工作目录和 Cargo 路径，不接受数据库/NAS/凭据参数；
  - 仅调用现有 Windows manifest runner 的 `rc_dbsync_` 过滤模式；
  - 输出每项 fixture 的 `passed/failed/blocked_external/not_run`；
  - 输出迁移集合/测试日志 SHA-256，不保留临时数据库或密钥；
  - 用 `finally` 清理 TempDir 与测试凭据，清理失败须使门禁失败。

在未实现过滤参数前，可直接运行现有全量脚本；不要根据动态 hash 手工寻找并执行 `target\debug\deps\*.exe`，也不要重复维护第二套 manifest 代码。

## 五、正式验证所需最小外部资源

本地夹具全部通过后，正式项仍只能按量表标记 `blocked_external`，直到用户明确提供/授权以下最小资源：

### 数据库与 checksum

1. 一份用户确认的正式台式机数据库在线一致性副本，带采集时间与 SHA-256；如果源存在 WAL/SHM，必须将 DB/WAL/SHM 原样成组备份，绝不能只复制主库或删除 sidecar。
2. 如需兼容笔记本历史 checksum，再提供一份故障库的一致性只读副本，或由可信本地脚本导出的 `_sqlx_migrations` 元数据和 schema sentinel 清单；不得把业务正文带入报告。
3. 至少再复制一份可丢弃的恢复工作副本；checkpoint、迁移、启动和升级只对该副本执行，原一致性副本保持只读封存。

### 物理双端

1. 两台用户确认的 Windows 物理设备，分别具备可回滚的 v0.8.2 基线和待测 v0.8.3 候选；本地模拟不得冒充该项。
2. 一个全新、空的隔离同步目录，放在用户确认的 NAS/共享存储位置；不得沿用当前失败组或当前事件目录。
3. 一个全新测试同步组和两端测试身份；正式组继续暂停。测试前分别备份两端数据库、同步凭据和组元数据，备份恢复路径需先验证。
4. 一组不含真实案件正文的可辨识测试记录，以及允许执行两轮“台式机→NAS→笔记本→NAS→台式机”和重复幂等的维护窗口。
5. 用户对“创建隔离目录、创建测试组、在两端候选副本写入合成记录、结束后保留或清理测试资源”的明确授权。授权不包含正式库、当前组、正式 NAS 目录或凭据的改写。

正式验证最低断言：两端版本/迁移一致；两轮后 canonical 投影相同；重复同步无额外业务变化；pending outbox/conflict/active quarantine 均为 0；两端 quick/FK 通过；备份和回滚路径仍可用。任一项未完成，不得恢复正式设备同步或声明发布接受。

## 六、RC-LOCAL 建议执行顺序

1. 先取得或明确放弃历史 checksum 正向兼容输入；如仍按 33 量表验收，则必须先完成 `M1-COMPAT`。
2. 新增 0.8.2→0063 成功夹具和兼容 checksum 正向/反向夹具。
3. 新增双文件端、两轮收敛、重复幂等、真实隔离恢复综合测试，并补凭据自动清理保护。
4. 通过可选过滤的 Windows manifest runner 串行跑 RC 专项，再运行完整 Windows Rust 全量。
5. 运行 Node、Vite、check、Clippy、source gate 和 diff/scope 审计。
6. 逐项写 `passed/blocked_external/not_run`；只有本地全部通过后，才向用户申请正式一致性副本、隔离 NAS 目录和物理两端窗口。

## 七、本轮未运行与安全声明

- 未运行 Cargo、Rust test executable、Node、Vite、应用或发布脚本；本文所有覆盖结论均来自静态源码与既有经主控记录的阶段报告。
- 未创建、打开或复制任何正式/默认路径数据库；未读取 DB/WAL/SHM、案件正文或迁移元数据实值。
- 未访问 NAS、同步组、Windows Credential Manager、飞书或任何 API/OAuth 凭据。
- 未修改源码、测试、迁移、依赖、版本或发布配置；仅新增本报告并提交工作流复审记录。
- 本报告只负责盘点，不把历史报告中的测试计数冒充本轮实跑，不写 `accepted`。
