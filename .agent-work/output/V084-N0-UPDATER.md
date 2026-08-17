# V084-N0-UPDATER：更新生命周期与原子发布实现契约

状态：`submitted_for_review` 前置报告；本报告只读审计，不修改产品代码、迁移、版本、workflow、公开清单或外部状态。

## 1. 结论

v0.8.4 不能继续采用“前端 `downloadAndInstall()` 返回后写 `localStorage`，再调用 `relaunch()`”的模型。Windows 下当前锁定的 `tauri-plugin-updater 2.10.1` 在验签、解包并启动安装器后直接 `std::process::exit(0)`；因此现有 `src/lib/updater.ts:73-81` 在成功路径不可达，`src/components/UpdateAvailableDialog.tsx:63-70` 对这一行为的注释虽已预期，但成功凭据实际不会写入，`relaunch()` 也不会执行。

v0.8.4 应冻结为：**Rust 后端协调下载和验签，验签成功后把包交给独立更新 helper；常驻专用 shutdown coordinator OS 线程完成 sidecar 与数据库收敛并写耐久屏障，helper 只有同时看到屏障和旧 PID 退出后才启动安装器；安装器成功后由 helper 在当前用户专用、受限 ACL 目录原子写一次性成功回执，并仅携非秘密 `attempt_id` 拉起新版本，新版本首次启动核验并一次消费。** 成功不是旧进程写出的结果，而是安装器成功退出、受限回执存在、目标二进制按对应 attempt 启动、当前版本等于目标版本且回执合法这一组事实共同证明的结果。随机秘密不得进入命令行、日志、进程列表或 SQLite。

发布侧必须把 `release/version.json` 与 `release/latest.json` 当作一个清单对，在同一独立 Git 提交中更新并一次快进推送；Windows Release 事实资产只能是 `FanglvCaseBoard_<version>_x64-setup.exe` 及其同名 `.sig`。现有发布脚本只原子替换和提交 `release/latest.json`，不满足此契约。

## 2. 源码事实、已有能力与缺口

### 2.1 更新生命周期

| 分类 | 事实及证据 | 判断 |
| --- | --- | --- |
| 已有 | 更新检查由 Tauri updater `check()` 提供，失败被折叠为 `null`（`src/lib/updater.ts:15-17,37-43`）。 | 可保留 UI 降级，但实现任务应返回稳定错误码，不应只靠本地化文本。 |
| 已有 | 下载进度来自 `Started/Progress/Finished`（`src/lib/updater.ts:54-71`）。 | 后端协调器需要保留等价事件。 |
| 缺口 | 当前仅在 `await update.downloadAndInstall(...)` 之后写 `localStorage`，再 `relaunch()`（`src/lib/updater.ts:57-81`）。 | Windows 成功路径不可达。 |
| 依赖事实 | 仓库锁定 `tauri-plugin-updater 2.10.1`（`Cargo.lock:5337-5341`）；该版本下载后先验签再安装（本机锁定源码 `tauri-plugin-updater-2.10.1/src/updater.rs:704-729`），Windows 安装前仅调用 Tauri 默认 cleanup，随后 `ShellExecuteW` 并 `std::process::exit(0)`（同文件 `:837-865`）。 | 旧进程不会获得安装器完成结果，也不会执行 TS 后续语句。 |
| 缺口 | Tauri 默认 `cleanup_before_exit()` 只清资源表并隐藏窗口（锁定 `tauri-2.11.2/src/app.rs:1106-1120`）；项目自己的退出收尾仅挂在主窗 `Destroyed` 上（`src-tauri/src/lib.rs:6881-6890`）。 | updater 的直接进程退出不能证明 `lifecycle::shutdown()` 与 `pool.close()` 已执行。 |
| 已有 | 正常窗口销毁会收敛 `llama-server` 并关闭 SQLite pool（`src-tauri/src/lifecycle/mod.rs:35-37,226-232`；`src-tauri/src/lib.rs:6883-6890`）。 | 应抽成幂等的统一 shutdown routine，由正常退出和 updater 安装前共同调用。禁止恢复强杀主应用。 |
| 缺口 | `PendingUpdate` 仅有目标版本与 notes（`src/lib/updater.ts:19-24`），消费只比较当前版本并无条件删除（`:88-127`）。 | 缺少来源版本、尝试 ID、状态、时限、完整性和跨进程原子消费；localStorage 也无法证明在直接退出前已持久落盘。 |
| 已有 | 成功框只呈现版本和说明（`src/components/UpdateSuccessDialog.tsx:18-57`），启动时调用消费函数（`src/App.tsx:268-278,1301-1306`）。 | UI 可复用，但数据源必须改为后端一次性 claim。 |
| 已有 | updater 配置启用签名资产、Windows `passive` 安装（`src-tauri/tauri.conf.json:30-33,59-67`）。 | `passive` 对 NSIS 会使用 `/P /R`；新版本自动启动是成功检测入口，不再额外 `relaunch()`。 |

