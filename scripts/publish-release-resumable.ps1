[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory)][ValidatePattern('^[^/]+/[^/]+$')][string]$Repository,
    [Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+-fanglv$')][string]$Tag,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{7,40}$')][string]$ExpectedCommit,
    [Parameter(Mandatory)][string]$ArtifactDirectory,
    [string]$GitRemote = 'origin',
    [string]$ReleaseTitle,
    [string]$NotesFile,
    [ValidateRange(1, 10)][int]$MaxAttempts = 5,
    [ValidateRange(0, 300)][int]$BaseDelaySeconds = 2,
    [ValidateRange(5, 600)][int]$CommandTimeoutSeconds = 90,
    [switch]$PreflightOnly,
    [switch]$Apply,
    [switch]$PublishUpdaterManifest,
    [string]$DraftManifestPath,
    [ValidatePattern('^[0-9a-fA-F]{40}$')][string]$ExpectedMainCommit
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path $PSScriptRoot 'release-resume-core.psm1') -Force

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$artifactRoot = (Resolve-Path -LiteralPath $ArtifactDirectory).Path
if ($artifactRoot -notlike "$root\*") {
    throw '产物目录必须位于当前仓库内。'
}

$assetFiles = @(Get-ChildItem -LiteralPath $artifactRoot -File | Where-Object {
    $_.Name -match '(?i)(-setup\.exe(?:\.sig)?|SHA256SUMS\.txt|RELEASE_NOTES\.md)$'
} | Sort-Object Name)
if ($assetFiles.Count -lt 2) { throw '产物目录至少应包含 setup.exe 及其 .sig。' }
$installers = @($assetFiles | Where-Object Name -Match '(?i)-setup\.exe$')
if ($installers.Count -ne 1) { throw "必须恰有一个 setup.exe，实际 $($installers.Count)。" }
if (-not (Test-Path -LiteralPath "$($installers[0].FullName).sig" -PathType Leaf)) {
    throw "缺少安装包同名签名：$($installers[0].Name).sig"
}
if ($NotesFile) { $NotesFile = (Resolve-Path -LiteralPath $NotesFile).Path }
if (-not $ReleaseTitle) { $ReleaseTitle = "方律案件看板 $Tag" }
if ($PublishUpdaterManifest -and (-not $DraftManifestPath -or -not $ExpectedMainCommit)) {
    throw '-PublishUpdaterManifest 必须同时提供 -DraftManifestPath 和 -ExpectedMainCommit。'
}
if ($DraftManifestPath) { $DraftManifestPath = (Resolve-Path -LiteralPath $DraftManifestPath).Path }
$readOnly = $PreflightOnly -or -not $Apply -or $WhatIfPreference

$localAssets = @($assetFiles | ForEach-Object {
    [pscustomobject]@{
        name = $_.Name
        path = $_.FullName
        size = $_.Length
        sha256 = Get-CaseBoardFileSha256 -LiteralPath $_.FullName
    }
})

function Invoke-NativeCapture {
    param([string]$FilePath, [string[]]$Arguments, [string]$Label, [switch]$AllowNotFound)
    $resolvedCommand = (Get-Command $FilePath -ErrorAction Stop).Source
    $startInfo = New-Object Diagnostics.ProcessStartInfo
    $startInfo.FileName = $resolvedCommand
    $startInfo.Arguments = (($Arguments | ForEach-Object { ConvertTo-CaseBoardWindowsArgument -Value $_ }) -join ' ')
    $startInfo.UseShellExecute = $false
    $startInfo.CreateNoWindow = $true
    $startInfo.RedirectStandardOutput = $true
    $startInfo.RedirectStandardError = $true
    $process = New-Object Diagnostics.Process
    $process.StartInfo = $startInfo
    [void]$process.Start()
    $stdoutTask = $process.StandardOutput.ReadToEndAsync()
    $stderrTask = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit($CommandTimeoutSeconds * 1000)) {
        try { $process.Kill() } catch { }
        $process.WaitForExit()
        throw "$Label timed out after $CommandTimeoutSeconds seconds"
    }
    $text = $stdoutTask.Result
    $errorText = $stderrTask.Result
    $exitCode = $process.ExitCode
    if ($exitCode -ne 0) {
        if ($AllowNotFound -and $errorText -match '(?i)(HTTP\s+404|not found)') { return $null }
        $safeText = $errorText -replace '(?i)(ghp_|github_pat_)[A-Za-z0-9_]+', '$1***'
        throw "$Label 失败（exit=$exitCode）：$($safeText.Trim())"
    }
    $text.TrimEnd([char[]]"`r`n")
}

