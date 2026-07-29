# V081-RELEASE：Windows 0.8.1 签名构建与产物核验指南

## 1. 结论与当前阻断

- Windows 正式构建应使用 `pnpm tauri build --bundles nsis`。`src-tauri/tauri.conf.json` 已启用 `createUpdaterArtifacts: true`，会生成 NSIS 安装包及同基名 `.sig`。
- updater 使用 Tauri minisign；它与 Windows Authenticode 是两套独立机制。`Get-AuthenticodeSignature` 显示 `NotSigned` 不等于 updater 签名无效，正式 updater 是否可用以仓库自带的 `verify_updater_signature` 验证结果为准。
- 发布 tag 必须为 `v0.8.1-fanglv`；安装包文件名必须含 `_0.8.1_`；候选目录中必须且只能有一份 `*-setup.exe` 和一份同基名 `.sig`。
- 当前只读检查确认：`package.json`、`src-tauri/Cargo.toml`、根 `Cargo.lock`、`src-tauri/tauri.conf.json` 均为 `0.8.1`；`release/latest.json` 保持已发布版本 `0.8.0`，这是允许的。
- **当前阻断：`CHANGELOG.md` 尚无 `## [0.8.1]` 标题，故 `pnpm.cmd run validate:source` 当前会失败。应先由主控补齐 0.8.1 变更记录，再开始正式签名构建。**

本指南只覆盖本地构建与候选产物核验，不创建 tag、不上传 GitHub、不改写正式 `release/latest.json`。

## 2. 前置条件

在已从批准的安全存储载入下列环境变量的 PowerShell 中执行；不得把私钥、密码写入命令行、脚本、报告或日志：

```powershell
if ([string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY) -or
    [string]::IsNullOrWhiteSpace($env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD)) {
  throw '缺少 Tauri updater 签名环境变量，停止构建。'
}
```

确认 Node 22+、pnpm 10+、Rust/Cargo、Windows Tauri/NSIS 构建依赖已可用：

```powershell
node --version
pnpm.cmd --version
cargo --version
```

## 3. 最短可执行命令清单

### 3.1 源码发布门禁

```powershell
$repo = 'D:\CodexWorkspace\008案件看板应用\case-board-v0.8.1'
Set-Location -LiteralPath $repo
pnpm.cmd run validate:source
```

必须看到 `source gate OK`。若提示 `CHANGELOG 缺少 0.8.1 标题`，先补齐变更记录，不得跳过门禁。

### 3.2 构建签名 NSIS 安装包

```powershell
pnpm.cmd tauri build --bundles nsis
```

无需另行先跑 `pnpm build`；Tauri 配置中的 `beforeBuildCommand` 已自动执行它。Windows 不得使用仓库的 `scripts/release.sh`，该脚本只用于 macOS DMG。

### 3.3 锁定本轮 0.8.1 原始产物

```powershell
$nsis = Join-Path $repo 'target\release\bundle\nsis'
$installer = @(
  Get-ChildItem -LiteralPath $nsis -File |
    Where-Object { $_.Name -match '_0\.8\.1_.*-setup\.exe$' }
)
if ($installer.Count -ne 1) {
  throw "NSIS 目录中应恰有一份 0.8.1 安装包，实际为 $($installer.Count) 份。"
}
$installer = $installer[0]
$signature = Get-Item -LiteralPath ($installer.FullName + '.sig')
```

不要直接把可能含旧版本残留的 `target\release\bundle\nsis` 交给 release gate。

### 3.4 验证原始 updater 签名

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml `
  --example verify_updater_signature -- `
  src-tauri/tauri.conf.json `
  $signature.FullName `
  $installer.FullName
```

该命令必须成功退出。随后记录文件、SHA-256 和 Authenticode 状态：

```powershell
Get-Item -LiteralPath $installer.FullName, $signature.FullName |
  Select-Object Name, Length, LastWriteTime
Get-FileHash -LiteralPath $installer.FullName, $signature.FullName -Algorithm SHA256
Get-AuthenticodeSignature -LiteralPath $installer.FullName |
  Select-Object Status, StatusMessage, SignerCertificate
```

### 3.5 建立无旧产物污染的 ASCII 候选目录

0.8.0 曾因中文安装包名在 GitHub CLI 上传后被规范化，导致 updater URL 与实际资产名不一致。0.8.1 沿用稳定 ASCII 名称，并同步改名 `.sig`：

```powershell
$artifactDir = Join-Path $repo (
  'release\v0.8.1-fanglv-candidate-' + (Get-Date -Format 'yyyyMMdd-HHmmss')
)
New-Item -ItemType Directory -Path $artifactDir | Out-Null