### 2.2 发布与资产

| 分类 | 事实及证据 | 判断 |
| --- | --- | --- |
| 已有 | workflow 校验源码、逻辑、TS、Vite、Rust，要求签名秘密且不回显（`.github/workflows/build-windows.yml:37-81`）。 | 保留。 |
| 缺口 | workflow 只搜索“唯一 `*-setup.exe` 和 `.sig`”，没有强制 ASCII 精确名称，也没有将构建产物规范化为事实目录（`.github/workflows/build-windows.yml:118-136`）。 | 必须新增清洁 staging 目录和精确名门禁。 |
| 已有 | 最终安装器会做 updater minisign 验证并单独记录 Authenticode 状态（`.github/workflows/build-windows.yml:138-153`）。 | 对改名后的最终事实文件复验。Updater minisign 与 Authenticode 不得混称。 |
| 缺口 | `release-gate.mjs` 只要求文件名含版本（`scripts/release-gate.mjs:107-134`），不是精确 ASCII 契约。 | 改为大小写敏感的精确等值。 |
| 已有 | 可恢复发布会核对 tag 指向、资产大小/SHA-256，瞬时错误有限重试，同名不同内容失败关闭（`scripts/publish-release-resumable.ps1:133-198`；`scripts/release-resume-core.psm1:67-113`）。 | 可扩展，不重写已有收敛算法。 |
| 缺口 | Release 不存在时脚本直接创建正式 Release，随后逐资产上传（`scripts/publish-release-resumable.ps1:141-162`）。 | 允许“资产尚未完整”的公开窗口；应先建 draft、上传并回读齐套，再发布正式 Release。 |
| 缺口 | 清单阶段只允许、替换、暂存和提交 `release/latest.json`（`scripts/publish-release-resumable.ps1:220-262`）。 | `version.json` 与 `latest.json` 尚未形成同一提交事务。 |
| 已有 | 当前两个公开入口分别由后端版本提示读取 `version.json`（`src-tauri/src/update.rs:17-18,70-102`）和 Tauri updater 读取 `latest.json`（`src-tauri/tauri.conf.json:59-64`）。 | 两者任一先更新都会产生手动提示与应用内更新不一致，必须成对发布。 |
| 现状证据 | `release/version.json:2-5` 与 `release/latest.json:2-8` 当前均为 0.8.3，但 latest 的 URL 已是 ASCII；其签名 trusted comment 仍可解码出中文构建文件名。 | v0.8.4 起以 workflow staging 后的 ASCII 文件为签名复验、清单 URL、上传和远端回读的唯一事实源。历史 0.8.3 不追改。 |

## 3. 冻结的更新状态机

### 3.1 状态与转换

