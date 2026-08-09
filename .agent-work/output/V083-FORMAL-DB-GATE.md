# V083-FORMAL-DB-GATE：本设备正式数据库谱系与备份门禁

- 逻辑线程：`worker-formal-db-gate`
- 方式：正式路径只读定位；停进程后成组复制 DB/WAL/SHM；仅在临时副本上执行 SQLite 查询
- 状态：`submitted_for_review`
- 结论：**正式库健康，但当前 0.8.3 候选不能直接对该库执行升级。P0=0，P1=2，P2=0；门禁为 `blocked_internal`。**
- 禁止事项：在两个 P1 关闭前，不启动 0.8.3 正式 EXE，不调用生产 `init_pool()`，不删除正式 WAL/SHM，不修改 `_sqlx_migrations`，不安装或发布。

## 一、正式库定位与进程门禁

源码 `src-tauri/src/db/mod.rs` 的当前 Windows 路径为 `%APPDATA%\FanglvCaseBoard\data\caseboard.db`；仅在当前库不存在时才考虑旧 `%APPDATA%\CaseBoard\data`。本机两处均有数据库，但当前库存在，因此正式目标精确确定为：

`C:\Users\William Feng\AppData\Roaming\FanglvCaseBoard\data\caseboard.db`

旧路径 `C:\Users\William Feng\AppData\Roaming\CaseBoard\data\caseboard.db` 只做存在性/文件元数据确认，未打开、未查询，不得误作本轮正式目标。

只读检查期间三次进程采样均为：`caseboard.exe = 0`。注册表卸载项和安装文件一致：

| 项目 | 结果 |
| --- | --- |
| 安装名称 | 方律案件看板 |
| 安装版本 | `0.8.2` |
| 安装 EXE | `%LOCALAPPDATA%\方律案件看板\caseboard.exe` |
| FileVersion / ProductVersion | `0.8.2 / 0.8.2` |
| EXE 大小 | `19,174,400` bytes |
| EXE SHA-256 | `62160F3E7011ACDB6D2EC89C9D15C9962D7D7C6C23EB380D83DAC14F13DFF359` |

## 二、正式 DB/WAL/SHM 一致性快照

正式库存在活动形状的三个文件，不能只复制主库，更不能直接删除 sidecar：

| 文件 | 大小 | LastWriteTime | SHA-256 |
| --- | ---: | --- | --- |
| `caseboard.db` | 556,773,376 | `2026-08-07T09:49:50.9269074+08:00` | `A82C2A8F305351209DF082D661B3FE8A8DC3C89058E7A9BA929D27690F67DE3C` |
| `caseboard.db-wal` | 5,100,592 | `2026-08-07T11:14:38.2033543+08:00` | `F00B1780FF873930ECF2AF3656FBD614C07B974DD6E278B1187DAA415158C466` |
| `caseboard.db-shm` | 32,768 | `2026-08-05T14:27:10.9983034+08:00` | `CED9B3599A31DDD45CC519CB111CDAA5E07B7B9FA3CE42E8A2EA71AF4BB10C09` |

在确认无进程后，将三文件成组复制到本线程临时目录。复制前、复制后和副本哈希逐项相同；查询副本后再次核对正式三文件，大小、时间和 SHA-256 均未改变，进程仍为 0。所有 SQLite 检查只对副本执行，未连接正式路径。

临时副本检查完成后已精确删除三份敏感副本及空目录，共 561,906,736 bytes；删除未经过回收站，普通恢复不可用。只保留不含业务正文的结构化审计结果：

- `.agent-work/threads/worker-formal-db-gate/formal-db-audit.json`
- SHA-256：`45C539B2C9F801FD0792CC657E3791D77A03325DA8C1A03DC0C9B94D2C39ADFA`
- 审计脚本：`.agent-work/threads/worker-formal-db-gate/audit_snapshot.py`
- 脚本 SHA-256：`27AE4062FE021502DC343A63797504703D747A43C5F587125AB7D9094287153F`

## 三、健康性、迁移谱系与 checksum

### 1. SQLite 健康性

