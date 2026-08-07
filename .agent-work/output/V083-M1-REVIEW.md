# V083-M1-REVIEW｜数据库迁移安全独立审计

日期：2026-08-07
状态：审计完成，建议主控退回修订；本报告不自行作 accepted/rejected 裁决。
范围：完整读取 M1 任务包、M1/N0-MIG/N0-GATE 报告、验收矩阵以及 `db/mod.rs`、`db/migration_safety.rs`、`db/migration_lineage_tests.rs`、`lib.rs` 四文件完整差异。仅做静态只读审计；未运行 Cargo、Node、构建或应用，未读取正式数据。

## 一、结论

建议**退回 M1 修订后复验**。当前实现已经正确移除任意 checksum 改写和 `set_ignore_missing(true)`，白名单确为空，四类错误结构、冻结 sentinel 和 setup 原生提示的主体方向正确；但存在两个直接击穿“既有未知数据库写前失败关闭”的阻断反例：

1. `_sqlx_migrations` 表存在但为空时，不再检查是否同时存在用户 schema/业务数据，会进入 RW/WAL 和 migrate；
2. 生产预检和测试 fingerprint 都只使用普通 SQLite `read_only(true)` 连接。SQLite 官方说明 WAL 模式只读连接在 sidecar 不齐时可能创建 `-wal/-shm`；现有测试又在采集物理基线前先自行打开只读连接，无法证明预检没有产生 sidecar 写入。

此外，未来 checksum allowlist 的 sentinel 绑定尚可被空集合或未知 sentinel code 绕过，RW/CAS 阶段也不复验 sentinel；该路径当前因白名单为空不可达，但应在首次发布这套兼容动作框架前收紧。

## 二、问题清单

### [P0/阻断] 空迁移历史表可携带用户 schema 绕过只读分类

- 文件/行：`src-tauri/src/db/migration_safety.rs:117-130`、`:132-195`、`:270`；进入写路径见 `src-tauri/src/db/mod.rs:232-260`。
- 触发形状：既有 SQLite 文件中存在结构正确的 `_sqlx_migrations`，但表内 0 行；同时存在 `legacy_cases` 或任意其他用户表及业务行。
- 实际路径：
  1. `object_exists(..., "_sqlx_migrations")` 为真，因此跳过 `has_user_schema_objects()`；
  2. `history` 为空；失败行、未知版本、history gap、sentinel、checksum 循环全部无结果；
  3. 返回空 `MigrationPreflightPlan`；
  4. `init_pool()` 创建 RW/WAL pool 并执行当前全部迁移。
- 影响：未知既有数据库可能在分类前后发生 DB header、WAL、schema 或迁移历史写入，直接违反 M1 核心不变量。若用户 schema 与当前迁移重名，会在已产生写路径后才报通用 migrate 错误；若不重名，当前迁移可能直接写入并把未知库包装成当前谱系。
- 为什么现有测试未发现：`existing_user_schema_without_migration_history_fails_closed_before_any_write` 只构造“完全没有迁移表”，没有构造“迁移表存在但为空”。
- 修正建议：
  1. `history.is_empty()` 时继续检查“除 `_sqlx_migrations` 及其 SQLite 内部对象外”是否存在用户 table/view/trigger/index；
  2. 只有迁移表为空且不存在其他用户对象时才允许作为空 schema 迁移；
  3. 有其他用户对象时返回 `DB_MIGRATION_LINEAGE_INCOMPATIBLE`，建议稳定 reason=`migration_history_empty_for_existing_schema`；
  4. 新增合成业务行夹具，并对 `_sqlx_migrations`、用户 schema/行、DB/WAL/SHM 做写前后指纹比较。

### [P0/阻断] 普通只读连接不能证明 WAL/SHM 零写入，现有指纹顺序会掩盖该副作用

