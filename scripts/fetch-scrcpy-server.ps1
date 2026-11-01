# Downloads the pinned scrcpy-server into app/src-tauri/binaries/ and verifies its SHA-256.
# The version and hash are read from app/src-tauri/src/scrcpy.rs (SERVER_VERSION and
# SERVER_SHA256), the single source of truth.
#
#   powershell -ExecutionPolicy Bypass -File scripts/fetch-scrcpy-server.ps1 [-PrintOnly]
#
# -PrintOnly shows the pinned version, hash and URL without downloading anything.

param([switch]$PrintOnly)

$ErrorActionPreference = 'Stop'

$RustSource = Join-Path $PSScriptRoot '..\app\src-tauri\src\scrcpy.rs'
$source = Get-Content -Raw $RustSource

function Get-RustConst([string]$Name) {
    $m = [regex]::Match($source, "pub const $Name\s*:\s*&str\s*=\s*""([^""]+)""")
    if (-not $m.Success) { throw "Could not find $Name in $RustSource" }
    return $m.Groups[1].Value
}

$Version = Get-RustConst 'SERVER_VERSION'
$Sha256  = (Get-RustConst 'SERVER_SHA256').ToLower()
$Url     = "https://github.com/Genymobile/scrcpy/releases/download/v$Version/scrcpy-server-v$Version"

if ($PrintOnly) {
    Write-Host "scrcpy-server $Version"
    Write-Host "sha256 $Sha256"
    Write-Host "url    $Url"
    exit 0
}

$OutDir  = Join-Path $PSScriptRoot '..\app\src-tauri\binaries'
$OutFile = Join-Path $OutDir 'scrcpy-server.jar'
$TmpFile = "$OutFile.download"

New-Item -ItemType Directory -Force $OutDir | Out-Null

if (Test-Path $OutFile) {
    $existing = (Get-FileHash -Algorithm SHA256 $OutFile).Hash.ToLower()
    if ($existing -eq $Sha256) {
        Write-Host "scrcpy-server $Version already present and verified."
        exit 0
    }
    Write-Host "Existing scrcpy-server.jar has the wrong hash; re-downloading."
}

Write-Host "Downloading $Url"
Invoke-WebRequest -Uri $Url -OutFile $TmpFile -UseBasicParsing

$actual = (Get-FileHash -Algorithm SHA256 $TmpFile).Hash.ToLower()
if ($actual -ne $Sha256) {
    Remove-Item $TmpFile -Force
    throw "SHA-256 mismatch for scrcpy-server v${Version}: expected $Sha256, got $actual"
}

Move-Item -Force $TmpFile $OutFile
Write-Host "scrcpy-server $Version verified -> $OutFile"
