# V083-RC-GATE｜RC 版本、签名与发布链只读门禁

- 盘点时间：2026-08-07
- 执行线程：`worker-rc-gate`
- 边界：只读源码、配置、脚本、Git 本地引用及“存在/不存在”状态；未构建、未签名、未读取或输出任何秘密，未访问正式数据库/NAS/飞书，未访问或修改 GitHub Release。
- 结论：**RC 本地准备尚未开始；最终发布状态必须为 `blocked_external`。** 当前链路结构完整，但源码仍是 `0.8.2`，本机无 updater 私钥环境变量、无 0.8.3 产物/标签，CI secret、远端 tag/Release/main 和正式两端资源均未在线核验。

## 一、当前版本源与最小修改

| 版本源 | 当前值 | 0.8.3 RC 要求 |
|---|---:|---|
| `package.json.version` | `0.8.2` | 改为 `0.8.3` |
| `src-tauri/Cargo.toml [package].version` | `0.8.2` | 改为 `0.8.3` |
| `src-tauri/tauri.conf.json.version` | `0.8.2` | 改为 `0.8.3` |
| 根 `Cargo.lock` 的 `caseboard` package | `0.8.2` | 同步为 `0.8.3` |
| `CHANGELOG.md` | 最高标题 `0.8.2` | 增加 `## [0.8.3]` 条目；否则 source gate 必然失败 |
| `release/latest.json` | 已发布 `0.8.2` | **版本准备阶段保持 0.8.2**；仅在 0.8.3 Release 安装包与 `.sig` 已验证并获发布授权后，用生成的 draft 原子替换 |

`pnpm-lock.yaml` 不保存根项目版本，无需因版本号单独修改。迁移文件中的 `v0.8.2` 注释、飞书 live-test 隔离记录名及 deprecated 注释不是发布版本源，不应为 RC 顺手改动。

最小版本差异应严格限制为：`package.json`、`src-tauri/Cargo.toml`、`src-tauri/tauri.conf.json`、根 `Cargo.lock`、`CHANGELOG.md`。发布最后一步才单独提交 `release/latest.json`。

`scripts/release-gate.mjs --mode source` 会校验上述前三个版本、Cargo.lock、Cargo metadata、CHANGELOG、LICENSE/NOTICE 与工作区路径，但只要求 `release/latest.json` 是合法 SemVer，不要求其提前等于源码版本；这一设计允许安全地保留当前已发布 0.8.2 清单。

## 二、pnpm、CI 与 Windows 本地门禁

### 已存在的自动化

- `pnpm test:logic`：Node 逻辑/UI 契约。
- `pnpm build`：TypeScript 编译加 Vite build；CI 另显式执行 `pnpm exec tsc --noEmit`。
- `cargo check --workspace --all-targets --locked`。
- `cargo clippy --workspace --all-targets --locked -- -D warnings`（在 `ci.yml`，Windows build workflow 本身未执行 Clippy）。
- `scripts/run-windows-rust-tests.ps1`：编译测试目标、为每个 Windows 测试 EXE 嵌入 manifest、逐个执行。
- `pnpm validate:source`：版本、Cargo workspace/lock、许可证、NOTICE、CHANGELOG 发布边界。
- `scripts/test-release-resume.ps1`：离线 Release 恢复/资产/manifest 防漂移测试。
- `python -m unittest discover -s scripts/windows-upgrade-validation/tests -v`：升级工具契约和 DB 审计测试。
- `capture-window.ps1 -SelfTest`：截图辅助工具自测。
- `Invoke-UpgradeValidation.ps1`：dry-run、隔离升级、正式安装三层门禁；正式模式强制同轮先过隔离升级、在线备份、安装包 SHA-256、安装后 EXE/卸载注册表版本、截图、DB 前后比较及明确确认短语。

### 本机只读能力状态

