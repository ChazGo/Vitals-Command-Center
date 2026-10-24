using System.Globalization;
using System.Text.Json;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Automation;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Media;

namespace Vitals.Windows;

public sealed partial class MainWindow : Window
{
    private HealthStatus _status = new();
    private bool _busy;
    private bool _dialogOpen;
    private bool _loaded;
    private bool _backendAvailable;
    private int _metricColumns;
    private readonly InputModeMonitor _inputMode;
    private ContentDialog? _activeDialog;

    public MainWindow()
    {
        InitializeComponent();
        ExtendsContentIntoTitleBar = true;
        SetTitleBar(TitleBarDragRegion);
        Root.ActualThemeChanged += Root_ActualThemeChanged;
        Root.RequestedTheme = ThemePreferences.Load();
        UpdateTitleBar();
        AppWindow.Resize(new global::Windows.Graphics.SizeInt32(1120, 900));
        _inputMode = new InputModeMonitor(this);
        _inputMode.Observe(Root);
        _inputMode.Changed += UpdateControlSizing;
        Closed += (_, _) => _inputMode.Dispose();
        UpdateControlSizing();
        RenderStatus();
    }

    private double TargetSize => _inputMode.TouchTargets ? 48 : 40;
    private Thickness ActionPadding => _inputMode.TouchTargets
        ? new Thickness(18, 10, 18, 10) : new Thickness(12, 6, 12, 6);

    private void SizeAction(ButtonBase button)
    {
        button.MinHeight = TargetSize;
        button.MinWidth = TargetSize;
        button.Padding = ActionPadding;
    }

    private void UpdateControlSizing()
    {
        foreach (var button in new[] { ConnectButton, SyncButton, SetupButton })
            SizeAction(button);
        SettingsButton.MinWidth = SettingsButton.MinHeight = TargetSize;
        SettingsButton.Width = SettingsButton.Height = TargetSize;
        SettingsGlyph.FontSize = _inputMode.TouchTargets ? 20 : 18;
        ConnectionActions.Spacing = _inputMode.TouchTargets ? 12 : 10;
        if (MessageBar.ActionButton is Button retry)
            SizeAction(retry);
        if (_activeDialog is not null)
            ApplyDialogSizing(_activeDialog);
    }

    private Style DialogButtonStyle(bool accent)
    {
        var style = new Style(typeof(Button))
        {
            BasedOn = (Style)Application.Current.Resources[accent ? "AccentButtonStyle" : "DefaultButtonStyle"]
        };
        style.Setters.Add(new Setter(Control.MinHeightProperty, TargetSize));
        style.Setters.Add(new Setter(Control.MinWidthProperty, TargetSize));
        style.Setters.Add(new Setter(Control.PaddingProperty, ActionPadding));
        return style;
    }

    private void ApplyDialogSizing(ContentDialog dialog)
    {
        dialog.PrimaryButtonStyle = DialogButtonStyle(true);
        dialog.SecondaryButtonStyle = DialogButtonStyle(false);
        dialog.CloseButtonStyle = DialogButtonStyle(false);
        ResizeDialogContent(dialog.Content as DependencyObject);
    }

    private void ResizeDialogContent(DependencyObject? element)
    {
        if (element is TextBox or PasswordBox or ComboBox)
        {
            ((Control)element).MinHeight = TargetSize;
            ((Control)element).Padding = _inputMode.TouchTargets
                ? new Thickness(12, 13, 12, 13) : new Thickness(12, 6, 12, 6);
            if (element is ComboBox combo)
            {
                var style = new Style(typeof(ComboBoxItem))
                {
                    BasedOn = (Style)Application.Current.Resources["DefaultComboBoxItemStyle"]
                };
                style.Setters.Add(new Setter(FrameworkElement.MinHeightProperty, TargetSize));
                combo.ItemContainerStyle = style;
            }
            return;
        }
        if (element is HyperlinkButton link)
        {
            SizeAction(link);
            return;
        }
        if (element is null)
            return;
        for (int index = 0; index < VisualTreeHelper.GetChildrenCount(element); index++)
            ResizeDialogContent(VisualTreeHelper.GetChild(element, index));
    }

