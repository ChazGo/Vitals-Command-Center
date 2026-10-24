using System.Text.Json;
using Microsoft.UI.Xaml;

namespace Vitals.Windows;

internal static class ThemePreferences
{
    private static string FilePath => Path.Combine(
        Environment.GetFolderPath(Environment.SpecialFolder.LocalApplicationData),
        "VitalsNative", "appearance.json");

    internal static ElementTheme Load()
    {
        try
        {
            var preference = JsonSerializer.Deserialize<Preference>(File.ReadAllText(FilePath));
            return Enum.TryParse<ElementTheme>(preference?.Theme, out var theme) && Enum.IsDefined(theme)
                ? theme : ElementTheme.Default;
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException or JsonException)
        {
            return ElementTheme.Default;
        }
    }

    internal static void Save(ElementTheme theme)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(FilePath)!);
        File.WriteAllText(FilePath, JsonSerializer.Serialize(new Preference { Theme = theme.ToString() }));
    }

    private sealed class Preference
    {
        public Preference() { }

        public string Theme { get; set; } = "Default";
    }
}