function Invoke-RetryNative {
    param([string]$FilePath, [string[]]$Arguments, [string]$Label, [switch]$AllowNotFound)
    Invoke-CaseBoardBoundedRetry -Label $Label -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
        Invoke-NativeCapture -FilePath $FilePath -Arguments $Arguments -Label $Label -AllowNotFound:$AllowNotFound
    }
}

function Get-LiveRelease {
    $json = Invoke-RetryNative -FilePath 'gh' -Arguments @('api', "repos/$Repository/releases/tags/$Tag") -Label '查询 GitHub Release' -AllowNotFound
    if (-not $json) { return $null }
    $json | ConvertFrom-Json
}

function Get-RemoteBranchCommit {
    param([string]$Branch)
    $output = Invoke-RetryNative -FilePath 'git' -Arguments @('-C', $root, 'ls-remote', '--heads', $GitRemote, "refs/heads/$Branch") -Label "查询远端 $Branch"
    if ($output -notmatch '^([0-9a-fA-F]{40})\s') { throw "无法解析远端 $Branch 提交。" }
    $Matches[1].ToLowerInvariant()
}

function Test-GitAncestor {
    param([string]$Ancestor, [string]$Descendant)
    & git -C $root merge-base --is-ancestor $Ancestor $Descendant
    $LASTEXITCODE -eq 0
}

function Assert-RemoteAssetHash {
    param($PlanItem)
    if ($PlanItem.reason -eq 'verified_by_api_digest') { return }
    $verifyDir = Join-Path ([IO.Path]::GetTempPath()) ("caseboard-release-verify-{0}" -f [guid]::NewGuid())
    New-Item -ItemType Directory -Path $verifyDir | Out-Null
    try {
        $downloaded = Join-Path $verifyDir $PlanItem.name
        $url = [string]$PlanItem.remote.browser_download_url
        if (-not $url) { throw "远端资产缺少下载 URL：$($PlanItem.name)" }
        Invoke-RetryNative -FilePath 'curl.exe' -Arguments @(
            '--fail', '--silent', '--show-error', '--location', '--http1.1',
            '--continue-at', '-', '--output', $downloaded, $url
        ) -Label "HTTP/1.1 断点下载校验 $($PlanItem.name)" | Out-Null
        if (-not (Test-Path -LiteralPath $downloaded -PathType Leaf)) { throw "下载后未找到资产：$($PlanItem.name)" }
        $hash = Get-CaseBoardFileSha256 -LiteralPath $downloaded
        if ($hash -ne $PlanItem.local.sha256) { throw "远端资产 SHA-256 不一致：$($PlanItem.name)" }
    }
    finally { Remove-Item -LiteralPath $verifyDir -Recurse -Force -ErrorAction SilentlyContinue }
}

Write-Host "[preflight] repository=$Repository tag=$Tag assets=$($localAssets.Count)"
Invoke-RetryNative -FilePath 'gh' -Arguments @('auth', 'status') -Label '检查 GitHub CLI 登录状态' | Out-Null
$tagOutput = Invoke-RetryNative -FilePath 'git' -Arguments @('-C', $root, 'ls-remote', '--tags', $GitRemote, "refs/tags/$Tag", "refs/tags/$Tag^{}") -Label '查询远端 tag'
if (-not $tagOutput) { throw "远端 tag 不存在：$Tag" }
if ($tagOutput -notmatch [regex]::Escape($ExpectedCommit)) {
    throw "远端 tag 未解析到预期提交 $ExpectedCommit；拒绝发布。"
}

$release = Get-LiveRelease
if (-not $release) {
    if ($readOnly) {
        Write-Host '[plan] Release 不存在：实际执行时将创建。'
        foreach ($asset in $localAssets) { Write-Host "[plan] upload $($asset.name) size=$($asset.size) sha256=$($asset.sha256)" }
        $release = [pscustomobject]@{ draft = $false; prerelease = $false; assets = @() }
    }
    $createArgs = @('release', 'create', $Tag, '--repo', $Repository, '--target', $ExpectedCommit, '--title', $ReleaseTitle)
    if ($NotesFile) { $createArgs += @('--notes-file', $NotesFile) } else { $createArgs += @('--notes', "方律案件看板 $Tag") }
    if (-not $readOnly -and $PSCmdlet.ShouldProcess("$Repository/$Tag", '创建 GitHub Release')) {
        Invoke-CaseBoardBoundedRetry -Label '创建 GitHub Release' -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
            $existing = Get-LiveRelease
            if ($existing) { return $existing }
            Invoke-NativeCapture -FilePath 'gh' -Arguments $createArgs -Label '创建 GitHub Release' | Out-Null
            Get-LiveRelease
        } | Out-Null
        $release = Get-LiveRelease
    }
}

