# V083-FORMAL-TOOLING-R2 独立只读复核报告

## 结论

**暂不通过 R2 发布门禁。**

- P0：0
- P1：1
- P2：1
- 验收要求为 P0=0、P1=0；当前仍不满足。

迁移历史原始语义、三类 sidecar 拒绝、进程枚举 fail closed、Backup→AuditCopy 的清单/产物/父链校验均已落地，仓库现有 20 项合成测试全部通过。但“一启/二启证明不可跳步”仍未成立：当前工具只能证明指定 proof DB 的快照与比较结果，不能证明应用实际完成了两轮启动和正常退出。

## P1-01：一启/二启仍可在未启动应用时仅凭 caller 声明通过，未满足“不可跳步”

### 证据

1. `Invoke-UpgradeValidation.ps1:378-386` 只读取前序 manifest，并检查调用方传入的 `ExitMode` 是否等于字符串 `graceful`；没有接收或验证进程身份、启动时间、退出时间、退出码、可执行文件身份或独立运行回执。
2. `Invoke-UpgradeValidation.ps1:388-435` 对 proof DB、sidecar、snapshot 和 comparison 的绑定是有效的，但这些检查只能证明“某个文件在两个时间点满足比较条件”，不能证明对应应用启动/退出事件确实发生。
3. 仓库现有正例 `scripts/windows-upgrade-validation/tests/test_formal_stages.py:107-137` 直接把 retained main-only DB 复制为 proof DB，随后两次传入 `ExitMode=graceful`，没有启动或退出应用，却预期并实际通过一启、二启两个阶段。这与派工单第 3 项要求的“跳步负例均 fail closed”相反。

### 影响

`manifest.isolated-first.json` 与 `manifest.isolated-second.json` 当前应解释为“两次数据库快照/比较通过”，不能解释为“两轮隔离首启与正常退出已发生”。因此正式切换前的两轮启动退出门禁仍可被流程跳过。

### 修复建议

- 将外部运行证明纳入阶段 schema，并与 run-specific nonce、proof DB 绝对路径/哈希和前序 manifest hash 绑定；至少记录可验证的 runner 身份、目标可执行文件身份、进程 ID、启动/退出时间、退出码和正常退出判定。
- `ValidateIsolatedExit` 必须消费该前序运行证明，而不是接受自由字符串 `ExitMode`；一启和二启使用不同、不可复用的运行证明。
- 新增真正的跳步负例：仅复制 DB 并声明 `graceful` 必须失败；缺失、复用、错 run_root、错 proof DB 或错父链的运行证明均须 fail closed。
- 本轮禁止启动应用，因此可先实现“预置 nonce + 外部回执校验”的合成契约；真实启退仍留给后续单独授权验收。

## P2-01：CurrentUser DPAPI/HMAC 的可信边界需要写入验收口径

`Invoke-UpgradeValidation.ps1:100-131` 将 HMAC key 以 CurrentUser DPAPI 保护后，与证据链一起保存在 `.resume-anchor.bin`；`188-245` 据此验证 manifest HMAC 与父链。该设计能拒绝无 HMAC 的手写清单、普通文件篡改、跨用户或跨机器复制，但其认证根仍属于创建证据链的同一 Windows 用户，并非独立见证方。

R2 报告第 57 行已部分说明“同一用户、同一机器继续”的运行边界。建议 README/验收标准进一步明确：HMAC 的目标是发现意外篡改及阻断无锚点复制，不把同一 Windows 用户视为对抗性攻击者；若验收语义要求抵御同一用户重签，则需把签名根迁移到调用方无法导出的独立信任域。本轮按“常规只读发布 QA”边界未追加攻击性实证。

## 已通过项

| 验收项 | 复核结果 | 主要证据 |
|---|---|---|
| migration 原始全字段进入 history hash | 通过 | `db_audit.py:304-320,345-382` 保留 `success` 整数及六字段；`failed_count` 使用 `success != 1` |
| migration 幂等变化负例 | 通过 | `scripts/windows-upgrade-validation/tests/test_db_audit.py:161-182` 覆盖 `success=1→2` 与 `installed_on` 变化并失败 |
| 固定 stage/status/run_root/HMAC/父链 | 通过 | `Invoke-UpgradeValidation.ps1:171-245` |
| artifact 路径位于 run_root 且 SHA-256 一致 | 通过 | `Invoke-UpgradeValidation.ps1:223-230` |
| AuditCopy 只使用 Backup 绑定 main-only | 通过 | `Invoke-UpgradeValidation.ps1:323-350`；无独立 DB 路径参数 |
| 二启 proof DB 与一启 proof DB 连续 | 通过（文件层） | `Invoke-UpgradeValidation.ps1:379-423` 绑定父 snapshot、proof DB 路径并作幂等比较 |
| WAL/SHM/journal 统一拒绝且不删除 | 通过 | `db_audit.py:24,102-141,424-429`；对应合成测试通过 |
| 进程枚举失败即停止 | 通过 | `Invoke-UpgradeValidation.ps1:73-84` 使用 `-ErrorAction Stop`；注入失败测试通过且不创建 run |
| FormalSwitch/Install 保持禁用 | 通过 | 静态契约测试通过；未执行正式切换或安装 |

## 实际复核命令与结果

```text
python -m unittest discover -s scripts/windows-upgrade-validation/tests -p 'test_*.py' -v
Ran 20 tests in 48.042s
OK

PowerShell Parser: PASS
git diff --check -- scripts/windows-upgrade-validation: PASS
```

`git diff --check` 仅输出 LF/CRLF 工作区提示，无 whitespace error。

## 边界声明

- 仅审查本地项目文件并运行仓库已有合成单测；未访问正式 DB/WAL/SHM/journal、凭据、NAS 或网络。
- 未运行 Cargo，未启动、停止、安装应用，未执行 FormalSwitch/Install。
- 结论不否定现有数据库文件级证据链修复；阻断点仅在“一启/二启事件本身仍由 caller 自由声明”。
