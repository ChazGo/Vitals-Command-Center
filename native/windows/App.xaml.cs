using Microsoft.UI.Xaml;
using System.Runtime.InteropServices;

namespace Vitals.Windows;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        try
        {
            InitializeComponent();
        }
        catch (Exception ex)
        {
            ReportStartupFailure(ex);
            throw;
        }
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        try
        {
            _window = new MainWindow();
            _window.Activate();
        }
        catch (Exception ex)
        {
            ReportStartupFailure(ex);
            throw;
        }
    }

    private static void ReportStartupFailure(Exception exception)
    {
        string message = $"Health could not open its window.\n\n{exception.Message}";
        string directory = Path.Combine(
            Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData), "VitalsNative");
        string path = Path.Combine(directory, "startup-error.log");
        try
        {
            Directory.CreateDirectory(directory);
            string details = string.Join("\n", exception.Data.Keys.Cast<object>()
                .Where(key => key is string name && name is "Description" or "RestrictedDescription")
                .Select(key => $"{key}: {exception.Data[key]}"));
            File.WriteAllText(path, $"{DateTimeOffset.Now:O}\n{exception}\n{details}");
            message += $"\n\nStartup details were saved to:\n{path}";
        }
        catch (Exception logException) when (logException is IOException or UnauthorizedAccessException)
        {
            message += "\n\nStartup details could not be saved.";
        }
        MessageBoxW(0, message, "Health startup error", 0x10);
    }

    [DllImport("user32.dll", CharSet = CharSet.Unicode, ExactSpelling = true)]
    private static extern int MessageBoxW(nint window, string text, string caption, uint type);
}