1. `idle -> available`：后端检查得到严格高于 `source_version` 的 `target_version`。
2. `available -> downloading`：用户明确点击“立即更新”；生成 `attempt_id`，但此时不写成功凭据。
3. `downloading -> verified`：包下载完成且 updater minisign 验证通过。下载失败、超时或签名失败直接进入 `failed`，不得留下 `prepared` 凭据。
4. `verified -> prepared`：后端把更新尝试文件原子落盘；只有落盘成功才能继续安装。
5. `prepared -> helper_ready`：复制到临时目录的独立 updater helper 启动并回报 ready；helper 只接收非秘密 `attempt_id`、受限状态目录、包路径、旧 PID 和目标 exe 路径，不持有数据库连接或秘密。helper 未 ready 不得开始 shutdown。
6. `helper_ready -> quiescing`：主线程向常驻专用 shutdown coordinator OS 线程发送一次性请求；coordinator 在自己的 current-thread Tokio runtime 中依次停止本应用拥有的 sidecar、`pool.close().await`，成功后原子写 `shutdown-complete-<attempt_id>` 耐久屏障，再回复内存 ack。此后应用进入终端 quiescing，拒绝新业务命令。
7. `quiescing -> old_exited`：主线程收到成功 ack 后执行 Tauri cleanup 并退出。helper 必须同时观察到耐久屏障和旧 PID/进程句柄已退出，才可继续；不能用固定 sleep 代替。
8. `old_exited -> installing`：helper 以可取得进程句柄/退出码的 Windows 启动方式运行 NSIS，禁用安装器自行 `/R`；helper 等待真实退出结果。退出码 0 后再校验目标 exe 存在且文件版本为 target，在受限状态目录原子写 `installer_succeeded` 一次性回执，然后仅以 `--caseboard-update-attempt <attempt_id>` 拉起目标 exe。`attempt_id` 是关联键而非认证秘密，允许出现在进程参数，但日志仍只记录截断/哈希形式。
9. 新进程启动时：
   - `current == target && current != source && phase == installer_succeeded && 命令行 attempt_id 与回执一致 && 回执合法未过期`：原子 claim 为 `consumed`，返回成功提示一次；
   - `current == source`：判为 `not_applied`（安装失败、取消或未完成），归档/清除凭据，不显示成功；
   - 其他版本、过期、损坏或 schema 不支持：`invalid_or_expired`，隔离凭据，不显示成功。
10. 成功框关闭只改变 UI；凭据在返回 UI 前已完成原子 claim，确保至多显示一次。进程在 claim 后、绘制前崩溃时不重复弹，这是“不得重复”优先于“至少显示一次”的明确取舍。

### 3.2 持久化契约

路径：`<app_data_dir>/update/attempts/update-attempt-v1-<attempt_id>.json`，不得放 SQLite，也不得依赖 WebView localStorage。建议字段：

```json
{
  "schema_version": 1,
  "attempt_id": "uuid-v4",
  "source_version": "0.8.3",
  "target_version": "0.8.4",
  "created_at": "RFC3339 UTC",
  "expires_at": "RFC3339 UTC, created_at + 30 minutes",
  "phase": "prepared | shutdown_complete | installer_succeeded",
  "installer_exit_code": "0 only when phase is installer_succeeded",
  "package_sha256": "hex sha256",
  "installed_exe_version": "0.8.4",
  "notes": "长度受限的纯文本或 null"
}
```

