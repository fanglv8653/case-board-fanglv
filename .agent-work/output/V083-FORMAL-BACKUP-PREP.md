# V083-FORMAL-BACKUP-PREP｜本机正式备份与隔离升级执行包

- 逻辑线程：`worker-formal-backup-prep`
- 交付状态：`submitted_for_review`
- 当前总状态：`blocked_internal + blocked_external`
- 本轮边界：只读取 DEVICE/DB Gate、源码和仓库脚本；只写本报告及 workflow 文件。未访问或修改正式 DB/WAL/SHM、NAS、凭据或注册表，未创建正式备份，未启动应用，未安装、卸载或调和 sidecar。

## 一、执行结论

本机具备备份空间和 0.8.2 回滚安装包，但**当前不能运行现有 `Invoke-UpgradeValidation.ps1 -Install`，也不能进入正式维护窗口**。

前置阻断有三项：

1. 正式库当前为 DB + 非零 WAL + SHM；0.8.3 会在首次连接前拒绝任何 sidecar。不能删 sidecar，也不能只复制主库。
2. 正式库存在来源已确认但当前源码未嵌入的迁移 36。`V083-M1-COMPAT36` 正在实现，必须实现及独立复审均 `accepted` 后才能制作 0.8.3 隔离升级候选。
3. 正式 0.8.3 NSIS setup、updater `.sig` 和受控发布校验链尚未齐备。

最安全的正式调和方式不是在活动目录内 checkpoint 或删除 WAL/SHM，而是：

> 先保留活动数据根的原样副本和 SQLite main-only 一致性副本；在同一 C 盘父目录构造已验的 main-only 候选数据根；应用停止后，以两次同卷目录重命名把原活动目录完整移为 `data.pre-v083-*`，再把候选目录改名为 `data`。原 DB/WAL/SHM 始终随旧目录保留，不删除。

## 二、冻结输入与基线

### 2.1 正式路径