    private async Task<ContentDialogResult> ShowDialogAsync(ContentDialog dialog)
    {
        _activeDialog = dialog;
        _inputMode.Observe(dialog);
        dialog.Loaded += (_, _) => ApplyDialogSizing(dialog);
        ApplyDialogSizing(dialog);
        try
        {
            return await dialog.ShowAsync();
        }
        finally
        {
            _inputMode.StopObserving(dialog);
            _activeDialog = null;
        }
    }

    private void Root_ActualThemeChanged(FrameworkElement sender, object args) => UpdateTitleBar();

    private void UpdateTitleBar()
    {
        var titleBar = AppWindow.TitleBar;
        titleBar.ButtonBackgroundColor = Microsoft.UI.Colors.Transparent;
        titleBar.ButtonInactiveBackgroundColor = Microsoft.UI.Colors.Transparent;
        if (TitleBarText.Foreground is Microsoft.UI.Xaml.Media.SolidColorBrush foreground)
            titleBar.ButtonForegroundColor = foreground.Color;
    }

    private async void Root_Loaded(object sender, RoutedEventArgs e)
    {
        if (_loaded)
            return;
        _loaded = true;
        await RunOperationAsync(new { operation = "status" }, "Reading locally saved health data…");
    }

    private async Task RunOperationAsync(object request, string activity, string? success = null)
    {
        if (_busy)
            return;

        _busy = true;
        UpdateControls();
        BusyProgress.Visibility = Visibility.Visible;
        BusyText.Text = activity;
        BusyText.Visibility = Visibility.Visible;
        MessageBar.IsOpen = false;
        try
        {
            _status = await Task.Run(() => CoreClient.Call(request));
            _backendAvailable = true;
            RenderStatus();
            if (success is not null)
                ShowMessage(InfoBarSeverity.Success, "Done", success);
        }
        catch (DllNotFoundException)
        {
            _backendAvailable = false;
            ShowBackendError("The ARM64 vitals_core.dll or one of its native dependencies is missing. Build the Rust core and place the DLL beside Vitals.Windows.exe, then retry.");
        }
        catch (BadImageFormatException)
        {
            _backendAvailable = false;
            ShowBackendError("The native health library cannot be loaded. Health and vitals_core.dll must both target Windows ARM64.");
        }
        catch (EntryPointNotFoundException)
        {
            _backendAvailable = false;
            ShowBackendError("The native health library is incompatible. Rebuild it with the vitals_call and vitals_free C exports.");
        }
        catch (JsonException)
        {
            ShowMessage(InfoBarSeverity.Error, "Invalid health response",
                "The native health service returned an unexpected response. Check that the frontend and core are from the same version.");
        }
        catch (InvalidOperationException ex)
        {
            ShowMessage(InfoBarSeverity.Error, "Couldn't complete the request", ex.Message);
        }
        catch (Exception)
        {
            // Do not include request payloads, credentials, or raw exception details in UI/logs.
            ShowMessage(InfoBarSeverity.Error, "Couldn't complete the request",
                "An unexpected local error occurred. Your last successfully loaded view has been kept. Try again or restart Health.");
        }
        finally
        {
            _busy = false;
            BusyProgress.Visibility = Visibility.Collapsed;
            BusyText.Visibility = Visibility.Collapsed;
            UpdateControls();
        }
    }

    private void ShowMessage(InfoBarSeverity severity, string title, string message)
    {
        MessageBar.ActionButton = null;
        MessageBar.Severity = severity;
        MessageBar.Title = title;
        MessageBar.Message = message;
        MessageBar.IsOpen = true;
        MessageBar.Visibility = Visibility.Visible;
    }

