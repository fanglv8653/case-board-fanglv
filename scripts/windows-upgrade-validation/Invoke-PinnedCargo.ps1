param(
  [Parameter(ValueFromRemainingArguments = $true)][string[]]$CargoArguments
)

$ErrorActionPreference = 'Stop'
$env:RUSTUP_TOOLCHAIN = 'stable-x86_64-pc-windows-msvc'
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$rustc = Join-Path $env:USERPROFILE '.cargo\bin\rustc.exe'
if (-not (Test-Path -LiteralPath $cargo) -or -not (Test-Path -LiteralPath $rustc)) {
  throw 'Rust tools are not installed in the expected user cargo directory'
}
$version = (& $rustc --version)
if ($LASTEXITCODE -ne 0 -or $version -notmatch '^rustc 1\.96\.0\b') {
  throw "Pinned local Rust 1.96.0 is unavailable; actual: $version"
}
Write-Host "Using $version without rustup channel synchronization"
& $cargo @CargoArguments
exit $LASTEXITCODE