- 文件/行：生产连接 `src-tauri/src/db/migration_safety.rs:98-115`；测试 helper `src-tauri/src/db/migration_lineage_tests.rs:81-132`、`:134-160`；失败后比较顺序 `:173-189`。
- 触发形状：既有数据库处于 WAL 模式，sidecar 处于崩溃/复制后的不完整组合，例如存在 WAL 但缺少可用 SHM，且所在目录可写。
- 依据：SQLite 官方 WAL 文档说明，只读 WAL 数据库能够打开的条件包括 `-wal/-shm` 已存在、目录允许创建这些文件，或连接使用 `immutable`；因此 `read_only(true)` 本身并不等于“不产生 sidecar”。官方还说明首个连接可能初始化/截断 SHM。参见：<https://www.sqlite.org/wal.html#readonly>、<https://www.sqlite.org/walformat.html>。
- 现有测试盲点：
  1. `database_fingerprint()` / `existing_schema_fingerprint()` 在 `before_physical` 之前先打开普通只读 SQLite 连接；若该 helper 已创建或改变 SHM，物理基线会把副作用当成初始状态；
  2. 失败后 `assert_failure_fingerprints_unchanged()` 先再次打开 SQLite 做逻辑 fingerprint，之后才采集物理指纹；后置 helper 也可能改变 sidecar；
  3. 夹具未明确构造 WAL 模式及 sidecar 缺失/遗留组合。
- 影响：报告中的“read-only pool 前后 DB/WAL/SHM 物理指纹完全不变”目前证据不足；在特定 WAL 形状下，预检本身可能在 RW pool 之前产生 sidecar 写入，违反任务明确门禁。
- 修正建议：
  1. 为预检使用经本地 sqlx/SQLite 行为验证的 immutable 只读打开方式，或采用其他不会创建/重建 sidecar 的只读机制；必须确认它仍读取所需的已提交 WAL 状态，不能简单忽略 WAL；
  2. 新增 WAL 形状夹具：至少覆盖完整 DB+WAL+SHM、缺 SHM、只读目录/文件权限以及不兼容谱系；
  3. 物理基线必须在任何 fingerprint SQLite 连接之前用文件 API 采集；调用 `init_pool()` 后也必须先采集物理结果，再打开任何验证连接；
  4. 分开证明“原 DB/sidecar 字节不变”和“逻辑迁移/schema/业务行不变”，避免验证工具污染被测状态。

### [P1] 未来 allowlist 的 sentinel 绑定可被空/未知 code 绕过，CAS 阶段不复验结构

- 文件/行：规则结构与空白名单 `src-tauri/src/db/migration_safety.rs:17-31`；规则匹配 `:196-249`；CAS 写入 `:273-315`。
- 当前安全事实：`MIGRATION_COMPATIBILITY_ALLOWLIST` 确实为空，当前版本不会执行 checksum update。
- 反例：未来加入 `required_sentinels: &[]`，或填入一个 sentinel catalog 中不存在的字符串。`missing_codes.contains(code)` 均为假，代码会把规则当作结构已满足并生成更新计划。类型字段“存在”并没有强制同时绑定有效 sentinel。
- 另一缺口：sentinel 只在独立 read-only pool 中检查；关闭该 pool、打开 RW pool 后，CAS 事务仅比较 `version + stored checksum`，不重新核验 sentinel。若两个连接之间结构被并发改变，仍可更新 checksum。
- 修正建议：
  1. 建立唯一 sentinel catalog；每条规则必须至少包含一个 sentinel，且全部 code 必须属于该版本的冻结 catalog；未知/重复/空集合直接 `LINEAGE_INCOMPATIBLE`；
  2. 规则最好绑定该版本要求的完整 sentinel 集合或 sentinel-set digest，而不是任意子集；
  3. 在 checksum CAS 的同一 RW 事务内重新核验 rule sentinels 与迁移行的 `version/checksum/success`；
  4. 更新成功日志移到整个事务 commit 成功之后，避免多规则中后项失败回滚而前项仍留下“已应用”日志。

### [P1] 组合异常的错误码优先级未冻结，checksum 会遮蔽已发现的 sentinel 缺失