| 项目 | 固定值 |
| --- | --- |
| 活动数据根 | `C:\Users\William Feng\AppData\Roaming\FanglvCaseBoard\data` |
| 正式 DB | `<活动数据根>\caseboard.db` |
| 正式安装目录 | `C:\Users\William Feng\AppData\Local\方律案件看板` |
| 卸载注册表 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\方律案件看板` |
| 当前 WebView2 | `C:\Users\William Feng\AppData\Local\com.fanglv.caseboard\EBWebView` |
| 旧数据根 | `C:\Users\William Feng\AppData\Roaming\CaseBoard\data` |
| 旧 WebView2 | `C:\Users\William Feng\AppData\Local\app.caseboard.desktop\EBWebView` |

### 2.2 DEVICE Gate 基线

- 安装/注册表版本：0.8.2。
- 已安装 EXE SHA-256：`62160F3E7011ACDB6D2EC89C9D15C9962D7D7C6C23EB380D83DAC14F13DFF359`。
- 0.8.2 回滚 setup SHA-256：`443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312`。
- 当前数据根约 603.74 MiB；当前 WebView2 约 98.87 MiB；完整已知集合约 784.28 MiB。
- D 盘可用约 309.63 GiB；本包要求执行前至少仍有 5 GiB。

### 2.3 DB Gate 基线

| 文件 | 大小 | SHA-256 |
| --- | ---: | --- |
| `caseboard.db` | 556,773,376 | `A82C2A8F305351209DF082D661B3FE8A8DC3C89058E7A9BA929D27690F67DE3C` |
| `caseboard.db-wal` | 5,100,592 | `F00B1780FF873930ECF2AF3656FBD614C07B974DD6E278B1187DAA415158C466` |
| `caseboard.db-shm` | 32,768 | `CED9B3599A31DDD45CC519CB111CDAA5E07B7B9FA3CE42E8A2EA71AF4BB10C09` |

这些摘要只代表 2026-08-09 Gate 采样；正式执行时必须重新采样，不能要求继续等于旧值。

当前结构基线：

- `quick_check=ok`，FK 0，迁移 62 条、max 62、失败 0。
- 唯一 unknown applied version 为 36，可信 tuple 为：description `feishu reminder runs`、success=1、stored SQLx SHA-384 `84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480`，并绑定精确三列表结构。
- 非设备同步业务投影：`9B9A26C02803252D6FDE2C2FAB06EF5CB02F949720C1DAE9A6DBF805897B218F`。
- 设备同步组 1 行且 `paused=1`；outbox 487 行且均 exported；旧 quarantine 11 行。

以上状态来自 DB Gate 的临时一致性副本，不代表未来维护窗口的实时状态。执行时必须重新生成同口径基线。

## 三、备份目录与证据结构

正式执行时使用全新目录，不复用旧批次：

```text
D:\CodexWorkspace\008案件看板应用\formal-backups\V083-YYYYMMDD-HHMMSS\
├─ 00-manifest\             # 时间、操作者、进程、磁盘、路径、哈希、门禁结果
├─ 01-raw-data\             # 活动数据根原样副本，含 DB/WAL/SHM
├─ 02-legacy-data\          # 旧数据根原样副本
├─ 03-install\              # 正式安装目录与 0.8.2 setup
├─ 04-registry\             # 卸载项导出
├─ 05-webview\              # 当前/旧 WebView2（敏感，受控保存）
├─ 06-main-only\            # SQLite online backup 生成的 caseboard.db
├─ 07-v082-restore-proof\   # 0.8.2 隔离恢复证明，绝不复用作升级候选
├─ 08-v083-upgrade-proof\   # 0.8.3 首启/二启隔离证明
├─ 09-formal-post\          # 正式升级后审计与程序/注册表证据
└─ 10-rollback\             # 回滚动作日志；不放凭据正文
```

备份目录必须位于 BitLocker/等效加密且仅当前授权用户可访问的卷。逐文件 manifest 属私有证据，不在普通报告中列出业务文件名。

## 四、维护窗口检查包

### MW-0：前置接受

以下全部满足才能约维护窗口：

- `V083-M1-COMPAT36` 实现和独立复审均 `accepted`。
- 兼容补丁后的完整 Node/Rust/Clippy/source gate 全绿。
- 0.8.3 隔离 release EXE 已重新构建；正式安装前另需受控 CI NSIS setup + `.sig`。
- 本报告所列脚本 P1 已修复并通过新增测试；未修复时只能人工分阶段执行，禁止 `-Install` 一键模式。
- 0.8.2 rollback setup 再次核验为上述 SHA-256。
- D 盘可用空间不少于 5 GiB。

### MW-1：静止点

1. 两台物理设备都由用户确认设备同步暂停；台式机本地状态需重新采集 `paused/auto_paused`、outbox、quarantine、成员状态。不得读取事件正文或密钥。
2. NAS 路径保持不写；本包不探测 NAS。若无法证明另一设备不会写同步目录，停止。
3. 关闭应用，至少连续两次确认 `caseboard.exe=0`；确认没有以正式 WebView2 profile 运行的子进程。
4. 停止任何会触碰正式目录的备份、索引、同步或杀毒隔离任务；不关闭系统安全软件。
5. 记录 DB/WAL/SHM 的存在性、大小、LastWriteTime 和 SHA-256。任一文件在备份过程中变化，本批次作废。

只读进程/容量检查模板：

```powershell
$dataRoot = Join-Path $env:APPDATA 'FanglvCaseBoard\data'
$sourceDb = Join-Path $dataRoot 'caseboard.db'
$installRoot = Join-Path $env:LOCALAPPDATA '方律案件看板'
$running = @(Get-CimInstance Win32_Process | Where-Object { $_.Name -ieq 'caseboard.exe' })
if ($running.Count -ne 0) { throw 'STOP: caseboard.exe is running' }
$d = Get-CimInstance Win32_LogicalDisk -Filter "DeviceID='D:'"
if ($d.FreeSpace -lt 5GB) { throw 'STOP: D drive free space below 5 GiB' }
foreach ($p in @($sourceDb, "$sourceDb-wal", "$sourceDb-shm")) {
  if (-not (Test-Path -LiteralPath $p -PathType Leaf)) { throw "STOP: missing $p" }
  Get-Item -LiteralPath $p | Select-Object FullName,Length,LastWriteTime
  Get-FileHash -LiteralPath $p -Algorithm SHA256
}
```

## 五、原样受控备份

### BK-1：建立新批次

执行前人工替换时间戳，并确认目标不存在：

```powershell
$runId = 'V083-YYYYMMDD-HHMMSS'
$backupRoot = Join-Path 'D:\CodexWorkspace\008案件看板应用\formal-backups' $runId
if (Test-Path -LiteralPath $backupRoot) { throw 'STOP: backup root already exists' }
New-Item -ItemType Directory -Path $backupRoot | Out-Null
```

### BK-2：复制完整数据面

应用保持停止。`robocopy` 返回码 0—7 才可接受；任何大于 7 的值均停止。活动数据根必须整体复制，DB/WAL/SHM 不得拆批、跳过或删除。

```powershell
function Invoke-RobocopyChecked([string]$Source,[string]$Destination,[string[]]$Extra=@()) {
  & robocopy.exe $Source $Destination /E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /XJ @Extra
  if ($LASTEXITCODE -gt 7) { throw "STOP: robocopy failed with $LASTEXITCODE" }
}

