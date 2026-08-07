# V083-N0-GATE｜独立门禁、覆盖与冲突审计

日期：2026-08-07
任务状态：报告已完成并由主控验收为 `accepted`
审计方式：只读核对计划、任务包、现场交接、现有源码/迁移/测试脚本；未运行生产构建，未读取正式数据库、NAS 事件、飞书 Base、凭据或业务正文。

## 一、结论先行

N0 只有在 MIG、SYNC 两组可执行证据均通过且本报告的综合门禁逐项闭合后，才可关闭。当前最重要的验收原则是：

1. “证明现状缺陷”必须是可执行断言，不接受只在 Markdown 中描述；
2. N0 不改变生产行为，因此当前缺陷可以用“测试通过但断言确认了不安全现状”证明，或用明确标注的 red/ignored 契约测试证明；两种模式都必须报告实际执行方式、退出码和跳过数，不能把未运行当成已证明；
3. 笔记本历史 checksum 尚未取得时可以完成 N0，但不得将合成值称为“已知旧 checksum”，也不得进入 M1 白名单兼容实现；
4. MIG 与 SYNC 当前写入范围没有直接重叠；共享 Cargo `target` 和全量构建必须由主控串行使用，避免并行编译/清单嵌入互相干扰；
5. 最优后续顺序仍为：N0 三任务验收 → M1 → S1 → F1 → RC。M1 与 S1 不应并行，因为 M1 先冻结数据库预检/错误结构，S1 又需要新增 0063 和复用稳定错误语义。

## 二、MIG 门禁矩阵

| 类别 | 必须证据 | 可接受跳过/延期 | 拒绝条件 |
| --- | --- | --- | --- |
| 构造库安全 | 所有数据库均为临时构造库或测试内存库；路径、创建/销毁方式可审计；不查询业务字段值 | 无 | 读取正式库、笔记本唯一库或业务正文；把正式库副本提交进仓库 |
| 谱系覆盖 | 可执行覆盖：全新库、当前 1—62 正常库、checksum 不一致、49 标记成功但缺 `feishu_sync_inbox`、同编号不同 SQL、未知已应用版本、`success=0` | “已知旧 checksum 白名单成功路径”可延期，前提是历史真实 checksum 尚未取得并明确标记 `pending_verified_input` | 只覆盖任务包简写的五类而遗漏全新库/当前 62/同编号不同 SQL；用随机值冒充已知旧 checksum |
| 现状缺陷证明 | 未修复实现下可执行证明：任意 checksum 会被改写；未知已应用版本被 `set_ignore_missing(true)` 接受；sentinel 缺失会进入后续迁移失败或出现不兼容结果；失败迁移不可误判为兼容 | 若 desired-contract red test 采用 `#[ignore]`，允许正常全量套件跳过，但必须单独显式运行并记录其预期非零结果；更推荐用通过的 characterization test 断言不安全现状 | 测试因关闭约束、删除 `_sqlx_migrations`、放宽断言或根本未调用相关路径而“通过”；仅报告源码观察 |
| checksum 来源 | 重新计算当前 47—52 SHA-384，与冻结任务包逐项一致；历史 checksum 的来源只允许只读 SQLite 元数据（version/description/success/checksum），不输出业务内容 | 笔记本历史 checksum 本轮不可得 | 编造、猜测或把当前 checksum 复制成旧白名单；来源无法追溯 |
| schema sentinel | 结构化检查 49、51、58、59、60、61、62 的关键表、列、外键、索引/触发器；至少验证一个缺失 sentinel 能被确定性识别；只使用 `sqlite_master`、`PRAGMA table_info/foreign_key_list/index_list` 等结构元数据 | 不要求 N0 实现生产预检；只冻结清单与夹具 | 只检查表存在而不检查关键列/外键/触发器；查询业务值；把迁移执行成功等同于 sentinel 通过 |
| 稳定错误契约 | DB 四码完整记录：触发条件、可重试、自动暂停、用户文案、日志字段、禁止展示数据；程序分类不得依赖中文文本 | N0 不要求生产 `DbError` 接线或原生对话框实现 | 仅列错误码名称；未知谱系仍建议继续迁移；错误日志包含数据库业务正文/SQL 参数 |
| 不改变生产行为 | diff 仅位于 `src-tauri/src/db/` 的 `#[cfg(test)]`/纯测试辅助、`scripts/windows-upgrade-validation/` 纯构造夹具及本任务交付物；生产函数体、迁移、版本和发布配置零变化 | 测试可引用现有私有函数，但编译后不能进入非测试产物 | 修改 `reconcile_migration_checksums()`、`init_pool()`、`DbError` 生产枚举、迁移 0063/0064、Cargo/Node 依赖或发布配置 |

