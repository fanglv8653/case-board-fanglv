# V083-RC-LOCAL-R2｜0.8.3 release executable 补验报告

- 逻辑线程：`worker-rc-local-r2`
- 交付状态：`submitted_for_review`
- 结论：原 RC-LOCAL 本地集成结果保持通过；本轮又成功生成并只读核验 0.8.3 release EXE。启动冒烟因系统凭据库无临时后端而 `not_run`；正式 bundle/签名/发布仍为 `blocked_external`。
- 边界：未读取正式 DB/NAS/同步组/凭据/飞书/GitHub，未启动可能接入正式默认数据的 EXE，未生成 bundle 或伪签名，未改 `release/latest.json`，未 commit/push/tag/Release。

## 一、原 V083-RC-LOCAL 结果汇总

| 项目 | 状态 | 本轮引用的已验证结果 |
| --- | --- | --- |
| 五处版本源 | `passed` | `package.json`、Tauri Cargo/config、根 lock 均为 0.8.3，CHANGELOG 已补 0.8.3；`release/latest.json` 保持 0.8.2 |
| pre-0063 生产升级 | `passed` | 真实执行 0001—0062，脱敏标记保留，两个子进程生产 init 升到 0063 并幂等重开；quick ok/FK 空 |
| 临时双文件端同步 | `passed` | A→B、B→A，无变更幂等，真实坏包隔离→显式 resume→修复重放→收敛，凭据精确清理 |
| 未知 checksum | `passed` | 未知 mismatch 继续在写入前 fail closed，未增 allowlist/未改 M1 策略 |
| 历史 checksum 正向兼容 | `blocked_external / pending_verified_input` | 尚无来源核验的旧 checksum 与对应发布谱系，未猜值 |
| Node logic | `passed` | 44 文件，123 passed |
| TS / Vite | `passed` | `tsc --noEmit` 通过；2879 modules build 通过，仅既有 chunk warning |
| Cargo check / Clippy | `passed` | all-targets locked check 通过；Clippy `-D warnings` 通过 |
| Windows Rust manifest 全量 | `passed` | 3 个 EXE 实际运行；lib 336 passed/4 ignored，main 0，device integration 60 passed |
| source gate | `passed` | source 0.8.3 / published 0.8.2 |
| release-resume | `passed` | 28 passed |
| Python 升级契约 | `passed` | 7 passed |
| capture self-test | `passed` | ASCII temp root，临时 PNG 成功 |
| `git diff --check` | `passed` | 无 whitespace error |

原始详细证据见 `.agent-work/output/V083-RC-LOCAL.md`。

## 二、本轮 release executable 补验

### 1. 实际构建

串行执行：

```powershell
pnpm tauri build --no-bundle
```

结果：`passed`。前端 production build 通过，Cargo `release` profile 优化构建通过，首次全量用时 21m44s；未进入 NSIS/MSI/updater 签名阶段。

### 2. EXE 只读核验

| 字段 | 结果 |
| --- | --- |
| 路径 | `D:\CodexWorkspace\008案件看板应用\case-board-v0.8.3-dev\target\release\caseboard.exe` |
| 大小 | 19,440,128 bytes |
| PE `FileVersion` | `0.8.3` |
| PE `ProductVersion` | `0.8.3` |
| PE `ProductName` | `方律案件看板` |
| SHA-256 | `277F1B151567AC5FB941E0CC28D7D19A389B022659216686B8A00985B08FCC61` |
| Authenticode | `NotSigned`（如实记录，不等于 updater minisign） |
| `target/release/bundle` | 不存在 |
| updater 私钥/密码环境变量 | 两者均 `Present=False` |
| `release/latest.json` | 仍为 `0.8.2` |

EXE 存在、文件版本和产品版本与源码 0.8.3 一致，本地 release executable 前置因此记为 `passed`。

## 三、12 秒隔离启动冒烟

状态：`not_run`（安全边界不满足，不冒充通过）。

已确认：

1. `CASEBOARD_DATA_DIR=<TempDir>` 能将仓库自管的 DB、`settings.json`、`crash.log` 和其他 `db::app_data_dir()` 内容指向临时目录。
2. `WEBVIEW2_USER_DATA_FOLDER=<TempDir>` 能隔离 WebView2 数据；还可把 `APPDATA/LOCALAPPDATA/TEMP/TMP` 指向临时目录。
3. 但前端 `App.tsx` 在首次启动的 `useEffect` 中立即调用 `getSettings()`；后端 `PublicSettings::from_settings()` 会调用 `credentials::static_statuses()`，而后者直接使用 `SystemCredentialBackend` 读取当前 Windows 用户凭据库。
4. 当前 release EXE 没有把凭据后端切换为 TempDir/内存测试实现的运行时入口。即使 DB/settings/log/webview 均已重定向，启动仍可能读取正式用户凭据状态。

因此本轮未启动 EXE、未创建或终止应用进程。要安全执行该冒烟，最小条件是以下二选一：

- 在可丢弃 Windows VM/全新本地测试用户中运行，该用户无正式凭据且全部 AppData 属于隔离环境；或
- 另开实现/复审，为 release 冒烟提供明确的临时凭据后端切换，并保证仅在测试运行中生效。

## 四、发布状态矩阵

| 项目 | 状态 | 说明 |
| --- | --- | --- |
| 0.8.3 本地 release EXE | `passed` | 实际优化构建，PE 版本与 hash 已核验 |
| 12 秒启动冒烟 | `not_run` | 系统凭据库无隔离后端，启动可能读正式凭据状态 |
| 本地 NSIS/MSI bundle | `blocked_external` | 本轮明确 `--no-bundle`；bundle 目录不存在 |
| updater minisign | `blocked_external` | 本机缺两个 updater signing 秘密，未生成伪签名 |
| Windows Authenticode | `not_configured` | EXE 如实为 `NotSigned`；不与 updater minisign 混淆 |
| 远端 tag/Release/assets/latest | `blocked_external` | 未访问 GitHub，未获发布授权，latest 保持 0.8.2 |
| 0.8.2 实机在线升级 | `blocked_external` | 缺正式 setup/.sig/latest、指定物理端和用户授权 |
| 物理双端 | `blocked_external` | 本地临时双端已绿，但不冒充物理设备验收 |
| 历史 checksum 正向兼容 | `blocked_external / pending_verified_input` | 需来源可追溯的 0.8.2 迁移元数据与在线一致性副本 |

## 五、本轮变更范围

- 新增本报告：`.agent-work/output/V083-RC-LOCAL-R2.md`
- 生成本地构建产物：`target/release/caseboard.exe`
- 保留构建日志：`.agent-work/output/V083-RC-LOCAL.release-exe.stdout.log` 与 `.stderr.log`
- 未修改任何产品源码、测试、版本源、迁移、签名配置或 `release/latest.json`

请主控独立复核；本线程不写 `accepted`。