Invoke-RobocopyChecked $dataRoot (Join-Path $backupRoot '01-raw-data\data')
Invoke-RobocopyChecked (Join-Path $env:APPDATA 'CaseBoard\data') (Join-Path $backupRoot '02-legacy-data\data')
Invoke-RobocopyChecked $installRoot (Join-Path $backupRoot '03-install\方律案件看板')
Invoke-RobocopyChecked (Join-Path $env:LOCALAPPDATA 'com.fanglv.caseboard\EBWebView') (Join-Path $backupRoot '05-webview\current')
Invoke-RobocopyChecked (Join-Path $env:LOCALAPPDATA 'app.caseboard.desktop\EBWebView') (Join-Path $backupRoot '05-webview\legacy')
```

若任一可选旧目录不存在，应在 manifest 记录 `not_present`，不能让空目录冒充成功复制。

### BK-3：安装项和注册表

```powershell
$rollbackSetup = 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.2-dev\agent-work\output\V082-FORMAL-1785729109360\public-download\FanglvCaseBoard_0.8.2_x64-setup.exe'
if ((Get-FileHash -LiteralPath $rollbackSetup -Algorithm SHA256).Hash -ne '443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312') {
  throw 'STOP: rollback setup hash mismatch'
}
Copy-Item -LiteralPath $rollbackSetup -Destination (Join-Path $backupRoot '03-install')
New-Item -ItemType Directory -Path (Join-Path $backupRoot '04-registry') -Force | Out-Null
& reg.exe export 'HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\方律案件看板' (Join-Path $backupRoot '04-registry\uninstall.reg') /y
if ($LASTEXITCODE -ne 0) { throw 'STOP: registry export failed' }
```

### BK-4：原样副本验收

1. 再次确认正式进程为 0。
2. 重新取得正式 DB/WAL/SHM 的大小、时间和 SHA-256；必须与 BK-2 前一致。
3. 原样副本三文件的 SHA-256 必须逐项等于同批次源摘要。
4. 对全部备份文件生成私有相对路径/大小/SHA-256 manifest；不得把 manifest 发到公开日志。
5. 若源发生变化、复制缺失或 manifest 失败：保留失败批次供审计，换新 runId 重做；不得覆盖。

## 六、SQLite online main-only

### OL-1：生成

只在 BK-4 通过后执行。SQLite backup API 读取主库 + WAL 的一致视图，目的文件必须不存在。该动作会读取正式 SQLite 文件，但不应改变源；执行前后仍须以三文件字节事实验证。

```powershell
$mainOnly = Join-Path $backupRoot '06-main-only\caseboard.db'
New-Item -ItemType Directory -Path (Split-Path $mainOnly -Parent) | Out-Null
$env:V083_SOURCE_DB = $sourceDb
$env:V083_MAIN_ONLY = $mainOnly
@'
import os, sqlite3
from pathlib import Path

