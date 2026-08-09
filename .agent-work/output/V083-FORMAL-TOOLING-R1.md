# V083-FORMAL-TOOLING-R1｜正式备份与隔离验收工具 R1

- 逻辑线程：`worker-formal-tooling-r1`
- 交付状态：`submitted_for_review`
- 自检结论：P0=0、P1=0；待主控独立只读复核
- 范围：仅 `scripts/windows-upgrade-validation/`、测试和本报告
- 正式资源边界：未读取正式 DB/WAL/SHM、凭据、NAS 或 GitHub secrets；未启动、停止或安装应用；未运行 Cargo；未切换目录或删除 sidecar。

## 一、结果

R1 已把旧的一键“隔离启动→安装→正式首启”脚本收缩为安全最小闭环：

1. DB/WAL/SHM 文件事实、原样三文件 staging、SQLite online main-only、复制后完整审计成为相互独立的硬门禁。
2. SQLite 不再打开调用方提供的正式源路径。工具先把三件套复制到新 staging，证明源前后字节事实不变和 staging 内容一致，再只对 staging 使用 SQLite backup API。
3. main-only 目标强制 `journal_mode=DELETE`（只修改新目标），并硬断言目标无 WAL/SHM、`quick_check=ok`、FK 0。
4. 快照新增完整 migration tuple/checksum/execution time、schema/migration history hash、逐表内容指纹、非设备同步总投影、0063 sentinel 和设备同步安全指标。
5. 比较可识别“行数相同但内容变化”；幂等比较还要求 schema、迁移历史和同步安全指标不变。
6. PowerShell 改为显式阶段、哈希绑定 resume manifest、证据不覆盖。强杀和 sidecar 只会使证明失败，工具没有删除路径。
7. R1 不含应用启动、杀进程、正式目录移动或安装实现。`FormalSwitch`/`Install` 仅作显式禁用门，校验前置 manifest 和确认语后仍返回 R1-disabled，不产生正式写入。

## 二、关键设计修正

合成测试首次验证时发现：对带 WAL 的源库直接执行 SQLite `mode=ro` online backup，会改变 SHM 锁状态。若仍以“调用前/后 SHM hash 相同”为门禁，直接打开正式源的设计不可满足。

最终实现采用 copy-first：

```text
显式 source DB/WAL/SHM
  → 记录 size/mtime/SHA-256
  → 字节复制到全新 retained staging
  → 证明 source 前后不变、staging 与 source 一致
  → SQLite 仅打开 staging
  → online backup 到全新 main-only DB
  → 仅对 main-only DB 做结构/业务审计
```

这既保留 WAL 一致视图，也避免 SQLite 对正式 SHM 产生可观察副作用。

## 三、修改文件

### `db_audit.py`

- `trio_facts()`：记录 main/WAL/SHM 的存在性、大小、mtime ns、SHA-256。
- `copy_source_trio()`：全新目录原样复制三件套，拒绝复用目标。
- `online_backup()`：source 前后硬比较；SQLite 只打开 staging；目标 main-only/quick/FK 硬断言。
- `snapshot()`：拒绝任何 sidecar，仅审计 main-only 副本；生成完整迁移、schema、业务投影、0063 与同步指标。
- `compare()`：按逐表内容 hash 判断，不再用行数代替内容；支持 `--idempotent`。
- CLI 新增 `facts`，并扩展 `backup/snapshot/compare`；所有 JSON 输出拒绝覆盖。

### `Invoke-UpgradeValidation.ps1`

显式阶段：

| 阶段 | 行为 | 正式写入 |
| --- | --- | --- |
| `Backup` | 进程门禁、copy-first trio、online main-only、硬断言、生成 hashed manifest | 无；只写指定证据目录 |
| `AuditCopy` | 校验前一 manifest 的外部 SHA-256，只审计 main-only | 无 |
| `Compare` | 比较两个既有 snapshot；可要求幂等 | 无 |
| `ValidateIsolatedExit` | 只登记外部隔离运行结果；forced 或 sidecar 立即失败 | 无，不启动应用 |
| `FormalSwitch` | 要求二启通过 manifest、显式 mutation switch 和确认语，随后 R1-disabled | 无实现 |
| `Install` | 要求 formal-switch manifest 和显式确认，随后 R1-disabled | 无实现 |

安全属性：