if (-not $release) { throw '无法确认 GitHub Release 已存在。' }
if ($release.draft -or $release.prerelease) { throw '目标 Release 为 draft/prerelease，拒绝混入正式资产。' }

foreach ($local in $localAssets) {
    $liveRelease = Get-LiveRelease
    if ($liveRelease) { $release = $liveRelease }
    $remoteAssets = if ($liveRelease) { @($liveRelease.assets) } else { @() }
    $plan = @(Get-CaseBoardAssetPlan -LocalAssets @($local) -RemoteAssets $remoteAssets)
    $item = $plan[0]
    if ($item.action -eq 'fail') { throw "远端同名资产不一致（$($item.reason)）：$($item.name)" }
    if ($item.action -eq 'verify') {
        Assert-RemoteAssetHash -PlanItem $item
        Write-Host "[skip] $($item.name) 已存在且完整性一致。"
        continue
    }
    if ($readOnly) {
        Write-Host "[plan] upload $($local.name) size=$($local.size) sha256=$($local.sha256)"
        continue
    }
    if ($PSCmdlet.ShouldProcess("$Repository/$Tag/$($local.name)", '上传 Release 资产')) {
        Invoke-CaseBoardBoundedRetry -Label "上传 $($local.name)" -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
            # Every attempt converges from live state before it is allowed to write.
            $current = Get-LiveRelease
            $currentPlan = @(Get-CaseBoardAssetPlan -LocalAssets @($local) -RemoteAssets @($current.assets))[0]
            if ($currentPlan.action -eq 'verify') { return $currentPlan }
            if ($currentPlan.action -eq 'fail') { throw "远端同名资产不一致：$($local.name)" }
            Invoke-NativeCapture -FilePath 'gh' -Arguments @('release', 'upload', $Tag, $local.path, '--repo', $Repository) -Label "上传 $($local.name)" | Out-Null
            $current = Get-LiveRelease
            $currentPlan = @(Get-CaseBoardAssetPlan -LocalAssets @($local) -RemoteAssets @($current.assets))[0]
            if ($currentPlan.action -eq 'upload') { throw "上传后查询仍缺少资产，可能发生 timeout：$($local.name)" }
            if ($currentPlan.action -eq 'fail') { throw "上传后发现远端同名错误资产：$($local.name)" }
            $currentPlan
        } | Out-Null
        $release = Get-LiveRelease
        $verified = @(Get-CaseBoardAssetPlan -LocalAssets @($local) -RemoteAssets @($release.assets))[0]
        if ($verified.action -ne 'verify') { throw "上传后远端未收敛：$($local.name)" }
        Assert-RemoteAssetHash -PlanItem $verified
        Write-Host "[ok] $($local.name) 已上传并校验。"
    }
}

