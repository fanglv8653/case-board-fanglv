# V083-FORMAL-TOOLING-R1 独立只读复核报告

## 结论

**暂不接受，退回定点修复。**

- P0：0
- P1：3
- P2：1
- 验收要求 P0=0、P1=0，当前不满足。

copy-first 主路径设计成立：`Backup` 在 SQLite 介入前只用普通文件 API记录并复制 source DB/WAL/SHM，SQLite 只打开 retained staging；源三件套前后事实、目标 main-only、`quick_check`、FK 均实现了异常级硬断言。默认脚本也没有安装、目录切换、强杀、正式 sidecar 删除、同步恢复或发布实现。

但迁移历史哈希会丢失原始语义、resume manifest 没有绑定阶段产物、目标 rollback journal 可被 SQLite 隐式删除，故本轮还不能用于建立正式备份。

## P1-01：migration tuple/history hash 不完整，`success=1→2` 与 `installed_on` 变化可被幂等比较漏掉

### 证据

1. `db_audit.py:302-319` 读取完整六字段后，把原始 `success` 立即转换为 `bool(success)`。SQLite 中 `1` 和 `2` 都变成 `True`，不再是原始 migration tuple。
2. `db_audit.py:343-352` 计算 `history_sha256` 时只纳入 version、description、布尔 success、stored checksum、execution_time，遗漏 `installed_on`。
3. `db_audit.py:507-515` 的幂等门禁只比较该不完整的 history hash；`failed_count` 也以布尔值判断，因此 `success=2` 不会被认定为异常。

本轮纯合成探针将同一记录从：

`installed_on='2026-01-01', success=1`

改为：

`installed_on='2099-12-31', success=2`

结果为：快照中的 success 前后均为 `true`，`history_sha256` 完全相等，`compare(..., idempotent=True)` 返回 `passed`，所有 checks 均为 true。这直接违反“完整 migration tuple/checksum/execution_time”与迁移历史哈希门槛。

### 修复建议

- 快照保留原始整数 `success`，明确要求标准成功值只能是 `1`；`failed_count` 使用 `success != 1`。
- `history_sha256` 纳入完整六字段：version、description、installed_on 原始稳定表示、success 原始整数、checksum 原始字节/规范十六进制、execution_time。
- 新增两个独立回归：仅 `success 1→2`、仅修改 `installed_on`，幂等比较都必须失败。

## P1-02：resume manifest 只校验调用方同时提供的 hash 与自由状态字符串，未绑定真实前序阶段和产物

### 证据

`Invoke-UpgradeValidation.ps1:93-107` 的 `Read-Resume` 只执行：

- 对当前 manifest 文件计算 SHA-256，与同一调用方传入的 expected hash 比较；
- 判断 JSON 的 `status` 是否在允许字符串列表。

它不验证 `schema_version`、`stage`、父链、manifest 所在 run root、canonical 相对路径、`formal_mutation=false`，也没有核验 main-only DB、backup result、snapshot 等被引用产物的独立 hash。

后续影响：

- `AuditCopy` 在脚本 `164-198` 行直接信任 manifest 内的 `main_only_database` 与 `run_root`；调用方可让 SQLite snapshot 打开任意 main-only 文件，而没有证明它来自 copy-first Backup。
- `ValidateIsolatedExit` 在 `219-255` 行又接受调用方独立提供的 `ProofDatabase`，未要求其与父 manifest 的 artifact 身份绑定；`ExitMode='graceful'` 也是自由声明。

本轮合成实证：

1. 手写只有 `status='backup-passed'`、任意 `run_root/main_only_database` 的 JSON，自行计算 hash 后调用 `AuditCopy`，返回码 0 并生成 `audit-passed`。
2. 手写 `status='audit-passed'` 或 `isolated-start-passed` 及自制 snapshot，自行计算 hash 后可分别生成 `isolated-start-passed` 和 `isolated-second-start-passed`，全程没有执行 Backup，也没有启动或退出应用。

这不只是“恶意用户可改代码”的问题：合法 manifest 引用的 DB/JSON 在阶段间被意外替换时，manifest hash 同样不会变化。当前属于 manifest 文件完整性校验，不是阶段产物/链路绑定，不能证明防跳步。

### 修复建议

