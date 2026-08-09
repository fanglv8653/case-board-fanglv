# V083-FORMAL-TOOLING-R2 交付报告

## 结论

R1 独立复核提出的阻断项已在限定范围内修复。合成正负测试、PowerShell 语法解析和 `git diff --check` 均通过；未访问正式数据库、凭据或 NAS，未启动/安装应用，未运行 Cargo，也未实现 FormalSwitch/Install。

## 实际修改

1. `db_audit.py`
   - SQLite sidecar 集合统一覆盖 `-wal`、`-shm`、`-journal`；源复制、目标预存检测和 main-only 审计均使用该集合。
   - migration history 保留原始 `success` 整数，并纳入 `version`、`description`、`installed_on`、`success`、`checksum`、`execution_time`。
   - `success != 1` 视为失败；`success: 1 -> 2` 或 `installed_on` 变化均改变 history hash，并使幂等比较失败。
2. `Invoke-UpgradeValidation.ps1`
   - 每个 run 创建随机 HMAC 密钥，并以 Windows CurrentUser DPAPI 保护；resume 除 caller SHA 外还校验固定 HMAC。
   - 固定校验 manifest 文件名、`stage + status`、`run_root`、父 manifest 路径/SHA/HMAC 链。
   - 所有 manifest artifact 使用绝对路径、限制在同一 `run_root` 内并校验 SHA-256。
   - AuditCopy 只读取 Backup manifest 绑定的 main-only artifact；一启/二启分别绑定父 snapshot、comparison、proof DB，二启 proof DB 路径不得替换。
   - 目标存在 WAL/SHM/rollback journal 时在 SQLite 打开前失败，不删除或改写 sidecar。
   - `Get-CimInstance` 进程枚举使用 `-ErrorAction Stop`；枚举失败立即停止，测试钩子可注入该故障。
   - FormalSwitch/Install 继续禁用，无启动、停止、删除、凭据、安装或正式切换实现。
3. 测试与说明
   - 新增手写 manifest + 自算 SHA、status 篡改、artifact 路径/内容替换、跨 run_root、父 manifest 篡改等负例。
   - 新增三类 sidecar 字节保留、进程枚举失败、migration 原始语义测试。
   - README 更新为 R2 的认证链、proof DB 和 sidecar 操作约束。

## 验收证据

### 合成测试

```text
python -m unittest discover -s scripts\windows-upgrade-validation\tests -p 'test_*.py' -v
Ran 20 tests in 51.616s
OK
```

覆盖：copy-first 非零 WAL 合并且源文件事实不变、main-only/quick/FK、内容指纹、migration 全字段及幂等失败、WAL/SHM/journal 预存保留拒绝、固定 resume 链、伪造与篡改负例、跨 run_root、进程枚举失败、FormalSwitch/Install 禁用静态契约。

### PowerShell 语法

```text
[System.Management.Automation.Language.Parser]::ParseFile(...)
0 errors
```

### diff-check

```text
git diff --check -- <本任务允许修改的工具、README、测试文件>
exit 0
```

仅有 Git 的 LF/CRLF 工作区提示，无 whitespace error。

## 保留边界

- 本轮全部动态测试只使用临时目录和合成 SQLite 数据库；没有执行正式备份或任何正式写入。
- HMAC anchor 由 Windows CurrentUser DPAPI 保护，同一证据链应由创建它的 Windows 用户在同一机器继续；跨用户或跨机器复制会 fail closed。
- proof DB 必须位于对应 `run_root` 内且不能直接使用保留的 main-only 备份；二启前序 proof DB 内容、路径或先前 snapshot/comparison 被替换都会拒绝续跑。
- FormalSwitch/Install 仍只有确认边界和禁用错误，正式切换、安装及真实设备验收仍属于后续独立授权任务。

## P0/P1 自检

- P0：0
- P1：0