### MIG 复验命令

先按 worker 报告运行其最窄过滤命令，再执行主控门禁。Windows 上直接运行 Cargo 测试若出现既知的入口点错误，不能计作测试失败或通过，必须使用仓库清单嵌入脚本复验。

```powershell
git diff --check
git diff --name-status -- src-tauri/src/db scripts/windows-upgrade-validation .agent-work/output/V083-N0-MIG.md
git diff -- src-tauri/src/db scripts/windows-upgrade-validation

$env:PATH='C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL='0'
cargo check --lib -j 1
cargo clippy --lib -j 1 -- -D warnings
powershell.exe -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
```

若 worker 采用独立脚本生成构造库，还需对每个情形执行脚本的 `--help`/最小测试入口，并核对输出只含结构和迁移元数据。不得连接用户数据目录。

### M1 必须取得的前置输入

- MIG 夹具及测试名、现状断言和预期 M1 后断言；
- 当前 47—52 SHA-384 的重新计算结果；
- 49/51/58—62 sentinel 清单；
- DB 四个稳定错误码契约；
- 笔记本真实旧 checksum。若仍缺失，M1 可先实现 fail-closed 分类，但不得实现该历史谱系的兼容白名单；
- 对不兼容库执行前后 `_sqlx_migrations`、schema 与业务表指纹不变的验收方法。

## 三、SYNC 门禁矩阵

| 类别 | 必须证据 | 可接受跳过/延期 | 拒绝条件 |
| --- | --- | --- | --- |
| 合成数据边界 | 全部使用合成 UUID、合成文本、临时 SQLite 与临时同步目录；不解密现场 `.cbe` | 精确到现场具体操作的解密确认可延期，根因继续标记“高置信度推断” | 读取正式事件载荷、NAS 当前失败组、成员密钥或案件正文 |
| 循环外键 | 空接收端，case 先于 contact，case 有非空 `judge_id`；contact 反向引用 case；外键保持开启；测试确定性复现 code 787/等价 FK 失败 | 两阶段导入后的成功测试留到 S1 实现，但 N0 必须冻结预期 | 通过 `PRAGMA foreign_keys=OFF`、去掉 `judge_id`、先预置引用对象或删除失败操作绕过 |
| 原子回滚 | 同一事件内在失败点前至少有一个本可成功写入的操作；失败后业务表、applied operations、revision/receipt 等关键状态无部分提交 | 后续成功补写语义延期到 S1 | 只检查最终错误文本，不检查零部分写入；失败后推进 sequence/receipt |
| 隔离重复 | 同一 `group_id + source_path + reason_code` 重复调度，确定性证明当前会产生重复 active 记录或等价现状缺陷 | lifecycle 去重、`retry_count`、resolved 状态与自动暂停留到 0063/S1 | 用不同 source/reason 冒充同包重复；删除历史隔离换取“0” |
| 分包边界 | 至少覆盖 500、501、1001（或其他明确 >1000）实体；构造依赖两端恰好跨 500 边界；断言当前按时间/500 limit 分包会拆散依赖或接收端失败 | 依赖闭包成功分包留到 S1 | 只证明有 501 条，不证明引用跨界；所有依赖都恰好在同包；用提高 limit 掩盖 |
| 审计语义 | 导入存在隔离时，确定性证明当前 audit 仍为 `succeeded` 或成功时间被错误推进；冻结未来必须为失败/暂停且最近成功时间不推进 | UI 文案、自动暂停和恢复重放实现留到 S1 | 只断言 `quarantined > 0`，不核对 audit/result/success time |
| 稳定错误契约 | SYNC 三码记录触发条件、可重试、自动暂停、用户文案、日志字段、禁止展示数据；确定性错误首次隔离后应自动暂停，用户明确恢复 | N0 不要求生产错误枚举/数据库生命周期列 | 依赖缺失无限重试；隔离存在仍标记 succeeded；日志泄露 payload/密钥 |
| 不改变生产行为 | diff 仅位于 `src-tauri/src/device_sync/` 的 `#[cfg(test)]`/纯测试辅助、`src-tauri/tests/` 专项测试及本任务交付物；导入/导出/隔离/审计生产路径零变化 | 可新增独立 integration test 文件；Cargo 默认发现时不应修改 manifest | 修改生产分包上限、导入顺序、quarantine SQL、migration、Cargo.lock/Cargo.toml 或发布配置 |

