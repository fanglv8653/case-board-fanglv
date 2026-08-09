# V083-FORMAL-TOOLING-R3 独立只读复核报告

## 结论

**通过 R3 发布门禁（限定为本地工具与合成证据范围）。**

- P0：0
- P1：0
- P2：0

R2 review 的唯一 P1 已关闭：本工具没有启动或观察应用时，只登记数据库后验检查为 `recorded`，不再生成或传播首启、二启或正常退出 `passed` 结论。调用方提供的 `ExitMode` 被明确标记为未验证外部声明；两类 recorded 状态均不能成为 FormalSwitch/Install 的父证据。R2 已确认的 migration、manifest/artifact、journal 与进程枚举修复未见回退。

## R3 验收逐项核对

| 验收项 | 结果 | 证据 |
|---|---|---|
| 未观察应用时仅登记 recorded | 通过 | `Invoke-UpgradeValidation.ps1:378-434` 的阶段名为 `RecordExternalRunDbPostcheck`，只生成 `isolated-db-postcheck-recorded` / `idempotent-db-postcheck-recorded` |
| 不输出首启/二启/退出 passed 结论 | 通过 | 脚本不存在 `ValidateIsolatedExit`、`isolated-start-passed`、`isolated-second-start-passed`、`first-start-passed`、`second-start-passed`、`graceful-exit-passed`；合成测试同时断言两次成功调用 stdout 不含 `passed` |
| `ExitMode` 明示为未验证声明 | 通过 | `Invoke-UpgradeValidation.ps1:429-430` 写入 `unverified_external_claim = { exit_mode, asserted_by='caller' }` 和 `observed_application_execution=false` |
| recorded manifest 固定 stage/status/文件名 | 通过 | `Invoke-UpgradeValidation.ps1:171-177,419-434`；父 manifest 递归校验仍由 `188-245` 执行 |
| 文件层后验连续性 | 通过 | `Invoke-UpgradeValidation.ps1:379-418`：幂等后验只接受第一次 recorded manifest，绑定同一 proof DB 路径、父 snapshot、comparison 和同一 run root |
| recorded 不能进入 FormalSwitch | 通过 | `Invoke-UpgradeValidation.ps1:436-444` 对 recorded 父链固定抛出 `RECORDED_POSTCHECK_NOT_FORMAL_SWITCH_EVIDENCE`，随后 FormalSwitch 仍固定 disabled |
| recorded 不能进入 Install | 通过 | `Invoke-UpgradeValidation.ps1:446-451` 只要提供 resume manifest 即拒绝，Install 仍固定 disabled |
| 无启动/停止/删除/安装实现 | 通过 | 脚本不含 `Start-Process`、`Stop-Process`、`Remove-Item` 或安装实现；静态契约测试通过 |
| DPAPI/HMAC 可信边界明示 | 通过 | README 明确其只防意外修改及无创建用户权限的修改，不是针对同一 Windows 用户的对抗性信任根，也不能证明应用运行 |

## R2 修复回归核对

- migration：`db_audit.py:304-320,345-382` 继续保留原始 `success` 整数、`installed_on` 和完整六字段 history hash；幂等负例继续通过。
- manifest/artifact：固定 stage/status/文件名、run root、HMAC、artifact SHA-256 与递归父链校验仍在；篡改、跨 run root 和父链破坏负例继续通过。
- AuditCopy：仍只从 Backup manifest 读取绑定的 `main_only_database`，没有调用方替代 DB 参数。
- sidecar：`-wal/-shm/-journal` 继续统一纳入复制、main-only 和目标预存拒绝；三类 sidecar 字节保留负例继续通过。
- 进程枚举：仍使用 `Get-CimInstance Win32_Process -ErrorAction Stop`；注入失败时在创建 run 前停止。
- copy-first、源 trio 不变、目标 main-only、quick check、FK 与内容指纹测试均继续通过。

## 实际测试结果

```text
python -m unittest discover -s scripts/windows-upgrade-validation/tests -p 'test_*.py' -v
Ran 20 tests in 48.120s
OK

PowerShell AST parser: PASS
git diff --check -- scripts/windows-upgrade-validation: PASS
```

`git diff --check` 仅有 LF/CRLF 工作区提示，无 whitespace error。

## 边界声明

- 本复核仅阅读本地项目文件并运行仓库已有合成测试。
- 未访问正式 DB/WAL/SHM/journal、凭据、NAS 或网络。
- 未运行 Cargo，未启动、观察、停止或安装应用，未执行 FormalSwitch/Install。
- 本结论只确认“数据库后验记录工具没有冒充应用运行验收”；真实应用启停 runner 及其可验证运行证据仍须由后续独立任务实现和验收。
