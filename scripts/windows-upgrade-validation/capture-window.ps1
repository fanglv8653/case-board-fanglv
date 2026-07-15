param(
  [int]$ProcessId,
  [Parameter(Mandatory = $true)][string]$Output,
  [string]$AsciiTempRoot,
  [switch]$SelfTest
)

$ErrorActionPreference = 'Stop'
$candidateRoot = if ($AsciiTempRoot) { $AsciiTempRoot } elseif ($env:CASEBOARD_CAPTURE_TEMP_ROOT) { $env:CASEBOARD_CAPTURE_TEMP_ROOT } else { [IO.Path]::GetTempPath().TrimEnd('\') }
if ($candidateRoot -notmatch '^[\x20-\x7E]+$') { throw 'TEMP contains non-ASCII characters; provide -AsciiTempRoot or CASEBOARD_CAPTURE_TEMP_ROOT' }
if ($candidateRoot -notmatch '^(?:[A-Za-z]:\\|\\\\)') { throw 'ASCII temp root must be an absolute path' }
[IO.Directory]::CreateDirectory($candidateRoot) | Out-Null
$asciiTemp = Join-Path $candidateRoot ('caseboard-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $asciiTemp -Force | Out-Null
$oldTemp, $oldTmp = $env:TEMP, $env:TMP
$env:TEMP = $asciiTemp
$env:TMP = $asciiTemp

try {
  Add-Type -AssemblyName System.Drawing
  if ($SelfTest) {
    $bitmap = [System.Drawing.Bitmap]::new(2, 2)
  } else {
    if ($ProcessId -le 0) { throw 'ProcessId is required unless -SelfTest is used' }
    Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class WinCapture {
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool PrintWindow(IntPtr hWnd, IntPtr hdc, uint flags);
  public struct RECT { public int Left; public int Top; public int Right; public int Bottom; }
}
"@
    $process = Get-Process -Id $ProcessId -ErrorAction Stop
    $handle = $process.MainWindowHandle
    if ($handle -eq [IntPtr]::Zero) { throw 'No main window handle' }
    $rect = [WinCapture+RECT]::new()
    if (-not [WinCapture]::GetWindowRect($handle, [ref]$rect)) { throw 'GetWindowRect failed' }
    $width = $rect.Right - $rect.Left
    $height = $rect.Bottom - $rect.Top
    if ($width -le 0 -or $height -le 0) { throw 'Invalid window dimensions' }
    $bitmap = [System.Drawing.Bitmap]::new($width, $height)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $hdc = $graphics.GetHdc()
    try {
      if (-not [WinCapture]::PrintWindow($handle, $hdc, 2)) { throw 'PrintWindow failed' }
    } finally {
      $graphics.ReleaseHdc($hdc)
      $graphics.Dispose()
    }
  }
  $outputPath = [IO.Path]::GetFullPath($Output)
  [IO.Directory]::CreateDirectory([IO.Path]::GetDirectoryName($outputPath)) | Out-Null
  $bitmap.Save($outputPath, [System.Drawing.Imaging.ImageFormat]::Png)
  $bitmap.Dispose()
  [ordered]@{ status = 'passed'; output = $outputPath; temp_is_ascii = $true; temp_directory = $asciiTemp } | ConvertTo-Json -Compress
} finally {
  $env:TEMP, $env:TMP = $oldTemp, $oldTmp
  # Add-Type may keep its generated assembly locked until this short-lived
  # PowerShell process exits. A later invocation removes stale directories.
  Get-ChildItem -LiteralPath $candidateRoot -Directory -ErrorAction SilentlyContinue |
    Where-Object { $_.Name -match '^caseboard-[0-9a-f]{32}$' -and $_.FullName -ne $asciiTemp } |
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue
}
