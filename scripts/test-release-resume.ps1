$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Import-Module (Join-Path $PSScriptRoot 'release-resume-core.psm1') -Force

$passed = 0
function Assert-Equal($Actual, $Expected, [string]$Label) {
    if ($Actual -ne $Expected) { throw "$Label：actual=$Actual expected=$Expected" }
    $script:passed++
}
function Assert-Throws([scriptblock]$Operation, [string]$Pattern, [string]$Label) {
    try { & $Operation; throw "$Label：未抛出异常" }
    catch {
        if ($_.Exception.Message -notmatch $Pattern) { throw "$Label：异常不匹配：$($_.Exception.Message)" }
        $script:passed++
    }
}

Assert-Equal (Test-CaseBoardRetryableFailure 'unexpected EOF') $true 'EOF 可重试'
Assert-Equal (Test-CaseBoardRetryableFailure 'TLS handshake timeout') $true 'TLS 超时可重试'
Assert-Equal (Test-CaseBoardRetryableFailure 'HTTP 422 validation failed') $false '确定性校验错误不可重试'
Assert-Equal (ConvertTo-CaseBoardWindowsArgument 'plain') 'plain' '普通参数不加引号'
Assert-Equal (ConvertTo-CaseBoardWindowsArgument 'D:\案件 看板\asset.exe') '"D:\案件 看板\asset.exe"' '带空格路径安全引用'
Assert-Equal (ConvertTo-CaseBoardWindowsArgument '') '""' '空参数安全引用'

$attempts = 0
$sleeps = @()
$result = Invoke-CaseBoardBoundedRetry -Label '模拟 EOF' -MaxAttempts 4 -BaseDelaySeconds 1 -SleepAction {
    param($seconds) $script:sleeps += $seconds
} -Operation {
    $script:attempts++
    if ($script:attempts -lt 3) { throw 'unexpected EOF' }
    'recovered'
}
Assert-Equal $result 'recovered' 'EOF 后恢复'
Assert-Equal $attempts 3 'EOF 重试次数'
Assert-Equal ($sleeps -join ',') '1,2' '指数退避序列'

$remoteAppeared = $false
$uploadCalls = 0
$converged = Invoke-CaseBoardBoundedRetry -Label '模拟上传 EOF 后远端已存在' -MaxAttempts 3 -BaseDelaySeconds 0 -SleepAction { } -Operation {
    if ($script:remoteAppeared) { return 'verified-existing' }
    $script:uploadCalls++
    $script:remoteAppeared = $true
    throw 'upload unexpected EOF'
}
Assert-Equal $converged 'verified-existing' 'EOF 后先查远端并收敛'
Assert-Equal $uploadCalls 1 '远端已存在时不重复上传'

$local = @([pscustomobject]@{ name='app.exe'; size=10; sha256='abc'; path='x' })
$matching = @([pscustomobject]@{ name='app.exe'; size=10; digest='sha256:abc'; id=1 })
$plan = @(Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets $matching)
Assert-Equal $plan[0].action 'verify' '正确的已存在资产跳过上传'
Assert-Equal $plan[0].reason 'verified_by_api_digest' 'API digest 直接校验'

$legacy = @([pscustomobject]@{ name='app.exe'; size=10; digest=$null; id=1 })
$plan = @(Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets $legacy)
Assert-Equal $plan[0].reason 'download_required' '无 digest 时要求回下载'

$missing = @(Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets @())
Assert-Equal $missing[0].action 'upload' '缺失资产计划上传'

$wrongSize = @([pscustomobject]@{ name='app.exe'; size=11; digest='sha256:abc'; id=1 })
$plan = @(Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets $wrongSize)
Assert-Equal $plan[0].action 'fail' '同名不同大小 fail closed'

$wrongHash = @([pscustomobject]@{ name='app.exe'; size=10; digest='sha256:def'; id=1 })
$plan = @(Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets $wrongHash)
Assert-Equal $plan[0].action 'fail' '同名不同哈希 fail closed'

Assert-Throws {
    Get-CaseBoardAssetPlan -LocalAssets $local -RemoteAssets @($matching[0], $matching[0]) | Out-Null
} '重名资产' '远端重名 fail closed'

Assert-Throws {
    Get-CaseBoardAssetPlan -LocalAssets @($local[0], $local[0]) -RemoteAssets @() | Out-Null
} '本地存在重名资产' '本地重名 fail closed'

$installer = [pscustomobject]@{ browser_download_url='https://example.invalid/app.exe' }
$signature = 'trusted-signature'
$validDraft = [pscustomobject]@{
    version='0.6.3'
    platforms=[pscustomobject]@{
        'windows-x86_64'=[pscustomobject]@{ url='https://example.invalid/app.exe'; signature=$signature }
    }
}
$manifest = Get-CaseBoardManifestPlan -Draft $validDraft -ExpectedVersion '0.6.3' -Installer $installer -Signature $signature
Assert-Equal $manifest.action 'publish' '正确 manifest 可发布'
$wrongVersion = $validDraft.PSObject.Copy(); $wrongVersion.version = '0.6.2'
Assert-Equal (Get-CaseBoardManifestPlan -Draft $wrongVersion -ExpectedVersion '0.6.3' -Installer $installer -Signature $signature).reason 'version_mismatch' 'manifest 版本漂移'
$wrongUrl = $validDraft.PSObject.Copy(); $wrongUrl.platforms = [pscustomobject]@{ 'windows-x86_64'=[pscustomobject]@{ url='https://example.invalid/wrong.exe'; signature=$signature } }
Assert-Equal (Get-CaseBoardManifestPlan -Draft $wrongUrl -ExpectedVersion '0.6.3' -Installer $installer -Signature $signature).reason 'url_mismatch' 'manifest URL 漂移'
Assert-Equal (Get-CaseBoardManifestPlan -Draft $validDraft -ExpectedVersion '0.6.3' -Installer $installer -Signature 'different').reason 'signature_mismatch' 'manifest 签名漂移'

$main = Get-CaseBoardMainPlan -RemoteCommit ('a' * 40) -ExpectedCommit ('a' * 40) -LocalCommit ('b' * 40) -LocalDescendsFromExpected $true
Assert-Equal $main.action 'push' 'main 正确快进计划'
$main = Get-CaseBoardMainPlan -RemoteCommit ('c' * 40) -ExpectedCommit ('a' * 40) -LocalCommit ('b' * 40) -LocalDescendsFromExpected $true
Assert-Equal $main.reason 'remote_main_drift' 'main 漂移 fail closed'
$main = Get-CaseBoardMainPlan -RemoteCommit ('b' * 40) -ExpectedCommit ('a' * 40) -LocalCommit ('b' * 40) -LocalDescendsFromExpected $true
Assert-Equal $main.action 'converged' '推送超时后远端已收敛'

$timeoutAttempts = 0
Assert-Throws {
    Invoke-CaseBoardBoundedRetry -Label '模拟超时耗尽' -MaxAttempts 3 -BaseDelaySeconds 0 -SleepAction { } -Operation {
        $script:timeoutAttempts++
        throw 'request timed out'
    }
} 'timed out' '有限重试耗尽'
Assert-Equal $timeoutAttempts 3 '超时最大尝试次数'

Write-Host "release resume tests passed: $passed"
