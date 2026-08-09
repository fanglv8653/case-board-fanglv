# V083-FORMAL-DEVICE-GATE｜本设备正式安装与回滚门禁

- 逻辑线程：`worker-formal-device-gate`
- 交付状态：`submitted_for_review`
- 盘点时间：2026-08-09 21:55—21:59（Asia/Shanghai）
- 当前门禁结论：`blocked_external`，**不得直接安装或启动 0.8.3**
- 本轮操作边界：仅查询安装项、进程、文件/目录元数据、磁盘空间和源码存储约定；未启动或停止进程，未安装、卸载、修改、删除或备份任何正式文件；未打开 `settings.json`、`crash.log`、数据库业务表或 NAS 内容；未调用或枚举 Windows 凭据管理器。

## 一、结论先行

本机正式 0.8.2 安装真实存在，程序文件、卸载项和注册表版本一致；两次进程采样均未发现 `caseboard.exe`。本机也保留了已公开 0.8.2 安装包，SHA-256 与发布记录一致，C/D 盘空间足够形成多份回滚副本。

但当前仍不能进入安装：

1. 正式数据目录中存在非零 `caseboard.db-wal`（5,100,592 bytes）和 `caseboard.db-shm`（32,768 bytes）。0.8.3 的 `db::init_pool` 会在第一次 SQLite 连接前拒绝任何 WAL/SHM sidecar；若直接安装后启动，将被迁移安全门禁阻断。
2. 0.8.3 目前只有本地 `--no-bundle` release EXE，`target/release/bundle` 不存在，EXE 为 `NotSigned`；公开 `release/latest.json` 仍为 0.8.2。正式 NSIS setup、updater `.sig`、minisign 校验链尚未产生。
3. 本轮遵守“不得读正式正文/凭据”边界，未读取当前数据库中的迁移 checksum、设备同步 `paused` 值、同步组标识或 `connector_root`。项目日志仅能证明 2026-08-03 两端曾确认暂停，不能替代安装当日重新核验。
4. 尚未生成“安装时点”的新鲜一致性备份，也未验证该备份能由 0.8.2 恢复。8 月 3 日旧备份不能覆盖 8 月 7 日以后正式库/WAL 的变化。

因此当前只能确认“设备具备准备条件”，不能确认“可以安装”。

## 二、正式安装与卸载信息

| 项目 | 只读盘点结果 | 状态 |
| --- | --- | --- |
| 安装作用域 | 当前用户（HKCU） | `passed` |
| 卸载注册表 | `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\方律案件看板` | `passed` |
| DisplayName / Publisher | 方律案件看板 / 江苏漫修(无锡)律师事务所 | `passed` |
| DisplayVersion | 0.8.2 | `passed` |
| InstallLocation | `C:\Users\William Feng\AppData\Local\方律案件看板` | `passed` |
| 主程序 | `C:\Users\William Feng\AppData\Local\方律案件看板\caseboard.exe` | `passed` |
| 卸载程序 | `C:\Users\William Feng\AppData\Local\方律案件看板\uninstall.exe` | `passed` |
| 主程序版本 | FileVersion 0.8.2；ProductVersion 0.8.2；ProductName 方律案件看板 | `passed` |
| 主程序大小/摘要 | 19,174,400 bytes；SHA-256 `62160F3E7011ACDB6D2EC89C9D15C9962D7D7C6C23EB380D83DAC14F13DFF359` | `passed` |
| 主程序 Authenticode | `NotSigned`（如实记录，不与 updater minisign 混淆） | `observed` |
| 卸载程序版本/摘要 | 0.8.2；79,508 bytes；SHA-256 `B3F1640DF6CB6976FAFB1D30175186E17E86050A0F3A17FA8F681D1DD2D0948B` | `passed` |
| 当前进程 | 两次采样均无 `caseboard.exe` 或来自正式安装目录的方律案件看板进程 | `passed_at_observation` |

安装目录共 33 个文件、19,505,337 bytes（18.60 MiB）。本轮未列出其余文件名，避免扩大盘点范围。

## 三、正式数据、设置、日志与 WebView2 位置

### 3.1 当前数据根

源码约定 `ProjectDirs("", "", "FanglvCaseBoard")`，Windows 当前数据根为：

`C:\Users\William Feng\AppData\Roaming\FanglvCaseBoard\data`

该目录存在，共 632 个文件、633,063,516 bytes（603.74 MiB）。仅对下列已知路径读取了文件元数据，未读取内容：

