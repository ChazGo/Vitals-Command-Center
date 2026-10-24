param([switch]$Install)

$ErrorActionPreference = 'Stop'
$root = $PSScriptRoot
$cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$dotnetCommand = Get-Command dotnet -ErrorAction SilentlyContinue
$dotnet = if ($dotnetCommand) { $dotnetCommand.Source } else {
    Join-Path $env:LOCALAPPDATA 'Programs\VitalsNative\dotnet\dotnet.exe'
}
$installer = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer'
$vswhere = Join-Path $installer 'vswhere.exe'
foreach ($tool in @($cargo, $dotnet, $vswhere)) {
    if (-not (Test-Path -LiteralPath $tool)) { throw "Required build tool is missing: $tool" }
}
$visualStudio = & $vswhere -latest -products '*' -requires Microsoft.VisualStudio.Component.VC.Tools.ARM64 -property installationPath
if (-not $visualStudio) { throw 'Install Visual Studio C++ ARM64 build tools and a Windows 11 SDK.' }
$vcvars = Join-Path $visualStudio 'VC\Auxiliary\Build\vcvarsall.bat'
$env:PATH = $installer + ';' + (Split-Path $cargo) + ';' + $env:PATH
$env:DOTNET_CLI_TELEMETRY_OPTOUT = '1'
$env:DOTNET_NOLOGO = '1'

Push-Location (Join-Path $root 'core')
try {
    & $cargo fmt -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }
    $commands = 'call "{0}" arm64 >nul && "{1}" test --locked --target aarch64-pc-windows-msvc && "{1}" clippy --locked --all-targets --target aarch64-pc-windows-msvc -- -D warnings && "{1}" build --locked --release --target aarch64-pc-windows-msvc' -f $vcvars, $cargo
    & $env:ComSpec /c $commands
    if ($LASTEXITCODE -ne 0) { throw 'Rust validation/build failed.' }
} finally {
    Pop-Location
}

$output = Join-Path $root 'artifacts\win-arm64'
& $dotnet publish (Join-Path $root 'windows\Vitals.Windows.csproj') -c Release -p:Platform=ARM64 -r win-arm64 --self-contained true -o $output --nologo -v:minimal
if ($LASTEXITCODE -ne 0) { throw 'WinUI publish failed.' }
foreach ($file in @('vitals_core.dll', 'Vitals.Windows.pri', 'App.xbf', 'MainWindow.xbf', 'LICENSE.txt', 'Assets\Health.ico', 'Assets\Health.png')) {
    if (-not (Test-Path -LiteralPath (Join-Path $output $file))) {
        throw "The publish output is missing a required runtime file: $file"
    }
}
Write-Host "ARM64 application published to $output"

if ($Install) { & (Join-Path $root 'Install.ps1') }
