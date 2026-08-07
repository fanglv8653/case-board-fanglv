[CmdletBinding()]
param(
    [string]$ManifestTool
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

# Windows PowerShell 5.1 otherwise decodes Cargo's UTF-8 JSON using the active
# console code page, corrupting executable paths when the workspace is under a
# directory with non-ASCII characters.
$utf8NoBom = New-Object Text.UTF8Encoding($false)
[Console]::OutputEncoding = $utf8NoBom
$OutputEncoding = $utf8NoBom

if ($env:OS -ne 'Windows_NT') {
    throw 'run-windows-rust-tests.ps1 requires Windows.'
}

if (-not $ManifestTool) {
    $command = Get-Command mt.exe -ErrorAction SilentlyContinue
    if ($command) {
        $ManifestTool = $command.Source
    } else {
        $kitsRoot = 'C:\Program Files (x86)\Windows Kits\10\bin'
        $candidate = Get-ChildItem -LiteralPath $kitsRoot -Recurse -Filter mt.exe -ErrorAction SilentlyContinue |
            Where-Object { $_.FullName -match '\\x64\\mt\.exe$' } |
            Sort-Object FullName -Descending |
            Select-Object -First 1
        if ($candidate) {
            $ManifestTool = $candidate.FullName
        }
    }
}

if (-not $ManifestTool -or -not (Test-Path -LiteralPath $ManifestTool -PathType Leaf)) {
    throw 'Windows SDK mt.exe was not found.'
}

$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$tempRoot = if ($env:RUNNER_TEMP) { $env:RUNNER_TEMP } else { [IO.Path]::GetTempPath() }
$manifestPath = Join-Path $tempRoot 'caseboard-rust-test.manifest'

$manifest = @'
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*" />
    </dependentAssembly>
  </dependency>
</assembly>
'@
[IO.File]::WriteAllText($manifestPath, $manifest, (New-Object Text.UTF8Encoding($false)))

try {
    Push-Location $root
    try {
        $jsonLines = @(& cargo test --workspace --locked --no-run --message-format=json)
        if ($LASTEXITCODE -ne 0) {
            throw "cargo test --no-run failed: exit=$LASTEXITCODE"
        }
    } finally {
        Pop-Location
    }

    $testExecutables = @(
        foreach ($line in $jsonLines) {
            try {
                $record = $line | ConvertFrom-Json -ErrorAction Stop
                if (
                    $record.reason -eq 'compiler-artifact' -and
                    $record.profile.test -eq $true -and
                    $record.executable
                ) {
                    [IO.Path]::GetFullPath([string]$record.executable)
                }
            } catch {
                # Ignore non-JSON Cargo diagnostics while discovering test targets.
            }
        }
    ) | Sort-Object -Unique

    if ($testExecutables.Count -eq 0) {
        throw 'Cargo did not report any test executables.'
    }

    foreach ($testExecutable in $testExecutables) {
        if (-not (Test-Path -LiteralPath $testExecutable -PathType Leaf)) {
            throw "Test executable was not found: $testExecutable"
        }
        & $ManifestTool -nologo -manifest $manifestPath "-outputresource:$testExecutable;#1"
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to embed test manifest: $testExecutable"
        }
    }

    foreach ($testExecutable in $testExecutables) {
        Write-Host "[test] $testExecutable"
        & $testExecutable
        if ($LASTEXITCODE -ne 0) {
            throw "Rust tests failed: $testExecutable (exit=$LASTEXITCODE)"
        }
    }
} finally {
    Remove-Item -LiteralPath $manifestPath -Force -ErrorAction SilentlyContinue
}

Write-Host "[ok] Windows Rust tests passed: $($testExecutables.Count) executables"
