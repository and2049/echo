#Requires -Version 5.0
<#
.SYNOPSIS
    Install echo — the desktop app plus the `spotify` terminal command — on Windows.

.DESCRIPTION
    Downloads the release MSI and installs it silently for the current user. The MSI puts both
    frontends in %LOCALAPPDATA%\Programs\echo and adds that directory to the user PATH, which is
    what lets `spotify upgrade` replace them later without elevation or another installer run.

.EXAMPLE
    irm https://github.com/and2049/echo/releases/latest/download/install.ps1 | iex

.EXAMPLE
    & ([scriptblock]::Create((irm https://github.com/and2049/echo/releases/latest/download/install.ps1))) -Version 0.4.6
#>
[CmdletBinding()]
param(
    [Alias("v")]
    [string]$Version,
    [switch]$Uninstall,
    [switch]$Help
)

$ErrorActionPreference = "Stop"
$Repo = "and2049/echo"

function Write-Info { param([string]$Message) Write-Host $Message -ForegroundColor Gray }
function Write-Failure { param([string]$Message) Write-Host $Message -ForegroundColor Red }

if ($Help) {
    Write-Host @"
Install echo - the desktop app plus the 'spotify' terminal command.

Usage: install.ps1 [options]

Options:
    -Version <version>   Install a specific release (e.g. 0.4.6)
    -Uninstall           Remove echo (leaves your settings in %APPDATA%\echo alone)
    -Help                Show this message

After installing, upgrade with 'spotify upgrade' - no need to re-run this script.
"@
    exit 0
}

# The installed product, as it appears in Add/Remove Programs. Per-user MSIs register under
# HKCU, so this finds echo without the notoriously slow Win32_Product query.
function Get-EchoRegistration {
    $roots = @(
        "HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*",
        "HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*"
    )
    Get-ItemProperty -Path $roots -ErrorAction SilentlyContinue |
        Where-Object { $_.DisplayName -eq "echo" } |
        Select-Object -First 1
}

# --- Uninstall -------------------------------------------------------------

if ($Uninstall) {
    $installed = Get-EchoRegistration
    if (-not $installed) {
        Write-Info "echo is not installed."
        exit 0
    }
    Write-Info "Removing echo $($installed.DisplayVersion)"
    $result = Start-Process msiexec.exe -ArgumentList "/x", $installed.PSChildName, "/qn", "REBOOT=ReallySuppress" -Wait -PassThru
    if ($result.ExitCode -ne 0) {
        Write-Failure "msiexec failed with exit code $($result.ExitCode)"
        exit 1
    }
    Write-Info "echo has been uninstalled. Your settings in %APPDATA%\echo were left alone."
    exit 0
}

# --- Platform --------------------------------------------------------------

if ($env:PROCESSOR_ARCHITECTURE -ne "AMD64" -and $env:PROCESSOR_ARCHITEW6432 -ne "AMD64") {
    Write-Failure "No Windows build for $env:PROCESSOR_ARCHITECTURE - releases are x64 only."
    exit 1
}

# --- Resolve version -------------------------------------------------------

if ($Version) {
    $Version = $Version -replace '^v', ''
} else {
    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest" -UseBasicParsing
        $Version = $release.tag_name -replace '^v', ''
    } catch {
        Write-Failure "Could not work out the latest version from the GitHub API: $($_.Exception.Message)"
        exit 1
    }
}

$installed = Get-EchoRegistration
if ($installed -and $installed.DisplayVersion -eq $Version) {
    Write-Info "echo $Version is already installed. Run 'spotify upgrade' to move to a newer release."
    exit 0
}

# --- Download and install --------------------------------------------------

# The MSI cannot rewrite files a running echo holds open; msiexec would otherwise schedule the
# replacement for the next reboot and appear to have done nothing.
$running = Get-Process -Name "spotify", "echo-desktop" -ErrorAction SilentlyContinue
if ($running) {
    Write-Failure "echo is running. Close it (and any 'spotify' terminals) and try again."
    exit 1
}

$msi = "echo-desktop_${Version}_x64_en-US.msi"
$url = "https://github.com/$Repo/releases/download/v$Version/$msi"
$tmp = Join-Path $env:TEMP "echo_install_$PID"
New-Item -ItemType Directory -Path $tmp -Force | Out-Null

try {
    Write-Info "Downloading $msi"
    $path = Join-Path $tmp $msi
    # Invoke-WebRequest's progress UI slows a large download to a crawl in some hosts.
    $previousProgress = $ProgressPreference
    $ProgressPreference = "SilentlyContinue"
    try {
        Invoke-WebRequest -Uri $url -OutFile $path -UseBasicParsing
    } finally {
        $ProgressPreference = $previousProgress
    }

    Write-Info "Installing echo $Version"
    $log = Join-Path $tmp "install.log"
    $result = Start-Process msiexec.exe `
        -ArgumentList "/i", "`"$path`"", "/qn", "REBOOT=ReallySuppress", "/l*v", "`"$log`"" `
        -Wait -PassThru
    if ($result.ExitCode -ne 0) {
        Write-Failure "msiexec failed with exit code $($result.ExitCode)"
        if (Test-Path $log) {
            $kept = Join-Path $env:TEMP "echo-install.log"
            Copy-Item $log $kept -Force
            Write-Info "Installer log: $kept"
        }
        exit 1
    }
} catch {
    Write-Failure "Install failed: $($_.Exception.Message)"
    exit 1
} finally {
    Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
}

# --- Done ------------------------------------------------------------------

$installDir = Join-Path $env:LOCALAPPDATA "Programs\echo"

if ($env:GITHUB_ACTIONS -eq "true" -and $env:GITHUB_PATH) {
    Add-Content -Path $env:GITHUB_PATH -Value $installDir
}

Write-Host ""
Write-Info "echo $Version is installed."
Write-Info "  App:      $installDir\echo-desktop.exe (also in your Start menu)"
Write-Info "  Terminal: $installDir\spotify.exe"
Write-Host ""
Write-Info "Open a new terminal, then run 'spotify' to start."
Write-Info "Later on, 'spotify upgrade' updates both the terminal client and the app."
Write-Host ""