    private void InfoBar_Closed(InfoBar sender, InfoBarClosedEventArgs args)
    {
        if (!sender.IsOpen)
            sender.Visibility = Visibility.Collapsed;
    }

    private void ShowBackendError(string message)
    {
        ShowMessage(InfoBarSeverity.Error, "Native health service unavailable", message);
        ConnectionText.Text = "Google Health status unavailable";
        var retry = new Button { Content = "Retry local status" };
        SizeAction(retry);
        retry.Click += async (_, _) =>
        {
            if (!_dialogOpen)
                await RunOperationAsync(new { operation = "status" }, "Reading locally saved health data…");
        };
        MessageBar.ActionButton = retry;
    }

    private void UpdateControls()
    {
        bool idle = !_busy && !_dialogOpen;
        SettingsButton.IsEnabled = idle;
        SetupButton.IsEnabled = idle;
        ConnectButton.IsEnabled = idle && _backendAvailable && _status.Configured && !_status.Connected;
        SyncButton.IsEnabled = idle && _backendAvailable && _status.Connected;
        if (MessageBar.ActionButton is Button retry)
            retry.IsEnabled = idle;
    }

    private async void Connect_Click(object sender, RoutedEventArgs e)
    {
        if (_dialogOpen)
            return;
        await RunOperationAsync(new { operation = "connect" },
            "Complete Google sign-in in your browser. Health is waiting for authorization (up to 3 minutes).",
            "Google Health is connected. Choose Sync to fetch your available health data.");
    }

    private async void Sync_Click(object sender, RoutedEventArgs e)
    {
        if (_dialogOpen)
            return;
        await RunOperationAsync(new { operation = "sync", days = 30 },
            "Syncing the last 30 days from Google Health. This may take a few minutes.",
            "The sync request completed. See data notes for any limitations.");
    }

