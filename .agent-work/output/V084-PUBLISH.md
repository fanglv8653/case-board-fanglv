# V084-PUBLISH｜v0.8.4 正式发布与公开清单收敛

- 状态：`submitted_for_review`（等待主控验收）
- 发布标签：`v0.8.4-fanglv`
- 冻结产品/发布工具提交：`5b1c93d97d5eecfada3a6ada22345ad4dace1075`
- 双清单提交：`f709e5ee24b39c110796b6136e1d52df1e4cccdf`
- Release：<https://github.com/fanglv8653/case-board-fanglv/releases/tag/v0.8.4-fanglv>

## 结论

0.8.4 已由精确标签提交完成 CI、Windows 安装包构建、updater minisign 验签、本机覆盖安装、正式数据库迁移与重复启动验收。GitHub Release 已公开，资产严格收敛为 ASCII 安装包与同名 `.sig`；`release/version.json` 和 `release/latest.json` 已在同一独立提交中快进到 `main`，raw 回读均为 0.8.4。

## 最终产物

| 项目 | 事实 |
| --- | --- |
| Build Windows | <https://github.com/fanglv8653/case-board-fanglv/actions/runs/32241065031>，success |
| 冻结提交 CI | <https://github.com/fanglv8653/case-board-fanglv/actions/runs/32240832643>，success |
| 安装包 | `FanglvCaseBoard_0.8.4_x64-setup.exe` |
| 安装包大小 | `9,349,099` bytes |
| 安装包 SHA-256 | `CCCDAD90CA6BFBE8AB841AD8C1387BAF970C839AC3658830A642D2521954D4FC` |
| updater 签名 | minisign 验签通过；清单签名与 `.sig` 文本一致 |
| `.sig` SHA-256 | `85794DBC6A22A1CFF217DB660B414699AD1589130635A417CC2068859EC705D7` |
| PE 版本 | FileVersion `0.8.4` |
| Authenticode | `NotSigned`；未将 updater minisign 冒充 Windows Authenticode |

Release API 回读为公开、非 prerelease，target 为冻结提交，且只有上述两个资产；API digest 与本地 SHA-256 一致。同标签 Release 列表精确为 1 条。

## 正式安装与数据库验收

- 安装前正式备份：`D:\CodexWorkspace\008案件看板应用\release-backups\v0.8.4-preinstall-20260819-142046`。
- 静默覆盖安装退出码 0；最终安装包启动后窗口标题为“方律案件看板”、响应正常，优雅退出码 0。
- 首次升级后：`PRAGMA quick_check=ok`，外键违规 0，迁移最高版本 65、迁移记录 65。
- 业务投影保持：案件 6、文档 852、案件进展 94；新待办表当前 0，符合首次发布前无本地待办的事实。
- 连续三次普通重复启动均正常响应并退出 0。保留外部 SQLite 读事务时，应用按 fail-closed 契约显示兼容性提示；读取连接明确关闭后立即启动正常。

## 发布中发现并修复的问题

1. 旧候选把正常退出遗留的 0-byte WAL 误判为损坏，导致 `wal_sidecar_physical_validation_failed`。已改为在取得独占 SQLite 所有权、完成主库审计与完整性检查后通过 SQLite checkpoint/journal 流程恢复；非空、不可读或异常 sidecar 仍失败关闭。
2. macOS CI 发现 Windows 专用 WAL 测试辅助函数及 `Sha256` 导入未条件化；已按平台门控，最终 Clippy 和全量 Rust 测试通过。
3. Windows 升级工具在长路径/8.3 路径比较中误判同一文件；改为按文件身份比较，20 项工具测试通过。
4. 可恢复发布脚本用 release-by-tag REST 接口回读草稿，但 GitHub 对草稿返回 404，导致网络 EOF 后重复创建空草稿。已增加认证 Release 列表回退、精确 tag 选择、重复 tag 失败关闭，并清理两条精确识别的多余空草稿；保留对象最终公开。
5. 草稿 `html_url` 为 `untagged-*` 临时地址，旧预检错误要求其提前等于正式 URL。现仅在只读草稿预检中使用确定的 canonical tag URL，发布后仍严格回读正式 URL。
6. 第一次发布后的 raw 回读遇到短暂缓存延迟，脚本安全停止；raw 内容收敛后以同一命令恢复，确认资产跳过重复上传、清单跳过重复提交并成功退出。

## 验证汇总

- 本机 Windows Rust：主库 373 passed / 5 ignored；设备同步 60 passed；其余两个目标 0 tests。
- 发布恢复 PowerShell：40/40。
- 发布合同 Node：3/3。
- Windows 升级工具 Python：20/20；截图 helper 自测通过。
- 冻结提交 CI：Frontend、Rust check/clippy/test、Windows release tooling 全部成功。
- 公开 raw：`version.json.version=0.8.4`，`latest.json.version=0.8.4`，下载 URL 与正式资产精确一致。

## 残余说明

- Windows Authenticode 仍为 `NotSigned`；Tauri updater 的 minisign 链已验证，两者不是同一种签名。
- 正式飞书首次绑定仍应在 0.8.4 界面先生成只读预览，由用户确认映射后再执行导入；本发布阶段没有自动改写正式飞书记录。
