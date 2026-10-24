$ErrorActionPreference = 'Stop'
$source = Join-Path $PSScriptRoot 'artifacts\win-arm64'
$exe = Join-Path $source 'Vitals.Windows.exe'
if (-not (Test-Path -LiteralPath $exe)) { throw 'Run native\Build.ps1 first.' }
if (-not (Test-Path -LiteralPath (Join-Path $source 'vitals_core.dll'))) {
    throw 'The Rust backend is missing. Rebuild before installing.'
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
    $link = $shell.CreateShortcut((Join-Path $folder 'Vitals Native.lnk'))
    $link.TargetPath = $installedExe
    $link.WorkingDirectory = $destination
    $link.Description = 'Native WinUI 3 Google Health dashboard with a local Rust backend.'
    $link.Save()
}
Write-Host "Installed Vitals Native to $destination"
Write-Host 'Launch Vitals Native from the desktop or Start menu. No web server is needed.'