- 目录与 ACL：状态目录固定为 `<app_data_dir>/update/attempts/`。创建时解析当前登录用户 SID，设置 protected DACL（禁用继承），只授予该 SID 所需读写/删除权限；不得授予 `Everyone`、`Users`、`Authenticated Users` 或其他宽主体。每次使用前同时核对目录 owner、DACL 已保护且 allow ACE 仅为预期 SID；不符合即 `UPD_RECEIPT_ACL_INVALID` 并停止更新。不得把回执放 SQLite、Temp 公共目录或 WebView localStorage。
- 写入：在上述同目录用不跟随重解析点的安全句柄创建随机临时文件，显式施加同一 protected DACL，UTF-8 无 BOM，`flush/sync_all` 后原子 rename/replace 为 `update-attempt-v1-<attempt_id>.json`；rename 后复核 owner/DACL。Windows 目标已存在时只允许同 attempt 的合法前态受控替换，失败保持旧文件不动。
- 校验：严格 schema、严格 SemVer、`source != target`、命令行 `attempt_id` 与文件名及正文完全一致、当前版本必须等于 target、phase 必须为 `installer_succeeded`、`installer_exit_code == 0`、`installed_exe_version == target`、时间窗有效、notes 限长（建议 16 KiB），且 owner/DACL 仍满足上述契约。未知字段可忽略，未知 schema 失败关闭。
- claim：对 pending 文件取得进程级互斥后原子 rename 为 `update-attempt-v1.consumed-<attempt_id>.json`；只有 rename 成功者得到 UI 数据。消费档案可在后续启动清理，不参与再次提示。
- 失败：下载/验签/包落盘/helper 启动在旧进程仍正常服务时失败，应删除本次同 `attempt_id` 的 prepared 文件；删除失败返回稳定错误并阻止“重试安装”。旧版本再次启动发现自己的 source 凭据时归档为 `not-applied`。
- 屏障超时：主线程最多等待固定时限（建议 15 秒）取得 shutdown ack。超时后不得启动安装器、不得删除可能仍在使用的数据库状态，也不得恢复业务服务；UI 进入不可逆“正在安全退出/更新未启动”终端页。一个非 Tokio watcher 继续等待 coordinator 结束；成功则正常退出，明确失败则记录并让用户正常关闭/重启。禁止 `std::process::exit`、强杀或在 Tokio worker 中嵌套 `block_on`。
- 通道断开：请求发送前断开表示 shutdown 未开始，helper 无耐久屏障故不得安装，应用可删除本 attempt 并保持正常；请求发送后 ack 通道断开属于结果不明，应用进入上述终端 quiescing。helper 只信 coordinator 原子写出的耐久屏障，不信内存 ack，因此不会因 ack 丢失误装。
- shutdown 失败：coordinator 不写耐久屏障，返回 `UPD_SHUTDOWN_FAILED`；helper 超时退出并不得启动安装器。若失败发生在 pool/sidecar 已部分收敛后，应用不恢复业务服务，只允许正常关闭后由用户重启；这避免“窗口可继续操作但数据库已关闭”的半失效状态。
- 安装失败：helper 启动安装器失败、用户取消、非零退出、超时或目标 exe 版本不符时，写入明确失败 phase，绝不写 `installer_succeeded`、绝不启动目标应用；下次人工启动只显示更新未完成/允许重试，不显示成功。
- 安全边界：成功回执绑定安装器成功退出、attempt、包摘要和目标版本。取消后 helper 不写 `installer_succeeded`，因此随后手工安装同一版本也不会误报为本次应用内更新成功。受限 ACL 防止其他本机用户、宽权限继承及普通跨用户进程读取/篡改，但**不把同一用户权限下的恶意进程视为可抵御对手**；同用户恶意代码可冒充用户读写其应用数据，这是本地用户账户失陷边界。若未来要求抵御同用户恶意进程，必须引入 Windows DPAPI/CNG 保护的秘密或受保护服务，并通过受限 ACL 文件、不可继承句柄或命名管道传递；秘密仍不得出现在命令行、环境变量、日志、SQLite 或崩溃报告。本版不引入这类秘密认证，也不得制造已有该保证的表述。

### 3.3 实现方式

新增 Rust 更新协调模块，使用 `UpdaterExt` 的 `check()`/`download()` 完成元数据检查、下载和 minisign 验证，并通过 Tauri channel 发进度；下载字节原子写入 updater 临时目录。**不得调用当前插件的 `install()` 或 `download_and_install()`。** 锁定插件的 `on_before_exit` 类型是同步 `Fn()`（本机锁定源码 `tauri-plugin-updater-2.10.1/src/updater.rs:288-291`），没有返回错误的通道；即使 hook 内有界等待超时，hook 返回后插件仍会无条件执行 `ShellExecuteW` 和 `std::process::exit(0)`（同文件 `:837-865`）。所以它无法表达“shutdown 失败则禁止安装”，也无法满足失败关闭。