### SYNC 复验命令

```powershell
git diff --check
git diff --name-status -- src-tauri/src/device_sync src-tauri/tests .agent-work/output/V083-N0-SYNC.md
git diff -- src-tauri/src/device_sync src-tauri/tests

$env:PATH='C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL='0'
cargo check --lib -j 1
cargo clippy --lib -j 1 -- -D warnings
powershell.exe -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
```

清单脚本无过滤参数，会发现并运行所有 Cargo test executable；若新增独立 integration test，最终 executable 数会变化，应报告实际数量，不能机械沿用基线“3 个”。

### S1 必须取得的前置输入

- 循环 FK、零部分写入、重复隔离、500/501/>1000、audit succeeded 的可执行夹具；
- 实体依赖图和安全延后字段清单，至少明确 `case.judge_id → contact.id` 与 `contact.case_id → case.id`；
- 依赖闭包分包不变量和无法安全分包时“写 NAS 前失败”的断言；
- 0063 生命周期 schema 契约：active/resolved、first/last seen、retry_count、resolved_at、last_error_code 及活动隔离唯一性；
- `SYNC_PACKAGE_DEPENDENCY_MISSING`、`SYNC_PACKAGE_QUARANTINED`、`SYNC_GROUP_AUTO_PAUSED` 契约；
- “最近尝试”与“最近业务成功”分离的审计/界面断言。

## 四、综合门禁矩阵

| 门禁 | 必须通过 | N0 可接受跳过 | 拒绝条件 |
| --- | --- | --- | --- |
| 范围 | MIG/SYNC 仅改各自测试范围和独立报告；GATE 仅改本线程与本报告 | 无 | 产品行为、迁移、版本、依赖、正式数据变化；跨线程覆盖 |
| 八码契约 | DB 四码 + SYNC 三码 + `FEISHU_ORPHAN_BINDING` 均具备六项字段：触发条件、可重试、自动暂停、用户文案、日志字段、禁止展示 | F1 生产接线可延期；但 N0 书面契约不可缺 | 只有七码；飞书码完全遗漏；按中文错误文本分类 |
| 可执行性 | 核心缺陷都有执行证据；报告列测试名、命令、退出码、通过/失败/跳过数 | 明确的 red/ignored 契约测试允许跳过正常套件，但必须单独运行证明且解释 M1/S1 如何转绿 | 只写报告；未执行；把 skipped 当 passed；未报告预期失败 |
| 质量 | `git diff --check`、Node logic、Vite build、Cargo check、Clippy、Windows Rust 清单测试均由主控串行通过 | `validate:release`、签名安装包、updater、真实双设备/NAS、飞书测试 Base 均属于 RC/F1，N0 不执行 | 任一全量门禁失败且未归因解决；并发构建造成结果不可信 |
| 数据安全 | 明确声明未读写正式数据库、NAS、同步组、飞书、凭据；测试临时资源可追溯清理 | 现场历史 checksum 只读提取可单独授权后补充 | 用唯一正式数据做夹具；输出业务正文/密钥；删除历史事件/隔离 |
| Git 证据 | 主控记录最终 `git status --short`、`git diff --stat`、`git diff --check`、HEAD；逐文件确认无越权 | Worker 不提交 Git符合任务要求 | Worker 私自 commit；未区分其他 Agent 并行修改；reset/checkout 覆盖 |

