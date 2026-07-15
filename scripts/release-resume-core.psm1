Set-StrictMode -Version Latest

function Get-CaseBoardFileSha256 {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$LiteralPath)

    (Get-FileHash -LiteralPath $LiteralPath -Algorithm SHA256).Hash.ToLowerInvariant()
}

function Test-CaseBoardRetryableFailure {
    [CmdletBinding()]
    param([Parameter(Mandatory)][string]$Message)

    $Message -match '(?i)(EOF|TLS|handshake|timed?\s*out|timeout|connection\s+(?:reset|closed|refused)|temporar(?:y|ily)|HTTP\s+(?:408|429|5\d\d)|stream\s+error|broken\s+pipe)'
}

function ConvertTo-CaseBoardWindowsArgument {
    [CmdletBinding()]
    param([Parameter(Mandatory)][AllowEmptyString()][string]$Value)

    if ($Value -and $Value -notmatch '[\s"]') { return $Value }
    $builder = New-Object Text.StringBuilder
    [void]$builder.Append('"')
    $slashes = 0
    foreach ($character in $Value.ToCharArray()) {
        if ($character -eq '\') { $slashes++; continue }
        if ($character -eq '"') {
            [void]$builder.Append(('\' * ($slashes * 2 + 1)))
            [void]$builder.Append('"')
            $slashes = 0
            continue
        }
        if ($slashes) { [void]$builder.Append(('\' * $slashes)); $slashes = 0 }
        [void]$builder.Append($character)
    }
    if ($slashes) { [void]$builder.Append(('\' * ($slashes * 2))) }
    [void]$builder.Append('"')
    $builder.ToString()
}

function Invoke-CaseBoardBoundedRetry {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][scriptblock]$Operation,
        [Parameter(Mandatory)][string]$Label,
        [ValidateRange(1, 10)][int]$MaxAttempts = 5,
        [ValidateRange(0, 300)][int]$BaseDelaySeconds = 2,
        [scriptblock]$SleepAction = { param($Seconds) Start-Sleep -Seconds $Seconds }
    )

    for ($attempt = 1; $attempt -le $MaxAttempts; $attempt++) {
        try {
            return & $Operation $attempt
        }
        catch {
            $message = $_.Exception.Message
            if ($attempt -ge $MaxAttempts -or -not (Test-CaseBoardRetryableFailure -Message $message)) {
                throw
            }
            $delay = [Math]::Min(60, $BaseDelaySeconds * [Math]::Pow(2, $attempt - 1))
            Write-Warning ("{0} 第 {1}/{2} 次失败，将在 {3} 秒后重试（瞬时网络错误）。" -f $Label, $attempt, $MaxAttempts, $delay)
            & $SleepAction ([int]$delay)
        }
    }
}

function Get-CaseBoardAssetPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][ValidateNotNullOrEmpty()][object[]]$LocalAssets,
        [Parameter(Mandatory)][AllowEmptyCollection()][object[]]$RemoteAssets
    )

    $localNames = @{}
    foreach ($local in $LocalAssets) {
        $localName = [string]$local.name
        if ($localNames.ContainsKey($localName)) { throw "本地存在重名资产，无法安全收敛：$localName" }
        $localNames[$localName] = $true
    }

    $remoteByName = @{}
    foreach ($remote in $RemoteAssets) {
        if ($remoteByName.ContainsKey([string]$remote.name)) {
            throw "远端存在重名资产，无法安全收敛：$($remote.name)"
        }
        $remoteByName[[string]$remote.name] = $remote
    }

    $plan = foreach ($local in $LocalAssets) {
        $name = [string]$local.name
        if (-not $remoteByName.ContainsKey($name)) {
            [pscustomobject]@{ name = $name; action = 'upload'; reason = 'missing'; local = $local; remote = $null }
            continue
        }

        $remote = $remoteByName[$name]
        if ([int64]$remote.size -ne [int64]$local.size) {
            [pscustomobject]@{ name = $name; action = 'fail'; reason = 'size_mismatch'; local = $local; remote = $remote }
            continue
        }

        $remoteDigest = if ($remote.PSObject.Properties.Name -contains 'digest') { [string]$remote.digest } else { '' }
        if ($remoteDigest -and $remoteDigest -ne "sha256:$($local.sha256)") {
            [pscustomobject]@{ name = $name; action = 'fail'; reason = 'sha256_mismatch'; local = $local; remote = $remote }
            continue
        }

        $reason = if ($remoteDigest) { 'verified_by_api_digest' } else { 'download_required' }
        [pscustomobject]@{ name = $name; action = 'verify'; reason = $reason; local = $local; remote = $remote }
    }

    @($plan)
}

function Get-CaseBoardManifestPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)]$Draft,
        [Parameter(Mandatory)][string]$ExpectedVersion,
        [Parameter(Mandatory)]$Installer,
        [Parameter(Mandatory)][string]$Signature
    )

    if ([string]$Draft.version -ne $ExpectedVersion) {
        return [pscustomobject]@{ action = 'fail'; reason = 'version_mismatch' }
    }
    if ($Draft.PSObject.Properties.Name -notcontains 'platforms') {
        return [pscustomobject]@{ action = 'fail'; reason = 'platform_missing' }
    }
    $platform = $Draft.platforms.'windows-x86_64'
    if (-not $platform) { return [pscustomobject]@{ action = 'fail'; reason = 'platform_missing' } }
    if ($platform.PSObject.Properties.Name -notcontains 'url' -or $platform.PSObject.Properties.Name -notcontains 'signature') {
        return [pscustomobject]@{ action = 'fail'; reason = 'platform_fields_missing' }
    }
    if ([string]$platform.url -ne [string]$Installer.browser_download_url) {
        return [pscustomobject]@{ action = 'fail'; reason = 'url_mismatch' }
    }
    if ([string]$platform.signature.Trim() -ne $Signature.Trim()) {
        return [pscustomobject]@{ action = 'fail'; reason = 'signature_mismatch' }
    }
    [pscustomobject]@{ action = 'publish'; reason = 'validated'; json = $Draft }
}

function Get-CaseBoardMainPlan {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$RemoteCommit,
        [Parameter(Mandatory)][string]$ExpectedCommit,
        [Parameter(Mandatory)][string]$LocalCommit,
        [Parameter(Mandatory)][bool]$LocalDescendsFromExpected
    )

    if (-not $LocalDescendsFromExpected) {
        return [pscustomobject]@{ action = 'fail'; reason = 'local_not_fast_forward' }
    }
    if ($RemoteCommit -eq $LocalCommit -and $LocalCommit -ne $ExpectedCommit) {
        return [pscustomobject]@{ action = 'converged'; reason = 'remote_already_updated' }
    }
    if ($RemoteCommit -ne $ExpectedCommit) {
        return [pscustomobject]@{ action = 'fail'; reason = 'remote_main_drift' }
    }
    [pscustomobject]@{ action = 'push'; reason = 'fast_forward_allowed' }
}

Export-ModuleMember -Function @(
    'Get-CaseBoardFileSha256',
    'Test-CaseBoardRetryableFailure',
    'ConvertTo-CaseBoardWindowsArgument',
    'Invoke-CaseBoardBoundedRetry',
    'Get-CaseBoardAssetPlan',
    'Get-CaseBoardManifestPlan',
    'Get-CaseBoardMainPlan'
)