- 文件/行：`src-tauri/src/db/migration_safety.rs:196-267`。
- 触发形状：同一个已应用版本同时具有未知 checksum 和缺失 sentinel。
- 实际分类：代码先收集 sentinel 缺失，但在 checksum 循环中先返回 `DB_MIGRATION_CHECKSUM_UNKNOWN`；只有所有 checksum 均通过后才返回 `DB_MIGRATION_SCHEMA_SENTINEL_MISSING`。
- 影响：仍会安全阻断，不会写库；但错误分类随组合形状漂移，支持人员可能只看到 checksum 问题而忽略已确认的结构缺失。未来如果 checksum 被加入白名单，才会转为 lineage/sentinel 错误。
- 修正建议：冻结组合异常优先级并增加一项组合夹具。建议“迁移历史不可读/失败行 → 未知版本/history gap → sentinel 缺失 → checksum 未知”；或者统一以 `LINEAGE_INCOMPATIBLE` 返回多个结构化 signal。无论选择哪种，都应写入报告和测试，不能依赖循环顺序偶然决定。

### [P1] 预检连接阶段错误仍走通用 setup 失败路径，原生提示覆盖不完整

- 文件/行：`src-tauri/src/db/migration_safety.rs:101-110`；setup 分支 `src-tauri/src/lib.rs:6380-6392`（当前文件对应区域）。
- 触发形状：既有数据库因 ACL、共享锁、损坏 URI/路径或底层连接阶段错误而无法建立 read-only pool。
- 实际分类：连接错误映射为 `DbError::Connect("数据库只读预检连接失败: ...")`，不是 `MigrationCompatibility`；setup 只对 compatibility 弹原生提示，其他错误继续返回 setup error，仍可能表现为无提示退出/panic 记录。
- 判断：元数据查询和字段解码错误已经正确映射为结构化 `schema_metadata_unreadable` / `migration_history_unreadable`；本项仅针对查询开始前的连接失败。
- 修正建议：不要把所有连接错误误报为谱系不兼容。应增加稳定的启动数据库访问错误分类及原生提示，或至少明确 M1 只承诺四类谱系错误、将连接/权限/损坏文件提示另列阻断任务。对于“file is not a database”等明确不兼容/损坏错误可安全映射到恢复提示，但不得在用户文案中泄露底层 SQL 参数。

### [P2] sentinel 对索引/触发器只验证名称，无法识别同名但语义已漂移

- 文件/行：sentinel 定义 `src-tauri/src/db/migration_safety.rs:473-551`；`object_exists()` `:632-642`。
- 反例：保留 `device_sync_cases_insert` 名称但把 trigger 挂到错误表或改成空操作；保留 `idx_device_sync_outbox_pending` 名称但索引错误列。当前只查 `sqlite_master.type/name`，均会通过。
- 影响：当前实现覆盖了 N0 冻结的最小名称集合，但不能完全证明“同编号不同 SQL”结构语义一致。checksum 正常而 schema 曾被错误重建时可能漏报。
- 修正建议：至少同时验证 index 所属表和关键列；trigger 验证 `tbl_name`，并对关键触发器保存规范化 SQL digest 或结构化动作 sentinel。若本版不扩展，应在报告中明确这是最小 sentinel 而非完整 schema 证明，并留到 M1 后续安全加固。

## 三、已核对且未发现阻断的问题

1. `MIGRATION_COMPATIBILITY_ALLOWLIST` 当前为空，没有编造笔记本旧 checksum。
2. 原来的 `reconcile_migration_checksums()` 已移除；M1 差异中没有 `set_ignore_missing(true)`。
3. 已应用未知版本、`success=0`、history gap、metadata decode/query failure均在正常 RW pool 前返回结构化错误。
4. 49、51、58—62 的表/列/外键/索引/触发器集合与 N0-MIG 冻结清单一致；只查询 `_sqlx_migrations`、`sqlite_master` 和 PRAGMA 结构元数据，不读取业务字段值。
5. 四个稳定错误码、结构化序列化和恢复文案均不依赖中文文本进行程序分类。
6. setup 只捕获 `DbError::MigrationCompatibility`；兼容错误对话框使用既有 `rfd`，文案包含错误码、数据库路径、备份和退出说明，不展示案件正文、SQL 参数、Token 或凭据。日志只记录 code/version/static reason/sentinel code。
7. 七个夹具没有读取默认数据目录或正式数据；`foreign_keys(false)` 仅用于构造“迁移历史成功但表已损坏”的不兼容夹具，没有在被测生产预检中关闭外键。
8. 源码实际差异仅为四个授权 Rust 文件；迁移 SQL、Cargo/Node 依赖、版本、device sync、飞书逻辑和发布配置无差异。递归 rustfmt 事故恢复后，未发现 161 个授权外源码的残留 diff。
9. `git diff --check` 当前退出 0；仅有 Git 的 LF/CRLF 提示，不是 whitespace error。
10. M1 报告与主控提供的实测计数不冲突：主控已报告 Cargo check/Clippy、Node 119/119、Vite/source gate、Windows Rust 275/0/3 和 device sync 23/23 通过；本审计按任务要求未重复运行。

