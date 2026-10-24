#requires -Version 5.1
param(
    [string]$Executable = (Join-Path $env:LOCALAPPDATA 'Programs\VitalsNative\App\Vitals.Windows.exe')
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class VitalsWindowProbe
{
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
}
'@

function Get-Elements($Window) {
    $Window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
}

for ($launch = 1; $launch -le 2; $launch++) {
    $process = Start-Process -FilePath $Executable -WorkingDirectory $env:TEMP -PassThru
    try {
        $deadline = (Get-Date).AddSeconds(20)
        do {
            Start-Sleep -Milliseconds 250
            $process.Refresh()
            if ($process.HasExited) { throw "App exited before showing a window: $($process.ExitCode)" }
        } while ($process.MainWindowHandle -eq 0 -and (Get-Date) -lt $deadline)

        if ($process.MainWindowTitle -ne 'Health' -or
            -not [VitalsWindowProbe]::IsWindowVisible($process.MainWindowHandle) -or
            -not $process.Responding) {
            throw "Expected a visible responsive dashboard; got '$($process.MainWindowTitle)'."
        }
        $window = [System.Windows.Automation.AutomationElement]::FromHandle($process.MainWindowHandle)
        if ($launch -eq 2) {
            if (-not [VitalsWindowProbe]::SetWindowPos($process.MainWindowHandle, [IntPtr]::Zero, 0, 0, 600, 900, 6)) {
                throw 'Could not resize the app for the portrait-width check.'
            }
        }
        do {
            $elements = Get-Elements $window
            $settings = $elements | Where-Object { $_.Current.Name -eq 'Open Health settings' } | Select-Object -First 1
            $names = @($elements | ForEach-Object { $_.Current.Name })
            if ($settings -and $settings.Current.IsEnabled -and $names -contains 'Steps') { break }
            Start-Sleep -Milliseconds 250
        } while ((Get-Date) -lt $deadline)
        foreach ($name in @('Daily snapshot', 'Steps', 'Recent days')) {
            if ($names -notcontains $name) { throw "Dashboard control did not render: $name" }
        }
        if (-not $settings -or -not $settings.Current.IsEnabled) { throw 'Settings did not become available.' }
        $bounds = $window.Current.BoundingRectangle
        foreach ($element in ($elements | Where-Object { $_.Current.Name -in @('Connect Google', 'Sync last 30 days', 'Open Health settings') })) {
            $control = $element.Current.BoundingRectangle
            if ($control.Left -lt $bounds.Left -or $control.Right -gt $bounds.Right) {
                throw "Control overflows the window: $($element.Current.Name)"
            }
        }
        $settings.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        $deadline = (Get-Date).AddSeconds(10)
        do {
            Start-Sleep -Milliseconds 250
            $elements = Get-Elements $window
            $cancel = $elements | Where-Object { $_.Current.Name -eq 'Cancel' } | Select-Object -First 1
        } while (-not $cancel -and (Get-Date) -lt $deadline)
        if (-not $cancel) { throw 'Settings dialog did not open.' }
        $cancel.GetCurrentPattern([System.Windows.Automation.InvokePattern]::Pattern).Invoke()
        Start-Sleep -Milliseconds 250
        if (-not $process.CloseMainWindow() -or -not $process.WaitForExit(10000)) {
            throw 'App did not close normally.'
        }
        if ($process.ExitCode -ne 0) { throw "App closed with exit code $($process.ExitCode)." }
        Write-Host "Launch ${launch}: visible dashboard, rendered metrics, settings open/cancel, and clean exit verified."
    }
    finally {
        if (-not $process.HasExited) { Stop-Process -Id $process.Id }
        $process.Dispose()
    }
}
