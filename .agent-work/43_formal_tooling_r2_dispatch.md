# V083-FORMAL-TOOLING-R2 派工单

## 目标

修复 R1 独立复核的 P1/P2，使备份、审计、隔离退出证据形成不可跳步的产物绑定链。

## 允许修改

- `scripts/windows-upgrade-validation/` 内 R1 已修改的工具、README、测试
- 本任务报告/线程状态

禁止访问正式 DB/凭据/NAS，禁止启动或安装应用，禁止运行 Cargo，禁止实现 FormalSwitch/Install。

## 必须完成

1. migration tuple 和 history hash 使用原始 `success` 整数并包含 `installed_on`、version、description、checksum、execution_time；`success 1→2` 或 installed_on 变化必须使幂等比较失败。
2. resume manifest 不得只信 caller 的 hash/status。每阶段必须校验固定 `stage + status` 组合、parent hash、run_root、预期 artifact 绝对路径与 artifact SHA-256；AuditCopy 只能打开 Backup manifest 内绑定的 main-only 文件，不能由调用方替换路径；一启/二启证明同样绑定前序 snapshot/comparison/proof DB。
3. 增加伪造/跳步负例：手写 manifest、自算 hash、改 status、换 artifact 路径或内容、跨 run_root，均 fail closed。
4. source/staging/target sidecar 集合必须覆盖 `-wal`、`-shm`、`-journal`；任何目标 journal 预存在即在 SQLite 打开前失败且字节不变。
5. 进程枚举错误必须 fail closed；不能以 `SilentlyContinue` 将查询失败解释为 0 进程。增加可注入失败测试或静态契约。
6. 保留 copy-first、source trio 不变、main-only/quick/FK、内容指纹和禁用 FormalSwitch/Install 等 R1 已确认项。
7. 报告 `.agent-work/output/V083-FORMAL-TOOLING-R2.md`，列出实际合成测试与剩余边界。
