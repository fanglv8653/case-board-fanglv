# V083-RC-LOCAL｜0.8.3 RC 本地集成交付报告

- 执行线程：`worker-rc-local`
- 交付状态：`submitted_for_review`
- 本地结论：本任务范围内的版本、隔离数据库、临时双端同步与可本地执行门禁已通过；正式发布仍为 `blocked_external`
- 安全边界：未访问正式数据库、NAS、同步组、飞书、GitHub 或任何正式凭据；未 commit/push/tag/Release，未生成伪签名，未改 `release/latest.json`

## 一、版本准备

1. 已将以下五处最小同步到 `0.8.3`：
   - `package.json`
   - `src-tauri/Cargo.toml`
   - `src-tauri/tauri.conf.json`
   - 根 `Cargo.lock` 中唯一 workspace `caseboard` package
   - `CHANGELOG.md` 新增 `0.8.3` 可审计条目
2. `pnpm validate:source` 最终通过：`source=0.8.3` 、`published=0.8.2`，Cargo metadata/lock/许可证/NOTICE/CHANGELOG 一致。
3. `release/latest.json` 仍为已发布的 `0.8.2`，无 diff；未生成 latest draft。

## 二、真实 pre-0063 升级夹具

新增 `rc_local_pre_0063_database_upgrades_through_production_init_idempotently`：

1. 仅加载并实际执行仓库 `0001`—`0062` 迁移，不先建当前库、不回删 0063 对象。
2. 插入固定脱敏标记，断言 pre-state 为 61 条成功迁移、`max=62`、无 63。
3. 用 `VACUUM INTO` 冻结无 WAL/SHM 的 main-only 临时输入库。
4. 两个独立子进程先后调用生产 `init_pool`，真实模拟首次升级与应用退出后再次重开，避免同一 SQLite 进程内持久 WAL 影响夹具语义。
5. 每次均断言 62 条成功迁移、`max=63`、0063 恰好一次、标记保留、`quick_check=ok`、`foreign_key_check` 为空；两次逻辑指纹完全一致。

定向结果：`1 passed / 0 failed / 339 filtered`，3.50s。

## 三、临时双文件端与真实隔离恢复

新增 Windows integration test `rc_local_two_file_endpoints_converge_idempotently_and_recover_real_quarantine`：

1. 使用两个 `TempDir` 文件 SQLite 端点和一个新 `TempDir` mounted-folder，通过生产配对和 `sync_once` 运行。
2. 第一轮 A 写入、A 导出、B 导入；第二轮 B 写入不同记录、B 导出、A 导入，最终 canonical 投影一致。
3. 双方无变更再运行时，导出/导入/冲突/隔离均为 0，canonical、sequence、outbox 总量和 revision 指纹不变。
4. 对 A 真实生成的待导入包做确定性 JSON 损坏；B 的生产 `sync_once` 返回 `SYNC_GROUP_AUTO_PAUSED`，只生成一条 active quarantine，业务投影未部分写入。
5. 恢复同一序列的原始认证包、显式 resume，再走生产 `sync_once`；包成功重放，active quarantine 归零并留下一条 resolved 历史。
6. 最终两端 canonical 一致、`quick_check=ok`、FK 空、pending outbox/conflict/active quarantine/manual review 全部为 0，最后一轮仍幂等。
7. 为新夹具和原 Windows pairing 测试增加同一测试级串行锁与 RAII 凭据清理；只记录本测试随机 group/device/invite 的精确条目，成功路径删除后逐项反查，panic 路径也只按记录的精确 key 清理；从不枚举凭据。

定向结果：`1 passed / 0 failed / 59 filtered`，4.65s。

## 四、checksum 边界

- 未增加 allowlist，未修改迁移，未新建 0064，未改 M1 checksum 策略。
- Windows Rust 全量实跑中，`unknown_checksum_fails_closed_before_any_database_write` 与 sentinel 优先级反例通过，未知 mismatch 继续 fail closed。
- “来源核验的历史 checksum 正向兼容”保持 `blocked_external / pending_verified_input`，不用当前 checksum 相等或猜值冒充。

## 五、完整本地门禁

