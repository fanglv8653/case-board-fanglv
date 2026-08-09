# V083-FORMAL-BACKUP-EXECUTE 交付报告

## 结论

固定批次 `V083-20260809-234237` 的正式原样备份与 SQLite main-only 一致性副本已完成，全部硬门禁通过。正式源 trio 在 Backup、AuditCopy及补充复制七个阶段前后保持 size、mtime、SHA-256 完全一致；`caseboard.exe` 全程为 0，正式 `-journal` 始终不存在。

本轮仅执行只读、复制、哈希、EFS/ACL校验及卸载项导出；未启动、安装或终止应用，未切换正式目录，未删除或修改 sidecar，未访问 NAS，未读取凭据内容，未运行 Cargo。

## 固定路径与批次

- run id：`V083-20260809-234237`
- run root：`D:\CodexWorkspace\008案件看板应用\formal-backups\V083-20260809-234237`
- 正式源：`C:\Users\William Feng\AppData\Roaming\FanglvCaseBoard\data\caseboard.db`
- main-only：`D:\CodexWorkspace\008案件看板应用\formal-backups\V083-20260809-234237\02-main-only\caseboard.db`

## 执行前门禁

- `caseboard.exe`：0；进程枚举成功并采用错误即停。
- 正式 main/WAL/SHM：均存在；`-journal`：不存在。
- run root：执行前不存在。
- D 盘可用空间：约 306.8 GiB，高于 5 GiB。
- 0.8.2 rollback setup SHA-256：`443AA2FE1A64DDA780BE9CF999E432F070A4BD6F60EA972B8180230DDD402312`，精确匹配。
- 正式 trio 初始事实与准备阶段采样一致。

## Manifest 与 main-only

| 证据 | SHA-256 | 结果 |
| --- | --- | --- |
| `manifest.backup.json` | `8CE5582A2F9E879353F0ED129D3785B9FC67E0DE7C7370BB8B9302822D083913` | SHA/HMAC 通过 |
| `manifest.audit.json` | `F386A61F2514EE9AF460E618DCA9E57E362550C85F388FEC656AEA06673A7859` | SHA/HMAC/父链通过 |
| `supplementary-manifest.json` | `832F55471ED71E78C2C0C8D27D73CE309C2A9F9B1A3C13E80B141092D23D75F5` | 私有逐文件清单通过 |
| main-only DB | `3BE3C721505018728B3B835D37D25B9D0AD2B8968A64C7AE15FEDA480AB6F2FC` | 唯一主文件，无 sidecar |

main-only 审计结果：

- `quick_check=ok`；FK violation=0；journal mode=`delete`。
- migration count=62，max=62，failed=0。
- 唯一 embedded gap 为 version 36；description=`feishu reminder runs`、success=1、stored SQLx SHA-384=`84F859102447ACB5DBEE9E179A0AE3493D7ED2483B28A447BDF0F4F9360CC2399FC1F7AA08CBA2F0BE50F444F4841480`，与可信 tuple 一致。
- version 0063 未应用；m63 sentinel 为预升级状态。
- 非设备同步业务投影 SHA-256=`9B9A26C02803252D6FDE2C2FAB06EF5CB02F949720C1DAE9A6DBF805897B218F`。
- 同步安全摘要：group 1 且 paused 1；outbox exported 487；legacy quarantine 11。

## 补充原样备份

以下七个阶段均在执行前后重新采集正式 trio facts，并进行全字段比对：

1. 完整正式数据根；
2. 旧数据根；
3. 当前安装目录；
4. 已核验的 0.8.2 rollback setup；
5. HKCU 卸载项导出；
6. 当前 WebView；
7. 旧 WebView。

本机上述可选旧数据/WebView 源均存在并已原样复制。各次 robocopy 实际退出码均为 1，处于允许的 0—7 范围；参数固定为 `/E /COPY:DAT /DCOPY:DAT /R:1 /W:1 /XJ`。私有清单登记 2001 个复制/生成文件，最终逐文件重算 size/SHA-256，mismatch=0。完整 run root 当前共 2003 个文件，约 1.809 GiB。

当前安装 EXE SHA-256 仍为 `62160F3E7011ACDB6D2EC89C9D15C9962D7D7C6C23EB380D83DAC14F13DFF359`；卸载项 DisplayVersion 仍为 0.8.2；备份内 rollback setup SHA 再次匹配固定值。

## 最终不可变性与保护检查

- 正式 trio 最终 facts 与首阶段初始 facts：完全一致。
- `01-raw-data` 内 main/WAL/SHM SHA：逐项等于同批次正式源。
- 正式 `-journal`：仍不存在。
- 最终 `caseboard.exe`：0。
- Backup/Audit manifest：SHA、HMAC、parent hash 全部通过。
- run root 及递归 2201 个文件/目录项：EFS violation=0。
- EFS：AES-256，解密者为当前 Windows 用户；无 recovery certificate。
- ACL：仅当前用户、SYSTEM、Administrators；递归 ACL violation=0。

## 执行记录说明

补充脚本第一次调用在任何复制动作前即因 Windows PowerShell 5 对无 BOM UTF-8 中文路径误解码，以 `STOP_RUN_ROOT_MISSING` 停止。未运行 robocopy、未触碰正式源。随后仅将线程内脚本机械转换为 UTF-8 BOM，再经 AST 与路径检查后执行成功；该次无动作失败未污染备份批次。

## 剩余边界

- 本报告不证明 0.8.2 隔离恢复或 0.8.3 隔离升级/二次运行；本轮未启动任何应用。
- FormalSwitch/Install 仍禁用；未构造或切换正式 candidate，未执行正式安装。
- 补充 manifest 属私有证据，完整文件清单未复制到本报告。

## P0/P1 自检

- P0：0
- P1：0