| 检查 | 结果 |
| --- | --- |
| journal mode | `wal` |
| page size / page count | `4096 / 135931` |
| `PRAGMA quick_check` | `ok` |
| `PRAGMA foreign_key_check` | 0 行 |
| 失败迁移 | 0 |

### 2. `_sqlx_migrations`

- 共 62 条，最大版本 62，全部 `success=1`。
- 正式库实际版本集合包含 1—62，包括版本 36。
- 版本 1—35、37—62 的 stored checksum 与当前仓库同版本 SQL 的 SQLx SHA-384 **全部逐项一致**。
- 唯一不在当前嵌入集合中的版本为 36：

| 字段 | 正式库值 |
| --- | --- |
| version | `36` |
| description | `feishu reminder runs` |
| installed_on | `2026-06-25 03:46:28` |
| success | `1` |
| stored checksum (SQLx SHA-384) | `84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480` |
| execution_time | `5205500` |

当前仓库没有 `0036_*.sql`。Git 历史提交 `a12cad0840794877c5ff626ac37f1633b29ea236` 可以取回 `0036_feishu_reminder_runs.sql`，但该 Git blob 的 SHA-384 为 `ABB35B6E576DAD9B81A3561E67D730626EBC351967EDEA808BCDA12560C0E985FB6FDE48F8810F63EA48C5B648223824`，**与正式库 stored checksum 不同**。因此不能声称已找到产生正式 checksum 的原始 SQL 字节。

正式库的 `feishu_reminder_runs` 表存在、当前 0 行，定义为三列：

- `sent_date TEXT PRIMARY KEY NOT NULL`
- `sent_at TEXT NOT NULL DEFAULT datetime('now')`
- `item_count INTEGER NOT NULL DEFAULT 0`

项目历史审计 `agent-work/evidence/V062-S7/01-formal-before.json` 在 2026-07-15 已记录同一正式库版本 36 的相同 checksum；本轮 2026-08-09 再次从当前正式 0.8.2 库取得相同值。故该 stored checksum 现在属于**来源可追溯的正式库输入**，不再是猜值；但它仍不足以单独放行，必须进入独立 `M1-COMPAT` 实现与复审。

## 四、0063 升级前 sentinel

正式库未应用 0063，且 0063 专属结构均未提前出现：

- `_sqlx_migrations.version=63`：不存在。
- `device_sync_export_drafts`：不存在。
- `device_sync_groups.last_attempt_at / last_success_at / auto_paused / pause_reason_code`：均不存在。
- `device_sync_outbox.capture_sequence`：不存在。
- 0063 的六个索引：均不存在。
- `device_sync_quarantine` 是 0058 已存在的旧表；`source_path` 是旧列，0063 新增的 `source_device_id / source_sequence / status / first_seen_at / last_seen_at / retry_count / resolved_at / last_error_code` 均不存在。
- 旧 quarantine 的 `group_id → device_sync_groups.id ON DELETE SET NULL` 外键存在。

这与预期的“0.8.2 / pre-0063”结构一致，没有发现半应用 0063 的迹象。

## 五、业务计数与稳定指纹

核心只读计数：

| 表 | 行数 | 表 | 行数 |
| --- | ---: | --- | ---: |
| `cases` | 6 | `documents` | 852 |
| `case_income_records` | 9 | `case_payments` | 14 |
| `case_work_items` | 94 | `case_stage_items` | 9 |
| `criminal_case_profiles` | 4 | `criminal_case_tasks` | 3 |
| `feishu_sync_links` | 5 | `feishu_sync_inbox` | 16 |
| `device_sync_groups` | 1 | `device_sync_outbox` | 487 |
| `device_sync_quarantine` | 11 | `device_sync_export_drafts` | 不存在 |

设备同步聚合状态：1 个组中 1 个 `paused=1`；487 条 outbox 全部为 `exported`；11 条旧 quarantine 全部为 `SYNC_DATABASE`。这证明当前正式组仍处于暂停状态，但不代表物理双端已经恢复。

