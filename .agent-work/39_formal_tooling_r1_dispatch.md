# V083-FORMAL-TOOLING-R1 派工单

## 目标

把本机正式验收工具改造成“先备份、后审计、分阶段恢复”的安全工具；本轮只修改工具和合成测试，不执行正式备份、安装或数据目录切换。

## 输入

- `.agent-work/output/V083-FORMAL-BACKUP-PREP.md` 第十一节
- `scripts/windows-upgrade-validation/`

## 允许修改

- `scripts/windows-upgrade-validation/db_audit.py`
- `scripts/windows-upgrade-validation/Invoke-UpgradeValidation.ps1`
- `scripts/windows-upgrade-validation/README.md`
- `scripts/windows-upgrade-validation/tests/`
- 本任务报告/线程状态

## R1 必须交付

1. `db_audit.py` 新增独立、可测试的阶段：记录源 DB/WAL/SHM 文件事实；SQLite online backup；验证源三文件事实不变、目标 main-only、quick/FK；只对副本做结构和业务审计。
2. 审计快照包含完整 migration tuple/checksum/execution_time、schema/migration history hash、逐表稳定内容指纹、非设备同步投影、0063 sentinel 与同步安全指标；同数异内容必须能检出。
3. PowerShell 改为显式分阶段和 resume manifest；默认不得安装、不得替换正式目录、不得强杀后继续、不得删除 sidecar。涉及正式切换或安装的阶段必须另有显式参数与前置 manifest，且本轮不运行。
4. 增加合成测试，至少证明：非零 WAL 被 online backup 合并且源不变；目标无 sidecar且 quick/FK 通过；同数异内容比较失败；强杀/sidecar 状态不能冒充通过；路径逃逸/目标已存在 fail closed。
5. 不读取正式数据库、凭据、NAS、GitHub secrets，不启动/安装应用，不运行 Cargo。允许仅运行 Python 单测和 PowerShell 静态/合成测试。
6. 报告写入 `.agent-work/output/V083-FORMAL-TOOLING-R1.md`，附实际命令和结果、范围、剩余缺口。
