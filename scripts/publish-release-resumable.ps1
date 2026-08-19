[CmdletBinding(SupportsShouldProcess = $true)]
param(
    [Parameter(Mandatory)][ValidatePattern('^[^/]+/[^/]+$')][string]$Repository,
    [Parameter(Mandatory)][ValidatePattern('^v\d+\.\d+\.\d+-fanglv$')][string]$Tag,
    [Parameter(Mandatory)][ValidatePattern('^[0-9a-fA-F]{40}$')][string]$ExpectedCommit,
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

$expectedVersion = $Tag -replace '^v', '' -replace '-fanglv$', ''
$expectedInstallerName = "FanglvCaseBoard_${expectedVersion}_x64-setup.exe"
$expectedAssetNames = @($expectedInstallerName, "$expectedInstallerName.sig")
$assetFiles = @(Get-ChildItem -LiteralPath $artifactRoot -File | Sort-Object Name)
$assetContract = Get-CaseBoardReleaseAssetContract -Names @($assetFiles.Name) -ExpectedVersion $expectedVersion
if ($assetContract.action -ne 'accept') {
    throw "REL_ASSET_NAME_INVALID：产物目录必须且只能包含 $($expectedAssetNames -join ', ')"
}
$installers = @($assetFiles | Where-Object Name -CEQ $expectedInstallerName)
if ($installers.Count -ne 1) { throw 'REL_ASSET_NAME_INVALID：未找到唯一精确安装包。' }
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
    if ($json) { return ($json | ConvertFrom-Json) }

    # GitHub's release-by-tag endpoint returns 404 for draft releases. Fall
    # back to the authenticated release list so an interrupted publication can
    # resume the exact draft instead of trying to create a duplicate.
    $listJson = Invoke-RetryNative -FilePath 'gh' -Arguments @('api', "repos/$Repository/releases?per_page=100") -Label '查询 GitHub Release 列表'
    $releases = @($listJson | ConvertFrom-Json)
    Select-CaseBoardReleaseByTag -Releases $releases -Tag $Tag
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
Invoke-RetryNative -FilePath 'gh' -Arguments @('api', 'user', '--jq', '.login') -Label '检查 GitHub CLI API 登录状态' | Out-Null
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
        $release = [pscustomobject]@{ draft = $true; prerelease = $false; assets = @(); target_commitish = $ExpectedCommit }
    }
    $createArgs = @('release', 'create', $Tag, '--repo', $Repository, '--target', $ExpectedCommit, '--title', $ReleaseTitle, '--draft')
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
if ($release.prerelease) { throw '目标 Release 是 prerelease，拒绝发布。' }
if ($release.PSObject.Properties.Name -contains 'target_commitish' -and
    ([string]$release.target_commitish).ToLowerInvariant() -ne $ExpectedCommit.ToLowerInvariant()) {
    throw 'REL_RELEASE_TARGET_MISMATCH：Release target 与冻结提交不一致。'
}

