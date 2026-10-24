$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot 'artifacts\win-arm64'
$exe = Join-Path $source 'Vitals.Windows.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run native\Build.ps1 first.' }
foreach ($file in @('vitals_core.dll', 'Vitals.Windows.pri', 'App.xbf', 'MainWindow.xbf', 'LICENSE.txt', 'Assets\Health.ico', 'Assets\Health.png')) {
    if (-not (Test-Path -LiteralPath (Join-Path $source $file))) {
        throw "A required runtime file is missing: $file. Rebuild before installing."
    }
}
$destination = Join-Path $env:LOCALAPPDATA 'Programs\VitalsNative\App'
New-Item -Path $destination -ItemType Directory -Force | Out-Null
Get-ChildItem -LiteralPath $source | Copy-Item -Destination $destination -Recurse -Force
$installedExe = Join-Path $destination 'Vitals.Windows.exe'
$shell = New-Object -ComObject WScript.Shell
$folders = @(
    [Environment]::GetFolderPath('Desktop'),
    (Join-Path ([Environment]::GetFolderPath('StartMenu')) 'Programs')
)
foreach ($folder in $folders) {
    if (-not (Test-Path -LiteralPath $folder)) { throw "Shortcut folder does not exist: $folder" }
    $shortcut = Join-Path $folder 'Health.lnk'
    if (Test-Path -LiteralPath $shortcut) {
        $existing = $shell.CreateShortcut($shortcut)
        if ($existing.TargetPath -ne $installedExe) {
            throw "An unrelated Health shortcut already exists: $shortcut"
        }
    }
    $link = $shell.CreateShortcut($shortcut)
    $link.TargetPath = $installedExe
    $link.WorkingDirectory = $destination
    $link.IconLocation = (Join-Path $destination 'Assets\Health.ico') + ',0'
    $link.Description = 'Health: native Google Health dashboard, inspired by Vitals Command Center.'
    $link.Save()
    $legacyShortcut = Join-Path $folder 'Vitals Native.lnk'
    if (Test-Path -LiteralPath $legacyShortcut) {
        $legacy = $shell.CreateShortcut($legacyShortcut)
        if ($legacy.TargetPath -eq $installedExe) {
            Remove-Item -LiteralPath $legacyShortcut
        }
    }
}
Write-Host "Installed Health to $destination"
Write-Host 'Launch Health from the desktop or Start menu. No web server is needed.'