- `-Stage` 必填，没有默认执行路径。
- resume 必须同时提供 manifest 和调用方持有的 64 位十六进制 SHA-256。
- 输出目录不得位于 source DB 所在目录内；RunId/证据/manifest 均拒绝覆盖。
- 备份前后均确认 `caseboard.exe=0`。
- 没有 `Start-Process`、`Stop-Process`、`Remove-Item`、`Directory.Move`、安装器调用或 Credential API。
- 没有删除 sidecar、修改 `_sqlx_migrations`、恢复同步或更新 tag/Release/latest 的代码路径。

### README 与测试

README 已改为 R1 分阶段契约和明确停用边界。测试新增：

- 非零 WAL 合并，显式 source trio 前后不变。
- retained staging 与 main-only/quick/FK。
- 同数异内容失败。
- 设备同步投影分离及幂等比较。
- sidecar 原样保留并拒绝证明。
- resume manifest SHA 验证、forced exit 失败。
- 路径逃逸和已存在目标 fail closed。
- PowerShell 静态不存在启动、强杀、删除、安装实现。

## 四、实际验证

### Python + PowerShell 合成/静态测试

```powershell
python -m unittest discover -s scripts\windows-upgrade-validation\tests -p 'test_*.py' -v
```

结果：13 passed，0 failed，耗时 21.078 秒。

其中 3 项测试会从 Python 调用 Windows PowerShell，但只使用系统临时目录中的合成 SQLite 数据；未传入任何正式路径。

### PowerShell 语法

使用 `System.Management.Automation.Language.Parser.ParseFile` 解析 `Invoke-UpgradeValidation.ps1`：`PowerShell parse: OK`。

### 差异检查

```powershell
git diff --check -- scripts/windows-upgrade-validation
```

结果：通过，无 whitespace error；仅显示 Windows 工作树 LF→CRLF 提示。

未运行 Cargo、应用 EXE、installer、正式备份或目录切换。

## 五、验收标准逐项

| 标准 | 结果 |
| --- | --- |
| 默认调用不写正式数据、不安装、不切换目录 | `passed`：Stage 必填；无启动/安装/移动实现 |
| source trio 前后不变为硬断言 | `passed`：三件套 size/mtime/hash 全等，否则异常 |
| main-only/quick/FK 为硬断言 | `passed` |
| 只对副本做结构和业务审计 | `passed`：SQLite 仅打开 retained staging/main-only |
| 完整 migration/schema/内容/sentinel/同步指标 | `passed` |
| 同数异内容可检出 | `passed`：合成测试覆盖 |
| 强杀/sidecar 不能冒充通过 | `passed`：两者均硬失败，sidecar 原样保留 |
| 路径逃逸/目标已存在 fail closed | `passed`：PowerShell 合成测试覆盖 |
| 无 sidecar 删除、迁移表修改、自动同步/发布 | `passed`：实现与静态测试均无该路径 |

## 六、剩余缺口

1. R1 不启动应用，因此 0.8.2 恢复证明、兼容补丁后 0.8.3 首启/二启仍须在无正式凭据的 VM/全新 Windows 用户中外部执行，再由 `ValidateIsolatedExit` 登记。
2. `FormalSwitch` 与 `Install` 明确未实现；应在 R1 独立复核通过后单独派发 R2，并复用 hashed resume manifest。当前不存在任何默认或隐藏的正式写路径。
3. `V083-M1-COMPAT36` 必须独立 accepted；R1 未修改或替代其产品兼容逻辑。
4. 本轮全部为合成验证。正式 backup、恢复、sidecar 调和和安装均为 `not_run`，仍需用户授权维护窗口。
5. 正式使用时必须提供 `src-tauri/migrations` 生成 checksum 映射，并由主控核验 migration 36 tuple 与 0063 断言；R1 不把某一台设备的旧 hash 写死进通用工具。

## 七、范围自检

修改仅限：

- `scripts/windows-upgrade-validation/db_audit.py`
- `scripts/windows-upgrade-validation/Invoke-UpgradeValidation.ps1`
- `scripts/windows-upgrade-validation/README.md`
- `scripts/windows-upgrade-validation/tests/test_db_audit.py`
- `scripts/windows-upgrade-validation/tests/test_formal_stages.py`
- `scripts/windows-upgrade-validation/tests/test_tooling_contract.py`
- `.agent-work/output/V083-FORMAL-TOOLING-R1.md`
- 本线程 workflow 状态/通知

请主控独立复核。本线程不写 `accepted`。