### 综合复验命令（主控串行）

```powershell
git status --short
git diff --stat
git diff --check

pnpm test:logic
pnpm build

$env:PATH='C:\Users\William Feng\.cargo\bin;' + $env:PATH
$env:CARGO_INCREMENTAL='0'
cargo check --lib -j 1
cargo clippy --lib -j 1 -- -D warnings
powershell.exe -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1

pnpm validate:source
```

说明：`pnpm validate:source` 只验证源码版本一致性、Cargo workspace、许可证/NOTICE 和 changelog 等，不替代测试、迁移或数据库完整性门禁。`pnpm validate:release` 需要 tag、隔离产物目录、base URL 和 draft output，N0 没有 0.8.3 发布资产，跳过是正确的；RC 必须按下式真实执行：

```powershell
node scripts/release-gate.mjs --mode release `
  --tag v0.8.3-fanglv `
  --artifact-dir <isolated-artifact-dir> `
  --base-url <release-base-url> `
  --draft-output <isolated-draft-path>
```

## 五、跨任务冲突与重叠文件

### 当前 N0 并行风险

| 风险 | 判断 | 控制措施 |
| --- | --- | --- |
| MIG 与 SYNC 源文件直接重叠 | 当前无：MIG 限 `db/` 与 upgrade fixture；SYNC 限 `device_sync/` 与 `src-tauri/tests/` | 主控按 `git diff --name-status` 复核；出现 Cargo/迁移/公共脚本修改立即驳回 |
| Cargo `target` 并发 | 高风险：即便源码不重叠，并行 Cargo 会争用构建缓存；Windows 脚本还会对测试 executable 嵌入 manifest | Worker 不跑全量 Cargo/生产构建；主控统一串行，当前进程 `CARGO_INCREMENTAL=0` |
| `scripts/run-windows-rust-tests.ps1` | 公共发布门禁，N0 worker 均无权修改 | 仅主控执行；新 integration test 由现有自动发现逻辑覆盖 |
| `src-tauri/Cargo.toml`/`Cargo.lock` | 两任务均不需要修改 | 任一 diff 直接暂停审查并要求解释；不得为测试引入依赖 |
| 迁移 0063/0064 | N0 明确禁止创建 | 0063 只在 S1、0064 只在 F1 且确需 schema 时由单一任务创建；先核对最新最大迁移 |
| 公共错误码 | 契约跨 MIG/SYNC/F1，最容易产生语义漂移 | N0 先合并一份八码矩阵；M1 先实现 DB 四码，S1 再实现 SYNC 三码，F1 最后实现飞书码；每阶段复核名称不变 |
| 报告/线程状态 | 文件物理分离，但 workflow 脚本无文件锁且会更新公共状态/看板 | start/submit 错峰执行；主控每次运行 workflow audit，并以本地文件状态为准 |

### 后续阶段潜在重叠