- Node、pnpm、GitHub CLI：可发现。
- Cargo/rustc：环境 PATH 未直接发现，但 `C:\Users\William Feng\.cargo\bin` 中两者均存在；计划要求只修改当前进程 PATH。
- Windows SDK `mt.exe`、`signtool.exe`：可发现；本任务未执行。
- `Cert:\CurrentUser\My` 代码签名证书数量：0。
- 本地 `target/release/bundle/nsis`、`target/release/bundle/msi`：均不存在，0.8.3 setup/.sig/MSI 数量均为 0。

### RC-LOCAL 应串行执行的精确命令

以下命令本任务均为 `not_run`，只供唯一写入/构建窗口使用：

```powershell
$env:PATH = 'C:\Users\William Feng\.cargo\bin;' + $env:PATH
pnpm install --frozen-lockfile
pnpm test:logic
pnpm exec tsc --noEmit
pnpm build
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\run-windows-rust-tests.ps1
pnpm validate:source
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\test-release-resume.ps1
$env:PYTHONDONTWRITEBYTECODE = '1'
python -m unittest discover -s scripts/windows-upgrade-validation/tests -v
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-upgrade-validation\capture-window.ps1 -SelfTest -Output "$env:TEMP\caseboard-capture-self-test.png" -AsciiTempRoot 'D:\CodexWorkspace\tmp\caseboard-capture'
```

版本修改后若 `--locked` 报 Cargo.lock 需要更新，应只同步根 lock 中本 workspace package 的版本，并用 `cargo metadata --locked --no-deps --format-version 1` 与 `pnpm validate:source` 反证未引入依赖漂移；不要运行会批量升级依赖的命令。

## 三、Windows bundle、updater 与签名边界

### 配置事实

- `tauri.conf.json`：`bundle.active=true`、`createUpdaterArtifacts=true`，声明 bundle targets 为 `nsis`、`msi`；正式 Windows workflow 实际只执行 `pnpm tauri build --bundles nsis`。
- updater endpoint 固定为 GitHub `main/release/latest.json`；0.8.2 已内置同一 updater 公钥、检查/下载/安装/重启链和所需 capability。
- Windows workflow 要求且只使用 `TAURI_SIGNING_PRIVATE_KEY`、`TAURI_SIGNING_PRIVATE_KEY_PASSWORD` 生成 Tauri updater `.sig`；遥测两个变量为可选，不是发布签名材料。
- workflow 要求恰好一个 `*-setup.exe` 和一个同名 `.sig`，随后用 `verify_updater_signature` 读取内置公钥并对最终安装包字节验签。
- release gate 再要求安装包文件名包含源码版本、签名可解码为 minisign、tag 严格为 `v<version>-fanglv`，并生成带版本、时间、签名和精确 Release URL 的 latest draft。

### 不得混淆的签名语义

仓库明确规定 `.sig` 是 Tauri updater minisign，不是 Windows Authenticode。当前 workflow 仅记录 Authenticode 状态并同时接受 `Valid` 与 `NotSigned`；README/SECURITY 也明确当前未配置 Authenticode 证书、时间戳和“已验证发布者”。因此：

- 可以在 minisign 验证通过后称“updater 制品签名已验证”；
- 不得称“Windows Authenticode/代码签名已验证”；
- 如果主控将 33 号量表的“正式签名安装包”解释为 Authenticode，则当前流程和凭据均不足，必须另行取得代码签名证书、时间戳服务并把 `NotSigned` 改为拒绝；这是新增外部发布范围，不应在 RC-LOCAL 中暗自实现。

### 仅检查存在性的安全方法

本机检查不得打印值：

```powershell
@('TAURI_SIGNING_PRIVATE_KEY','TAURI_SIGNING_PRIVATE_KEY_PASSWORD') | ForEach-Object {
  [pscustomobject]@{
    Name = $_
    Present = -not [string]::IsNullOrWhiteSpace([Environment]::GetEnvironmentVariable($_, 'Process'))
  }
}
```

本次结果两项均为 `Present=False`。不得把私钥或密码放入命令参数、日志、报告或文件。

GitHub Actions secret 只能安全检查名字是否存在，不能从本机状态推断；获准联网后可执行下列“只列名称”预检，不得请求 secret 值：