    private void RenderStatus()
    {
        ConnectionText.Text = _status.Connected ? "Google Health · Connected"
            : _status.Configured ? "Google Health · Not connected" : "Google Health · Setup needed";
        LastSyncText.Text = _status.LastSync is null ? "No successful sync yet."
            : DateTimeOffset.TryParse(_status.LastSync, CultureInfo.InvariantCulture,
                DateTimeStyles.AssumeUniversal, out var lastSync)
                ? $"Last synced {lastSync.ToLocalTime():g}"
                : $"Last synced {_status.LastSync}";
        WelcomePanel.Visibility = _status.Connected ? Visibility.Collapsed : Visibility.Visible;
        WarningBar.Message = string.Join(Environment.NewLine, _status.Warnings);
        WarningBar.IsOpen = _status.Warnings.Count > 0;
        WarningBar.Visibility = WarningBar.IsOpen ? Visibility.Visible : Visibility.Collapsed;

        var days = _status.Days.OrderByDescending(day => day.Date, StringComparer.Ordinal).ToList();
        HealthDay? latest = days.FirstOrDefault(day => day.HasData);
        SnapshotDate.Text = latest is null ? "No health data available. Connect and sync to get started."
            : $"Latest day with available data · {FormatDate(latest.Date)}";

        MetricsGrid.Children.Clear();
        AddMetric("Steps", Value(latest?.Steps, "N0"), "steps", "\uE805");
        AddMetric("Resting heart rate", Value(latest?.RestingHeartRate, "0.#"), "bpm", "\uE95E");
        AddMetric("Heart rate variability", Value(latest?.Hrv, "0.#"), "ms", "\uE9D9");
        AddMetric("Sleep", Value(latest?.SleepHours, "0.##"), "hours", "\uE708");
        AddMetric("Blood oxygen", Value(latest?.Spo2, "0.#"), "% SpO₂", "\uE81C");
        AddMetric("Respiratory rate", Value(latest?.RespiratoryRate, "0.#"), "breaths/min", "\uE9D9");
        AddMetric("Active calories", Value(latest?.ActiveCalories, "N0"), "kcal", "\uECAD");
        LayoutMetrics();

        SleepDetailText.Text = $"Deep: {Value(latest?.DeepHours, "0.##")} h   ·   REM: {Value(latest?.RemHours, "0.##")} h   ·   Light: {Value(latest?.LightHours, "0.##")} h";
        HistoryList.ItemsSource = days.Take(30).Select(day => new HistoryRow(
            FormatDate(day.Date),
            $"Steps: {Value(day.Steps, "N0")}   ·   Active: {Value(day.ActiveCalories, "N0")} kcal",
            $"Sleep: {Value(day.SleepHours, "0.##")} h   ·   Resting HR: {Value(day.RestingHeartRate, "0.#")} bpm   ·   HRV: {Value(day.Hrv, "0.#")} ms",
            $"SpO₂: {Value(day.Spo2, "0.#")}%   ·   Respiratory rate: {Value(day.RespiratoryRate, "0.#")} breaths/min")).ToList();
        HistoryEmptyText.Visibility = days.Count == 0 ? Visibility.Visible : Visibility.Collapsed;
        UpdateControls();
    }

    private static string Value(double? number, string format) =>
        number.HasValue && double.IsFinite(number.Value)
            ? number.Value.ToString(format, CultureInfo.CurrentCulture) : "—";

    private static string FormatDate(string value) =>
        DateOnly.TryParseExact(value, "yyyy-MM-dd", CultureInfo.InvariantCulture,
            DateTimeStyles.None, out var date) ? date.ToString("ddd, d MMM yyyy", CultureInfo.CurrentCulture) : value;

    private void AddMetric(string label, string value, string unit, string glyph)
    {
        var card = new ContentControl
        {
            Content = new MetricRow(label, value, unit, glyph),
            ContentTemplate = (DataTemplate)Root.Resources["MetricCardTemplate"],
            HorizontalContentAlignment = HorizontalAlignment.Stretch,
            IsTabStop = false
        };
        AutomationProperties.SetName(card, $"{label}: {value} {unit}");
        MetricsGrid.Children.Add(card);
    }

    private void Metrics_SizeChanged(object sender, SizeChangedEventArgs e)
    {
        int columns = ColumnCount(e.NewSize.Width);
        if (columns != _metricColumns)
            LayoutMetrics();
    }

    private static int ColumnCount(double width) => width >= 1050 ? 4 : width >= 760 ? 3 : width >= 490 ? 2 : 1;

    private void LayoutMetrics()
    {
        _metricColumns = ColumnCount(MetricsGrid.ActualWidth);
        MetricsGrid.ColumnDefinitions.Clear();
        MetricsGrid.RowDefinitions.Clear();
        for (int column = 0; column < _metricColumns; column++)
            MetricsGrid.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        for (int row = 0; row < (MetricsGrid.Children.Count + _metricColumns - 1) / _metricColumns; row++)
            MetricsGrid.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        for (int index = 0; index < MetricsGrid.Children.Count; index++)
        {
            Grid.SetRow((FrameworkElement)MetricsGrid.Children[index], index / _metricColumns);
            Grid.SetColumn((FrameworkElement)MetricsGrid.Children[index], index % _metricColumns);
        }
    }

    private async void Settings_Click(object sender, RoutedEventArgs e)
    {
        if (_busy || _dialogOpen)
            return;
        _dialogOpen = true;
        UpdateControls();
        ElementTheme oldTheme = Root.RequestedTheme;
        var clientId = new TextBox
        {
            Header = "Desktop OAuth client ID", Text = _status.ClientId,
            PlaceholderText = "…apps.googleusercontent.com",
            TextWrapping = TextWrapping.Wrap
        };
        var clientSecret = new PasswordBox
        {
            Header = "Client secret (optional)",
            PasswordRevealMode = PasswordRevealMode.Hidden
        };
        var appearance = new ComboBox { Header = "Appearance", HorizontalAlignment = HorizontalAlignment.Stretch };
        appearance.Items.Add("Use system theme");
        appearance.Items.Add("Light");
        appearance.Items.Add("Dark");
        appearance.SelectedIndex = oldTheme switch { ElementTheme.Light => 1, ElementTheme.Dark => 2, _ => 0 };
        var validation = new InfoBar { Severity = InfoBarSeverity.Error, IsClosable = false };
        var content = new StackPanel { Spacing = 14, MaxWidth = 540 };
        content.Children.Add(new TextBlock
        {
            Text = "Use your own Google Cloud project: enable the Google Health API, configure OAuth consent, add your account as a test user when the app is in testing, and create an OAuth client with application type Desktop app. This cloud setup is required; Health cannot create it for you.",
            TextWrapping = TextWrapping.Wrap
        });
        content.Children.Add(clientId);
        content.Children.Add(clientSecret);
        content.Children.Add(new TextBlock
        {
            Text = "Leave the secret blank to preserve the saved secret for the same client ID. Changing the client ID clears saved tokens and cached health data; connect again afterward. Credentials are handled locally by the Rust core; tokens use Windows DPAPI.",
            TextWrapping = TextWrapping.Wrap
        });
        content.Children.Add(new TextBlock
        {
            Text = "Google Health cloud data is not the same as all Android Health Connect data. Missing metrics stay unavailable. Health reads cached data at launch and contacts Google only when you connect or sync.",
            TextWrapping = TextWrapping.Wrap
        });
        content.Children.Add(appearance);
        content.Children.Add(validation);
        content.Children.Add(new TextBlock { Text = "About Health", Style = (Style)Application.Current.Resources["SubtitleTextBlockStyle"] });
        content.Children.Add(new TextBlock
        {
            Text = "An independent native Windows adaptation inspired by Vitals Command Center by 8tp. Built with WinUI 3 and Rust. Original copyright 2026 8tp and MIT license retained; the full license is included with the app.",
            TextWrapping = TextWrapping.Wrap
        });
        content.Children.Add(new HyperlinkButton
        {
            Content = new TextBlock { Text = "Health source on GitHub", TextWrapping = TextWrapping.Wrap },
            NavigateUri = new Uri("https://github.com/ChazGo/Vitals-Command-Center/tree/native/windows-rust-winui3"),
            HorizontalAlignment = HorizontalAlignment.Left
        });
        content.Children.Add(new HyperlinkButton
        {
            Content = new TextBlock { Text = "Original Vitals Command Center by 8tp", TextWrapping = TextWrapping.Wrap },
            NavigateUri = new Uri("https://github.com/8tp/Vitals-Command-Center"),
            HorizontalAlignment = HorizontalAlignment.Left
        });
        var dialog = new ContentDialog
        {
            XamlRoot = Root.XamlRoot, RequestedTheme = oldTheme, Title = "Health settings",
            Content = new ScrollViewer
            {
                Content = content, VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
                HorizontalScrollBarVisibility = ScrollBarVisibility.Disabled
            },
            PrimaryButtonText = "Save", CloseButtonText = "Cancel",
            SecondaryButtonText = "Disconnect & clear",
            IsSecondaryButtonEnabled = _backendAvailable && (_status.Connected || _status.Days.Count > 0),
            DefaultButton = ContentDialogButton.Primary
        };
        appearance.SelectionChanged += (_, _) =>
        {
            Root.RequestedTheme = appearance.SelectedIndex switch
            {
                1 => ElementTheme.Light, 2 => ElementTheme.Dark, _ => ElementTheme.Default
            };
            dialog.RequestedTheme = Root.RequestedTheme;
        };
        dialog.PrimaryButtonClick += (_, args) =>
        {
            string id = clientId.Text.Trim();
            if ((id.Length > 0 && !id.EndsWith(".apps.googleusercontent.com", StringComparison.Ordinal))
                || (id.Length == 0 && (_status.Configured || clientSecret.Password.Length > 0)))
            {
                validation.Message = "Enter a Google Desktop OAuth client ID ending in .apps.googleusercontent.com.";
                validation.IsOpen = true;
                args.Cancel = true;
            }
        };
        ContentDialogResult result;
        try
        {
            result = await ShowDialogAsync(dialog);
        }
        finally
        {
            _dialogOpen = false;
            UpdateControls();
        }
        string secret = result == ContentDialogResult.Primary ? clientSecret.Password : "";
        clientSecret.Password = "";
        if (result != ContentDialogResult.Primary)
            Root.RequestedTheme = oldTheme;

        if (result == ContentDialogResult.Secondary)
        {
            await ConfirmDisconnectAsync();
            return;
        }
        if (result != ContentDialogResult.Primary)
            return;
        string idToSave = clientId.Text.Trim();
        if (idToSave.Length > 0 && !string.IsNullOrWhiteSpace(_status.ClientId)
            && !string.Equals(idToSave, _status.ClientId, StringComparison.Ordinal)
            && !await ConfirmClientChangeAsync())
        {
            Root.RequestedTheme = oldTheme;
            return;
        }
        if (idToSave.Length > 0)
            await RunOperationAsync(new { operation = "configure", clientId = idToSave, clientSecret = secret },
                "Saving Google Health configuration…", "Google Health configuration saved.");
        try
        {
            ThemePreferences.Save(Root.RequestedTheme);
        }
        catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
        {
            ShowMessage(InfoBarSeverity.Warning, "Appearance not saved",
                "The theme applies for this session, but Health could not save it in your local app data folder.");
        }
    }

    private async Task<bool> ConfirmClientChangeAsync()
    {
        _dialogOpen = true;
        UpdateControls();
        var confirmation = new ContentDialog
        {
            XamlRoot = Root.XamlRoot, RequestedTheme = Root.RequestedTheme,
            Title = "Change Google OAuth client?",
            Content = "Saving a different client ID clears saved Google tokens and locally cached health data. You will need to connect again. A blank secret will not preserve the previous client's secret. This does not revoke authorization at Google.",
            PrimaryButtonText = "Change client & clear", CloseButtonText = "Cancel",
            DefaultButton = ContentDialogButton.Close
        };
        try
        {
            return await ShowDialogAsync(confirmation) == ContentDialogResult.Primary;
        }
        finally
        {
            _dialogOpen = false;
            UpdateControls();
        }
    }

    private async Task ConfirmDisconnectAsync()
    {
        _dialogOpen = true;
        UpdateControls();
        var confirmation = new ContentDialog
        {
            XamlRoot = Root.XamlRoot, RequestedTheme = Root.RequestedTheme,
            Title = "Disconnect and clear local health data?",
            Content = "This removes locally cached health data and saved Google tokens from Health. It does not revoke authorization at Google. To revoke access, remove your OAuth application from your Google Account's third-party connections.",
            PrimaryButtonText = "Disconnect & clear", CloseButtonText = "Keep connection",
            DefaultButton = ContentDialogButton.Close
        };
        ContentDialogResult result;
        try
        {
            result = await ShowDialogAsync(confirmation);
        }
        finally
        {
            _dialogOpen = false;
            UpdateControls();
        }
        if (result == ContentDialogResult.Primary)
            await RunOperationAsync(new { operation = "disconnect" }, "Clearing local health data and tokens…",
                "Disconnected and cleared local health data. Google authorization must be revoked separately in your Google Account.");
    }

    public sealed record HistoryRow(string Date, string Activity, string Recovery, string Other);
    public sealed record MetricRow(string Label, string Value, string Unit, string Glyph);
}
