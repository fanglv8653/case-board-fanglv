# V081-MIGRATION：0055—0058 迁移与敏感信息边界静态审计

## 1. 审计结论

本轮对 `0055_yuandian_balance_snapshots.sql` 至
`0058_device_sync_core.sql` 做了空库、从 0.8.0 基线升级、触发器/实体白名单及
同步敏感信息边界的静态审计。

结论如下：

1. 57 个迁移文件可按版本顺序在 SQLite 空库完整执行；从 0054 升级到
   0055—0058 也可逐个执行。
2. 0058 的 20 类同步触发器与 Rust 注册表的 20 类实体严格一致；财务记录及
   飞书业务关联已经纳入。
3. 原始材料、抽取结果与任务、聊天、案件/用户记忆、凭据与设置均未进入
   0058 的同步触发器和注册表。
4. NAS 同步包的设计边界是“传输/备份包加密”，不是本机 SQLite 整库加密；
   本机 outbox、冲突记录等仍会保存允许同步的业务字段明文。
5. 发现 4 项需要主控安排修复的设备同步逻辑问题，其中入站字段误杀、
   初始基线外键顺序、删除墓碑不落到业务表是上线阻断项。本轮遵守边界，
   未修改 `src-tauri/src/device_sync/*`。
6. 另发现并修复一项非设备范围问题：0057 的记忆激活约束可通过直接
   `INSERT` 或未确认修订绕过；现已补齐数据库守卫并调整确认事务顺序。

## 2. 审计范围与方法

审计文件：

- `src-tauri/migrations/0055_yuandian_balance_snapshots.sql`
- `src-tauri/migrations/0056_legal_skill_packages.sql`
- `src-tauri/migrations/0057_case_memory_mvp.sql`
- `src-tauri/migrations/0058_device_sync_core.sql`
- `src-tauri/src/device_sync/`（只读）
- `src-tauri/src/db/case_memory.rs`

校验方法：

- 使用 Python 标准库 `sqlite3` 在内存数据库执行迁移，不写入正式数据库；
- 空库依次执行全部 57 个迁移；
- 先执行至 0054，再依次执行 0055—0058，模拟 0.8.0 升级；
- 解析 0058 的触发器来源表、实体标签和字段，并与 Rust 注册表静态比对；
- 检查同步入站/出站字段、凭据存放、初始基线、墓碑删除及敏感字段过滤；
- 未运行 Cargo，避免争用正在进行的构建锁。

## 3. 迁移顺序验证

### 3.1 空库

结果：

```text
EMPTY_DB_OK count=57 max=0058_device_sync_core.sql
```

迁移版本连续且无重复：

```text
MIGRATION_FILES 57 MIN 1 MAX 58 DUPLICATES {}
```

### 3.2 从 0.8.0 基线升级

先执行至 0054：

```text
V080_BASE_OK count=53 max=0054_scrub_court_filing_credentials.sql
```

随后逐个执行：

```text
UPGRADE_APPLY_OK 0055_yuandian_balance_snapshots.sql
UPGRADE_APPLY_OK 0056_legal_skill_packages.sql
UPGRADE_APPLY_OK 0057_case_memory_mvp.sql
UPGRADE_APPLY_OK 0058_device_sync_core.sql
```

0058 所引用的 20 个业务表均在触发器创建前存在；58 个触发器的列引用静态
校验无异常：

```text
TRIGGER_COLUMN_CHECK triggers=58 issues=0
```

## 4. 同步实体白名单

0058 触发器实体标签与 Rust 注册表均为以下 20 类，双方无缺项、无多项：

```text
agency_contact
calendar_event
case
case_payment
case_todo
contact
criminal_deadline
criminal_task
criminal_workflow
feishu_binding_audit
feishu_conflict
feishu_inbox
feishu_link
feishu_snapshot
income_record
legal_skill_binding
legal_skill_package
party
stage_item
work_item
```

校验结果：

```text
trigger entities = 20
policy entities  = 20
trigger without policy = 0
policy without trigger = 0
```

目前数据库协议表本身没有 `CHECK(entity_type IN (...))` 约束。现有安全性依赖：

- SQL 触发器只产生固定实体标签；
- Rust 注册表对未知实体 fail-closed。

建议后续增加数据库级白名单约束或集中白名单表，作为纵深防御；这不是当前
边界泄露，但可降低未来新增代码绕开注册表的风险。

## 5. 纳入和排除边界

### 5.1 已纳入

财务：

- `case_income_records`
- `case_payments`

飞书业务关联：

- `feishu_sync_links`
- `feishu_sync_snapshots`
- `feishu_sync_conflicts`
- `feishu_sync_inbox`
- `feishu_sync_binding_audits`

飞书同步中纳入的是案件绑定、映射快照、冲突和审计等业务信息，不包含
OAuth access token、refresh token 或应用密钥。`app_token` 在该模型中是飞书
多维表格/应用的业务标识，不是登录授权令牌。

财务记录中的 `auto_source_document_id`、`auto_source_filename` 及自动/手工
字段 JSON 会随财务业务记录同步；它们是财务来源追溯元数据和派生业务值，
不包含原文件字节或完整抽取文本。该细节与“财务记录全部同步”的产品确认
一致，但应在同步说明中明确告知用户。

### 5.2 明确排除

静态核对确认，以下表/领域均没有 0058 同步触发器，也没有对应注册表实体：

- 原始材料：`documents`
- 抽取与指标：`extraction_metrics`
- 材料处理任务：`material_processing_batches`、
  `material_processing_items`
- 聊天：`chat_messages`
- 记忆：`case_memory_items`、`case_memory_candidates`、
  `user_memory_preferences`
- 凭据定位与设置：`credential_locators` 及其他设置/凭据实体

