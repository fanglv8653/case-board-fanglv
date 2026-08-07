# V083-RC-REVIEW：0.8.3 RC 独立总复核

- 逻辑线程：`worker-rc-review`
- 复核方式：只读审查当前差异、RC 报告、原始日志及本地 release EXE；补跑快速只读门禁
- 交付状态：`submitted_for_review`
- 复核结论：**P0 = 0，P1 = 0，P2 = 2；建议主控接受本地 RC 证据，并在有界提交后完成本地 RC 落档。正式发布仍为 `blocked_external`。**
- 安全边界：本线程未修改产品源码、测试、版本源、迁移、发布配置或 `release/latest.json`，未访问正式数据库、NAS、同步组、飞书、GitHub 或正式凭据，未运行 release EXE，未 commit/push/tag/Release。

## 一、验收结论

依据 `.agent-work/33_rc_acceptance_rubric.md`，结合主控已接受的两份 Gate 和 `.agent-work/34_rc_local_dispatch.md` 对历史 checksum 输入及提交职责的进一步冻结：

1. 当前差异没有发现会破坏数据库启动/升级、Windows Rust 全量门禁或设备同步收敛的 P0/P1。
2. 本地自动化与临时夹具证据已闭合：版本源一致、pre-0063 生产入口升级、临时双文件端双向收敛/幂等/真实隔离恢复、Node/Rust/Clippy/source gate、release EXE 构建与只读版本/hash 均有原始证据。
3. 历史 checksum 正向兼容缺少来源核验输入。按 33 号量表它是最终验收所需场景；但 34 号调度明确禁止猜值并将其冻结为 `blocked_external / pending_verified_input`。当前实现保持未知 mismatch 写前失败关闭，报告也没有冒充通过，因此不把该外部输入缺口倒算为本地实现 P1。
4. 33 号量表的本地接受条件包含“计划内改动并提交”。34 号调度又明确要求实现线程不提交、由主控复核后统一提交。故本报告建议主控接受证据并只提交计划内文件；在该有界提交完成前，工作区流程状态不应表述为已完成最终本地落档。
5. 本地 RC 接受不等于最终发布接受。签名安装包、远端资产、`latest.json` 快进、0.8.2 在线实机升级和物理双端仍未完成，必须保持 `blocked_external`。

## 二、P0 / P1 / P2

### P0：0

- 未发现数据库升级夹具、Windows Rust 全量或临时双端同步门禁失败后仍声称可发布。
- 未使用正式数据库、当前失败 NAS、正式飞书或正式同步组做测试；没有将本地临时目录冒充物理双端。
- 未发布、上传、改 tag 或更新 `release/latest.json`；未将未签名 EXE 冒充签名 bundle。
- 未新增 0064，未修改迁移 SQL、migration sentinel 或 checksum 生产策略。

### P1：0

- 真实 pre-0063 fixture 仅在 `TempDir` 中执行仓库 0001—0062，写入脱敏标记，经 `VACUUM INTO` 形成 main-only 输入；父测试两次启动独立子进程调用生产 `init_pool()`，均到 0063，并断言迁移数量/最大版本/0063 唯一、标记保留、`quick_check=ok`、FK 空和二次逻辑指纹一致。
- 临时双端 fixture 使用两个临时文件数据库和临时 mounted folder，完成 A→B、B→A、无变化重复幂等、确定性坏包触发生产 `sync_once` 自动暂停/隔离、恢复认证原包、显式 resume、生产重放和最终收敛；两端最终 canonical 投影一致，pending outbox/conflict/active quarantine/manual review 均为 0，quick/FK 健康。
- Windows 凭据写入使用随机 group/device/invite 的精确键；RAII 在正常及 unwind 路径只删除已记录键，成功路径逐项反查 signing/exchange/group-key/invite 均不可再读，不枚举或覆盖其他凭据。
- 五处版本来源一致为 0.8.3：`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、根 `Cargo.lock`、`CHANGELOG.md`；`release/latest.json` 仍为 0.8.2。
- `scripts/run-windows-rust-tests.ps1` 仅增加 PowerShell 5.1 的 UTF-8 无 BOM Console/管道编码设置，解决中文工作区中的 Cargo JSON 路径误解码；未削弱 manifest 嵌入或实际执行门禁。
- RC 报告清楚区分 `passed`、`not_run` 与 `blocked_external`，未对签名、在线升级、物理设备或历史 checksum 作虚假声明。

### P2：2

1. **提交前工作区收口。** 当前实质 diff 为 12 个计划内文件，共 `618 insertions / 12 deletions`，另有 RC 计划、报告、日志、review/thread 记录尚未跟踪；实现尚未提交。主控应按 34 号调度做有界提交，不应把无内容差异的设备同步文件一并提交。`identity.rs`、`manifest.rs`、`pairing.rs`、`recovery.rs`、`registry.rs`、`snapshot.rs` 在 `git status` 显示修改，但普通 diff、`--numstat` 及忽略行尾差异的 diff 均为空，只出现 LF/CRLF 工作区提示。
2. **保留日志的解释完整性。** `V083-RC-LOCAL.migration-test.stderr.log` 记录了一次旧测试 EXE 的 `STATUS_ENTRYPOINT_NOT_FOUND`；随后 `migration-rebuild` 明确重编译，新全量 Windows runner 中对应父夹具通过。最终门禁有效，但建议主控在汇总/提交说明中明确这是已被重编译和全量复跑覆盖的中间失败，避免后续只读日志者误判。

## 三、版本、迁移与发布资产复核

| 项目 | 独立复核结果 |
| --- | --- |
| `package.json` | `0.8.3` |
| `src-tauri/Cargo.toml` | `0.8.3` |
| `src-tauri/tauri.conf.json` | `0.8.3` |
| 根 `Cargo.lock` 中 `caseboard` | `0.8.3` |
| `CHANGELOG.md` | 存在 `## [0.8.3] - 2026-08-07` |
| `release/latest.json` | `0.8.2`，URL 仍指向 0.8.2 setup |
| 迁移目录尾部 | 最大为 `0063_device_sync_quarantine_lifecycle.sql` |
| 迁移/sentinel diff | 0 |
| 0064 产品文件 | 0 |
| checksum/allowlist 产品改动 | 0；新增命中仅为日志/说明文字 |