- M1 会修改 `src-tauri/src/db/mod.rs`、错误结构和 Tauri setup；必须在 MIG characterization tests 冻结并 accepted 后串行进行。
- S1 会修改 `src-tauri/src/device_sync/engine.rs`、可能新增 `0063_device_sync_quarantine_lifecycle.sql`；必须在 M1 的结构化错误/启动边界 accepted 后再开始。
- F1 可能修改 `src-tauri/src/db/feishu_sync.rs`、案件删除事务、前端提示，必要时才创建 0064；不得与 S1 同时占用公共迁移编号或错误码汇总。
- RC 会使用 `scripts/run-windows-rust-tests.ps1`、`scripts/release-gate.mjs`、Windows 升级脚本和发布资产；此前阶段不得顺手修改发布门禁，确需修改应单独任务、单独验收。

## 六、最优后续顺序与阶段停点

1. **先审 MIG**：确认七类谱系、sentinel、当前 checksum 和 DB 四码；若真实旧 checksum 未取得，只允许 fail-closed 设计，不允许白名单兼容。
2. **再审 SYNC**：确认循环 FK、原子回滚、重复隔离、500/501/>1000 和审计状态均为可执行证据，且全程未关闭 FK。
3. **最后按本报告做 N0 综合复验**：检查八码契约、越权 diff、全量串行门禁、跳过项和安全声明。三任务由主控分别 accepted 后才能关闭 N0。
4. **M1 单独实施并验收**：先做只读预检/分类和写前失败关闭，再做原生恢复提示；取得真实旧 checksum 后才添加窄白名单。M1 未 accepted 不建 0063。
5. **S1 单独实施并验收**：先依赖分析/两阶段事务，再依赖闭包分包，再 0063 生命周期/熔断，最后 UI/audit 成功时间语义；顺序可减少将 schema 与导入算法同时调试的风险。
6. **F1 单独实施并验收**：只做本地孤立绑定归档、隔离和稳定错误提示；网络写入断言必须为 0。
7. **RC**：全新库、0.8.2 正常库副本、不兼容谱系库、正式库在线一致性副本、隔离同步目录、双机两轮收敛、安装升级和签名 updater 全部通过后才发布；不在唯一正式库或当前 NAS 失败组试验。

阶段硬停点：任何核心夹具缺失、未知谱系发生写入、同步失败有部分提交、隔离仍报告 succeeded、外键被关闭、历史 checksum 无来源、越权修改迁移/发布配置或全量门禁不绿，均不得进入下一阶段。

## 七、审计期间观察到的计划/脚本注意事项

1. `run-windows-rust-tests.ps1` 会自动发现所有 Cargo test executable、嵌入 Windows Common-Controls manifest 并逐个运行；新增独立 integration test 后 executable 数可能不再是基线 3，报告必须使用实测数。
2. `release-gate.mjs --mode source` 不执行 Node/Rust 测试、数据库迁移、`quick_check` 或 `foreign_key_check`；只能作为综合门禁的一项，不能单独证明可发布。
3. `release-gate.mjs --mode release` 验证单个 NSIS 安装包、单个 `.sig` 并生成 updater draft，但代码注释已经明确 minisign 不是 Windows Authenticode；RC 报告必须继续如实区分两类签名。
4. N0 总任务包的“所有失败夹具稳定失败”与“N0 全量测试必须通过”存在表述张力。主控验收时必须要求 worker 说明其采用 characterization-pass 还是 desired-contract-red/ignored 模式；否则测试计数不可解释。
5. 当前现场循环 FK 根因仍是基于外键、导入顺序和 SQLite 787 的高置信度推断；N0 合成夹具能证明同形状缺陷，但不能谎称已经解密确认现场具体操作。

## 八、待主控收件后补核

- MIG/SYNC 实际修改文件是否完全落在授权范围；
- 两份 worker 报告是否列出测试名、命令、退出码、计数、跳过与遗留风险；
- 八码契约是否完整（特别是容易遗漏的 `FEISHU_ORPHAN_BINDING`）；
- 是否出现 Cargo/迁移/发布脚本/公共配置的越权 diff；
- workflow `submitted_for_review` 仅表示待审，不等于 accepted。