| 门禁 | 结果 |
| --- | --- |
| `pnpm install --frozen-lockfile` | 通过，lockfile 无漂移 |
| `pnpm test:logic` | 通过：44 文件，123 passed / 0 failed |
| `pnpm exec tsc --noEmit` | 通过 |
| `pnpm build` | 通过：2879 modules；仅既有 chunk size warning |
| `cargo check --workspace --all-targets --locked` | 通过，1m30s |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | 通过，0 warning/error，2m05s |
| `scripts/run-windows-rust-tests.ps1` | 通过：3 个 EXE 均嵌入 manifest 并实际运行；lib 336 passed / 4 ignored，main 0，device integration 60 passed |
| `pnpm validate:source` | 通过：source 0.8.3 / published 0.8.2 |
| `scripts/test-release-resume.ps1` | 通过：28 |
| Python 升级工具契约 | 通过：7 |
| `capture-window.ps1 -SelfTest` | 通过：ASCII temp root，产出临时 PNG |
| `git diff --check` | 通过；仅 Git 的 LF/CRLF 工作区提示，无 whitespace error |

Windows Rust 脚本首次运行暴露 PowerShell 5.1 将 Cargo UTF-8 JSON 中的中文工作区路径误解码，导致已编译 EXE 被误报不存在。已在 `scripts/run-windows-rust-tests.ps1` 最小增加 Console/$OutputEncoding UTF-8 设置；复跑后脚本成功发现、嵌入 manifest 并运行全部 3 个 EXE。

保留的 `V083-RC-LOCAL.migration-test.stderr.log` 是修复前使用旧测试 EXE 直接运行时产生的中间失败证据，其中 `STATUS_ENTRYPOINT_NOT_FOUND` 不代表最终门禁失败。随后已重新编译测试 EXE、按仓库 Windows runner 嵌入 Common-Controls manifest，并以 `migration-rebuild` 定向复跑和 Windows Rust 全量复跑先后覆盖验证；最终父升级夹具及全量 396 项 Rust 测试均通过。

4 项 ignored 中新增的一项是只由 pre-0063 父夹具以独立进程显式调用的 helper，它已在父用例中运行两次并通过；其余三项为仓库既有 live/interactive ignored 测试。

## 六、release 本地前置与外部阻塞

已实际执行 release mode 的不需秘密部分：源码阶段通过，tag 参数为 `v0.8.3-fanglv`；随后按预期在“本地 NSIS 产物目录不存在”处 fail closed。本机状态为：

- `TAURI_SIGNING_PRIVATE_KEY` 存在性：`False`
- `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 存在性：`False`
- `target/release/bundle/nsis`：不存在
- `target/release/bundle/msi`：不存在
- 0.8.3 latest draft：未生成
- `release/latest.json`：仍为 `0.8.2`

本轮不尝试无私钥的 Tauri 签名 bundle，不降级为 unsigned 成功。

## 七、`blocked_external` 的最小资源与授权

1. **历史 checksum 正向兼容**：需用户确认的 0.8.2 在线一致性只读副本，包括来源/采集时间/文件 SHA-256、`_sqlx_migrations` 元数据、对应发布 SQL 哈希和完整 sentinel 结果；之后应另开 `M1-COMPAT` 实现/复审。
2. **正式签名产物**：需在受控 CI 中配置且使用两个 updater signing secret，生成唯一 0.8.3 NSIS setup 及同名 `.sig`，并对最终 setup 字节执行 updater minisign 验签和 SHA-256 记录。
3. **远端发布**：需用户单独授权 commit/push、创建 `v0.8.3-fanglv` tag、运行 Windows workflow、下载并核验 artifact；随后再分别授权 Release 和 `release/latest.json` 快进更新。
4. **0.8.2 实机在线升级**：需正式 setup/.sig/latest、用户指定的 0.8.2 物理测试端、在线一致性数据库副本及回滚备份，并明确授权在线下载/验签/安装/重启验收。
5. **物理双端**：需两台可回滚 Windows 测试设备、新空隔离同步目录、新测试组/身份、两端备份和明确的写入/清理授权；不得接触当前失败组或正式 NAS 目录。

## 八、本线程修改范围

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `Cargo.lock`
- `CHANGELOG.md`
- `src-tauri/src/db/migration_lineage_tests.rs`
- `src-tauri/tests/device_sync_contract.rs`
- `scripts/run-windows-rust-tests.ps1`

其他已有脏工作树修改属于前置任务/其他线程，本线程未回退、未改写。请主控按 33 号量表独立复审；本线程不写 `accepted`。