| 文件 | 存在 | 大小 | 最后写入 | 状态 |
| --- | ---: | ---: | --- | --- |
| `caseboard.db` | 是 | 556,773,376 bytes | 2026-08-07 09:49:50 | `observed` |
| `caseboard.db-wal` | 是 | 5,100,592 bytes | 2026-08-07 11:14:38 | `blocking` |
| `caseboard.db-shm` | 是 | 32,768 bytes | 2026-08-05 14:27:10 | `blocking` |
| `settings.json` | 是 | 2,291 bytes | 2026-07-29 03:16:13 | `observed_not_opened` |
| `crash.log` | 否 | — | — | `not_present` |

`settings.json` 与数据库同目录；`crash.log` 只会在 panic 记录时追加到同一数据根。本轮没有打开 settings，也没有因“确认无秘密”而扫描其正文。

### 3.2 旧版数据根

兼容旧目录仍存在：

`C:\Users\William Feng\AppData\Roaming\CaseBoard\data`

聚合元数据为 8 个文件、942,971 bytes（0.90 MiB），其中旧 `caseboard.db`、WAL、SHM 和 `settings.json` 均存在。该目录不能在升级时自动清理；应随安装前备份一起保留，待 0.8.3 正式验收后再单独判断是否归档。

### 3.3 WebView2

| 用途 | 路径 | 聚合元数据 | 状态 |
| --- | --- | --- | --- |
| 当前标识 `com.fanglv.caseboard` | `C:\Users\William Feng\AppData\Local\com.fanglv.caseboard\EBWebView` | 620 文件；103,669,144 bytes（98.87 MiB） | `observed` |
| 旧标识 `app.caseboard.desktop` | `C:\Users\William Feng\AppData\Local\app.caseboard.desktop\EBWebView` | 687 文件；56,261,704 bytes（53.66 MiB） | `observed_legacy` |

WebView2 可能包含登录会话、缓存及本地浏览器状态，备份时按敏感数据处理，不逐文件列名、不进入普通日志或报告。旧 profile 同样不得在升级时顺手删除。

## 四、凭据与设备同步元数据位置

以下结论已对照 0.8.2 与 0.8.3 源码的存储契约，仅说明位置/命名空间；两版所列根目录和凭据服务前缀一致。本轮没有读取凭据存在状态或秘密正文。

### 4.1 Windows Credential Manager

| 类别 | Windows Generic Credential 目标命名空间 | 本轮动作 |
| --- | --- | --- |
| 静态服务商、MCP、团队、TickTick 等 | `com.fanglv.caseboard.credentials.v1/<scope>/<owner>/<slot>` | 只读源码定位；未查询凭据库 |
| 飞书 OAuth | `com.fanglv.caseboard.feishu/<app-id>:<kind>` | 只读源码定位；未查询凭据库 |
| 设备同步密钥 | `FanglvCaseBoard/device-sync/<group-id>/<device-id>/<kind>` | 只读源码定位；未查询凭据库 |

设备同步密钥类别包括签名密钥、交换密钥、分组密钥代次及一次性邀请码。任何备份流程均不得将这些值明文写入工作区、报告、命令参数或日志。

### 4.2 本地 SQLite

设备同步组、成员、暂停/自动暂停、待发送事件、已应用操作、冲突、隔离、审计、快照与导出草稿等元数据均位于当前 `caseboard.db` 的 `device_sync_*` 表。NAS 选择根路径保存在 `device_sync_groups.connector_root`。

本轮没有打开正式数据库，因此：

- 当前 `paused/auto_paused`：`not_run`；仅有项目日志记录 2026-08-03 两端曾暂停。
- 当前同步组、成员、隔离计数、pending outbox：`not_run`。
- 当前 `connector_root` 的真实路径：`not_run`。

以上三项必须从安装时点的一致性副本或经用户明确授权的正式状态页重新核验，不能沿用旧结论。

### 4.3 NAS 挂载目录

源码布局为：

