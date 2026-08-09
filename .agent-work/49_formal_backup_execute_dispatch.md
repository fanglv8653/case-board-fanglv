# V083-FORMAL-BACKUP-EXECUTE 派工单

## 授权与目标

用户已确认进入正式发布验收并先在本设备验收。本任务只建立可恢复的正式原样备份与 SQLite main-only 一致性副本，不启动/安装应用，不切换正式目录，不删除/修改 sidecar，不访问 NAS 或凭据内容。

固定 run id：`V083-20260809-234237`

固定输出父目录：`D:\CodexWorkspace\008案件看板应用\formal-backups`。该父目录已由主控验证为 EFS AES-256，解密者仅当前 Windows 用户；ACL 仅当前用户、SYSTEM、Administrators。

## 执行前门禁

1. `caseboard.exe` 必须为 0；进程枚举错误即停止。
2. 固定源 DB：`C:\Users\William Feng\AppData\Roaming\FanglvCaseBoard\data\caseboard.db`；main/WAL/SHM 必须存在，`-journal` 必须不存在。
3. 输出 run root 必须不存在；D 盘剩余空间至少 5 GiB。
4. 核验 rollback setup：
   `D:\CodexWorkspace\008案件看板应用\case-board-v0.8.2-dev\agent-work\output\V082-FORMAL-1785729109360\public-download\FanglvCaseBoard_0.8.2_x64-setup.exe`
   SHA-256 必须为 `443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312`。

## 执行

1. 使用已 accepted 的 R3 工具执行 `Backup`：
   - `-SourceDatabase` 固定源 DB；
   - `-OutputDirectory` 固定父目录；
   - `-RunId V083-20260809-234237`。
2. 使用返回的 manifest 路径和 SHA 执行 `AuditCopy`，`-MigrationsDirectory` 指向仓库 `src-tauri\migrations`。
3. 在同一 run root 建立补充目录并原样复制（只复制，不读取内容）：
   - `01-raw-data\data`：完整正式数据根，含 DB/WAL/SHM；
   - `02-legacy-data\data`：旧数据根，若不存在则 manifest 记 `not_present`；
   - `03-install\方律案件看板`：当前安装目录；并复制已核验 0.8.2 rollback setup；
   - `04-registry\uninstall.reg`：导出 HKCU 卸载项；
   - `05-webview\current` 与 `05-webview\legacy`：若存在则原样复制，不解析内容。
4. `robocopy` 仅接受返回码 0-7；使用 `/E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /XJ`。
5. 生成私有 `supplementary-manifest.json`：相对路径、大小、SHA-256、源存在状态、复制时间；不得输出业务文件内容。

## 验收硬断言

- Backup/AuditCopy manifest HMAC/SHA 链通过；main-only 无 WAL/SHM/journal，quick=ok、FK=0。
- 正式源 main/WAL/SHM 在全部复制前后 size、mtime、SHA-256 完全一致；正式 `-journal` 仍不存在。
- `01-raw-data` 三件套逐文件 SHA 等于同批次源。
- 完整迁移审计：62 条、max 62、失败 0；唯一 embedded 缺口为可信 v36 tuple；0063 未应用。
- 正式数据根、正式安装目录、正式注册表、正式 WebView 未被修改；应用保持 0 进程。
- run root 及其文件保持 EFS 加密继承，ACL 不宽于父目录。

## 报告

写 `.agent-work/output/V083-FORMAL-BACKUP-EXECUTE.md`，列出路径、非敏感摘要、manifest/SHA、门禁结论与任何缺口；不要把完整私有文件清单复制到报告。完成后 workflow submit，主控独立复核。
