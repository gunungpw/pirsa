[CmdletBinding()]
param(
    [switch]$Startup,
    [switch]$NoLaunch
)

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot

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

Push-Location $root
try {
    if (Get-Process -Name pirsa -ErrorAction SilentlyContinue) {
        throw 'Pirsa is running. Open it and choose Quit (or press Ctrl+Q) before installing.'
    }

    Write-Host 'Building Pirsa (release)...' -ForegroundColor Cyan
    cargo build --release
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build failed with exit code $LASTEXITCODE"
    }

    $destination = Join-Path $env:LOCALAPPDATA 'Programs\Pirsa'
    New-Item -ItemType Directory -Force -Path $destination | Out-Null

    $source = Join-Path $root 'target\release\pirsa.exe'
    $target = Join-Path $destination 'pirsa.exe'
    Copy-Item -Path $source -Destination $target -Force

    $programsFolder = [Environment]::GetFolderPath('Programs')
    $startMenuShortcut = Join-Path $programsFolder 'Pirsa.lnk'
    New-PirsaShortcut -Path $startMenuShortcut -Target $target -WorkingDirectory $destination
    Write-Host "Start Menu shortcut created: $startMenuShortcut" -ForegroundColor Green

    if ($Startup) {
        $startupFolder = [Environment]::GetFolderPath('Startup')
        $startupShortcut = Join-Path $startupFolder 'Pirsa.lnk'
        New-PirsaShortcut -Path $startupShortcut -Target $target -WorkingDirectory $destination
        Write-Host "Startup shortcut created: $startupShortcut" -ForegroundColor Green
    }

    Write-Host "Installed: $target" -ForegroundColor Green

    if (-not $NoLaunch) {
        Start-Process -FilePath $target -WindowStyle Hidden
        Write-Host 'Pirsa is running in the background. Press Alt+Space.' -ForegroundColor Green
    }
}
finally {
    Pop-Location
}
