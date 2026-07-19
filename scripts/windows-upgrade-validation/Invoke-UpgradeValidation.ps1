[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)][string]$SourceDatabase,
  [Parameter(Mandatory = $true)][string]$OutputDirectory,
  [string]$PythonPath,
  [string]$AsciiTempRoot,
  [string]$AppExecutable,
  [string]$InstallerPath,
  [string]$ExpectedInstallerSha256,
  [switch]$RunIsolatedUpgrade,
  [switch]$Install,
  [string]$FormalDatabase,
  [string]$InstalledExecutable,
  [string]$ExpectedInstalledVersion,
  [string]$UninstallRegistryPath,
  [string]$ConfirmFormalInstall,
  [int]$StartupSeconds = 20,
  [int]$MinimumScreenshotBytes = 20000
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$env:PYTHONDONTWRITEBYTECODE = '1'
$scriptRoot = $PSScriptRoot
$audit = Join-Path $scriptRoot 'db_audit.py'
$capture = Join-Path $scriptRoot 'capture-window.ps1'
$formalDefault = [IO.Path]::GetFullPath((Join-Path $env:APPDATA 'FanglvCaseBoard\data\caseboard.db'))

function Resolve-Absolute([string]$Value, [bool]$MustExist) {
  if ($Value -notmatch '^(?:[A-Za-z]:\\|\\\\)') { throw "Path must be absolute: $Value" }
  $full = [IO.Path]::GetFullPath($Value)
  if ($MustExist -and -not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "File not found: $full" }
  return $full
}

function Resolve-Python([string]$ExplicitPath) {
  $candidates = @()
  if ($ExplicitPath) { $candidates += $ExplicitPath }
  if ($env:CASEBOARD_VALIDATION_PYTHON) { $candidates += $env:CASEBOARD_VALIDATION_PYTHON }
  $repoPython = Join-Path (Split-Path (Split-Path $scriptRoot -Parent) -Parent) '.venv\Scripts\python.exe'
  $codexPython = Join-Path $env:USERPROFILE '.cache\codex-runtimes\codex-primary-runtime\dependencies\python\python.exe'
  $candidates += @($repoPython, $codexPython)
  $pathPython = Get-Command python -ErrorAction SilentlyContinue
  if ($pathPython) { $candidates += $pathPython.Source }
  foreach ($candidate in $candidates) {
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) { return [IO.Path]::GetFullPath($candidate) }
  }
  throw 'Python was not found; provide -PythonPath or CASEBOARD_VALIDATION_PYTHON'
}

function Invoke-Audit([string[]]$Arguments) {
  & $python $audit @Arguments
  if ($LASTEXITCODE -ne 0) { throw "db_audit.py failed with exit code $LASTEXITCODE" }
}

function Stop-IsolatedProcesses([string]$WebViewPath) {
  $matches = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
    $_.CommandLine -and $_.CommandLine.IndexOf($WebViewPath, [StringComparison]::OrdinalIgnoreCase) -ge 0
  })
  foreach ($item in $matches) { Stop-Process -Id $item.ProcessId -Force -ErrorAction SilentlyContinue }
  $remaining = @()
  for ($attempt = 0; $attempt -lt 20; $attempt++) {
    Start-Sleep -Milliseconds 250
    $remaining = @(Get-CimInstance Win32_Process -ErrorAction SilentlyContinue | Where-Object {
      $_.CommandLine -and $_.CommandLine.IndexOf($WebViewPath, [StringComparison]::OrdinalIgnoreCase) -ge 0
    })
    if ($remaining.Count -eq 0) { break }
    foreach ($item in $remaining) { Stop-Process -Id $item.ProcessId -Force -ErrorAction SilentlyContinue }
  }
  if ($remaining.Count -ne 0) { throw "Failed to clean isolated WebView2 processes: $($remaining.ProcessId -join ',')" }
}