本地 release EXE 的当前只读结果与 R2 报告逐项一致：

| 项目 | 结果 |
| --- | --- |
| 路径 | `target/release/caseboard.exe` |
| 文件大小 | `19,440,128` bytes |
| FileVersion / ProductVersion | `0.8.3 / 0.8.3` |
| ProductName | `方律案件看板` |
| SHA-256 | `277F1B151567AC5FB941E0CC28D7D19A389B022659216686B8A00985B08FCC61` |
| Authenticode | `NotSigned` |
| bundle | 未生成；R2 使用 `pnpm tauri build --no-bundle` |

`NotSigned` 已被如实记录，且没有与 updater minisign 混淆。12 秒启动冒烟标为 `not_run`：当前 release 运行入口会读取 Windows 用户系统凭据状态，没有临时凭据后端；不启动是对正式资源边界的正确遵守，不可改写为 `passed`。

## 四、原始日志与快速复跑

### 已有日志

- Node logic：44 个文件，`123 passed / 0 failed`。
- Windows Rust：lib `336 passed / 0 failed / 4 ignored`，bin `0 passed`，integration `60 passed / 0 failed`，合计 `396 passed / 0 failed`；新增 RC 父迁移夹具和双文件端夹具均在全量输出中为 `ok`。被 ignored 的子进程 helper 已由父夹具显式运行两次。
- Clippy：`cargo clippy --workspace --all-targets --locked -- -D warnings` 完成，日志无 warning/error。
- Cargo check：完成，无 error。
- release EXE：前端 2879 modules 构建成功；Rust release profile 21m44s 完成，日志明确写出生成的 `caseboard.exe`，无 error/warning。
- 定向双端 fixture：`1 passed / 0 failed / 59 filtered`。

### 本线程补跑/复核

- `pnpm test:logic`：`123 passed / 0 failed`。
- 设置 `C:\Users\William Feng\.cargo\bin` 到当前进程 PATH 后，`pnpm validate:source`：`source=0.8.3, published=0.8.2`，通过。
- `git diff --check`：通过；只有 LF/CRLF 工作区提示，无 whitespace error。
- 对 release EXE 重新读取 PE 版本、ProductName、文件大小、SHA-256 和 Authenticode，均与 R2 报告一致。

## 五、外部阻塞与下一步

以下项目不得由本地结果替代，继续保持 `blocked_external`：

1. 来源可追溯的 0.8.2 历史 checksum、发布迁移 SQL 哈希、完整 sentinel 和在线一致性只读副本；取得后另开 `M1-COMPAT` 实现/复审，不得猜值。
2. 受控 CI 中生成并验证 0.8.3 NSIS/updater minisign 产物；如项目要求 Windows Authenticode，需另行配置并独立核验。
3. 获得明确授权后才可 commit/push/tag/Release 和把 `release/latest.json` 从 0.8.2 快进到已核验的 0.8.3 唯一资产。
4. 使用有回滚备份的指定 0.8.2 物理测试端完成真实在线下载、验签、安装、重启和升级后数据库检查。
5. 使用两台可回滚 Windows 物理设备、全新隔离同步目录及新测试组完成双向收敛与恢复验收；不得沿用当前失败组或正式 NAS 目录。
6. release EXE 启动冒烟只能在可丢弃 Windows VM/全新无正式凭据用户中执行，或先另开任务实现仅测试可用的临时凭据后端。

建议主控的最小收尾顺序：先复核并有界提交计划内 RC 差异与证据，确认工作区剩余变化可解释；再把本地 RC 状态落档为已接受、最终发布保持 `blocked_external`；待外部输入和授权齐备后，依次做签名资产、实机升级、物理双端，最后才更新远端 Release 与 `latest.json`。