可执行方案由三个部件组成：

1. **常驻 shutdown coordinator OS 线程**：在数据库 pool 建立后启动一次；线程独占一个 current-thread Tokio runtime 和请求 receiver。只有该线程允许 `runtime.block_on(pool.close())`，且它不位于任何 Tokio runtime worker 内；普通 async command、Tauri hook 和 Tokio worker 中一律禁止嵌套 `block_on`。
2. **同步/耐久双屏障**：内存 one-shot ack 只用于旧进程 UI 决定何时退出；同目录原子写出的 `shutdown-complete` 文件才是 helper 是否可安装的授权。写屏障必须晚于 sidecar 收敛和 pool close 成功。
3. **独立 updater helper**：冻结为极小专用 Rust binary，并在使用前把 helper 复制到 attempt 临时目录、核对构建期记录的 SHA-256 后启动；不得复用安装目录内正在运行的主 exe，以免安装时文件锁行为不确定。helper 等旧 PID 退出、启动并等待 NSIS、核对退出码和目标文件版本、在受限 ACL 目录写成功回执、仅携非秘密 `attempt_id` 启动新 app。命令行构造及日志 redaction 测试必须证明不存在 token、密码、密钥或回执正文。

`Update::install()` 返回 `Err` 的分支在本冻结方案中被结构性消除，因为该 API 根本不调用。若实现仍调用它，则必须判定不符合 N0：它可能在 hook 之前因解包返回 `Err`，也可能在同步 hook 无法否决时继续启动安装器。等价的 helper 分支为：包写入/PE 预检失败发生在 shutdown 前，清理 attempt 并保持原应用；安装器 launch 返回错误发生在 shutdown 后，helper 写 `install_launch_failed`，旧应用已安全退出，不留下半失效窗口，用户下次正常启动恢复。

主控需冻结该 **helper + 专用 coordinator** 方案。仅在前端提前关库，或在同步 hook 内调用 `tauri::async_runtime::block_on`，都会分别产生半失效窗口或 Tokio 嵌套 runtime 风险，均禁止。

## 4. 原子发布与 ASCII 资产契约

### 4.1 唯一资产名

对版本 `X.Y.Z`，且只允许严格 release SemVer（本轮即 `0.8.4`）：

- `FanglvCaseBoard_X.Y.Z_x64-setup.exe`
- `FanglvCaseBoard_X.Y.Z_x64-setup.exe.sig`

正则门禁：`^FanglvCaseBoard_[0-9]+\.[0-9]+\.[0-9]+_x64-setup\.exe(?:\.sig)?$`，同时按预期版本做精确字符串等值。文件名所有字符必须位于 ASCII `0x20..0x7E`，不得有空格、路径分隔符、百分号编码别名或大小写变体。

workflow 在构建结束后创建空的 staging 目录，将唯一 NSIS exe 与其签名复制/改名为上述两项；随后对 **staging 中的最终名称** 重做 minisign 验证、SHA-256、`release-gate`、artifact upload。后续脚本、清单 URL、GitHub Release 远端回读只接受 staging 事实，不再从中文 `productName` 推导外部资产名。

### 4.2 发布事务顺序

1. 冻结 source commit；校验 `package.json`、Cargo、Tauri、lock、CHANGELOG 版本一致。
2. tag 必须为 `vX.Y.Z-fanglv` 且远端解析到冻结 commit；workflow 必须 checkout/验证该 tag commit，而非任意默认分支 HEAD。
3. 构建、规范化 ASCII staging、验签、Windows 覆盖安装验收，生成 exe/sig SHA-256。
4. 创建或复用 **draft Release**，target 必须等于冻结 commit；上传两项精确资产及允许的附属 ASCII 文件，逐项远端回读 size/digest（API 无 digest 时回下载算 SHA-256）。齐套前保持 draft。
5. 发布 Release 后再次回读：非 draft、非 prerelease、target、tag、资产名/URL/digest/签名均一致。
6. 生成 `version.json` 与 `latest.json` 两份本地 draft，并交叉校验同一 version/tag：
   - `version.json.download_url` 精确为该正式 Release tag 页；
   - `latest.json.platforms.windows-x86_64.url` 精确为 ASCII exe 的 `browser_download_url`；
   - `latest.json.signature` 精确等于远端事实 exe 对应本地 `.sig` 内容；
   - notes/released_at/pub_date 格式有效。