$candidateInstaller = Join-Path $artifactDir 'FanglvCaseBoard_0.8.1_x64-setup.exe'
$candidateSignature = $candidateInstaller + '.sig'
Copy-Item -LiteralPath $installer.FullName -Destination $candidateInstaller
Copy-Item -LiteralPath $signature.FullName -Destination $candidateSignature
```

改名不会改变安装包字节，但必须对最终候选文件再次验证：

```powershell
cargo run --locked --manifest-path src-tauri/Cargo.toml `
  --example verify_updater_signature -- `
  src-tauri/tauri.conf.json `
  $candidateSignature `
  $candidateInstaller
```

### 3.6 执行正式产物门禁并生成候选 manifest

```powershell
$draft = Join-Path $artifactDir 'latest-v0.8.1-draft.json'
node scripts/release-gate.mjs --mode release `
  --tag 'v0.8.1-fanglv' `
  --artifact-dir $artifactDir `
  --base-url 'https://github.com/fanglv8653/case-board-fanglv/releases/download' `
  --draft-output $draft
```

必须看到 `release gate OK`。该命令只生成候选 manifest，不发布、不覆盖正式 `release/latest.json`。

### 3.7 生成校验和并完成最终核验

```powershell
Get-FileHash -LiteralPath $candidateInstaller, $candidateSignature -Algorithm SHA256 |
  ForEach-Object {
    '{0}  {1}' -f $_.Hash.ToLowerInvariant(), [IO.Path]::GetFileName($_.Path)
  } |
  Set-Content -LiteralPath (Join-Path $artifactDir 'SHA256SUMS.txt') -Encoding UTF8

$manifest = Get-Content -LiteralPath $draft -Raw -Encoding UTF8 | ConvertFrom-Json
$asset = $manifest.platforms.'windows-x86_64'
$expectedUrl = 'https://github.com/fanglv8653/case-board-fanglv/releases/download/v0.8.1-fanglv/FanglvCaseBoard_0.8.1_x64-setup.exe'
$signatureText = (Get-Content -LiteralPath $candidateSignature -Raw -Encoding UTF8).Trim()

if ($manifest.version -ne '0.8.1') { throw '候选 manifest 版本错误。' }
if ($asset.url -ne $expectedUrl) { throw '候选 manifest 资产 URL 错误。' }
if ($asset.signature -ne $signatureText) { throw '候选 manifest 签名与 .sig 不一致。' }

Get-ChildItem -LiteralPath $artifactDir -File |
  Select-Object Name, Length, LastWriteTime
Get-Content -LiteralPath (Join-Path $artifactDir 'SHA256SUMS.txt') -Encoding UTF8
```

最终候选目录至少应包含：

1. `FanglvCaseBoard_0.8.1_x64-setup.exe`
2. `FanglvCaseBoard_0.8.1_x64-setup.exe.sig`
3. `latest-v0.8.1-draft.json`
4. `SHA256SUMS.txt`

## 4. 主控验收口径

- `validate:source` 成功，版本一致且 Changelog 已覆盖 0.8.1。
- Tauri 构建成功，原始安装包与 `.sig` 同基名。
- 仓库自带 updater 签名验证器对原始文件、ASCII 候选文件均成功退出。
- `release-gate --mode release` 成功，且候选目录无旧版本安装包污染。
- manifest 版本、tag URL、ASCII 资产名、签名内容逐项一致。
- SHA-256、文件大小、构建时间已留证；Authenticode 状态单独记录，不与 updater minisign 混淆。
- 本阶段不运行 `scripts/publish-release-resumable.ps1 -Apply`，不创建 tag、不上传、不发布。

## 5. v0.8.0 历史产物与证据位置

- 历史 release worktree：`D:\CodexWorkspace\008案件看板应用\case-board-v0.8.0-release`
- 本地初始产物：`D:\CodexWorkspace\008案件看板应用\case-board-v0.8.0-release\release\v0.8.0-fanglv`
- 最终 ASCII 发布候选：`D:\CodexWorkspace\008案件看板应用\case-board-v0.8.0-release\release\v0.8.0-fanglv-publish`
- 公网回下载证据：`D:\CodexWorkspace\008案件看板应用\agent-work\output\V080-release-30392616607\public-redownload`
- 正式发布验收报告：`D:\CodexWorkspace\008案件看板应用\agent-work\output\V080-FINAL-20260728_发布与正式升级验收.md`

0.8.0 的关键经验是：GitHub 上的最终资产名必须与 updater manifest URL 完全一致；中文文件名不能依赖上传工具自动保持不变。
