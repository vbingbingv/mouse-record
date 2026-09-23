# Verifies that the application manifest embedded in the built exe is valid UTF-8
# and still carries requireAdministrator plus the Common Controls v6 dependency.
#
# Why this exists: tauri-winres compiles src-tauri/app.manifest with rc.exe, which
# re-encodes the manifest resource using the system ANSI code page. Any non-ASCII
# character in that file (even inside a comment) makes the embedded manifest invalid
# UTF-8, and Windows then refuses to start the app with "the application has failed
# to start because its side-by-side configuration is incorrect". build.rs already
# asserts the source file is pure ASCII; this check validates the actual bytes that
# ended up in the exe, so it also catches toolchain changes.
#
# Exit code 0 = pass, 1 = fail.

param(
  [string]$Exe = 'src-tauri/target/release/mouse-record.exe'
)

$ErrorActionPreference = 'Stop'

if (-not (Test-Path -LiteralPath $Exe)) {
  Write-Output "FAIL: exe not found: $Exe"
  exit 1
}

$exePath = (Resolve-Path -LiteralPath $Exe).Path

# ISO-8859-1 keeps a 1 char <-> 1 byte mapping, so string indexes equal byte offsets.
$latin1 = [System.Text.Encoding]::GetEncoding(28591)
$bytes = [System.IO.File]::ReadAllBytes($exePath)
$asLatin1 = $latin1.GetString($bytes)

$start = $asLatin1.IndexOf('<assembly xmlns=')
$end = $asLatin1.IndexOf('</assembly>') + 11

if ($start -lt 0 -or $end -le $start) {
  Write-Output "FAIL: no embedded application manifest found in $exePath"
  exit 1
}

$length = $end - $start
$fragment = New-Object byte[] $length
[Array]::Copy($bytes, $start, $fragment, 0, $length)
Write-Output "embedded manifest: byte offset $start, length $length"

$ok = $true

$strict = New-Object System.Text.UTF8Encoding($false, $true)
try {
  $text = $strict.GetString($fragment)
  Write-Output 'OK: embedded manifest is valid UTF-8'
} catch {
  $ok = $false
  $text = ''
  Write-Output "FAIL: embedded manifest is not valid UTF-8 -> $($_.Exception.Message)"
}

$nonAscii = 0
foreach ($byte in $fragment) {
  if ($byte -gt 0x7F) { $nonAscii++ }
}
Write-Output "non-ASCII bytes in embedded manifest: $nonAscii (expected 0)"
if ($nonAscii -ne 0) { $ok = $false }

$hasAdmin = $text -match 'requireAdministrator'
$hasCommonControls = $text -match 'Microsoft\.Windows\.Common-Controls'
Write-Output "requireAdministrator present: $hasAdmin"
Write-Output "Common-Controls v6 dependency present: $hasCommonControls"
if (-not $hasAdmin -or -not $hasCommonControls) { $ok = $false }

if ($ok) {
  Write-Output 'RESULT: PASS'
  exit 0
}

Write-Output 'RESULT: FAIL'
exit 1
