[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)]
  [ValidateSet('Backup','AuditCopy','Compare','RecordExternalRunDbPostcheck','FormalSwitch','Install')]
  [string]$Stage,
  [string]$SourceDatabase,
  [string]$OutputDirectory,
  [string]$RunId,
  [string]$PythonPath,
  [string]$MigrationsDirectory,
  [string]$ResumeManifest,
  [string]$ExpectedResumeManifestSha256,
  [string]$BeforeSnapshot,
  [string]$AfterSnapshot,
  [switch]$Idempotent,
  [string]$ProofDatabase,
  [ValidateSet('graceful','forced')]
  [string]$ExitMode,
  [switch]$IdempotentPostcheck,
  [switch]$AllowFormalMutation,
  [string]$ConfirmFormalMutation
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
Add-Type -AssemblyName System.Security
$env:PYTHONDONTWRITEBYTECODE = '1'
$scriptRoot = $PSScriptRoot
$audit = Join-Path $scriptRoot 'db_audit.py'

function Resolve-Absolute([string]$Value, [bool]$MustExist, [bool]$MustBeFile) {
  if (-not $Value) { throw 'Required path is missing' }
  if ($Value -notmatch '^(?:[A-Za-z]:\\|\\\\)') { throw "Path must be absolute: $Value" }
  $full = [IO.Path]::GetFullPath($Value)
  if ($MustExist) {
    $kind = if ($MustBeFile) { 'Leaf' } else { 'Container' }
    if (-not (Test-Path -LiteralPath $full -PathType $kind)) { throw "Path not found: $full" }
  }
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
    if ($candidate -and (Test-Path -LiteralPath $candidate -PathType Leaf)) {
      return [IO.Path]::GetFullPath($candidate)
    }
  }
  throw 'Python was not found; provide -PythonPath or CASEBOARD_VALIDATION_PYTHON'
}

function Invoke-Audit([string[]]$Arguments) {
  $auditOutput = @(& $python $audit @Arguments)
  if ($LASTEXITCODE -ne 0) { throw "db_audit.py failed with exit code $LASTEXITCODE" }
}