```powershell
$present = @(gh secret list --repo fanglv8653/case-board-fanglv --json name | ConvertFrom-Json | ForEach-Object name)
@('TAURI_SIGNING_PRIVATE_KEY','TAURI_SIGNING_PRIVATE_KEY_PASSWORD') | ForEach-Object {
  [pscustomobject]@{ Name = $_; Present = $present -contains $_ }
}
```

即使两个名称都存在，也只能证明配置槽存在；私钥是否可解密、是否匹配 `tauri.conf.json` 公钥，必须由一次正式构建产生 `.sig` 后运行最终字节验签才能证明。

## 四、CI、Release 与资产要求

### CI/build workflow 要求

1. 源码提交中所有版本源一致，CHANGELOG 有 0.8.3，source gate 通过。
2. 远端存在用于构建的确切提交/tag；workflow input 必须为 `v0.8.3-fanglv`。
3. GitHub Actions 两个 updater signing secret 存在且有效。
4. Windows build 通过 install、source、Node、TS、Vite、Cargo check、Windows Rust、12 秒隔离启动冒烟。
5. 产物目录只有一个 NSIS setup 和一个同基名 `.sig`；minisign 验证通过；Authenticode 状态只如实记录。
6. 生成 `latest.json` draft，并作为 workflow artifact 与 setup/.sig 一起上传。该步骤只上传 Actions artifact，不会自动创建 GitHub Release。

获发布授权且远端 tag 已指向正式提交后，构建触发命令为：

```powershell
gh workflow run build-windows.yml --repo fanglv8653/case-board-fanglv --ref v0.8.3-fanglv -f release_tag=v0.8.3-fanglv
```

### GitHub Release 资产

`publish-release-resumable.ps1` 至少要求一个 `*-setup.exe` 及其同名 `.sig`；可同时发布 `SHA256SUMS.txt`、`RELEASE_NOTES.md`。它要求：

- tag 已存在且解析到 `ExpectedCommit`；
- GitHub CLI 已认证；
- 正式 Release 不是 draft/prerelease；
- 同名远端资产的 size、SHA-256 与本地一致，不一致即 fail closed；
- updater draft 的 version、installer URL、signature 与已验证 Release 资产一致；
- 更新 manifest 时远端 main 仍等于 `ExpectedMainCommit`，且本地没有夹带 `release/latest.json` 之外的后续改动；只允许快进，不强推。

先只读预检：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\publish-release-resumable.ps1 `
  -Repository fanglv8653/case-board-fanglv `
  -Tag v0.8.3-fanglv `
  -ExpectedCommit <40位正式提交> `
  -ArtifactDirectory <仓库内0.8.3资产目录> `
  -NotesFile <RELEASE_NOTES.md绝对路径> `
  -PreflightOnly
```

只有用户明确授权后才可增加：

```powershell
-Apply -PublishUpdaterManifest -DraftManifestPath <latest-draft.json> -ExpectedMainCommit <发布前远端main的40位提交>
```

## 五、0.8.2 → 0.8.3 升级链

当前代码具备基础链：0.8.2 内置 updater 公钥和 main/latest endpoint；0.8.3 可生成同公钥对应的 signed NSIS updater asset；前端会 `check -> downloadAndInstall -> relaunch` 并核对运行版本。

必须按顺序补齐：

1. RC-LOCAL 完成五个最小版本文件、全部本地门禁与隔离数据库/双端同步证据。
2. 用正式 updater key 生成 0.8.3 setup/.sig；对最终 setup 字节执行 `verify_updater_signature`；记录 SHA-256。
3. 在隔离 0.8.2 数据库副本上运行 `Invoke-UpgradeValidation.ps1 -RunIsolatedUpgrade`；只有通过后，才评估获授权的正式安装模式。
4. tag、Release、资产全部验证后再发布 0.8.3 `latest.json`；不能提前替换当前 0.8.2 清单。
5. 在用户指定的 0.8.2 物理测试端执行实际在线更新，确认下载、验签、安装、重启、EXE/注册表版本均为 0.8.3，数据库 quick/FK/业务指纹与回滚备份合格。

隔离升级命令模板（不得使用唯一正式数据库）：

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\windows-upgrade-validation\Invoke-UpgradeValidation.ps1 `
  -SourceDatabase <0.8.2在线一致性副本绝对路径> `
  -OutputDirectory <D盘隔离证据目录> `
  -PythonPath <python.exe绝对路径> `
  -AsciiTempRoot 'D:\CodexWorkspace\tmp\caseboard-capture' `
  -RunIsolatedUpgrade `
  -AppExecutable <0.8.3待验EXE绝对路径>
```

