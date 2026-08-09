# V083-FORMAL-TOOLING-R3 派工单

## 目标

修复 R2 唯一 P1：工具未实际启动/观察应用时，不得生成或传播“首启/二启通过”结论。

## 允许范围

- `scripts/windows-upgrade-validation/` 内 R2 工具、README、测试
- 本任务报告/线程状态

禁止访问正式数据/凭据/NAS，禁止启动或安装应用，禁止运行 Cargo，禁止实现 FormalSwitch/Install。

## 必须完成

1. 未实际执行应用的阶段只能登记并校验“外部运行后的数据库证据”，名称、manifest stage/status、输出文案均不得出现 first-start-passed、second-start-passed、graceful-exit-passed 或等效结论。
2. `ExitMode` 等调用方声明不能被提升为运行事实；可保留为未验证声明字段，但必须显式标注 `unverified_external_claim`。
3. 数据库后验检查仍绑定前序 proof DB、snapshot、comparison 和父 HMAC 链，可给出 `isolated-db-postcheck-recorded`/`idempotent-db-postcheck-recorded`，不得给出应用启动/退出通过。
4. FormalSwitch/Install 继续禁用，且不得接受上述 recorded 状态作为正式切换父状态；未来实际启退 runner 必须由单独任务实现与验收。
5. 新增合成/静态测试，证明仅凭 `ExitMode=graceful` 不能产生任何 `passed` 状态，也不能满足 FormalSwitch 前置条件。
6. README/报告明确 DPAPI/HMAC 只防意外/非同用户篡改，不把同一 Windows 用户视为对抗性信任根。
7. 报告 `.agent-work/output/V083-FORMAL-TOOLING-R3.md`。