稳定指纹采用以下口径，未把案件名、当事人、正文、路径或 ID 写入报告：

1. schema：按 `sqlite_master(type,name,tbl_name,sql)` 排序后作长度分隔 SHA-256，当前为 `FD536BFCADCD2E9289F56DAB6355800B403BB34FA2515AD2A9DD5305FD87446A`。
2. migration history：对 version/description/success/stored checksum/execution_time 排序编码，当前为 `3B0153F793EECD5BD0E7D1F2E901679179F2F929016131002865B5A4A0F1EBED`。
3. 全表计数摘要（排除 FTS shadow）：`012786766C29592F38D362FD56A9BE6591A1CBACB5A7BF1F2F98919E163D8DE4`。
4. 非设备同步业务投影：排除 `_sqlx_migrations`、`device_sync_*`、`cases_fts*`，对 94 张表按 PK（无 PK 时 rowid）排序，流式编码全部列后先取逐表 SHA-256，再取总 SHA-256；当前为 `9B9A26C02803252D6FDE2C2FAB06EF5CB02F949720C1DAE9A6DBF805897B218F`。

第 4 项应作为 0063 升级前后“非设备同步业务数据未改变”的主断言；schema、迁移历史和设备同步表因 0063 合法变化，不能要求总 hash 不变。

## 六、P0 / P1 / P2

### P0：0

未修改正式 DB/WAL/SHM，未启动应用，未触发生产 `init_pool()`，未把主库单独复制后冒充一致性备份，未修改迁移历史或 checksum。

### P1：2

1. **正式 sidecar 阻断。** 当前正式 DB 同时存在 WAL/SHM，而 0.8.3 的写前安全门禁会在首次 SQLite 连接前拒绝任何 sidecar 形状。因此直接启动 0.8.3 会先以 sidecar 错误安全阻断。不得删除 WAL/SHM；必须先做 SQLite 在线一致性备份并验证，再在明确升级窗口内把正式工作副本规范化为 main-only。
2. **版本 36 未嵌入。** 即使消除 sidecar，当前 0.8.3 嵌入集合没有版本 36，`migration_safety` 会返回 `DB_MIGRATION_APPLIED_VERSION_UNKNOWN`，不会运行 0063。必须先实现/复审来源绑定的兼容规则；不能直接改正式 `_sqlx_migrations`，不能设置 `ignore_missing`，不能用当前 checksum 相等冒充历史兼容。

### P2：0

没有单独的低优先级缺陷；两项都是正式升级前必须关闭的门禁。

## 七、历史 checksum 的可用性判断

结论分两层：

- **可作为有来源输入：是。** `84F859...41480` 来自本机正式 0.8.2 DB，当前 DB/WAL/SHM 有完整路径、大小、时间和哈希；2026-07-15 的正式库审计已记录相同值；当前安装 EXE/卸载项均为 0.8.2；其余 61 个 stored checksum 与当前源逐项一致，版本 36 表的完整结构存在。
- **可直接作为放行规则：否。** 对应 checksum 的原始 SQL 字节尚未在 Git 中找到；Git 历史 0036 文件的 SHA-384 不同。兼容实现至少必须绑定：`version=36`、正式 stored checksum、恢复后的当前 0036 checksum、上述三列精确定义、表存在且无冲突对象、其余迁移 checksum 全匹配、完整 schema sentinel 通过。任何一项不符都继续 fail closed。

建议立即创建单独的 `V083-M1-COMPAT36` 实现与独立复审任务；该任务关闭前，正式升级不得继续。

## 八、正式一致性备份命令

正式升级窗口应先关闭应用并确认无进程，再用 SQLite online backup 读取 WAL 一致视图，生成 main-only 备份。以下命令只写入 `D:\CodexWorkspace\008案件看板应用\formal-backups`，不修改源库；执行前仍须由主控再次核对目标时间戳目录不存在：