`<connector_root>\fanglv-caseboard-sync\groups\<group-id>\`

组目录含 `members`、`invites`、`events`、`receipts`、`snapshots`、`manifests`、`quarantine` 等。由于本轮未读取 `connector_root`，没有探测、列举或访问任何 NAS 路径。

## 五、回滚资产与容量

### 5.1 0.8.2 回滚安装包

本机存在公开回下载的 0.8.2 安装包：

`D:\CodexWorkspace\008案件看板应用\case-board-v0.8.2-dev\agent-work\output\V082-FORMAL-1785729109360\public-download\FanglvCaseBoard_0.8.2_x64-setup.exe`

- 大小：8,936,055 bytes
- SHA-256：`443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312`
- FileVersion / ProductVersion：0.8.2
- Authenticode：`NotSigned`
- 摘要与项目日志中 0.8.2 正式发布记录一致

该文件可作为程序回滚资产，但仍应在正式升级包齐备时再次核验 updater minisign/发布来源；安装程序本身不包含正式业务数据与 Windows 凭据。

### 5.2 空间

| 卷 | 可用空间 | 判断 |
| --- | ---: | --- |
| C: | 97,896,857,600 bytes（91.17 GiB） | 足够保留当前安装与临时验证空间 |
| D: | 332,467,859,456 bytes（309.63 GiB） | 足够建立多份加密/访问受控备份 |

已盘点的当前安装、当前数据、当前 WebView2、两套旧目录和 0.8.2 回滚安装包合计约 784.28 MiB。建议安装前备份目的卷至少预留 5 GiB（约为已知集合三倍再加 2 GiB 验证余量）；D 盘满足。

空间足够不等于备份已完成。当前仍无安装时点的可恢复备份。

## 六、安全备份清单（后续须另行授权执行）

### A. 静止点

1. 在两台正式设备上重新确认设备同步为人工暂停；记录 `paused/auto_paused`、pending outbox、active/manual_review quarantine 和成员状态，但不得输出业务事件正文或密钥。
2. 关闭方律案件看板，确认 `caseboard.exe` 及其 WebView2 子进程均为 0；连续两次采样无重新拉起。
3. 停止一切会写正式 DB/NAS 的计划任务或辅助脚本；同步目录不得由另一设备继续写入。

### B. 原样证据副本

在应用完全停止后，将以下内容复制到 D 盘新的时间戳目录，并限制访问权限：

1. 完整当前数据根（必须把 DB/WAL/SHM 作为同一静止点的一组复制，不能只复制 `caseboard.db`）。
2. 完整旧版数据根。
3. 当前与旧版 WebView2 profile。
4. 完整安装目录、HKCU 卸载项导出、已安装 EXE/卸载器版本与 SHA-256。
5. 0.8.2 官方回下载 setup 及其摘要/签名证据。
6. 在取得 `connector_root` 后，对整个当前同步组目录做同一静止点副本；不得只挑 `.cbe/.cbm` 文件，也不得删除现有隔离项。

### C. 可恢复数据库副本

1. 只在原样副本完成且校验通过后，针对**副本**合并 WAL 并生成单文件一致性数据库。
2. 对单文件副本执行 `quick_check`、`foreign_key_check`、迁移版本/完整 checksum/sentinel 核验；业务数据只做计数/稳定指纹，不写入报告正文。
3. 将副本恢复到新的隔离目录，用 0.8.2 打开并验证最小关键路径后关闭；不得拿 0.8.3 迁移后的库做 0.8.2 回滚验证。
4. 正式活动目录中的 WAL/SHM 调和（checkpoint、替换主库、移出 sidecar 等）属于数据修改，必须在备份验证通过后由用户另行授权；本轮未执行。

### D. 凭据与恢复材料

1. 不做明文导出，不调用凭据枚举命令。升级同一 Windows 用户时应保留现有 Credential Manager，不卸载/删除凭据。
2. 对供应商、飞书、团队和 TickTick，确认存在可重新授权/重新录入的用户控制路径；记录“可恢复性已确认”，不记录值。
3. 设备同步密钥不能仅靠复制 DB 恢复。须确认受控的设备同步恢复包/恢复流程可用，或保留 Windows 用户配置文件级系统备份；不把密钥写入普通文件。
4. 若任一同步设备、组密钥或恢复材料不可恢复，保持同步暂停并阻断升级。

## 七、安装前停止条件

任一项命中即停止，不进入安装：

| 停止条件 | 当前状态 |
| --- | --- |
| 0.8.3 正式 NSIS setup、同名 updater `.sig`、SHA-256/minisign 链未齐 | **已命中** |
| `release/latest.json` 仍为 0.8.2，或资产并非受控 CI 输出 | **已命中** |
| 正式 DB 存在任何 `-wal` / `-shm` | **已命中** |
| 安装时点新鲜备份尚未完成、未校验或未做 0.8.2 恢复演练 | **已命中** |
| 正式 0.8.2 迁移 checksum/sentinel 未从一致性副本核验 | **已命中** |
| 两台设备同步的当日暂停状态、待发送/隔离状态和 NAS 静止点未重验 | **已命中** |
| 凭据/设备同步恢复路径不能证明可恢复 | **已命中** |
| `caseboard.exe` 或其 WebView2 子进程仍在运行 | 当前未命中，但安装时必须重验 |
| C/D 盘空间低于 5 GiB 安全余量 | 当前未命中 |
| 0.8.2 回滚安装包摘要不匹配 | 当前未命中 |

## 八、建议的最优正式执行顺序

只有在主控和用户另行授权后，按以下顺序执行：

1. `G0 资产门禁`：受控 CI 基于锁定 commit 生成 0.8.3 NSIS setup + `.sig`，核验 commit/version、SHA-256、minisign；此时不推 tag/Release，也不改 latest。
2. `G1 状态门禁`：选定维护窗口；两设备暂停同步；采集非秘密同步状态；确认应用和 WebView2 子进程已停。
3. `G2 原样备份`：同一静止点备份当前/旧数据根、DB/WAL/SHM、WebView2、安装目录、卸载项、0.8.2 installer 和完整同步组目录；逐文件摘要入私有清单。
4. `G3 恢复证明`：在隔离目录合并 WAL，核验数据库完整性/迁移谱系，并用 0.8.2 完成恢复演练。
5. `G4 sidecar 调和`：在明确授权下把正式活动库转换为经核验的单文件一致状态，确保活动目录无 WAL/SHM；再次哈希/结构核验。若不能无歧义完成，立即回滚到 G2 原样副本。
6. `G5 安装`：仅运行已验签的 0.8.3 setup；不先更新 public latest，不删除旧数据、旧 WebView2、凭据或同步组。
7. `G6 首启验收`：首次启动先验证迁移提示/版本/数据库完整性和关键业务稳定指纹；保持设备同步暂停，不做飞书写回。
8. `G7 回滚窗口`：完成离线最小功能验收后再决定是否继续物理双端隔离同步验收；正式同步组恢复应是独立用户确认动作。
9. `G8 发布`：物理端与回滚验证均通过后，才允许 tag/Release/latest 快进和 updater 验收。

## 九、回滚方式

### 触发条件

首次启动出现迁移谱系错误、数据库完整性/FK 异常、关键稳定指纹变化、启动循环、凭据不可用、同步状态异常或无法保持暂停，立即回滚；不在正式库上继续尝试“修一下再开”。

### 回滚顺序

1. 停止 0.8.3 及其 WebView2 子进程，保留失败现场的完整副本用于诊断。
2. 卸载 0.8.3 或覆盖安装已核验的 0.8.2 setup；不删除 Credential Manager 项。
3. **绝不让 0.8.2 打开已经迁移到 0063 的数据库。** 先移出失败后的活动数据根，再恢复 G2/G3 中的 0.8.2 安装前一致性副本。
4. 恢复当前数据根、必要的旧目录和 WebView2 profile；恢复时保持 NAS 与两端同步暂停。
5. 用 0.8.2 只做本地离线恢复验证；核验版本、`quick_check`、FK、迁移版本和关键稳定指纹。
6. 只有本地回滚验收通过且双端/NAS 状态重新核对后，才由用户决定是否恢复设备同步。不得自动重放隔离包、自动删除 NAS 文件或重建同步组。

## 十、本轮状态矩阵

| 项目 | 状态 |
| --- | --- |
| 正式安装/版本/卸载项盘点 | `passed` |
| 当前正式进程采样 | `passed_at_observation` |
| 数据/settings/crash/WebView2 位置盘点 | `passed_metadata_only` |
| 凭据存储位置确认 | `passed_source_contract_only` |
| 凭据存在性/正文 | `not_run_by_design` |
| 当前设备同步状态/NAS 路径 | `not_run_by_design` |
| 备份容量 | `passed` |
| 0.8.2 回滚 installer | `passed` |
| 安装时点一致性备份/恢复演练 | `not_run` |
| 正式 0.8.3 bundle/签名链 | `blocked_external` |
| 正式安装放行 | `blocked_external` |

请主控独立复核。本线程不写 `accepted`，也不执行安装、备份或 sidecar 调和。