7. 在远端 main 仍等于 `ExpectedMainCommit` 时，原子替换两文件；暂存区和 `ExpectedMainCommit..HEAD` 的文件集合必须**恰好**是 `release/version.json`、`release/latest.json`；单个提交 `chore: publish X.Y.Z release manifests`；一次非强制快进推送。
8. 推送后从 raw GitHub 回读两清单，校验其 commit（或 ETag/响应内容）、版本、URL、签名与 Release 事实一致，才宣告发布完成。

### 4.3 恢复语义

- draft Release/上传中断：公开清单仍指旧版；重跑先查远端并按 digest 收敛，禁止覆盖同名不同内容。
- Release 已发布、清单未提交：公开更新入口仍是旧版；重跑验证 Release 后生成清单对。
- 两清单已在本地同一提交、尚未推送：允许仅当该提交恰含两文件且从 ExpectedMainCommit 快进时继续。
- push 返回超时：先查远端 main；已等于本地提交则收敛，仍等于 expected 才重试，其他值为漂移并停止。
- raw 回读一新一旧或内容不一致：返回失败，禁止创建第二个修补提交自动“猜测修复”；保留证据交人工决定。Git 单提交推送本应避免该状态，该分支用于 CDN/缓存异常检测。
- 任何阶段失败均不得把本地或远端同名资产强制覆盖，也不得修改 tag。

## 5. 稳定错误码

UI 根据 code 映射中文提示；日志可附不含秘密的 `detail`。不得用中文错误文本判断分支。

| 错误码 | 触发条件 | 原子性要求 |
| --- | --- | --- |
| `UPD_CHECK_UNAVAILABLE` | 更新端点不可达/无有效更新 | 无凭据、无退出 |
| `UPD_METADATA_INVALID` | 版本、URL、平台字段不合法 | 无凭据、无下载 |
| `UPD_DOWNLOAD_FAILED` | 下载/超时失败 | 无 prepared 凭据 |
| `UPD_SIGNATURE_INVALID` | updater minisign 不通过 | 无 prepared 凭据、不得安装 |
| `UPD_ATTEMPT_PERSIST_FAILED` | prepared 原子落盘失败 | 不得安装 |
| `UPD_SHUTDOWN_FAILED` | sidecar/数据库收尾失败 | 不得启动安装器，应用进入受控错误态 |
| `UPD_SHUTDOWN_TIMEOUT` | 15 秒内未取得 coordinator ack | helper 无屏障不得安装；应用进入终端 quiescing 并继续等收敛 |
| `UPD_SHUTDOWN_CHANNEL_CLOSED` | shutdown 请求或 ack 通道断开 | 发送前可回滚 attempt；发送后按结果不明失败关闭 |
| `UPD_INSTALL_PREPARE_FAILED` | 解包或安装启动前失败 | 清除本 attempt；旧进程继续 |
| `UPD_INSTALL_LAUNCH_FAILED` | helper 无法取得安装器进程句柄 | 不写成功、不启动目标应用 |
| `UPD_INSTALL_CANCELLED` | 安装器明确取消 | 不写成功、不启动目标应用 |
| `UPD_INSTALL_EXIT_NONZERO` | 安装器退出码非 0 或超时 | 不写成功、不启动目标应用 |
| `UPD_TARGET_BINARY_INVALID` | 安装后目标 exe 缺失或版本不符 | 不写成功、不启动目标应用 |
| `UPD_NOT_APPLIED` | source 版本再次启动发现自己的 prepared | 不显示成功，归档失败凭据 |
| `UPD_RECEIPT_INVALID` | schema、attempt、phase、安装结果、当前/目标版本或时间窗非法 | 隔离凭据，不显示成功 |
| `UPD_RECEIPT_ACL_INVALID` | 状态目录/文件 owner、继承位或 ACE 不符合当前用户专用契约 | 不写、不 claim、不显示成功，停止更新 |
| `UPD_RECEIPT_PERSIST_FAILED` | helper 无法原子写入或回读成功回执 | 不启动目标应用、不显示成功 |
| `UPD_RECEIPT_CONSUME_FAILED` | 原子 claim 失败 | 不显示成功，允许下次重新判断 |
| `REL_ASSET_NAME_INVALID` | 本地或远端资产非精确 ASCII 名 | 停止发布 |
| `REL_ASSET_CONTENT_MISMATCH` | 同名资产 size/digest 不一致 | 禁止覆盖 |
| `REL_RELEASE_TARGET_MISMATCH` | tag/Release target 不等于冻结 commit | 停止发布 |
| `REL_MANIFEST_PAIR_INVALID` | 两清单版本、URL、签名或文件集合不一致 | 不提交/不推送 |
| `REL_MAIN_DRIFT` | main 不等于 expected 且未收敛到本地清单提交 | 停止，不强推 |
| `REL_REMOTE_VERIFY_FAILED` | Release 或 raw 清单远端回读不一致 | 不宣告完成 |

