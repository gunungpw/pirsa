[CmdletBinding()]
param(
    [switch]$Startup,
    [switch]$NoLaunch
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
$localAppData = [Environment]::GetFolderPath('LocalApplicationData')

if ([string]::IsNullOrWhiteSpace($localAppData)) {
    throw 'Could not locate the local application data folder.'
}

# Pirsa is a per-user app. Settings, usage history, and the file index live in
# %LOCALAPPDATA%\Pirsa and are deliberately never touched by this script.
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = [Security.Principal.WindowsPrincipal]::new($identity)

if ($principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Warning "Pirsa is a per-user app. Run this script from a normal, non-elevated PowerShell so it uses your own LOCALAPPDATA. An elevated install belongs to the administrator profile and starts with a different Pirsa data folder."
}

$source = Join-Path $root 'target\release\pirsa.exe'
$destination = Join-Path $localAppData 'Programs\Pirsa'
$target = Join-Path $destination 'pirsa.exe'
$processName = 'pirsa'
$quitEventName = 'Local\Pirsa.Quit.v1'
$eventModifyState = 0x0002

function New-PirsaShortcut {
    param(
        [Parameter(Mandatory)]
        [string]$Path,
        [Parameter(Mandatory)]
        [string]$Target,
        [Parameter(Mandatory)]
        [string]$WorkingDirectory
    )

    $shell = New-Object -ComObject WScript.Shell
    $shortcut = $shell.CreateShortcut($Path)
    $shortcut.TargetPath = $Target
    $shortcut.WorkingDirectory = $WorkingDirectory
    $shortcut.Description = 'Pirsa (Alt+Space)'
    $shortcut.IconLocation = "$Target,0"
    $shortcut.Save()
}

# Named events belong to the running process, not to .NET, so use the Win32
# APIs directly. This mirrors what the app does in src/platform/instance.rs.
function Initialize-NativeMethods {
    if ('Pirsa.Install.NativeMethods' -as [type]) {
        return $true
    }

    try {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;

namespace Pirsa.Install
{
    public static class NativeMethods
    {
        [DllImport("kernel32.dll", SetLastError = true, CharSet = CharSet.Unicode, EntryPoint = "OpenEventW")]
        public static extern IntPtr OpenEvent(uint desiredAccess, bool inheritHandle, string name);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool SetEvent(IntPtr handle);

        [DllImport("kernel32.dll", SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        public static extern bool CloseHandle(IntPtr handle);
    }
}
'@
        return $true
    }
    catch {
        Write-Warning "Clean shutdown is unavailable: $($_.Exception.Message)"
        return $false
    }
}

function Get-PirsaProcess {
    Get-Process -Name $processName -ErrorAction SilentlyContinue
}

# Asks the running app to shut down through its quit event. Returns $false when
# the event does not exist, which happens when an older build is running.
function Request-PirsaQuit {
    if (-not ('Pirsa.Install.NativeMethods' -as [type])) {
        return $false
    }

    $handle = [Pirsa.Install.NativeMethods]::OpenEvent(
        $eventModifyState,
        $false,
        $quitEventName
    )

    if ($handle -eq [IntPtr]::Zero) {
        return $false
    }

    try {
        return [Pirsa.Install.NativeMethods]::SetEvent($handle)
    }
    finally {
        [void][Pirsa.Install.NativeMethods]::CloseHandle($handle)
    }
}

function Wait-ForPirsaExit {
    param([int]$TimeoutMilliseconds)

    $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)

    while ([DateTime]::UtcNow -lt $deadline) {
        if (@(Get-PirsaProcess).Count -eq 0) {
            return $true
        }

        Start-Sleep -Milliseconds 100
    }

    return (@(Get-PirsaProcess).Count -eq 0)
}

# Stops every running copy, since a surviving copy would keep the single
# instance mutex and make the freshly installed one exit at startup.
function Stop-PirsaInstances {
    $running = @(Get-PirsaProcess)

    if ($running.Count -eq 0) {
        Write-Host 'No running Pirsa instance found.' -ForegroundColor DarkGray
        return
    }

    $pids = ($running | ForEach-Object { $_.Id }) -join ', '
    Write-Host "Stopping running Pirsa instance(s): $pids" -ForegroundColor Cyan

    $quitRequested = $false

    try {
        $quitRequested = Request-PirsaQuit
    }
    catch {
        Write-Warning "Could not ask Pirsa to quit cleanly: $($_.Exception.Message)"
    }

    if ($quitRequested) {
        if (Wait-ForPirsaExit -TimeoutMilliseconds 5000) {
            Write-Host 'Pirsa closed cleanly.' -ForegroundColor Green
            return
        }
    }

    foreach ($process in @(Get-PirsaProcess)) {
        try {
            Stop-Process -Id $process.Id -Force -ErrorAction Stop
        }
        catch {
            Write-Warning "Could not stop PID $($process.Id): $($_.Exception.Message)"
        }
    }

    if (-not (Wait-ForPirsaExit -TimeoutMilliseconds 10000)) {
        $pids = (@(Get-PirsaProcess) | ForEach-Object { $_.Id }) -join ', '
        throw "Pirsa is still running (PID: $pids). Close it manually and try again."
    }

    Write-Host 'Pirsa stopped.' -ForegroundColor Green
}

Push-Location $root
try {
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        throw 'cargo was not found. Install Rust from https://rustup.rs and try again.'
    }

    # Build before touching the running app, so a failed build leaves the
    # existing installation and its process untouched.
    Write-Host 'Building Pirsa (release)...' -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Build succeeded but $source was not produced."
    }

    Initialize-NativeMethods | Out-Null
    Stop-PirsaInstances

    New-Item -ItemType Directory -Force -Path $destination | Out-Null

    for ($attempt = 1; $attempt -le 5; $attempt++) {
        try {
            Copy-Item -LiteralPath $source -Destination $target -Force -ErrorAction Stop
            break
        }
        catch {
            if ($attempt -eq 5) {
                throw "Could not replace ${target}: $($_.Exception.Message)"
            }

            # A virus scanner or a process still releasing its image can hold
            # the file for a moment; retry briefly before giving up.
            Start-Sleep -Milliseconds 300
        }
    }

    $programsFolder = [Environment]::GetFolderPath('Programs')
    if ([string]::IsNullOrWhiteSpace($programsFolder)) {
        throw 'Could not locate the Start Menu programs folder.'
    }

    $startMenuShortcut = Join-Path $programsFolder 'Pirsa.lnk'
    New-PirsaShortcut -Path $startMenuShortcut -Target $target -WorkingDirectory $destination
    Write-Host "Start Menu shortcut created: $startMenuShortcut" -ForegroundColor Green

    $startupFolder = [Environment]::GetFolderPath('Startup')
    if ([string]::IsNullOrWhiteSpace($startupFolder)) {
        throw 'Could not locate the Startup folder.'
    }

    $startupShortcut = Join-Path $startupFolder 'Pirsa.lnk'

    if ($Startup) {
        New-PirsaShortcut -Path $startupShortcut -Target $target -WorkingDirectory $destination
        Write-Host "Startup shortcut created: $startupShortcut" -ForegroundColor Green
    }
    elseif (Test-Path -LiteralPath $startupShortcut) {
        Remove-Item -LiteralPath $startupShortcut -Force
        Write-Host "Startup shortcut removed: $startupShortcut" -ForegroundColor Green
    }

    Write-Host "Installed: $target" -ForegroundColor Green

    if ($NoLaunch) {
        Write-Host 'Pirsa is not running (-NoLaunch).' -ForegroundColor Yellow
    }
    else {
        $process = Start-Process -FilePath $target -WorkingDirectory $destination -WindowStyle Hidden -PassThru
        Start-Sleep -Milliseconds 750

        if ($process.HasExited) {
            throw "Pirsa started but exited immediately (exit code $($process.ExitCode)). Check $localAppData\Pirsa\launcher.log."
        }

        Write-Host 'Pirsa is running in the background. Press Alt+Space.' -ForegroundColor Green
    }
}
finally {
    Pop-Location
}
