#requires -Version 5.1
param(
    [string]$Executable = (Join-Path $env:LOCALAPPDATA 'Programs\VitalsNative\App\Vitals.Windows.exe'),
    [switch]$InputChecks
)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
Add-Type @'
using System;
using System.ComponentModel;
using System.Runtime.InteropServices;
public static class VitalsWindowProbe
{
    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool SetWindowPos(IntPtr window, IntPtr after, int x, int y, int width, int height, uint flags);
    [DllImport("user32.dll")]
    public static extern uint GetDpiForWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern IntPtr SetThreadDpiAwarenessContext(IntPtr context);
    [DllImport("user32.dll")]
    public static extern bool SetForegroundWindow(IntPtr window);
    [DllImport("user32.dll")]
    public static extern bool GetCursorPos(out Point point);
    [DllImport("user32.dll")]
    public static extern bool SetCursorPos(int x, int y);
    [StructLayout(LayoutKind.Sequential)]
    private struct MouseInput
    {
        public int X, Y;
        public uint Data, Flags, Time;
        public UIntPtr Extra;
    }
    [StructLayout(LayoutKind.Explicit, Size = 40)]
    private struct Input
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public MouseInput Mouse;
    }
    [DllImport("user32.dll", SetLastError = true)]
    private static extern uint SendInput(uint count, Input[] inputs, int size);
    public static void MoveMouse(int x, int y)
    {
        if (!SetCursorPos(x, y)) throw new Win32Exception();
        var input = new Input { Mouse = new MouseInput { X = 2, Flags = 1 } };
        if (SendInput(1, new[] { input }, Marshal.SizeOf(typeof(Input))) != 1) throw new Win32Exception();
    }

    [StructLayout(LayoutKind.Sequential)]
    public struct Point { public int X, Y; }
    [StructLayout(LayoutKind.Sequential)]
    public struct PointerInfo
    {
        public uint Type, Id, Frame, Flags;
        public IntPtr Source, Target;
        public Point Pixel, Himetric, PixelRaw, HimetricRaw;
        public uint Time, History;
        public int Input;
        public uint Keys;
        public ulong Performance;
        public uint ButtonChange;
    }
    [StructLayout(LayoutKind.Explicit, Size = 152)]
    public struct PointerTypeInfo
    {
        [FieldOffset(0)] public uint Type;
        [FieldOffset(8)] public PointerInfo Info;
    }
    [DllImport("user32.dll", SetLastError = true)]
    private static extern IntPtr CreateSyntheticPointerDevice(uint type, uint count, uint feedback);
    [DllImport("user32.dll", SetLastError = true)]
    private static extern bool InjectSyntheticPointerInput(IntPtr device, [In] PointerTypeInfo[] info, uint count);
    [DllImport("user32.dll")]
    private static extern void DestroySyntheticPointerDevice(IntPtr device);

    public static void Tap(uint type, int x, int y)
    {
        if (IntPtr.Size != 8) throw new PlatformNotSupportedException("Input checks require a 64-bit test host.");
        IntPtr device = CreateSyntheticPointerDevice(type, 1, 3);
        if (device == IntPtr.Zero) throw new Win32Exception();
        try
        {
            var info = new PointerTypeInfo { Type = type };
            info.Info.Type = type;
            info.Info.Id = 1;
            info.Info.Pixel = new Point { X = x, Y = y };
            info.Info.Flags = 0x10000 | 0x2 | 0x4 | 0x10 | 0x2000 | 0x4000;
            if (!InjectSyntheticPointerInput(device, new[] { info }, 1)) throw new Win32Exception();
            System.Threading.Thread.Sleep(80);
            info.Info.Flags = 0x40000 | 0x2;
            if (!InjectSyntheticPointerInput(device, new[] { info }, 1)) throw new Win32Exception();
            System.Threading.Thread.Sleep(80);
        }
        finally { DestroySyntheticPointerDevice(device); }
    }
}
'@
[VitalsWindowProbe]::SetThreadDpiAwarenessContext([IntPtr](-4)) | Out-Null

function Get-Elements($Window) {
    $Window.FindAll(
        [System.Windows.Automation.TreeScope]::Descendants,
        [System.Windows.Automation.Condition]::TrueCondition)
}