正式安装模式还要求脚本规定的 `-Install`、安装包路径与 SHA-256、正式数据库规范路径、安装后 EXE、期望版本 `0.8.3`、卸载注册表路径和字面确认；必须在单次调用中先通过隔离门禁。实际 updater 在线链仍需单独的人机验收，因为该脚本测试的是安装包升级，不会替代应用内 updater 下载路径。

重要限制：0.8.2 的 endpoint 固定指向 production main，没有独立 staging endpoint。发布 `release/latest.json=0.8.3` 后所有 0.8.2 客户端都可能看到更新；因此实际在线升级只能作为获用户授权的最终发布步骤，不能在本地 RC 阶段冒充完成。

## 六、状态矩阵与外部阻塞

| 项目 | 状态 | 说明 |
|---|---|---|
| RC 发布链源码盘点 | `passed` | 版本源、脚本、CI、bundle/updater、资产与升级链已定位 |
| 本机工具“存在性”盘点 | `passed` | 未输出秘密；cargo 需当前进程补 PATH |
| 0.8.3 版本准备 | `not_run` | 当前仍为 0.8.2；须唯一写入窗口修改五处 |
| 本地源码/构建/Rust/release tooling 门禁 | `not_run` | 本任务明令不构建 |
| 本地签名 NSIS/updater/latest draft | `blocked_external` | 本机两个 updater signing env 均不存在 |
| GitHub Actions 签名能力 | `blocked_external` | secret 名称/有效性未获联网授权核验 |
| 0.8.3 tag/Release/assets/main manifest | `blocked_external` | 本地无 v0.8.3 tag、无产物；远端未在线核验且未获发布授权 |
| 0.8.2→0.8.3 实际在线升级 | `blocked_external` | 需先有正式资产/manifest、指定物理测试端和用户授权 |
| 两台正式设备与正式数据恢复 | `blocked_external` | 需在线一致性副本、新隔离同步目录、两端备份与用户授权；不得接当前失败组 |
| Windows Authenticode | `not_configured` | 当前产品明确不提供；不得误报为已签名发布者 |

最小外部资源/授权：

1. 确认两个 GitHub updater signing secret 名称存在，并允许在受控 workflow 中使用；无需向 Agent 提供值。
2. 允许创建/推送正式提交与 `v0.8.3-fanglv` tag、运行 Windows workflow、下载 Actions artifact。
3. 对 setup/.sig/SHA-256/latest draft 验证后，单独授权创建/更新 GitHub Release；再单独授权快进发布 `release/latest.json`。
4. 指定 0.8.2 物理测试端、在线一致性数据库副本、新隔离同步目录和两端备份位置，授权最终在线更新与双设备验收。
5. 若另行要求 Authenticode，再提供受控代码签名服务/证书和时间戳方案；不得把 updater 私钥替代为 Authenticode 证书。

## 七、只读 Git 快照

- 当前分支：`fix/v0.8.3-data-safety`
- 当前 HEAD：`8ec1b8e94a94cd0683e9cdf8bf4e633a02ae6215`
- 本地 `origin/main` 跟踪引用：`76e4788627bef621c500a3f82c5c63f6b21dcbed`
- 相对本地跟踪引用：ahead 11、behind 0；**未 fetch，不能代表远端当前状态**。
- 本地 `v0.8.3-fanglv` tag：不存在。

本报告只完成 RC-GATE 只读门禁，不代表 RC-LOCAL、本地接受或最终发布接受。