foreach ($local in $localAssets) {
    $liveRelease = Get-LiveRelease
    if ($liveRelease) { $release = $liveRelease }
    $remoteAssets = if ($liveRelease) { @($liveRelease.assets) } else { @($release.assets) }
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

if (-not $readOnly) {
    $release = Get-LiveRelease
    if (-not $release) { throw '上传后无法回读 Release。' }
    foreach ($local in $localAssets) {
        $verified = @(Get-CaseBoardAssetPlan -LocalAssets @($local) -RemoteAssets @($release.assets))[0]
        if ($verified.action -ne 'verify') { throw "REL_ASSET_CONTENT_MISMATCH：$($local.name) 未收敛" }
        Assert-RemoteAssetHash -PlanItem $verified
    }
    $remoteNames = @($release.assets | ForEach-Object { [string]$_.name })
    if ($remoteNames.Count -ne 2 -or @($remoteNames | Where-Object { $_ -notin $expectedAssetNames }).Count -ne 0) {
        throw 'REL_ASSET_NAME_INVALID：远端 Release 资产集合不等于精确资产对。'
    }
    if ($release.draft -and $PSCmdlet.ShouldProcess("$Repository/$Tag", '发布已齐套 draft Release')) {
        Invoke-RetryNative -FilePath 'gh' -Arguments @('release', 'edit', $Tag, '--repo', $Repository, '--draft=false') -Label '发布 draft Release' | Out-Null
        $release = Get-LiveRelease
    }
    if (-not $release -or $release.draft -or $release.prerelease) {
        throw 'REL_REMOTE_VERIFY_FAILED：Release 发布后状态不正确。'
    }
    if (($release.PSObject.Properties.Name -contains 'target_commitish') -and
        ([string]$release.target_commitish).ToLowerInvariant() -ne $ExpectedCommit.ToLowerInvariant()) {
        throw 'REL_RELEASE_TARGET_MISMATCH：正式 Release 回读 target 漂移。'
    }
}
elseif ($release.draft) {
    Write-Host '[plan] 资产齐套并回读后发布 draft Release。'
}

if ($PublishUpdaterManifest) {
    if (-not $release) { throw '必须先确认 Release 存在，才能发布 updater manifest。' }
    if (-not $readOnly -and ($release.draft -or $release.prerelease)) {
        throw 'REL_REMOTE_VERIFY_FAILED：正式 Release 未就绪，禁止发布清单。'
    }
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
    $signatureText = (Get-Content -LiteralPath "$($installers[0].FullName).sig" -Raw -Encoding UTF8).Trim()
    $manifestPlan = Get-CaseBoardManifestPlan -Draft $draft -ExpectedVersion $expectedVersion -Installer $installerRemote[0] -Signature $signatureText
    if ($manifestPlan.action -eq 'fail') { throw "REL_MANIFEST_PAIR_INVALID：$($manifestPlan.reason)" }

    $publishedAt = if ($release.PSObject.Properties.Name -contains 'published_at') { [string]$release.published_at } else { '' }
    if (-not $readOnly -and -not $publishedAt) { throw 'REL_REMOTE_VERIFY_FAILED：Release 缺少 published_at。' }
    $expectedReleaseUrl = "https://github.com/$Repository/releases/tag/$Tag"
    $releaseUrl = if ($readOnly -and $release.draft) {
        $expectedReleaseUrl
    } elseif ($release.PSObject.Properties.Name -contains 'html_url') {
        [string]$release.html_url
    } else {
        $expectedReleaseUrl
    }
    if ($releaseUrl -ne $expectedReleaseUrl) { throw 'REL_MANIFEST_PAIR_INVALID：Release URL 不符合 tag。' }
    $versionDraft = [ordered]@{
        version = $expectedVersion
        released_at = if ($publishedAt) { ([datetime]$publishedAt).ToUniversalTime().ToString('yyyy-MM-dd') } else { '<release-date>' }
        notes = [string]$draft.notes
        download_url = $releaseUrl
    }
    if (-not $readOnly) {
        $pairPlan = Get-CaseBoardManifestPairPlan -Latest $draft -Version ([pscustomobject]$versionDraft) -ExpectedVersion $expectedVersion -Installer $installerRemote[0] -Signature $signatureText -ReleaseUrl $releaseUrl
        if ($pairPlan.action -ne 'publish') { throw "REL_MANIFEST_PAIR_INVALID：$($pairPlan.reason)" }
    }

    $latestPath = Join-Path $root 'release/latest.json'
    $versionPath = Join-Path $root 'release/version.json'
    $manifestFiles = @('release/latest.json', 'release/version.json')
    $localHead = (& git -C $root rev-parse HEAD).Trim().ToLowerInvariant()
    if (-not (Test-GitAncestor -Ancestor $ExpectedMainCommit -Descendant $localHead)) {
        throw '本地 HEAD 不是 ExpectedMainCommit 的快进后代。'
    }
    if ($localHead -ne $ExpectedMainCommit.ToLowerInvariant()) {
        $rangeFiles = @(& git -C $root diff --name-only "$ExpectedMainCommit..$localHead")
        if (@($rangeFiles).Count -ne 2 -or @($rangeFiles | Where-Object { $_ -notin $manifestFiles }).Count -gt 0) {
            throw 'REL_MANIFEST_PAIR_INVALID：ExpectedMainCommit 之后必须恰好只有两份发布清单。'
        }
    }
    $remoteMain = Get-RemoteBranchCommit -Branch 'main'
    $mainPlan = Get-CaseBoardMainPlan -RemoteCommit $remoteMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected $true
    if ($mainPlan.action -eq 'fail') { throw "main 安全门禁失败：$($mainPlan.reason)" }

    $draftJson = $draftText.TrimEnd([char[]]"`r`n") + "`n"
    $versionJson = ($versionDraft | ConvertTo-Json -Depth 5) + "`n"
    $latestJson = Get-Content -LiteralPath $latestPath -Raw -Encoding UTF8
    $currentVersionJson = Get-Content -LiteralPath $versionPath -Raw -Encoding UTF8
    if ($mainPlan.action -eq 'converged') {
        if ($latestJson -ne $draftJson -or $currentVersionJson -ne $versionJson) {
            throw 'REL_MANIFEST_PAIR_INVALID：远端已更新，但本地清单对与事实不一致。'
        }
        Write-Host '[skip] 发布清单对已提交并推送。'
    }
    elseif ($readOnly) {
        Write-Host "[plan] validate, atomically replace, commit and fast-forward push the manifest pair from $ExpectedMainCommit"
    }
    else {
        $manifestStatus = (& git -C $root status --porcelain -- $manifestFiles) -join "`n"
        if ($manifestStatus -and ($latestJson -ne $draftJson -or $currentVersionJson -ne $versionJson)) {
            throw '发布清单已有不一致的未提交修改，拒绝覆盖。'
        }
        $alreadyStaged = @(& git -C $root diff --cached --name-only)
        if ($alreadyStaged.Count -gt 0) { throw 'REL_MANIFEST_PAIR_INVALID：暂存区必须为空。' }
        $tempLatest = Join-Path (Split-Path -Parent $latestPath) ("latest-{0}.tmp" -f [guid]::NewGuid())
        $tempVersion = Join-Path (Split-Path -Parent $versionPath) ("version-{0}.tmp" -f [guid]::NewGuid())
        try {
            [IO.File]::WriteAllText($tempLatest, $draftJson, (New-Object Text.UTF8Encoding($false)))
            [IO.File]::WriteAllText($tempVersion, $versionJson, (New-Object Text.UTF8Encoding($false)))
            Move-Item -LiteralPath $tempLatest -Destination $latestPath -Force
            Move-Item -LiteralPath $tempVersion -Destination $versionPath -Force
        }
        finally {
            Remove-Item -LiteralPath $tempLatest -Force -ErrorAction SilentlyContinue
            Remove-Item -LiteralPath $tempVersion -Force -ErrorAction SilentlyContinue
        }
        & git -C $root add -- $manifestFiles
        $cachedFiles = @(& git -C $root diff --cached --name-only)
        if ($cachedFiles.Count -eq 2 -and @($cachedFiles | Where-Object { $_ -notin $manifestFiles }).Count -eq 0) {
            & git -C $root commit -m "chore: publish $expectedVersion release manifests" -- $manifestFiles
            if ($LASTEXITCODE -ne 0) { throw '提交 updater manifest 失败。' }
        }
        elseif ($cachedFiles.Count -ne 0) { throw 'REL_MANIFEST_PAIR_INVALID：暂存文件集合不等于清单对。' }
        $localHead = (& git -C $root rev-parse HEAD).Trim().ToLowerInvariant()
        $remoteMain = Get-RemoteBranchCommit -Branch 'main'
        $mainPlan = Get-CaseBoardMainPlan -RemoteCommit $remoteMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected (Test-GitAncestor -Ancestor $ExpectedMainCommit -Descendant $localHead)
        if ($mainPlan.action -eq 'fail') { throw "推送前 main 漂移：$($mainPlan.reason)" }
        if ($mainPlan.action -eq 'push') {
            Invoke-CaseBoardBoundedRetry -Label '快进推送发布清单对' -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
                $liveMain = Get-RemoteBranchCommit -Branch 'main'
                $livePlan = Get-CaseBoardMainPlan -RemoteCommit $liveMain -ExpectedCommit $ExpectedMainCommit.ToLowerInvariant() -LocalCommit $localHead -LocalDescendsFromExpected $true
                if ($livePlan.action -eq 'converged') { return }
                if ($livePlan.action -eq 'fail') { throw "推送重试前 main 漂移：$($livePlan.reason)" }
                Invoke-NativeCapture -FilePath 'git' -Arguments @('-C', $root, 'push', '--porcelain', $GitRemote, 'HEAD:refs/heads/main') -Label '快进推送发布清单对' | Out-Null
            }
        }
        $finalRemote = Get-RemoteBranchCommit -Branch 'main'
        if ($finalRemote -ne $localHead) { throw '推送后远端 main 未收敛到本地 manifest 提交。' }
        $rawBase = "https://raw.githubusercontent.com/$Repository/main/release"
        Invoke-CaseBoardBoundedRetry -Label '回读公开清单对' -MaxAttempts $MaxAttempts -BaseDelaySeconds $BaseDelaySeconds -Operation {
            $remoteLatest = Invoke-NativeCapture -FilePath 'curl.exe' -Arguments @('--fail', '--silent', '--show-error', "$rawBase/latest.json") -Label '回读 latest.json'
            $remoteVersion = Invoke-NativeCapture -FilePath 'curl.exe' -Arguments @('--fail', '--silent', '--show-error', "$rawBase/version.json") -Label '回读 version.json'
            if (($remoteLatest.TrimEnd() + "`n") -ne $draftJson -or ($remoteVersion.TrimEnd() + "`n") -ne $versionJson) {
                throw 'REL_REMOTE_VERIFY_FAILED：raw 清单对尚未收敛。'
            }
        } | Out-Null
        Write-Host "[ok] 发布清单对已安全快进推送并回读：$localHead"
    }
}
else {
    Write-Host '[ok] Release 资产状态已收敛；未请求 updater manifest 发布。'
}