- 为每种 allowed status 定义严格 schema，并联合校验 `schema_version + stage + status + formal_mutation`。
- manifest 写入所有关键 artifact 的 SHA-256、大小和预期相对路径；resume 时重新计算并逐项核验，且要求 artifact、manifest 均位于 canonical `run_root` 下。
- `AuditCopy` 只能接受经上述验证的 Backup manifest；`ValidateIsolatedExit` 必须绑定父 snapshot/proof DB 身份，并记录可验证的外部运行证据，而不只接受自由 `ExitMode` 字符串。
- 增加伪造 status、自制 hash、替换 main-only DB、替换 snapshot、路径越界四类合成负例。

## P1-03：目标存在 `-journal` 时未 fail closed，SQLite 会隐式删除该 sidecar

### 证据

- `db_audit.py:423-424` 的目标存在检查只覆盖 main、`-wal`、`-shm`。
- `require_main_only` 在 `135-139` 行也只覆盖 `-wal/-shm`。
- rollback journal `-journal` 未纳入拒绝集合。

本轮在纯临时目录预建 `backup.db-journal`，再调用 `online_backup(source, backup.db)`：函数返回 `backup-passed`，预置 journal 文件在 SQLite 打开/规范化目标过程中被删除。工具代码没有显式 `unlink`，但这是可预见的 SQLite 写副作用；结果仍然违反“目标已存在 fail closed”“main-only 硬断言”和“不删除 sidecar 路径”。PowerShell 正常路径使用全新 run root，降低了触发概率，但 Python CLI 与竞态窗口仍真实存在。

### 修复建议

- 目标预检和 `require_main_only` 至少加入 `-journal`，并评估/覆盖 SQLite 可能使用的 super-journal/multi-journal 命名；检测到任何 sidecar 都应在 SQLite 打开目标前异常退出并原样保留。
- 新增“预置非空 `destination-journal`”测试，断言调用失败且字节完全不变。

## P2-01：进程枚举失败会被当作“没有 caseboard 进程”

`Invoke-UpgradeValidation.ps1:72-76` 使用 `Get-CimInstance ... -ErrorAction SilentlyContinue`。若 CIM/WMI 查询因权限或服务异常失败，结果可能为空，`Assert-NoCaseboardProcess` 会通过。虽然 source trio 前后哈希仍能捕获多数并发写入，正式证明不应把“无法枚举”解释为“进程数为零”。建议改为 `-ErrorAction Stop` 并将枚举失败作为硬失败，增加 mock/子进程测试。

## 已确认符合的部分

- `db_audit.py:428-443` 先记录 source trio，再以 `shutil.copy2` 复制到全新 raw-copy 目录；只有 `copied_main` 被传给 `sqlite3.connect`。Backup 路径没有 SQLite 打开正式 source。
- `db_audit.py:435-460` 两次硬比较 source trio；比较包含存在性、大小、mtime ns、SHA-256。staging 与 source 内容也逐项硬比较。
- 目标经过 `journal_mode=DELETE`、无 WAL/SHM、`quick_check == ['ok']`、FK violation count=0 的异常级硬断言；PowerShell Backup 又复核一次。
- 逐表 fingerprint 包含列名、类型标记、行值及稳定排序，现有“同数异内容”合成测试能检出；非设备同步业务投影、schema hash、0063 字段/索引/FK信息、同步安全指标均已记录。
- `-Stage` 强制必填；R1 orchestrator 不包含 `Start-Process`、`Stop-Process`、`Remove-Item`、目录移动、Credential API、安装器、迁移表 UPDATE、同步恢复或发布实现。
- `FormalSwitch`/`Install` 即使提供确认参数仍只返回 disabled error，不产生正式 mutation。
- 13 项官方 Python/PowerShell 合成测试全部通过（19.360 秒）；PowerShell parser 通过；`git diff --check -- scripts/windows-upgrade-validation` 无 whitespace error，仅有 LF/CRLF 提示。
- 未访问任何正式 DB/WAL/SHM、凭据、NAS、GitHub secrets，未启动/停止/安装应用，未运行 Cargo。

## 验收建议与修复顺序

1. 先修 migration 原始 tuple 与完整 history hash，并补两项漏检回归。
2. 再建立 manifest 对阶段 schema、父链和 artifact hashes/paths 的强绑定，补伪造及替换负例。
3. 在所有目标存在/main-only 检查中覆盖 rollback journal，证明绝不隐式删除。
4. 将进程枚举失败改为 fail closed。
5. 修复后重新运行合成测试并再派独立只读复核；在 P1 清零前，不应使用该工具接触正式 source 或建立正式备份。
