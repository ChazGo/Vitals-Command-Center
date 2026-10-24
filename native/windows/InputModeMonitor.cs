using System.Runtime.InteropServices;
using Microsoft.UI.Dispatching;
using Microsoft.UI.Input;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Input;

namespace Vitals.Windows;

internal sealed class InputModeMonitor : IDisposable
{
    private readonly nint _window;
    private readonly DispatcherQueue _dispatcher;
    private readonly SubclassProcedure _procedure;
    private readonly PointerEventHandler _pressed;
    private readonly PointerEventHandler _finished;
    private readonly PointerEventHandler _captureLost;
    private readonly PointerEventHandler _moved;
    private readonly HashSet<uint> _contacts = [];
    private readonly HashSet<FrameworkElement> _roots = [];
    private bool _attached;
    private bool _tabletPosture;

    public bool TouchTargets { get; private set; }
    public event Action? Changed;

    public InputModeMonitor(Window window)
    {
        _window = WinRT.Interop.WindowNative.GetWindowHandle(window);
        _dispatcher = window.DispatcherQueue;
        _procedure = WindowProcedure;
        _pressed = PointerPressed;
        _finished = PointerFinished;
        _captureLost = PointerCaptureLost;
        _moved = PointerMoved;
        _tabletPosture = ReadTabletPosture();
        TouchTargets = _tabletPosture;
        if (!SetWindowSubclass(_window, _procedure, 1, 0))
            throw new InvalidOperationException("Cannot monitor Windows tablet posture changes.");
        _attached = true;
    }

    public void Observe(FrameworkElement root)
    {
        if (!_roots.Add(root))
            return;
        root.AddHandler(UIElement.PointerPressedEvent, _pressed, true);
        root.AddHandler(UIElement.PointerReleasedEvent, _finished, true);
        root.AddHandler(UIElement.PointerCanceledEvent, _finished, true);
        root.AddHandler(UIElement.PointerCaptureLostEvent, _captureLost, true);
        root.AddHandler(UIElement.PointerMovedEvent, _moved, true);
    }

    public void StopObserving(FrameworkElement root)
    {
        if (!_roots.Remove(root))
            return;
        root.RemoveHandler(UIElement.PointerPressedEvent, _pressed);
        root.RemoveHandler(UIElement.PointerReleasedEvent, _finished);
        root.RemoveHandler(UIElement.PointerCanceledEvent, _finished);
        root.RemoveHandler(UIElement.PointerCaptureLostEvent, _captureLost);
        root.RemoveHandler(UIElement.PointerMovedEvent, _moved);
    }

    private void PointerPressed(object sender, PointerRoutedEventArgs args) => _contacts.Add(args.Pointer.PointerId);

    private void PointerFinished(object sender, PointerRoutedEventArgs args)
    {
        _contacts.Remove(args.Pointer.PointerId);
        bool touch = args.Pointer.PointerDeviceType is PointerDeviceType.Touch or PointerDeviceType.Pen;
        // Apply after the tap/click finishes, not while a target is under a finger.
        _dispatcher.TryEnqueue(() =>
        {
            if (_attached && _contacts.Count == 0)
                SetTouchTargets(touch);
        });
    }

    private void PointerCaptureLost(object sender, PointerRoutedEventArgs args)
    {
        _contacts.Remove(args.Pointer.PointerId);
        if (!args.Pointer.IsInContact)
            PointerFinished(sender, args);
    }

    private void PointerMoved(object sender, PointerRoutedEventArgs args)
    {
        if (_attached && !args.Pointer.IsInContact && _contacts.Count == 0)
            SetTouchTargets(args.Pointer.PointerDeviceType != PointerDeviceType.Mouse);
    }

    private void SetTouchTargets(bool touch)
    {
        if (TouchTargets == touch)
            return;
        TouchTargets = touch;
        Changed?.Invoke();
    }

    private static bool ReadTabletPosture()
    {
        // Slate mode is undefined on desktops; require an integrated digitizer and rotation support.
        return (GetSystemMetrics(94) & 1) != 0
            && GetAutoRotationState(out uint rotation)
            && (rotation & (0x10 | 0x20)) == 0
            && GetSystemMetrics(0x2003) == 0;
    }

    private nint WindowProcedure(nint window, uint message, nuint wParam, nint lParam, nuint id, nuint data)
    {
        if (message == 0x001A && lParam != 0
            && Marshal.PtrToStringUni(lParam) == "ConvertibleSlateMode")
        {
            _dispatcher.TryEnqueue(() =>
            {
                if (!_attached)
                    return;
                bool tablet = ReadTabletPosture();
                if (tablet != _tabletPosture)
                {
                    _tabletPosture = tablet;
                    if (_contacts.Count == 0)
                        SetTouchTargets(tablet);
                }
            });
        }
        return DefSubclassProc(window, message, wParam, lParam);
    }

    public void Dispose()
    {
        foreach (var root in _roots.ToArray())
            StopObserving(root);
        if (_attached)
        {
            if (!RemoveWindowSubclass(_window, _procedure, 1))
                throw new InvalidOperationException("Cannot remove the Windows tablet posture monitor.");
            _attached = false;
        }
    }

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    private delegate nint SubclassProcedure(nint window, uint message, nuint wParam, nint lParam, nuint id, nuint data);

    [DllImport("comctl32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool SetWindowSubclass(nint window, SubclassProcedure procedure, nuint id, nuint data);

    [DllImport("comctl32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool RemoveWindowSubclass(nint window, SubclassProcedure procedure, nuint id);

    [DllImport("comctl32.dll", ExactSpelling = true)]
    private static extern nint DefSubclassProc(nint window, uint message, nuint wParam, nint lParam);

    [DllImport("user32.dll", ExactSpelling = true)]
    private static extern int GetSystemMetrics(int index);

    [DllImport("user32.dll", ExactSpelling = true)]
    [return: MarshalAs(UnmanagedType.Bool)]
    private static extern bool GetAutoRotationState(out uint state);
}