source = Path(os.environ['V083_SOURCE_DB']).resolve(strict=True)
target = Path(os.environ['V083_MAIN_ONLY']).resolve()
if target.exists():
    raise SystemExit('STOP: destination exists')
src = sqlite3.connect(source.as_uri() + '?mode=ro', uri=True)
src.execute('PRAGMA query_only=ON')
dst = sqlite3.connect(target)
try:
    src.backup(dst)
    if dst.execute('PRAGMA quick_check').fetchone()[0] != 'ok':
        raise SystemExit('STOP: quick_check failed')
    if dst.execute('PRAGMA foreign_key_check').fetchall():
        raise SystemExit('STOP: foreign_key_check failed')
finally:
    dst.close()
    src.close()
'@ | python -
```

### OL-2：验收

- `06-main-only` 中只能有主库，不得存在 `caseboard.db-wal` 或 `caseboard.db-shm`。
- 以 DB Gate 的完整审计口径重新生成迁移 tuple/checksum、schema sentinel、全部计数和非设备同步业务投影。
- 预升级期望：62 条迁移、max 62、失败 0；只有 version 36 不在嵌入集合，且严格等于可信 tuple；0063 sentinel 全部未出现；业务投影应等于同批次原样 WAL 一致视图，不应机械要求等于 8 月 9 日旧摘要。
- online backup 前后正式三文件大小、时间、SHA-256必须相同；否则该 main-only 批次无效。

`scripts/windows-upgrade-validation/db_audit.py` 当前不能完整完成 OL-2，见第十一节，需补丁后才可作为正式证据工具。

## 七、0.8.2 隔离恢复证明

目的：证明“安装失败后，把 main-only 备份恢复给 0.8.2”确实可启动，而不是只证明 SQLite 可读。

安全前提：0.8.2/0.8.3 启动会读取 Windows Credential Manager 的状态。仅设置 `CASEBOARD_DATA_DIR` 和 `WEBVIEW2_USER_DATA_FOLDER` 不能隔离凭据。因此必须在以下环境之一运行：

- 可丢弃 Windows VM；或
- 全新、无任何方律正式凭据的本地测试用户。

当前正式 Windows 用户下不得运行该证明。

步骤：

1. 从 `06-main-only\caseboard.db` 新复制一份到 `07-v082-restore-proof\data\caseboard.db`；不复制正式 settings，使用默认/空设置。
2. 使用 BK-2 复制的 0.8.2 `caseboard.exe`，先核验版本与 SHA-256。
3. 设置独立 `CASEBOARD_DATA_DIR`、`WEBVIEW2_USER_DATA_FOLDER`、ASCII TEMP/TMP；启动 20 秒并确认渲染。
4. 使用 `CloseMainWindow()` 请求正常退出并等待；只有正常退出才计入证明。若必须 `Stop-Process -Force`，该轮作废，并从 main-only 重新创建新的 proof 目录。
5. 退出后对 proof DB 做 `quick_check`、FK、迁移、业务投影；记录 sidecar 状态。该目录仅用于回滚证明，绝不作为 0.8.3 升级输入。

接受条件：0.8.2 正常启动/渲染/正常退出；DB 健康；迁移仍 max 62；非设备同步业务投影与该轮启动前一致；未连接正式凭据、正式 WebView2、正式 DB 或 NAS。

## 八、兼容补丁后的 0.8.3 隔离升级与二次启动

仅在 `V083-M1-COMPAT36` accepted 后执行。

1. 从原始 main-only 再建立全新的 `08-v083-upgrade-proof\data\caseboard.db`，不能复用 0.8.2 proof。
2. 在同一无正式凭据的 VM/测试用户中运行兼容补丁后的 0.8.3 EXE；继续隔离 data/WebView2/TEMP。
3. 第一次启动应识别唯一 version 36 tuple，实际运行生产 `init_pool` 并迁移至 0063；正常退出，不强杀。
4. 第一次退出后若存在任何 WAL/SHM，立即停止并保留现场；不得删除 sidecar 后冒充二启通过。
5. 审计第一次结果：`quick_check=ok`、FK 0；迁移 63 条/max 63/失败 0；version 36 原记录不变；version 63 checksum 为 `D5309B70309D5B7465741253E83FEF71AE6BBCE12ACC160C13E8BF8CC373D8F92CD7403E3ACE2578D36274546FDD229B`；0063 全 sentinel 通过。
6. 非设备同步业务投影必须与升级前一致。设备同步期望：组仍暂停；outbox 487 行且 capture_sequence 1—487、无 0/重复；旧 quarantine 11 行全部进入 `manual_review` 且不伪造真实包身份；export drafts 0 行。
7. 第二次启动同一 proof DB；再次正常退出。迁移历史、schema、业务投影以及设备同步计数/投影必须与第一次退出后一致；不得产生第二次迁移或自动恢复同步。

任一断言失败，兼容补丁或关闭流程退回修复，不进入正式调和。

## 九、正式 sidecar 调和

这是本包中唯一会替换正式活动数据根的步骤，必须获得用户针对该维护窗口的单独授权。

### SW-1：在同卷构造候选数据根

在 `%APPDATA%\FanglvCaseBoard` 下创建候选 sibling，确保和活动数据根同属 C 盘。复制正式数据根中的非数据库内容，但不把三文件复制进候选；再将已验证的 main-only 复制为候选 `caseboard.db`。原活动三文件仍原地保留。

```powershell
$parent = Split-Path $dataRoot -Parent
$candidate = Join-Path $parent "data.candidate-$runId"
$retired = Join-Path $parent "data.pre-v083-$runId"
if ((Test-Path -LiteralPath $candidate) -or (Test-Path -LiteralPath $retired)) { throw 'STOP: swap target exists' }
New-Item -ItemType Directory -Path $candidate | Out-Null
& robocopy.exe $dataRoot $candidate /E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /XJ /XF caseboard.db caseboard.db-wal caseboard.db-shm
if ($LASTEXITCODE -gt 7) { throw 'STOP: candidate copy failed' }
Copy-Item -LiteralPath $mainOnly -Destination (Join-Path $candidate 'caseboard.db')
if (Test-Path -LiteralPath (Join-Path $candidate 'caseboard.db-wal')) { throw 'STOP: candidate WAL exists' }
if (Test-Path -LiteralPath (Join-Path $candidate 'caseboard.db-shm')) { throw 'STOP: candidate SHM exists' }
```

候选建立后再次运行完整只读审计并重新确认进程=0、正式三文件未变化、candidate 位于预期 parent。任何审计动作产生 sidecar，即作废候选并换新名字重建；不在候选中删除后继续。

### SW-2：两次同卷目录重命名

Windows 目录不存在真正的双目录原子交换；本方案使用两次同卷原子 rename，并在第二次失败时立即把旧目录 rename 回去。应用全程停止。

```powershell
$expectedParent = [IO.Path]::GetFullPath((Join-Path $env:APPDATA 'FanglvCaseBoard'))
foreach ($p in @($dataRoot,$candidate,$retired)) {
  if (-not [IO.Path]::GetFullPath($p).StartsWith($expectedParent + [IO.Path]::DirectorySeparatorChar,[StringComparison]::OrdinalIgnoreCase)) {
    throw "STOP: path escaped expected parent: $p"
  }
}
if (@(Get-Process caseboard -ErrorAction SilentlyContinue).Count -ne 0) { throw 'STOP: app restarted' }
[IO.Directory]::Move($dataRoot,$retired)
try {
  [IO.Directory]::Move($candidate,$dataRoot)
} catch {
  if (-not (Test-Path -LiteralPath $dataRoot) -and (Test-Path -LiteralPath $retired)) {
    [IO.Directory]::Move($retired,$dataRoot)
  }
  throw
}
```

结果：

- 新活动 `data` 包含经验证的 main-only DB 和原有非数据库文件，不含 WAL/SHM。
- 旧活动目录完整保留为 `data.pre-v083-*`，其中原 DB/WAL/SHM 未删除、未 checkpoint、未拆散。
- D 盘仍有第二份原样数据根与 main-only 备份。

目录 swap 完成后、安装前再次核验新活动 DB 摘要/审计等于已验 candidate。任何不一致立即 rename 回滚，不启动 0.8.3。

## 十、正式安装、验收与回滚

### FI-1：安装与首启

1. 再次核验 0.8.3 setup SHA-256、updater `.sig`、minisign、commit/version；setup 必须为受控 CI 资产。
2. 安装 0.8.3，核对安装 EXE 与 HKCU DisplayVersion 均为 0.8.3。
3. 正式首启使用正式 data，但 WebView2 不临时改到别处，以免把隔离 profile 冒充真实升级；设备同步保持暂停，禁止飞书写回。
4. 正常退出后做与第八节相同的完整审计；确认首次结果与隔离升级证明一致。
5. 正式第二次启动/正常退出，再做幂等审计。二启前若出现 sidecar，按产品门禁判失败，不删除。
6. 在回滚窗口关闭前，不删除 `data.pre-v083-*`、D 盘备份、旧 WebView2、旧数据根或 0.8.2 setup，不发布 latest。

### RB-1：应用/数据回滚

触发任一迁移、完整性、指纹、二启、凭据或同步异常后：

1. 停止 0.8.3，确认进程及正式 WebView2 子进程为 0。
2. 把失败后的新活动 `data` 同卷 rename 为 `data.failed-v083-*`；不删除。
3. 把 `data.pre-v083-*` 原样 rename 回 `data`。这一步把原 0.8.2 DB/WAL/SHM 整套恢复，绝不让 0.8.2 看见 0063 数据。
4. 安装已核验的 0.8.2 setup；保留 Windows Credential Manager，不清理凭据。
5. 用 0.8.2 做离线最小验证，核对版本、quick/FK、迁移 max 62 和安装前业务投影。
6. 正式同步继续暂停；NAS、隔离包、同步组和远端恢复由单独用户确认决定，不自动重放、删除或重建。

若 C 盘保留的 `data.pre-v083-*` 不可用，则从 D 盘 `01-raw-data\data` 恢复整套目录；不能只恢复主库，也不能把 `06-main-only` 和旧 WAL/SHM 混合。

## 十一、现有脚本审计：必须补丁

现有单元测试 7/7 通过，但只证明旧契约；不足以关闭本轮正式门禁。

### 11.1 `db_audit.py`

必须补：

1. `snapshot()` 目前直接以 `mode=ro` 打开传入 DB，并自动统计全部表；正式路径不应作为普通 snapshot 输入。应新增明确的 formal-backup 阶段：先记录源三文件事实和进程，执行 online backup，再记录源三文件事实；结构/业务查询只针对 backup。
2. online backup 目前只验证 `quick_check`，缺 `foreign_key_check`、main-only 无 sidecar断言和源三文件不变断言。
3. 迁移记录只取 version/description/success，缺 stored checksum、execution_time、version 36 可信 tuple 和 version 63 checksum。
4. compare 只比较表行数，并允许 runtime allowlist 增长；无法证明业务行内容不变。应纳入 DB Gate 的逐表流式指纹、非设备同步总投影、schema hash 和 migration history hash。
5. 缺 0063 table/column/index/FK sentinel、outbox capture_sequence、quarantine 生命周期和二次启动幂等断言。
6. 文件事实缺 LastWriteTime、完整 DB/WAL/SHM 组状态和阶段化证据链。

建议把 DB Gate 的 `audit_snapshot.py` 能力整理进正式 `db_audit.py`，补充合成测试后再使用；线程临时脚本本身不能未经复审直接成为发布工具。

### 11.2 `Invoke-UpgradeValidation.ps1`

必须补：

1. 当前 formal 模式把正式 source snapshot、isolated backup、单次隔离启动、第二次 formal backup、静默安装和正式首启串在一次调用中，没有人工审阅/授权断点；应拆成可恢复的独立阶段和不可伪造的 resume manifest。
2. 当前不实现原样完整数据根、安装目录、注册表、WebView2 和 DB/WAL/SHM 成组备份。
3. 当前不构造/验收 main-only candidate，也不做活动目录同卷 rename；直接安装后会被正式 sidecar 阻断。
4. 当前隔离/正式应用都只启动一次，缺二次启动幂等证明。
5. 当前通过 `Stop-Process -Force` 收尾；这可能制造 WAL/SHM，并使二次启动证明无效。应先正常关窗和有界等待，强杀只能标记该轮失败并从干净 main-only 重新开始。
6. 当前只隔离 DB/WebView2，不隔离 Windows Credential Manager；必须增加“VM/全新 Windows 用户”外部门禁，或另行实现只在测试启用的凭据后端。
7. 当前 compare 只看表计数和 runtime allowlist，需改为本报告的完整指纹/迁移/sentinel断言。
8. 当前 formal 启动使用临时 WebView2 profile，不能证明真实正式 profile 升级；应把隔离证明与正式首启证明分开，不混用结论。
9. 缺失败后自动保留新活动目录、恢复 `data.pre-v083-*` 和 0.8.2 恢复验证的分支。
10. 脚本不得包含删除正式 sidecar、修改 `_sqlx_migrations`、自动恢复同步、自动发布 tag/Release/latest 的路径。

### 11.3 必增测试

- online backup 合并非零 WAL，源三文件前后字节事实不变，目标 main-only 且 quick/FK 通过。
- version 36 正例和 checksum/description/schema/额外 unknown 负例。
- 0062→0063 首启、正常退出、无 sidecar、二启幂等。
- 非设备同步业务行内容改变但行数不变时仍失败。
- 强杀后 sidecar 存在必须失败，不允许删除后继续。
- swap 第二次 rename 失败时旧目录可原样恢复；任何路径逃逸、目标已存在或跨卷均 fail closed。
- 0.8.3 失败后 0.8.2 只能接收 pre-v083 整目录，绝不能接收 0063 DB。

## 十二、最终放行矩阵

| 门禁 | 当前状态 |
| --- | --- |
| DEVICE Gate 路径/容量/rollback setup | `passed` |
| DB Gate 健康性与正式 checksum 来源 | `passed_read_only` |
| `V083-M1-COMPAT36` 实现/复审 | `in_progress / blocked_internal` |
| 现有脚本适配本执行包 | `requires_patch / blocked_internal` |
| 安装时点原样备份 | `not_run` |
| SQLite main-only backup | `not_run` |
| 0.8.2 隔离恢复证明 | `not_run` |
| 0.8.3 隔离升级/二启 | `not_run` |
| 正式同卷目录调和 | `not_run_requires_explicit_authorization` |
| 正式 0.8.3 setup/签名链 | `blocked_external` |
| 正式安装放行 | `blocked` |

请主控独立复核。本线程不写 `accepted`，也不执行任何正式备份、启动、安装、sidecar 调和或回滚。