`cases.source_folder` 未进入 case 同步字段；远端首次出现的案件使用本地占位
路径 `device-sync-unbound://{id}`，避免同步原始材料目录路径。

设备签名密钥、交换密钥、同步组密钥和邀请码通过 Windows Credential Manager
以 `FanglvCaseBoard/device-sync` 名称空间保存。SQLite 仅保存公钥、指纹、
哈希、证明和签名等非秘密材料。

## 6. 设备同步只报告问题

以下问题位于 `src-tauri/src/device_sync/*`，本轮没有修改。

### P0-1：入站敏感字段过滤误杀合法业务字段

`registry::sanitize_fields` 使用子串拒绝规则：

- `key.contains("chat")` 会把联系人合法字段 `wechat` 判为聊天内容；
- `key.contains("token")` 会把飞书业务标识 `app_token` 判为凭据。

出站捕获没有做相同过滤，而入站应用会过滤，因此联系人和飞书同步可能在
另一台电脑落库失败。

建议：改为“明确禁止的凭据字段名”匹配，至少显式允许 `wechat`、`app_token`，
并精确拒绝 `access_token`、`refresh_token`、`api_key`、`secret`、
`password` 等真正秘密字段。

### P0-2：初始基线排序可能造成外键失败

初始基线先按策略枚举写入 dirty 队列，但捕获时使用：

```text
ORDER BY changed_at, entity_type, entity_id
```

同秒写入的记录可能按实体字母顺序处理，导致子表先于父表，例如：

- `agency_contact` 可能先于 `case`；
- `legal_skill_binding` 先于 `legal_skill_package`；
- 飞书冲突/审计等子实体可能先于 link/inbox 父实体。

建议：为初始基线建立明确的依赖层级和序号，并按该序号捕获/应用；或者实现
可识别外键失败的延迟重试队列。

### P0-3：墓碑没有删除或软删除业务记录

入站 tombstone 当前只更新
`device_sync_entity_revisions.tombstoned` 并写 applied 审计，没有删除或软删除
对应业务表记录。结果是第一台电脑删除后，第二台电脑仍可能继续显示该记录。

建议：每类可删除实体定义明确的删除策略；具有软删除列的更新软删除状态，
其余实体在满足引用约束时事务性删除，并为父子实体设置删除顺序。

### P1-1：法律 Skill “抑制内置默认项”语义未同步

0056 新增 `legal_skill_binding_suppressions`，用于删除导入项后阻止同名内置
Skill 自动回退。0058 未为其建立触发器，Rust 注册表也无对应实体。

结果是电脑 A 删除/解绑后的“不要回退”语义可能无法到达电脑 B，电脑 B 会
重新显示内置 Skill。

建议：将 suppression 作为独立同步实体，或把 suppression 状态并入
`legal_skill_binding` 的同步契约。

### P2-1：数据库层缺少实体类型 CHECK

参见第 4 节。当前 Rust fail-closed 足以阻止未知实体被正常应用，但建议增加
数据库级约束，避免未来直接写协议表的代码绕开注册表。

### 边界说明：NAS 加密不等于本机数据库加密

同步包/备份包在写入 NAS 前应使用同步组密钥加密；本机 SQLite 中的 dirty、
outbox、conflict、revision 等协议表仍会保存允许同步的业务字段或摘要。
这与本机案件数据库本身的明文存储边界一致，但产品文案不应表述为“全链路
本地数据静态加密”。

## 7. 已修复的非设备问题：0057 记忆激活守卫

### 原问题

- `case_memory_items` 只有 `BEFORE UPDATE` 激活守卫，直接
  `INSERT ... status='active'` 可绕过候选确认流程；
- `user_memory_preferences` 只校验 `confirmed_by/confirmed_at` 非空，未强制
  当前修订已被确认，直接插入或更新可绕过 revision gate。

### 修复

在 `0057_case_memory_mvp.sql` 增加：

- `trg_case_memory_active_insert_guard`
- `trg_user_memory_preference_active_insert_guard`
- `trg_user_memory_preference_active_revision_guard`

同时调整 `confirm_user_memory_preference` 的事务顺序：

1. 先确认目标 revision，并校验确实更新 1 行；
2. 再将 preference 激活；
3. 任一步失败均回滚。

新增数据库测试覆盖直接 active 插入、未确认修订激活被拒及确认后激活成功。

内存数据库复核：

```text
EMPTY_DB_RECHECK_OK 57
case active insert -> blocked
preference active insert -> blocked
unconfirmed preference update -> blocked
confirmed revision then preference activation -> active
```

## 8. 验收与限制

- `git diff --check`（本轮修改文件）：通过；
- 全量迁移空库执行：通过；
- 0054 → 0058 增量执行：通过；
- 触发器列及触发器/注册表实体一致性：通过；
- 未运行 Cargo，未争用 Cargo 构建锁；
- 未修改任何 `src-tauri/src/device_sync/*` 文件；
- 对 `case_memory.rs` 的定向 `rustfmt --check` 显示文件其他位置已有格式差异，
  与本轮补丁无关；为避免覆盖共享工作树，未对整文件自动格式化。

## 9. 主控验收建议

0.8.1 设备同步进入整体验收前，至少应完成并验证：

1. 修复 `wechat` / `app_token` 入站误杀；
2. 建立父子实体基线和入站应用顺序；
3. 让 tombstone 实际作用于业务记录；
4. 同步法律 Skill suppression 语义；
5. 双机演练财务与飞书关联，确认原材料、抽取、聊天、记忆和凭据没有进入
   NAS 加密包的解密后清单；
6. 在正式 NAS 挂载前，可先用两个本地目录和两个独立测试数据库模拟双机，
   NAS 挂载完成后再做真实路径与断网恢复验收。