$source = Resolve-Absolute $SourceDatabase $true
$output = Resolve-Absolute $OutputDirectory $false
$python = Resolve-Python $PythonPath
if ($source -eq $output -or $output.StartsWith($source + [IO.Path]::DirectorySeparatorChar)) {
  throw 'OutputDirectory must not be the database path or a child of it'
}
$isFormalSource = $source.Equals($formalDefault, [StringComparison]::OrdinalIgnoreCase)
if ($isFormalSource -and -not $Install) {
  throw 'Formal database is forbidden in dry-run and isolated-upgrade modes'
}
if ($Install) {
  if (-not $RunIsolatedUpgrade) { throw '-Install requires a successful isolated upgrade in the same invocation' }
  if ($ConfirmFormalInstall -ne 'INSTALL-AND-MIGRATE-FORMAL-DATABASE') { throw 'Formal install confirmation phrase is missing' }
  if (-not $FormalDatabase) { throw '-FormalDatabase is required with -Install' }
  $formal = Resolve-Absolute $FormalDatabase $true
  if (-not $formal.Equals($formalDefault, [StringComparison]::OrdinalIgnoreCase)) { throw 'FormalDatabase must equal the canonical application database path' }
  if (-not $source.Equals($formal, [StringComparison]::OrdinalIgnoreCase)) { throw 'Install mode SourceDatabase must equal FormalDatabase' }
  if (-not $InstallerPath -or -not $ExpectedInstallerSha256 -or -not $InstalledExecutable -or -not $ExpectedInstalledVersion -or -not $UninstallRegistryPath) { throw 'InstallerPath, ExpectedInstallerSha256, InstalledExecutable, ExpectedInstalledVersion and UninstallRegistryPath are required with -Install' }
}
if ($RunIsolatedUpgrade -and -not $AppExecutable) { throw '-AppExecutable is required with -RunIsolatedUpgrade' }
if ($RunIsolatedUpgrade) {
  if (-not $AsciiTempRoot) { throw '-AsciiTempRoot is required with -RunIsolatedUpgrade' }
  $asciiTempBase = Resolve-Absolute $AsciiTempRoot $false
  if ($asciiTempBase -notmatch '^[\x20-\x7E]+$') { throw 'AsciiTempRoot must contain ASCII characters only' }
}
if ($InstallerPath) {
  $installer = Resolve-Absolute $InstallerPath $true
  $actualHash = (Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant()
  if (-not $ExpectedInstallerSha256 -or $actualHash -ne $ExpectedInstallerSha256.ToLowerInvariant()) {
    throw 'Installer SHA-256 is missing or does not match'
  }
}

$runId = (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ')
$runRoot = Join-Path $output $runId
if (Test-Path -LiteralPath $runRoot) { throw "Refusing to reuse run directory: $runRoot" }
$dataDir = Join-Path $runRoot 'isolated-data'
$webviewDir = Join-Path $runRoot 'webview-data'
$captureTempDir = if ($RunIsolatedUpgrade) { Join-Path $asciiTempBase $runId } else { $null }
$evidenceDir = Join-Path $runRoot 'evidence'
[IO.Directory]::CreateDirectory($dataDir) | Out-Null
[IO.Directory]::CreateDirectory($webviewDir) | Out-Null
[IO.Directory]::CreateDirectory($evidenceDir) | Out-Null

$summary = [ordered]@{
  status = 'preflight-passed'
  mode = if ($Install) { 'formal-install' } elseif ($RunIsolatedUpgrade) { 'isolated-upgrade' } else { 'dry-run' }
  source_database = $source
  source_is_formal = $isFormalSource
  run_root = $runRoot
  isolated_data_dir = $dataDir
  webview2_user_data_folder = $webviewDir
  python_path = $python
  capture_temp_directory = $captureTempDir
  install_requested = [bool]$Install
}

try {
  $beforeJson = Join-Path $evidenceDir '01-source-before.json'
  Invoke-Audit @('snapshot', '--db', $source, '--output', $beforeJson)
  if ($RunIsolatedUpgrade) {
    $isolatedDb = Join-Path $dataDir 'caseboard.db'
    $backupJson = Join-Path $evidenceDir '02-isolated-backup.json'
    Invoke-Audit @('backup', '--source', $source, '--destination', $isolatedDb, '--output', $backupJson)
    $app = Resolve-Absolute $AppExecutable $true
    $oldData, $oldWebview = $env:CASEBOARD_DATA_DIR, $env:WEBVIEW2_USER_DATA_FOLDER
    $env:CASEBOARD_DATA_DIR, $env:WEBVIEW2_USER_DATA_FOLDER = $dataDir, $webviewDir
    $process = $null
    try {
      $process = Start-Process -FilePath $app -PassThru
      Start-Sleep -Seconds $StartupSeconds
      if ($process.HasExited) { throw "Isolated application exited early: $($process.ExitCode)" }
      $isolatedScreenshot = Join-Path $evidenceDir '03-window.png'
      & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $capture -ProcessId $process.Id -Output $isolatedScreenshot -AsciiTempRoot $captureTempDir | Out-File (Join-Path $evidenceDir '03-window.json') -Encoding utf8
      if ((Get-Item -LiteralPath $isolatedScreenshot).Length -lt $MinimumScreenshotBytes) { throw 'Screenshot is too small to prove a rendered UI' }
    } finally {
      if ($process -and -not $process.HasExited) { Stop-Process -Id $process.Id -Force }
      Stop-IsolatedProcesses $webviewDir
      $env:CASEBOARD_DATA_DIR, $env:WEBVIEW2_USER_DATA_FOLDER = $oldData, $oldWebview
    }
    $afterJson = Join-Path $evidenceDir '04-isolated-after.json'
    $compareJson = Join-Path $evidenceDir '05-compare.json'
    Invoke-Audit @('snapshot', '--db', $isolatedDb, '--output', $afterJson)
    Invoke-Audit @('compare', '--before', $beforeJson, '--after', $afterJson, '--output', $compareJson)
    $comparison = Get-Content -LiteralPath $compareJson -Raw -Encoding utf8 | ConvertFrom-Json
    $summary.status = 'isolated-upgrade-passed'
    $summary['business_table_changes'] = $comparison.business_table_changes
    $summary['runtime_snapshot_changes'] = $comparison.runtime_snapshot_changes
    $summary['new_tables'] = $comparison.new_tables
    $summary['wal_state'] = $comparison.wal_state_after
  }
  if ($Install) {
    $backupPath = Join-Path $runRoot 'formal-backup\caseboard.db'
    Invoke-Audit @('backup', '--source', $source, '--destination', $backupPath, '--output', (Join-Path $evidenceDir '10-formal-backup.json'))
    $runningCaseboard = @(Get-Process caseboard -ErrorAction SilentlyContinue | Where-Object { -not $_.HasExited })
    if ($runningCaseboard.Count -ne 0) { throw 'caseboard process is running; refusing formal installation' }
    $installProcess = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru
    if ($installProcess.ExitCode -ne 0) { throw "Installer failed: $($installProcess.ExitCode)" }
    $installed = Resolve-Absolute $InstalledExecutable $true
    $installedVersion = (Get-Item -LiteralPath $installed).VersionInfo.ProductVersion
    if ($installedVersion -ne $ExpectedInstalledVersion) { throw "Installed executable version mismatch: $installedVersion" }
    if (-not (Test-Path -LiteralPath $UninstallRegistryPath)) { throw "Uninstall registry path not found: $UninstallRegistryPath" }
    $registryVersion = (Get-ItemProperty -LiteralPath $UninstallRegistryPath -Name DisplayVersion -ErrorAction Stop).DisplayVersion
    if ($registryVersion -ne $ExpectedInstalledVersion) { throw "Registry DisplayVersion mismatch: $registryVersion" }
    $formalWebviewDir = Join-Path $runRoot 'formal-webview-data'
    [IO.Directory]::CreateDirectory($formalWebviewDir) | Out-Null
    $oldFormalWebview = $env:WEBVIEW2_USER_DATA_FOLDER
    $env:WEBVIEW2_USER_DATA_FOLDER = $formalWebviewDir
    $formalProcess = $null
    try {
      $formalProcess = Start-Process -FilePath $installed -PassThru
      Start-Sleep -Seconds $StartupSeconds
      if ($formalProcess.HasExited) { throw 'Installed application exited early' }
      $formalScreenshot = Join-Path $evidenceDir '11-formal-window.png'
      & powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $capture -ProcessId $formalProcess.Id -Output $formalScreenshot -AsciiTempRoot $captureTempDir | Out-File (Join-Path $evidenceDir '11-formal-window.json') -Encoding utf8
      if ((Get-Item -LiteralPath $formalScreenshot).Length -lt $MinimumScreenshotBytes) { throw 'Formal screenshot is too small to prove a rendered UI' }
    } finally {
      if ($formalProcess -and -not $formalProcess.HasExited) { Stop-Process -Id $formalProcess.Id -Force }
      Stop-IsolatedProcesses $formalWebviewDir
      $env:WEBVIEW2_USER_DATA_FOLDER = $oldFormalWebview
    }
    Invoke-Audit @('snapshot', '--db', $source, '--output', (Join-Path $evidenceDir '12-formal-after.json'))
    Invoke-Audit @('compare', '--before', $beforeJson, '--after', (Join-Path $evidenceDir '12-formal-after.json'), '--output', (Join-Path $evidenceDir '13-formal-compare.json'))
    $summary.status = 'formal-install-passed'
    $summary['installed_executable_version'] = $installedVersion
    $summary['registry_display_version'] = $registryVersion
    $summary['formal_webview2_user_data_folder'] = $formalWebviewDir
  }
} catch {
  $summary.status = 'failed'
  $summary['error'] = $_.Exception.Message
  throw
} finally {
  $summary | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $evidenceDir 'summary.json') -Encoding utf8
  $businessChanges = if ($summary.Contains('business_table_changes')) { $summary.business_table_changes | ConvertTo-Json -Compress } else { 'comparison-not-run' }
  $runtimeChanges = if ($summary.Contains('runtime_snapshot_changes')) { $summary.runtime_snapshot_changes | ConvertTo-Json -Compress } else { 'comparison-not-run' }
  $walState = if ($summary.Contains('wal_state')) { $summary.wal_state | ConvertTo-Json -Compress } else { 'comparison-not-run' }
  $markdown = @("# Windows upgrade validation summary", "", "- Status: $($summary.status)", "- Mode: $($summary.mode)", "- Formal database input: $($summary.source_is_formal)", "- Install requested: $($summary.install_requested)", "- Business table changes: $businessChanges", "- Runtime snapshot changes: $runtimeChanges", "- WAL state: $walState", "- Isolated data directory: $dataDir", "- WebView2 directory: $webviewDir")
  $markdown | Set-Content -LiteralPath (Join-Path $runRoot 'summary.md') -Encoding utf8
}

$summary | ConvertTo-Json -Depth 6
