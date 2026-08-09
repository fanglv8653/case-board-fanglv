# V083-FORMAL-BACKUP-EXECUTE 独立只读复核报告

## 结论

**通过正式备份执行门禁。** 固定批次 `V083-20260809-234237` 的原样完整数据根与 main-only 一致性副本均存在且可追溯；未发现影响恢复能力或升级前基线的阻断问题。

- P0：0
- P1：0
- P2：1（补充清单中 robocopy exit-code 字段的证据格式瑕疵，不影响本批次哈希与恢复有效性）

## 独立重算结果

### 1. 正式源 trio 当前事实与不可变性

当前只用普通文件 API 重算固定正式源，结果与 `supplementary-manifest.json` 的 initial/final 事实及 Backup 结果完全一致：

| 文件 | 大小 | mtime UTC | SHA-256 |
|---|---:|---|---|
| `caseboard.db` | 556,773,376 | `2026-08-07T01:49:50.9269074Z` | `A82C2A8F305351209DF082D661B3FE8A8DC3C89058E7A9BA929D27690F67DE3C` |
| `caseboard.db-wal` | 5,100,592 | `2026-08-07T03:14:38.2033543Z` | `F00B1780FF873930ECF2AF3656FBD614C07B974DD6E278B1187DAA415158C466` |
| `caseboard.db-shm` | 32,768 | `2026-08-05T06:27:10.9983034Z` | `CED9B3599A31DDD45CC519CB111CDAA5E07B7B9FA3CE42E8A2EA71AF4BB10C09` |

- 当前源与 initial facts mismatch：0。
- initial 与 final facts：size、mtime、SHA-256 全部相等。
- 正式 `caseboard.db-journal`：不存在。
- 最终 `caseboard.exe`：0；CIM 枚举成功。
- 补充执行脚本在每个阶段前后均以 `Get-CimInstance -ErrorAction Stop` 检查应用及正式 WebView 进程，并在每阶段后重算 trio；七阶段记录全部一致。

### 2. 原样副本与 staging trio

- `01-raw-data/data` 中 main/WAL/SHM 三项当前 SHA-256 逐项等于正式源，mismatch=0；这是本批次接受的完整原样恢复副本。
- `03-backup-result.json` 中 `raw_source_copy_before_sqlite` 的 main/WAL/SHM/journal 可比事实逐项等于正式源 Backup 前事实。
- `01-source-trio` 是供 SQLite online backup 打开的 retained staging，不应被误当作最终原样副本；SQLite 打开 staging 后其 SHM 锁字节可变化。正式源和独立的 `01-raw-data` 均未因此变化。

### 3. Backup → Audit 清单链

独立使用当前 Windows 用户 DPAPI 解开该 run anchor，仅在内存中重算 HMAC，未输出密钥：

| 清单 | 当前 SHA-256 | `.sha256` | HMAC |
|---|---|---|---|
| `manifest.backup.json` | `8CE5582A2F9E879353F0ED129D3785B9FC67E0DE7C7370BB8B9302822D083913` | 匹配 | 匹配 |
| `manifest.audit.json` | `F386A61F2514EE9AF460E618DCA9E57E362550C85F388FEC656AEA06673A7859` | 匹配 | 匹配 |

- Audit 的 `parent_manifest_sha256` 精确等于 Backup 当前 SHA-256。
- 两份 manifest 共 4 个 artifact 的实际 SHA-256 mismatch=0。
- 两份 manifest 的 run root、stage/status 与 artifact 路径均指向固定批次。

### 4. main-only 独立只读复核

对 `02-main-only/caseboard.db` 使用 SQLite `mode=ro&immutable=1`，未打开正式源：

- SHA-256：`3BE3C721505018728B3B835D37D25B9D0AD2B8968A64C7AE15FEDA480AB6F2FC`。
- `-wal/-shm/-journal`：均不存在。
- `quick_check`：`ok`；FK violation：0；审计记录 journal mode=`delete`。
- migration：count=62、max=62、failed=0。
- v36 唯一可信 tuple 精确匹配：version=36、description=`feishu reminder runs`、success=1、stored SQLx SHA-384=`84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480`。
- `feishu_reminder_runs` 精确三列结构成立：`sent_date TEXT PRIMARY KEY NOT NULL`、`sent_at TEXT NOT NULL DEFAULT datetime('now')`、`item_count INTEGER NOT NULL DEFAULT 0`。
- version 63 成功记录数=0；m63 `applied=false`，预升级 columns/index/FK sentinels 与审计报告一致。
- business projection SHA-256=`9B9A26C02803252D6FDE2C2FAB06EF5CB02F949720C1DAE9A6DBF805897B218F`。

### 5. 补充清单与备份覆盖

- `supplementary-manifest.json` 当前 SHA-256=`832F55471ED71E78C2C0C8D27D73CE309C2A9F9B1A3C13E80B141092D23D75F5`，与 sidecar 匹配。
- 清单登记文件：2001；逐项重新检查路径边界、存在性、size 与 SHA-256：missing=0、size mismatch=0、hash mismatch=0、越界路径=0。
- 清单排除自身及其 SHA sidecar 两项；当前 run root 文件总数 2003，与报告一致。
- 分类抽样及全量重算覆盖：正式完整数据根 632 项、旧数据 8 项、安装/rollback 34 项、注册表 1 项、当前 WebView 620 项、旧 WebView 687 项；其余为本批次证据文件。
- 安装 EXE SHA-256=`62160F3E7011ACDB6D2EC89C9D15C9962D7D7C6C23EB380D83DAC14F13DFF359`；rollback setup SHA-256=`443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312`。
- 旧数据、安装目录、注册表、当前/旧 WebView 的 source status 均为 present。WebView 与注册表仅作文件存在/大小/哈希核对，未解析或展示内容。

### 6. EFS 与 ACL 当前递归状态

- 当前递归对象总数（含 run root）：2201。
- EFS encrypted attribute violation：0。
- `cipher /c` 抽查确认 AES、key length 256，仅当前 Windows 用户可解密，无 recovery certificate。
- ACL 递归读取错误：0；unexpected allow identity violation：0。
- 父目录与 run root 的允许主体均仅当前用户、SYSTEM、Administrators，均为 FullControl；run root 继承父目录 ACL。

## P2-01：`robocopy_exit_code` 在补充清单中不是纯整数

五个 robocopy 阶段的 `detail.robocopy_exit_code` 实际是三元素数组：空行、robocopy 的 log-file 提示行、整数 `1`；提示行中的中文父路径还存在控制台解码乱码。原因是 `Invoke-RobocopyChecked` 将 robocopy 成功输出与最后返回的 `$LASTEXITCODE` 一并写入 PowerShell success stream。

这不影响本批次接受：函数内部在返回前使用标量 `$LASTEXITCODE` 执行 `>7` fail-closed，五个数组的末项均为整数 1；全部 2001 项文件也已独立重算为 mismatch=0。建议后续让 robocopy 输出单独重定向/抑制，并把 manifest 字段固定为整数，避免证据消费者依赖“取数组末项”。

## 操作边界

- 未访问 NAS、网络或凭据内容。
- 未打开或解析 WebView 内容；仅进行文件级属性和哈希核验。
- 未启动、停止、安装或切换应用，未删除或修改任何 sidecar/备份文件。
- 未对正式源使用 SQLite；SQLite 只以 immutable read-only 模式打开 main-only 副本。
