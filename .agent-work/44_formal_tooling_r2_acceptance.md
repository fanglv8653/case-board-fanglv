# V083-FORMAL-TOOLING-R2 验收标准

- P0=0、P1=0。
- migration 全字段原始语义进入 history hash 与幂等比较。
- manifest/产物/路径/父链不可由手写自算 hash 绕过。
- `-journal` 与 WAL/SHM 一样在打开前 fail closed且不被隐式删除。
- 进程枚举失败即停止。
- 合成正负测试、PowerShell 语法、diff-check、独立复核全部通过。
- FormalSwitch/Install 继续禁用；本任务不执行正式备份或任何正式写入。
