param(
    [string]$Filter = "",
    [switch]$Ignored,
    [switch]$NoCapture
)

$ErrorActionPreference = "Stop"

if ($env:OS -ne "Windows_NT") {
    throw "This script only supports Windows Rust tests."
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$manifestPath = Join-Path $repoRoot "src-tauri\windows-common-controls.manifest"
$cargoPath = Join-Path $env:USERPROFILE ".cargo\bin\cargo.exe"
if (-not (Test-Path -LiteralPath $cargoPath)) {
    $cargoPath = (Get-Command cargo -ErrorAction Stop).Source
}

$kitRoot = "C:\Program Files (x86)\Windows Kits\10\bin"
$mtPath = Get-ChildItem -LiteralPath $kitRoot -Filter mt.exe -Recurse -ErrorAction Stop |
    Where-Object { $_.FullName -like "*\x64\mt.exe" } |
    Sort-Object FullName -Descending |
    Select-Object -First 1 -ExpandProperty FullName
if (-not $mtPath) {
    throw "Windows SDK mt.exe was not found; the test manifest cannot be embedded."
}

$buildOutput = & $cargoPath test --manifest-path (Join-Path $repoRoot "src-tauri\Cargo.toml") --lib --no-run --message-format=json
if ($LASTEXITCODE -ne 0) {
    throw "cargo test --no-run failed with exit code $LASTEXITCODE"
}

$testExecutables = @(
    $buildOutput |
        ForEach-Object {
            try { $_ | ConvertFrom-Json } catch { $null }
        } |
        Where-Object { $_.reason -eq "compiler-artifact" -and $_.profile.test -eq $true -and $_.executable } |
        ForEach-Object { $_.executable } |
        Sort-Object -Unique
)
if ($testExecutables.Count -eq 0) {
    throw "No Rust test executable was found in Cargo output."
}

foreach ($testExecutable in $testExecutables) {
    & $mtPath -nologo -manifest $manifestPath "-outputresource:$testExecutable;#1"
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to embed the test manifest: $testExecutable"
    }

    $testArgs = @()
    if ($Filter) { $testArgs += $Filter }
    if ($Ignored) { $testArgs += "--ignored" }
    if ($NoCapture) { $testArgs += "--nocapture" }
    & $testExecutable @testArgs
    if ($LASTEXITCODE -ne 0) {
        throw "Rust tests failed: $testExecutable (exit code $LASTEXITCODE)"
    }
}