function Send-AppPointer($Element, [uint32]$Type, [int]$ProcessId) {
    $rect = $Element.Current.BoundingRectangle
    $x = [int]($rect.Left + $rect.Width / 2)
    $y = [int]($rect.Top + $rect.Height / 2)
    $hit = [System.Windows.Automation.AutomationElement]::FromPoint((New-Object System.Windows.Point $x, $y))
    if ($hit.Current.ProcessId -ne $ProcessId) {
        throw "Input point ($x, $y) belongs to process $($hit.Current.ProcessId), not test process $ProcessId; refusing injection."
    }
    if ($Type -eq 4) {
        [VitalsWindowProbe]::MoveMouse($x, $y)
    } else {
        [VitalsWindowProbe]::Tap($Type, $x, $y)
    }
    Start-Sleep -Milliseconds 350
}

function Assert-TargetSize($Element, [double]$Scale, [double]$Expected) {
    $rect = $Element.Current.BoundingRectangle
    if ([Math]::Abs($rect.Width / $Scale - $Expected) -gt 1 -or
        [Math]::Abs($rect.Height / $Scale - $Expected) -gt 1) {
        throw "Expected a ${Expected}x${Expected} Settings target; got $($rect.Width / $Scale)x$($rect.Height / $Scale)."
    }
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
        if ($names -contains 'About Health' -or $names -contains 'Original Vitals Command Center by 8tp') {
            throw 'Attribution should be in Settings, not on the dashboard.'
        }
        if (-not $settings -or -not $settings.Current.IsEnabled) { throw 'Settings did not become available.' }
        if ($InputChecks -and $launch -eq 1) {
            if (-not [VitalsWindowProbe]::SetWindowPos($process.MainWindowHandle, [IntPtr](-1), 0, 0, 0, 0, 0x13)) {
                throw 'Cannot bring the test app above other windows for safe pointer injection.'
            }
            [VitalsWindowProbe]::SetForegroundWindow($process.MainWindowHandle) | Out-Null
            $settings.SetFocus()
        }
        $scroller = $elements | Where-Object {
            $_.GetCurrentPropertyValue([System.Windows.Automation.AutomationElement]::IsScrollPatternAvailableProperty)
        } | Select-Object -First 1
        if ($scroller) {
            $scroller.GetCurrentPattern([System.Windows.Automation.ScrollPattern]::Pattern).SetScrollPercent(-1, 0)
            Start-Sleep -Milliseconds 250
        }
        $scale = [VitalsWindowProbe]::GetDpiForWindow($process.MainWindowHandle) / 96.0
        if ($InputChecks -and $launch -eq 1) {
            $heading = $elements | Where-Object { $_.Current.Name -eq 'Your health, at a glance' } | Select-Object -First 1
            $cursor = New-Object VitalsWindowProbe+Point
            if (-not [VitalsWindowProbe]::GetCursorPos([ref]$cursor)) { throw 'Cannot preserve cursor position.' }
            try {
                Send-AppPointer $heading 4 $process.Id
                Assert-TargetSize $settings $scale 40
                Write-Host 'Mouse target: 40x40.'
                Send-AppPointer $heading 2 $process.Id
                Assert-TargetSize $settings $scale 48
                Write-Host 'Touch target: 48x48.'
                Send-AppPointer $heading 4 $process.Id
                Assert-TargetSize $settings $scale 40
                Write-Host 'Returned to mouse target: 40x40.'
                Send-AppPointer $heading 3 $process.Id
                Assert-TargetSize $settings $scale 48
                Write-Host 'Pen target: 48x48.'
            } finally {
                [VitalsWindowProbe]::SetCursorPos($cursor.X, $cursor.Y) | Out-Null
            }
        }
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
        $names = @($elements | ForEach-Object { $_.Current.Name })
        foreach ($name in @('About Health', 'Health source on GitHub', 'Original Vitals Command Center by 8tp')) {
            if ($names -notcontains $name) { throw "Settings attribution is missing: $name" }
        }
        if ($InputChecks -and $launch -eq 1) {
            $title = $elements | Where-Object {
                $_.Current.Name -eq 'Health settings' -and
                $_.Current.ControlType -eq [System.Windows.Automation.ControlType]::Text
            } | Select-Object -First 1
            if (-not $title) { throw 'Cannot find the Settings title for safe input checks.' }
            try {
                foreach ($case in @(@{Type=4;Size=40}, @{Type=2;Size=48}, @{Type=4;Size=40}, @{Type=3;Size=48})) {
                    Send-AppPointer $title $case.Type $process.Id
                    $height = $cancel.Current.BoundingRectangle.Height / $scale
                    if ([Math]::Abs($height - $case.Size) -gt 1) {
                        throw "Settings buttons did not adapt: expected $($case.Size), got $height."
                    }
                }
                Write-Host 'Settings dialog targets adapt to mouse, touch, and pen without saving changes.'
            } finally {
                [VitalsWindowProbe]::SetCursorPos($cursor.X, $cursor.Y) | Out-Null
            }
        }
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
