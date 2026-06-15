# digse installer — Windows (no administrator privileges required).
#
# Downloads the latest release binary from github.com/openepoch/digse and
# installs it into %LOCALAPPDATA%\digse, adding that folder to the per-user
# PATH (HKCU — no UAC prompt). Re-running updates in place.
#
# One-liner (in PowerShell or a Terminal window):
#   irm https://raw.githubusercontent.com/openepoch/digse/main/docs/install.ps1 | iex
#
# Install a specific release tag:
#   $env:DIGSE_VERSION = "v0.2.0"; irm https://raw.githubusercontent.com/openepoch/digse/main/docs/install.ps1 | iex
#Requires -Version 5.1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'

$Repo = 'openepoch/digse'
$Target = 'x86_64-pc-windows-msvc'
$InstallDir = if ($env:DIGSE_INSTALL_DIR) { $env:DIGSE_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'digse' }

function Fail($msg) {
    Write-Host "install: error: $msg" -ForegroundColor Red
    exit 1
}

# --- capture previous version (from the existing exe, if any) BEFORE overwrite
$prevVersion = $null
$existingExe = Join-Path $InstallDir 'digse.exe'
if (Test-Path $existingExe) {
    $prevVersion = & $existingExe --version 2>$null | Select-Object -First 1
}

# --- resolve the release tag -------------------------------------------------
if ($env:DIGSE_VERSION) {
    $Tag = $env:DIGSE_VERSION
} else {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -Headers @{ 'User-Agent' = 'digse-installer' }
    } catch {
        Fail "could not fetch latest release: $_"
    }
    $Tag = $release.tag_name
    if (-not $Tag) { Fail "could not parse tag_name from release JSON" }
}

$assetUrl = "https://github.com/$Repo/releases/download/$Tag/digse-$Target.zip"
Write-Host "install: latest release is $Tag for $Target"

# --- download + extract ------------------------------------------------------
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
$tmp = New-TemporaryFile
try {
    Invoke-WebRequest -Uri $assetUrl -OutFile $tmp.FullName -UseBasicParsing
    # Expand-Archive needs a .zip extension to recognise the archive.
    $zipPath = "$($tmp.FullName).zip"
    Move-Item -Path $tmp.FullName -Destination $zipPath -Force
    Expand-Archive -Path $zipPath -DestinationPath $InstallDir -Force
} finally {
    Remove-Item -Path "$($tmp.FullName)*" -Force -ErrorAction SilentlyContinue
}

$exe = Join-Path $InstallDir 'digse.exe'
if (-not (Test-Path $exe)) { Fail "archive did not contain digse.exe" }

# --- report installed/updated ------------------------------------------------
$newVersion = & $exe --version 2>$null | Select-Object -First 1
if ($prevVersion -and $prevVersion -ne $newVersion) {
    Write-Host "install: updated $prevVersion -> $newVersion"
} elseif ($prevVersion) {
    Write-Host "install: already at $newVersion (reinstalled)"
} else {
    Write-Host "install: installed $newVersion"
}

# --- add to user PATH (HKCU\Environment — no admin) -------------------------
$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if ($userPath -and ($userPath.Split(';') -notcontains $InstallDir)) {
    $newPath = "$InstallDir;$userPath"
    [Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
    Write-Host "install: added $InstallDir to your user PATH" -ForegroundColor Green
    Write-Host "         open a new terminal for it to take effect" -ForegroundColor Yellow
} elseif (-not $userPath) {
    [Environment]::SetEnvironmentVariable('Path', $InstallDir, 'User')
    Write-Host "install: added $InstallDir to your user PATH" -ForegroundColor Green
    Write-Host "         open a new terminal for it to take effect" -ForegroundColor Yellow
} else {
    Write-Host "install: $InstallDir already on your user PATH"
}

Write-Host ""
Write-Host 'Run "digse version" to confirm, then "digse start".'