## 6. 确定性测试与验收

### 6.1 更新器单元/集成测试

1. 状态机表驱动：每个允许转换成功，越级/回退转换返回固定错误码。
2. 文件原子性：临时写失败、rename 失败、旧文件存在、损坏 JSON、未知 schema、过期、notes 超限均失败关闭且不误删可诊断证据。
3. 一次消费：两个并发 claimant 只有一个取得成功提示；第二个返回空。
4. 版本矩阵：`source=0.8.3,target=0.8.4,current=0.8.4` 命中；current 为 source、其他版本、target==source、过期均不命中。
5. 下载/签名失败：未创建 prepared；包落盘/helper 启动失败会删除本 attempt；持久化失败时从未发 shutdown 请求。
6. coordinator 执行上下文：测试线程 ID 证明 shutdown 在专用 OS 线程运行；在 Tokio multi-thread/current-thread 测试中发请求均不触发嵌套 runtime panic；pool close 完成前绝不写耐久屏障。
7. 屏障故障注入：使用注入时钟而非真实等待，覆盖 ack 正常、15 秒逻辑超时、请求 sender 断开、请求已收后 ack receiver 断开、coordinator panic、sidecar 失败、pool close future 卡住/超时、耐久屏障写失败；各分支得到稳定状态，除成功耐久屏障外 helper 的 installer-launch spy 调用次数恒为 0。
8. timeout 后状态：业务命令门闩保持关闭；coordinator 最终成功时 watcher 触发正常退出；最终失败时不得强退或恢复半失效 UI。重复 shutdown 请求幂等且只写一个屏障。
9. helper：缺屏障、旧 PID 尚存、包哈希不符、启动器返回错误、用户取消、非零退出、安装超时、目标 exe 缺失/错误版本、ACL 不安全、回执落盘/回读失败均不写 `installer_succeeded` 且不启动 app；成功分支必须按“屏障→旧 PID 退出→exit 0→目标版本→受限回执→attempt_id 启动”的严格顺序。
10. `install()` 契约防回归：静态/编译测试保证产品代码不调用插件 `install`/`download_and_install`；若将来重新引入，测试必须失败并提示同步 hook 无法否决安装。
11. ACL/一次消费：注入继承开启、错误 owner、`Everyone`/`Users`/`Authenticated Users` allow ACE、重解析点替换、文件名与正文 attempt 不一致、重复 attempt、取消后手工安装同版，均不显示成功；合法回执的两个并发 claimant 只有一个成功。
12. 命令行与日志：捕获 helper、NSIS 和目标 app 的完整 argv 及测试 logger，断言仅含非秘密 attempt_id 和必要路径，不含随机 token、密钥、密码、回执正文；进程列表可见 attempt_id 不授予伪造受限回执的额外能力。
13. 前端：工作阶段禁止关闭；稳定 error code 映射到正确文案和手动下载；成功框只消费后展示一次。