```powershell
$sourceDb = Join-Path $env:APPDATA 'FanglvCaseBoard\data\caseboard.db'
$backupRoot = 'D:\CodexWorkspace\008案件看板应用\formal-backups\V083-YYYYMMDD-HHMMSS'
$running = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -ieq 'caseboard.exe' })
if ($running.Count -ne 0) { throw 'CaseBoard is still running' }
if (Test-Path -LiteralPath $backupRoot) { throw 'Backup target already exists' }

New-Item -ItemType Directory -Path $backupRoot | Out-Null
$env:V083_SOURCE_DB = $sourceDb
$env:V083_BACKUP_DB = Join-Path $backupRoot 'caseboard.main-only.db'

@'
import os, sqlite3
from pathlib import Path

source = Path(os.environ['V083_SOURCE_DB']).resolve()
target = Path(os.environ['V083_BACKUP_DB']).resolve()
src = sqlite3.connect(source.as_uri() + '?mode=ro', uri=True)
src.execute('PRAGMA query_only=ON')
dst = sqlite3.connect(target)
src.backup(dst)
assert dst.execute('PRAGMA quick_check').fetchone()[0] == 'ok'
assert dst.execute('PRAGMA foreign_key_check').fetchall() == []
dst.close()
src.close()
'@ | python -

Get-FileHash -LiteralPath $sourceDb,"$sourceDb-wal","$sourceDb-shm",$env:V083_BACKUP_DB -Algorithm SHA256
```

命令执行前后必须分别保存正式 DB/WAL/SHM 的大小、LastWriteTime、SHA-256 和进程数；若任一源成员变化或出现 `caseboard.exe`，该备份批次作废并重新开始。还应对 main-only backup 重新生成本报告中的迁移、schema、计数和业务投影指纹，不能只看 `quick_check`。

## 九、升级前后精确断言

### 升级前

1. 备份与正式 WAL 一致视图：`quick_check=ok`、FK 0。
2. 迁移：62 条、max 62、失败 0；版本 36 为上述固定 tuple；1—35、37—62 checksum 与候选源码一致。
3. 0063 专属表/列/索引均不存在；无半迁移。
4. 非设备同步业务投影为 `9B9A...218F`；核心计数与本报告一致。
5. 同步组 1/1 暂停；outbox 487 且全 exported；旧 quarantine 11。
6. `M1-COMPAT36` 的实现、测试和独立复审已经 accepted；否则停止。

### 升级后

1. `quick_check=ok`、FK 0；第二次启动不产生任何 schema/业务变化。
2. 迁移变为 63 条、max 63、失败 0；版本 36 原 stored checksum 保留并由兼容 tuple 识别；版本 63 checksum 等于当前 `0063_device_sync_quarantine_lifecycle.sql` 的 SQLx SHA-384 `D5309B70309D5B7465741253E83FEF71AE6BBCE12ACC160C13E8BF8CC373D8F92CD7403E3ACE2578D36274546FDD229B`。
3. 0063 全部 table/column/definition/index/FK sentinel 通过；`device_sync_export_drafts` 存在且 0 行。
4. 非设备同步 94 表逐表指纹及总指纹仍为 `9B9A...218F`；核心业务计数不变。
5. `device_sync_groups` 仍为 1 行且 `paused=1`；新增 `auto_paused=0`，不得自动恢复正式同步。
6. outbox 仍为 487 行、全 exported；同组 `capture_sequence` 为 1—487、无 0、无重复。
7. quarantine 仍为 11 行，旧 ID/group/reason/created_at 投影保持；全部转换为 `manual_review`、`source_device_id='__legacy__'`、`source_sequence=-1`、active=0，不伪造真实包身份。
8. 原始三文件备份、main-only 一致性备份及升级后文件均保留各自 hash；回滚只在应用关闭后按整套数据目录执行，绝不经 NAS 同步 DB/WAL/SHM。

## 十、主控建议

最优顺序为：先接受本只读门禁结论 → 派发 `M1-COMPAT36` 实现与独立复审 → 在隔离副本证明 sidecar 规范化、36 兼容、0063 升级和二次启动全部通过 → 再开启正式维护窗口制作持久备份并升级。当前不得进入安装、启动、签名发布或 `latest.json` 更新。