if ($PublishUpdaterManifest) {
    if (-not $release) { throw '必须先确认 Release 存在，才能发布 updater manifest。' }
    $installerRemote = @($release.assets | Where-Object name -EQ $installers[0].Name)
    if ($installerRemote.Count -ne 1 -and $readOnly) {
        $encodedName = [Uri]::EscapeDataString($installers[0].Name)
        $installerRemote = @([pscustomobject]@{
            name = $installers[0].Name
            browser_download_url = "https://github.com/$Repository/releases/download/$Tag/$encodedName"
        })
    }
    if ($installerRemote.Count -ne 1) { throw 'Release 中未找到唯一安装包资产。' }
    $draftText = Get-Content -LiteralPath $DraftManifestPath -Raw -Encoding UTF8
    $draft = $draftText | ConvertFrom-Json
    $expectedVersion = $Tag -replace '^v', '' -replace '-fanglv$', ''
    $signatureText = (Get-Content -LiteralPath "$($installers[0].FullName).sig" -Raw -Encoding UTF8).Trim()
    $manifestPlan = Get-CaseBoardManifestPlan -Draft $draft -ExpectedVersion $expectedVersion -Installer $installerRemote[0] -Signature $signatureText
    if ($manifestPlan.action -eq 'fail') { throw "updater manifest 校验失败：$($manifestPlan.reason)" }

    $latestPath = Join-Path $root 'release/latest.json'
    $localHead = (& git -C $root rev-parse HEAD).Trim().ToLowerInvariant()
    if (-not (Test-GitAncestor -Ancestor $ExpectedMainCommit -Descendant $localHead)) {
        throw '本地 HEAD 不是 ExpectedMainCommit 的快进后代。'
    }
    if ($localHead -ne $ExpectedMainCommit.ToLowerInvariant()) {
        $rangeFiles = @(& git -C $root diff --name-only "$ExpectedMainCommit..$localHead")
        if (@($rangeFiles | Where-Object { $_ -ne 'release/latest.json' }).Count -gt 0) {
            throw 'ExpectedMainCommit 之后包含 updater manifest 以外的提交，拒绝一并推送。'
        }
    }
    $remoteMain = Get-RemoteBranchCommit -Branch 'main'
    $mainPlan = Get-CaseBoardMainPlan -RemoteCommit $remoteMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected $true
    if ($mainPlan.action -eq 'fail') { throw "main 安全门禁失败：$($mainPlan.reason)" }

    $draftJson = $draftText.TrimEnd([char[]]"`r`n") + "`n"
    $latestJson = Get-Content -LiteralPath $latestPath -Raw -Encoding UTF8
    if ($mainPlan.action -eq 'converged') {
        if ($latestJson -ne $draftJson) { throw '远端已更新，但本地 latest.json 与 draft 不一致。' }
        Write-Host '[skip] updater manifest 已提交并推送。'
    }
    elseif ($readOnly) {
        Write-Host "[plan] validate, replace, commit and fast-forward push release/latest.json from $ExpectedMainCommit"
    }
    else {
        $latestStatus = (& git -C $root status --porcelain -- release/latest.json) -join "`n"
        if ($latestStatus -and $latestJson -ne $draftJson) {
            throw 'release/latest.json 已有不一致的未提交修改，拒绝覆盖。'
        }
        if ($latestJson -ne $draftJson) {
            $tempLatest = Join-Path (Split-Path -Parent $latestPath) ("latest-{0}.tmp" -f [guid]::NewGuid())
            try {
                [IO.File]::WriteAllText($tempLatest, $draftJson, (New-Object Text.UTF8Encoding($false)))
                Move-Item -LiteralPath $tempLatest -Destination $latestPath -Force
            }
            finally { Remove-Item -LiteralPath $tempLatest -Force -ErrorAction SilentlyContinue }
        }
        & git -C $root add -- release/latest.json
        & git -C $root diff --cached --quiet -- release/latest.json
        $hasCachedManifestDiff = $LASTEXITCODE -ne 0
        if ($hasCachedManifestDiff) {
            & git -C $root commit -m "chore: publish $expectedVersion updater manifest" -- release/latest.json
            if ($LASTEXITCODE -ne 0) { throw '提交 updater manifest 失败。' }
        }
        $localHead = (& git -C $root rev-parse HEAD).Trim().ToLowerInvariant()
        $remoteMain = Get-RemoteBranchCommit -Branch 'main'
        $mainPlan = Get-CaseBoardMainPlan -RemoteCommit $remoteMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected (Test-GitAncestor -Ancestor $ExpectedMainCommit -Descendant $localHead)
        if ($mainPlan.action -eq 'fail') { throw "推送前 main 漂移：$($mainPlan.reason)" }
        if ($mainPlan.action -eq 'push') {
            Invoke-CaseBoardBoundedRetry -Label '快进推送 updater manifest' -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
                $liveMain = Get-RemoteBranchCommit -Branch 'main'
                $livePlan = Get-CaseBoardMainPlan -RemoteCommit $liveMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected $true
                if ($livePlan.action -eq 'converged') { return }
                if ($livePlan.action -eq 'fail') { throw "推送重试前 main 漂移：$($livePlan.reason)" }
                Invoke-NativeCapture -FilePath 'git' -Arguments @('-C', $root, 'push', '--porcelain', $GitRemote, 'HEAD:refs/heads/main') -Label '快进推送 updater manifest' | Out-Null
            }
        }
        $finalRemote = Get-RemoteBranchCommit -Branch 'main'
        if ($finalRemote -ne $localHead) { throw '推送后远端 main 未收敛到本地 manifest 提交。' }
        Write-Host "[ok] updater manifest 已安全快进推送：$localHead"
    }
}
else {
    Write-Host '[ok] Release 资产状态已收敛；未请求 updater manifest 发布。'
}