function Assert-OutputSeparated([string]$SourceFile, [string]$OutputRoot) {
  $sourceParent = [IO.Path]::GetFullPath((Split-Path $SourceFile -Parent)).TrimEnd('\')
  $outputFull = [IO.Path]::GetFullPath($OutputRoot).TrimEnd('\')
  if ($outputFull.Equals($sourceParent, [StringComparison]::OrdinalIgnoreCase) -or
      $outputFull.StartsWith($sourceParent + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw 'PATH_ESCAPE_OR_SOURCE_CHILD: output must be outside the source database directory'
  }
}

function Assert-NoCaseboardProcess {
  if ($env:CASEBOARD_TEST_FORCE_PROCESS_ENUMERATION_FAILURE -eq '1') {
    throw 'PROCESS_ENUMERATION_FAILED: synthetic test hook'
  }
  try {
    $allProcesses = @(Get-CimInstance Win32_Process -ErrorAction Stop)
  } catch {
    throw "PROCESS_ENUMERATION_FAILED: $($_.Exception.Message)"
  }
  $running = @($allProcesses | Where-Object { $_.Name -ieq 'caseboard.exe' })
  if ($running.Count -ne 0) { throw 'CASEBOARD_PROCESS_RUNNING' }
}

function Write-NewJson([string]$Path, [object]$Value) {
  if (Test-Path -LiteralPath $Path) { throw "Refusing to overwrite evidence: $Path" }
  $Value | ConvertTo-Json -Depth 20 | Set-Content -LiteralPath $Path -Encoding utf8
}

function Assert-PathUnderRunRoot([string]$Path, [string]$RunRoot) {
  $full = [IO.Path]::GetFullPath($Path)
  $root = [IO.Path]::GetFullPath($RunRoot).TrimEnd('\')
  if (-not $full.StartsWith($root + '\', [StringComparison]::OrdinalIgnoreCase)) {
    throw "ARTIFACT_OUTSIDE_RUN_ROOT: $full"
  }
  return $full
}

function New-RunAnchor([string]$RunRoot) {
  $anchor = Join-Path $RunRoot '.resume-anchor.bin'
  if (Test-Path -LiteralPath $anchor) { throw "Refusing to overwrite run anchor: $anchor" }
  $key = New-Object byte[] 32
  $rng = [Security.Cryptography.RandomNumberGenerator]::Create()
  try {
    $rng.GetBytes($key)
    $protected = [Security.Cryptography.ProtectedData]::Protect(
      $key,
      $null,
      [Security.Cryptography.DataProtectionScope]::CurrentUser
    )
    [IO.File]::WriteAllBytes($anchor, $protected)
  } finally {
    $rng.Dispose()
    [Array]::Clear($key, 0, $key.Length)
  }
}

function Get-RunKey([string]$RunRoot) {
  $anchor = Join-Path $RunRoot '.resume-anchor.bin'
  if (-not (Test-Path -LiteralPath $anchor -PathType Leaf)) { throw 'MANIFEST_ANCHOR_MISSING' }
  try {
    return [Security.Cryptography.ProtectedData]::Unprotect(
      [IO.File]::ReadAllBytes($anchor),
      $null,
      [Security.Cryptography.DataProtectionScope]::CurrentUser
    )
  } catch {
    throw "MANIFEST_ANCHOR_INVALID: $($_.Exception.Message)"
  }
}

function Get-HmacSha256([string]$Path, [byte[]]$Key) {
  $hmac = New-Object Security.Cryptography.HMACSHA256(,$Key)
  try {
    $bytes = [IO.File]::ReadAllBytes($Path)
    return ([BitConverter]::ToString($hmac.ComputeHash($bytes))).Replace('-', '')
  } finally {
    $hmac.Dispose()
  }
}

function New-Artifact([string]$Name, [string]$Path, [string]$RunRoot) {
  $full = Resolve-Absolute $Path $true $true
  $full = Assert-PathUnderRunRoot $full $RunRoot
  return [ordered]@{
    name = $Name
    path = $full
    sha256 = (Get-FileHash -LiteralPath $full -Algorithm SHA256).Hash.ToUpperInvariant()
  }
}

function Write-Manifest([string]$Path, [hashtable]$Value, [string]$RunRoot) {
  Write-NewJson $Path $Value
  $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToUpperInvariant()
  $hashPath = "$Path.sha256"
  $hmacPath = "$Path.hmac"
  if ((Test-Path -LiteralPath $hashPath) -or (Test-Path -LiteralPath $hmacPath)) {
    throw 'Refusing to overwrite manifest integrity evidence'
  }
  Set-Content -LiteralPath $hashPath -Value $hash -Encoding ascii
  $key = Get-RunKey $RunRoot
  try {
    Set-Content -LiteralPath $hmacPath -Value (Get-HmacSha256 $Path $key) -Encoding ascii
  } finally {
    [Array]::Clear($key, 0, $key.Length)
  }
  return $hash
}

function Get-ExpectedManifestName([string]$ExpectedStage, [string]$ExpectedStatus) {
  $key = "$ExpectedStage|$ExpectedStatus"
  switch ($key) {
    'Backup|backup-passed' { return 'manifest.backup.json' }
    'AuditCopy|audit-passed' { return 'manifest.audit.json' }
    'RecordExternalRunDbPostcheck|isolated-db-postcheck-recorded' { return 'manifest.isolated-db-postcheck.json' }
    'RecordExternalRunDbPostcheck|idempotent-db-postcheck-recorded' { return 'manifest.idempotent-db-postcheck.json' }
    default { throw "UNSUPPORTED_RESUME_STAGE_STATUS: $key" }
  }
}

function Get-Artifact([object]$Manifest, [string]$Name) {
  $matches = @($Manifest.artifacts | Where-Object { [string]$_.name -ceq $Name })
  if ($matches.Count -ne 1) { throw "MANIFEST_ARTIFACT_MISSING_OR_DUPLICATE: $Name" }
  return $matches[0]
}

function Assert-ManifestFile(
  [string]$Path,
  [string]$ExpectedStage,
  [string]$ExpectedStatus,
  [string]$ExpectedRunRoot
) {
  $manifestPath = Resolve-Absolute $Path $true $true
  $runRoot = [IO.Path]::GetFullPath((Split-Path $manifestPath -Parent))
  if ($ExpectedRunRoot -and -not $runRoot.Equals([IO.Path]::GetFullPath($ExpectedRunRoot), [StringComparison]::OrdinalIgnoreCase)) {
    throw 'RESUME_RUN_ROOT_MISMATCH'
  }
  $expectedName = Get-ExpectedManifestName $ExpectedStage $ExpectedStatus
  if (-not (Split-Path $manifestPath -Leaf).Equals($expectedName, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'RESUME_MANIFEST_NAME_MISMATCH'
  }
  $hmacPath = "$manifestPath.hmac"
  if (-not (Test-Path -LiteralPath $hmacPath -PathType Leaf)) { throw 'MANIFEST_HMAC_MISSING' }
  $key = Get-RunKey $runRoot
  try {
    $expectedHmac = (Get-Content -LiteralPath $hmacPath -Raw -Encoding ascii).Trim().ToUpperInvariant()
    $actualHmac = Get-HmacSha256 $manifestPath $key
    if ($expectedHmac -notmatch '^[0-9A-F]{64}$' -or $actualHmac -cne $expectedHmac) {
      throw 'MANIFEST_HMAC_MISMATCH'
    }
  } finally {
    [Array]::Clear($key, 0, $key.Length)
  }
  $value = Get-Content -LiteralPath $manifestPath -Raw -Encoding utf8 | ConvertFrom-Json
  if ([int]$value.schema_version -ne 2) { throw 'RESUME_SCHEMA_VERSION_MISMATCH' }
  if ([string]$value.stage -cne $ExpectedStage -or [string]$value.status -cne $ExpectedStatus) {
    throw 'RESUME_STAGE_STATUS_MISMATCH'
  }
  if (-not ([IO.Path]::GetFullPath([string]$value.run_root)).Equals($runRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw 'RESUME_RUN_ROOT_MISMATCH'
  }
  foreach ($artifact in @($value.artifacts)) {
    $artifactPath = Resolve-Absolute ([string]$artifact.path) $true $true
    $artifactPath = Assert-PathUnderRunRoot $artifactPath $runRoot
    $actualArtifactHash = (Get-FileHash -LiteralPath $artifactPath -Algorithm SHA256).Hash.ToUpperInvariant()
    if ([string]$artifact.sha256 -notmatch '^[0-9A-Fa-f]{64}$' -or
        $actualArtifactHash -cne ([string]$artifact.sha256).ToUpperInvariant()) {
      throw "ARTIFACT_HASH_MISMATCH: $($artifact.name)"
    }
  }

  if ($ExpectedStage -eq 'Backup') {
    if ($value.PSObject.Properties.Name -contains 'parent_manifest') { throw 'UNEXPECTED_PARENT_MANIFEST' }
  } else {
    $parentStage = if ($ExpectedStatus -eq 'audit-passed') { 'Backup' } else { if ($ExpectedStatus -eq 'isolated-db-postcheck-recorded') { 'AuditCopy' } else { 'RecordExternalRunDbPostcheck' } }
    $parentStatus = if ($ExpectedStatus -eq 'audit-passed') { 'backup-passed' } else { if ($ExpectedStatus -eq 'isolated-db-postcheck-recorded') { 'audit-passed' } else { 'isolated-db-postcheck-recorded' } }
    $parentPath = Resolve-Absolute ([string]$value.parent_manifest) $true $true
    $parentPath = Assert-PathUnderRunRoot $parentPath $runRoot
    $actualParentHash = (Get-FileHash -LiteralPath $parentPath -Algorithm SHA256).Hash.ToUpperInvariant()
    if ([string]$value.parent_manifest_sha256 -notmatch '^[0-9A-Fa-f]{64}$' -or
        $actualParentHash -cne ([string]$value.parent_manifest_sha256).ToUpperInvariant()) {
      throw 'PARENT_MANIFEST_HASH_MISMATCH'
    }
    Assert-ManifestFile $parentPath $parentStage $parentStatus $runRoot | Out-Null
  }
  return [pscustomobject]@{
    Path = $manifestPath
    Value = $value
    Hash = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToUpperInvariant()
    RunRoot = $runRoot
  }
}

function Read-Resume([string]$ExpectedStage, [string]$ExpectedStatus) {
  $manifestPath = Resolve-Absolute $ResumeManifest $true $true
  if (-not $ExpectedResumeManifestSha256 -or $ExpectedResumeManifestSha256 -notmatch '^[0-9A-Fa-f]{64}$') {
    throw 'ExpectedResumeManifestSha256 is required'
  }
  $actual = (Get-FileHash -LiteralPath $manifestPath -Algorithm SHA256).Hash.ToUpperInvariant()
  if ($actual -cne $ExpectedResumeManifestSha256.ToUpperInvariant()) {
    throw 'RESUME_MANIFEST_HASH_MISMATCH'
  }
  return Assert-ManifestFile $manifestPath $ExpectedStage $ExpectedStatus ''
}

function New-RunRoot([string]$BaseDirectory, [string]$ExplicitRunId) {
  $base = Resolve-Absolute $BaseDirectory $false $false
  $id = if ($ExplicitRunId) { $ExplicitRunId } else { (Get-Date).ToUniversalTime().ToString('yyyyMMddTHHmmssZ') }
  if ($id -notmatch '^[A-Za-z0-9._-]+$') { throw 'RunId contains unsafe characters' }
  $root = Join-Path $base $id
  if (Test-Path -LiteralPath $root) { throw "TARGET_ALREADY_EXISTS: $root" }
  [IO.Directory]::CreateDirectory($root) | Out-Null
  New-RunAnchor $root
  return [IO.Path]::GetFullPath($root)
}

$python = Resolve-Python $PythonPath

switch ($Stage) {
  'Backup' {
    $source = Resolve-Absolute $SourceDatabase $true $true
    $outputBase = Resolve-Absolute $OutputDirectory $false $false
    Assert-OutputSeparated $source $outputBase
    Assert-NoCaseboardProcess
    $runRoot = New-RunRoot $outputBase $RunId
    $rawCopy = Join-Path $runRoot '01-source-trio'
    $mainOnly = Join-Path $runRoot '02-main-only\caseboard.db'
    $resultJson = Join-Path $runRoot '03-backup-result.json'
    $arguments = @('backup','--source',$source,'--destination',$mainOnly,'--raw-copy-dir',$rawCopy,'--output',$resultJson)
    if ($MigrationsDirectory) {
      $migrations = Resolve-Absolute $MigrationsDirectory $true $false
      $arguments += @('--migrations-dir',$migrations)
    }
    Invoke-Audit $arguments
    Assert-NoCaseboardProcess
    $result = Get-Content -LiteralPath $resultJson -Raw -Encoding utf8 | ConvertFrom-Json
    if ($result.status -ne 'backup-passed' -or
        -not $result.checks.source_trio_unchanged -or
        -not $result.checks.destination_main_only -or
        -not $result.checks.quick_check_ok -or
        -not $result.checks.foreign_key_check_ok) {
      throw 'BACKUP_HARD_ASSERTION_FAILED'
    }
    $manifestPath = Join-Path $runRoot 'manifest.backup.json'
    $manifest = @{
      schema_version = 2
      status = 'backup-passed'
      stage = 'Backup'
      created_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      run_root = $runRoot
      artifacts = @(
        (New-Artifact 'main_only_database' $mainOnly $runRoot),
        (New-Artifact 'backup_result' $resultJson $runRoot)
      )
      formal_mutation = $false
      app_started = $false
      installer_started = $false
    }
    $hash = Write-Manifest $manifestPath $manifest $runRoot
    [pscustomobject]@{ status='backup-passed'; manifest=$manifestPath; manifest_sha256=$hash; main_only_database=$mainOnly } | ConvertTo-Json -Depth 5
  }
  'AuditCopy' {
    $resume = Read-Resume 'Backup' 'backup-passed'
    $db = [string](Get-Artifact $resume.Value 'main_only_database').path
    $runRoot = $resume.RunRoot
    $snapshotJson = Join-Path $runRoot '04-copy-audit.json'
    $arguments = @('snapshot','--db',$db,'--output',$snapshotJson)
    if ($MigrationsDirectory) {
      $migrations = Resolve-Absolute $MigrationsDirectory $true $false
      $arguments += @('--migrations-dir',$migrations)
    }
    Invoke-Audit $arguments
    $snapshot = (Get-Content -LiteralPath $snapshotJson -Raw -Encoding utf8 | ConvertFrom-Json).snapshot
    if ($snapshot.sqlite.quick_check.Count -ne 1 -or $snapshot.sqlite.quick_check[0] -ne 'ok' -or
        $snapshot.sqlite.foreign_key_violation_count -ne 0 -or -not $snapshot.main_only) {
      throw 'COPY_AUDIT_HARD_ASSERTION_FAILED'
    }
    $manifestPath = Join-Path $runRoot 'manifest.audit.json'
    $manifest = @{
      schema_version = 2
      status = 'audit-passed'
      stage = 'AuditCopy'
      created_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      run_root = $runRoot
      parent_manifest = $resume.Path
      parent_manifest_sha256 = $resume.Hash
      artifacts = @(
        (New-Artifact 'main_only_database' $db $runRoot),
        (New-Artifact 'snapshot' $snapshotJson $runRoot)
      )
      formal_mutation = $false
    }
    $hash = Write-Manifest $manifestPath $manifest $runRoot
    [pscustomobject]@{ status='audit-passed'; manifest=$manifestPath; manifest_sha256=$hash; snapshot=$snapshotJson } | ConvertTo-Json -Depth 5
  }
  'Compare' {
    $before = Resolve-Absolute $BeforeSnapshot $true $true
    $after = Resolve-Absolute $AfterSnapshot $true $true
    $runRoot = New-RunRoot $OutputDirectory $RunId
    $resultJson = Join-Path $runRoot 'compare.json'
    $arguments = @('compare','--before',$before,'--after',$after,'--output',$resultJson)
    if ($Idempotent) { $arguments += '--idempotent' }
    Invoke-Audit $arguments
    $result = Get-Content -LiteralPath $resultJson -Raw -Encoding utf8 | ConvertFrom-Json
    $status = if ($result.status -eq 'passed') { 'compare-passed' } else { 'compare-failed' }
    $manifestPath = Join-Path $runRoot 'manifest.compare.json'
    $hash = Write-Manifest $manifestPath @{
      schema_version = 2; status = $status; stage = 'Compare'; created_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      run_root = $runRoot; artifacts = @((New-Artifact 'comparison' $resultJson $runRoot))
      before_snapshot_sha256 = (Get-FileHash -LiteralPath $before -Algorithm SHA256).Hash.ToUpperInvariant()
      after_snapshot_sha256 = (Get-FileHash -LiteralPath $after -Algorithm SHA256).Hash.ToUpperInvariant()
      idempotent = [bool]$Idempotent; formal_mutation = $false
    } $runRoot
    if ($status -ne 'compare-passed') { throw 'COMPARISON_FAILED' }
    [pscustomobject]@{ status=$status; manifest=$manifestPath; manifest_sha256=$hash } | ConvertTo-Json -Depth 5
  }
  'RecordExternalRunDbPostcheck' {
    if ($IdempotentPostcheck) {
      $resume = Read-Resume 'RecordExternalRunDbPostcheck' 'isolated-db-postcheck-recorded'
    } else {
      $resume = Read-Resume 'AuditCopy' 'audit-passed'
    }
    if ($ExitMode -ne 'graceful') {
      throw 'UNVERIFIED_FORCED_EXIT_CLAIM_REJECTED: rebuild from the accepted main-only backup'
    }
    $runRoot = $resume.RunRoot
    $db = Resolve-Absolute $ProofDatabase $true $true
    $db = Assert-PathUnderRunRoot $db $runRoot
    if ($IdempotentPostcheck) {
      $boundProof = [string](Get-Artifact $resume.Value 'proof_database').path
      if (-not $db.Equals($boundProof, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'PROOF_DATABASE_PATH_MISMATCH'
      }
    } else {
      $retainedMainOnly = [string](Get-Artifact $resume.Value 'main_only_database').path
      if ($db.Equals($retainedMainOnly, [StringComparison]::OrdinalIgnoreCase)) {
        throw 'PROOF_DATABASE_MUST_NOT_MUTATE_RETAINED_BACKUP'
      }
    }
    foreach ($sidecarSuffix in @('-wal','-shm','-journal')) {
      if (Test-Path -LiteralPath "$db$sidecarSuffix") {
        throw 'SIDECAR_INVALIDATES_PROOF: preserve evidence and rebuild; do not delete sidecars'
      }
    }
    $snapshotStem = if ($IdempotentPostcheck) { 'idempotent-db-postcheck' } else { 'isolated-db-postcheck' }
    $snapshotJson = Join-Path $runRoot "$snapshotStem.json"
    $snapshotArguments = @('snapshot','--db',$db,'--output',$snapshotJson)
    if ($MigrationsDirectory) {
      $migrations = Resolve-Absolute $MigrationsDirectory $true $false
      $snapshotArguments += @('--migrations-dir',$migrations)
    }
    Invoke-Audit $snapshotArguments
    $comparisonJson = Join-Path $runRoot "$snapshotStem-compare.json"
    $beforeSnapshot = [string](Get-Artifact $resume.Value 'snapshot').path
    $comparisonArguments = @('compare','--before',$beforeSnapshot,'--after',$snapshotJson,'--output',$comparisonJson)
    if ($IdempotentPostcheck) { $comparisonArguments += '--idempotent' }
    Invoke-Audit $comparisonArguments
    $status = if ($IdempotentPostcheck) { 'idempotent-db-postcheck-recorded' } else { 'isolated-db-postcheck-recorded' }
    $manifestPath = Join-Path $runRoot "manifest.$snapshotStem.json"
    $manifest = @{
      schema_version = 2; status = $status; stage = 'RecordExternalRunDbPostcheck'; created_at_utc = (Get-Date).ToUniversalTime().ToString('o')
      run_root = $runRoot; parent_manifest = $resume.Path; parent_manifest_sha256 = $resume.Hash
      artifacts = @(
        (New-Artifact 'proof_database' $db $runRoot),
        (New-Artifact 'snapshot' $snapshotJson $runRoot),
        (New-Artifact 'comparison' $comparisonJson $runRoot)
      )
      unverified_external_claim = @{ exit_mode = [string]$ExitMode; asserted_by = 'caller' }
      observed_application_execution = $false
      sidecar_free = $true; idempotent = [bool]$IdempotentPostcheck; formal_mutation = $false
    }
    $hash = Write-Manifest $manifestPath $manifest $runRoot
    [pscustomobject]@{ status=$status; manifest=$manifestPath; manifest_sha256=$hash; snapshot=$snapshotJson } | ConvertTo-Json -Depth 5
  }
  'FormalSwitch' {
    if ($ResumeManifest) {
      $resume = Read-Resume 'RecordExternalRunDbPostcheck' 'idempotent-db-postcheck-recorded'
      throw 'RECORDED_POSTCHECK_NOT_FORMAL_SWITCH_EVIDENCE'
    }
    if (-not $AllowFormalMutation -or $ConfirmFormalMutation -ne 'SWITCH-FORMAL-DATA-BY-ATOMIC-RENAME') {
      throw 'FORMAL_MUTATION_CONFIRMATION_REQUIRED'
    }
    throw 'FORMAL_SWITCH_DISABLED_IN_R3: implementation requires separate reviewed task'
  }
  'Install' {
    if ($ResumeManifest) { throw 'RECORDED_POSTCHECK_NOT_INSTALL_EVIDENCE' }
    if (-not $AllowFormalMutation -or $ConfirmFormalMutation -ne 'INSTALL-VERIFIED-FORMAL-PACKAGE') {
      throw 'FORMAL_MUTATION_CONFIRMATION_REQUIRED'
    }
    throw 'FORMAL_INSTALL_DISABLED_IN_R3: implementation requires separate reviewed task'
  }
}