## 四、夹具可靠性结论

七个现有夹具的结构化错误码和逻辑断言总体明确，但“写前物理零变化”的证明尚不能接受，原因是 fingerprint helper 自身会先建立普通 SQLite 只读连接。修复 P0 后，主控应至少新增并实跑：

1. `_sqlx_migrations` 存在但 0 行 + 用户表 + 合成业务行；
2. WAL 模式不兼容库，覆盖完整 sidecar 和缺 SHM 形状；
3. 组合异常：未知 checksum + 缺 sentinel，冻结优先级；
4. metadata query/decode 失败及 read-only connect 失败分别验证错误分类和原生提示；
5. 物理指纹在任何 SQLite 验证连接之前采集，错误返回后也先采集物理指纹。

修订后的定向测试仍应为 synthetic `TempDir`，不得连接默认应用目录或笔记本/台式机正式库。

## 五、主控复验建议

### 代码静态复核

- 确认 `history.is_empty()` 分支区分真正空 schema 与已有用户 schema；
- 确认 WAL/SHM 夹具的物理采样顺序不会被 fingerprint helper 污染；
- 确认只读预检机制对 WAL 数据是完整且不创建 sidecar 的，不能以忽略 WAL 换取“零写入”；
- 确认 allowlist 规则拒绝空/未知 sentinel，并在 CAS 事务中复验；
- 确认组合错误优先级成为显式契约，而不是代码循环副作用；
- 再次核对源码 diff 仍只限四个授权文件。

### 修订后自动门禁（由主控串行执行）

本审计线程未执行以下命令；修订后由主控按既有矩阵执行定向 7+ 新增反例、Cargo check、Clippy、Windows Rust 全量、Node 119、Vite build 和 source gate，并报告真实通过/失败/跳过数。

### 原生提示视觉门禁

使用隔离数据目录和纯合成不兼容库，在 Windows 正式候选环境逐一验证：

1. 四个谱系错误码均在 WebView 可用前显示原生对话框；
2. 文案显示实际隔离数据库路径、备份 WAL/SHM 建议和退出说明；
3. 关闭对话框后退出码为 2，不进入 setup `.expect()` panic/闪退路径；
4. 日志、对话框、WER/crash.log 不出现合成业务 payload，更不得出现正式业务正文或凭据；
5. DB、WAL、SHM 及父目录在提示前后满足修订后的物理不变量。

### 隔离副本门禁

M1 修订通过后，仍只能使用全新库、当前 0.8.2 正常库副本、合成未知谱系库和经单独授权的只读一致性副本；验证 `quick_check=ok`、`foreign_key_check` 为空、迁移历史及业务指纹。笔记本真实旧 checksum 未取得前，白名单继续为空，不得在唯一正式库上试验兼容动作。

## 六、接受/退回建议

建议主控当前**退回 M1**，至少修复两个 P0：

1. 空 `_sqlx_migrations` + 用户 schema 绕过；
2. WAL/SHM 只读副作用与 fingerprint 自污染证明缺口。

P1 allowlist 强约束建议一并修复；若主控决定延期，必须以“白名单保持为空”为硬门禁，并在取得任何真实旧 checksum、添加第一条 allowlist 之前完成。P1 连接失败原生提示与 P2 深层 sentinel 可作为明确残余风险继续跟踪，但不得在报告中宣称已覆盖所有启动数据库错误或完整 schema 语义。

安全声明：本线程未修改产品/测试源码、迁移、依赖、版本或其他线程文件；未运行 Cargo/Node/构建；未读取或修改正式数据库、NAS、飞书、凭据或业务正文；未提交 Git。
