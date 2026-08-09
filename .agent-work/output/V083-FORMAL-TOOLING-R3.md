# V083-FORMAL-TOOLING-R3 交付报告

## 结论

R2 review 的唯一 P1 已修复：本工具不启动、不观察应用，因此只登记外部运行后的数据库后验检查，不再生成或传播应用首启、二启、优雅退出“通过”结论。Recorded 状态不能作为 FormalSwitch/Install 的父证据。R2 已确认的 migration、manifest/artifact、journal、进程枚举修复均保留。

## 修改内容

- 阶段改为 `RecordExternalRunDbPostcheck`。
- 首次后验状态固定为 `isolated-db-postcheck-recorded`；幂等后验状态固定为 `idempotent-db-postcheck-recorded`。
- `ExitMode` 仅记录在 `unverified_external_claim` 下，并明确 `asserted_by=caller`；manifest 固定写入 `observed_application_execution=false`。
- 内部 snapshot/compare 的过程状态不再透传到编排器控制台，最终输出只有 recorded 后验状态，避免通用 `passed` 被误读为应用运行结论。
- 数据库后验仍绑定 proof DB、前序 snapshot/comparison、artifact SHA、父 manifest SHA/HMAC 与同一 run root。
- FormalSwitch 对 recorded 后验明确返回 `RECORDED_POSTCHECK_NOT_FORMAL_SWITCH_EVIDENCE`；Install 同样拒绝 recorded 证据；二者仍未实现。
- README 明确：CurrentUser DPAPI/HMAC 只防意外或无该用户访问权的篡改，不是针对同一 Windows 用户的对抗性信任根，也不能证明应用被启动或观察。

## 验收证据

```text
python -m unittest discover -s scripts\windows-upgrade-validation\tests -p 'test_*.py' -v
Ran 20 tests in 47.873s
OK
```

新增/更新的核心断言包括：

- 仅凭 `ExitMode=graceful`，顶层 stdout 与 manifest 不出现应用运行 `passed` 结论；
- manifest 为 recorded 状态、`observed_application_execution=false`，退出模式位于未验证声明字段；
- recorded 幂等后验不能满足 FormalSwitch 或 Install 前置条件；
- 静态禁止旧 `ValidateIsolatedExit`、`isolated-start-passed`、`isolated-second-start-passed` 等名称；
- R2 正负测试继续全部通过。

PowerShell AST 语法解析：0 errors。

`git diff --check`：exit 0，仅有 Git 的 LF/CRLF 工作区提示，无 whitespace error。

## 边界

- 未访问正式数据、凭据或 NAS。
- 未启动、观察、终止或安装应用。
- 未运行 Cargo。
- 未实现 FormalSwitch/Install 或任何正式写入。
- 真实应用启停 runner 及其可观察证据必须由后续独立任务实现和验收。

## P0/P1 自检

- P0：0
- P1：0