### 6.2 发布脚本离线测试

扩展 `scripts/test-release-resume.ps1` 覆盖：精确 ASCII 成对通过；中文、空格、大小写变体、错误版本、多 exe、多 sig、sig 非同基名全部失败；两清单必须同版且 URL/签名对应；暂存区多/少任一文件失败；draft 中断重跑、Release 已发布但清单未推、local commit 未推、push timeout 已收敛、main 漂移均有确定结果。

### 6.3 RC 真实门禁

- 从真实 0.8.3 应用内更新到 0.8.4；确认旧进程退出、NSIS 完成、新版本自动启动，成功提示仅一次。
- 分别模拟断网、签名错误、用户取消、解包失败、旧进程收尾失败；均不得显示成功。
- 退出后应用/安装器/sidecar 相关进程归零；目标数据库 `PRAGMA quick_check=ok`，且无残留 WAL/SHM/journal。
- 从 workflow 下载的事实资产名、GitHub Release 名、`latest.json` URL 完全一致并全 ASCII；远端 exe digest 和本地 digest 相同，`.sig` 对最终 exe 验证通过。
- raw `version.json` 与 `latest.json` 同版，Git 提交只含这两文件，Release target/tag/URL/signature/digest 全链一致。

## 7. 后续非重叠任务范围与依赖

| 后续任务 | 允许文件范围 | 依赖/边界 |
| --- | --- | --- |
| `V084-U1-RUST` | 新增 `src-tauri/src/update_lifecycle.rs`（或同名目录）与专用 updater helper binary；仅为 coordinator 启动、命令注册、启动参数早期分流和统一 shutdown 所必需的 `src-tauri/src/lib.rs` 窄行；对应 Rust 测试 | 先于前端；不得改发布脚本、workflow、清单、版本。与其他功能在 `lib.rs` 的注册由主控串行整合。禁止在 Tokio worker 内嵌套 `block_on`，禁止调用插件 install。 |
| `V084-U1-FRONTEND` | `src/lib/updater.ts`、`src/components/UpdateAvailableDialog.tsx`、`src/components/UpdateSuccessDialog.tsx`、`src/App.tsx` 及专属测试 | 依赖 U1-RUST 的命令/事件契约；不得改 Rust 数据模型或发布链。 |
| `V084-R1-RELEASE` | `.github/workflows/build-windows.yml`、`scripts/release-gate.mjs`、`scripts/publish-release-resumable.ps1`、`scripts/release-resume-core.psm1`、`scripts/test-release-resume.ps1`、发布说明 | 独立于 U1；不得改应用运行时代码。公开 `release/version.json`、`release/latest.json` 仅在用户审阅 RC 并执行正式发布时由脚本成对修改。 |
| `V084-RC-WINDOWS` | 仅隔离验收目录和报告；不直接改代码 | 依赖 U1、R1 均通过；真实发布仍需用户另行批准。 |

## 8. 待主控冻结事项

1. 接受“独立 updater helper + 常驻专用 shutdown coordinator OS 线程 + 耐久屏障”作为唯一实现路径，并废弃产品代码中的插件 `install/downloadAndInstall` 与前端 `relaunch()`。
2. 接受成功提示语义为“helper 已取得安装器成功退出码、核验目标版本、写入当前用户受限 ACL 一次性回执并携非秘密 attempt_id 拉起目标版本”，而不是旧进程推断成功；本版威胁边界不覆盖同一用户权限下的恶意进程。
3. 接受一次消费采用 at-most-once：claim 后 UI 绘制前崩溃不重复弹。
4. 接受 Release 先以 draft 收敛资产、正式发布后再一次推送清单对；清单发布失败时 Release 可已公开，但更新入口继续指向旧版，重跑可恢复。
