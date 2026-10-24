# Health

Windows ARM64 app: **WinUI 3/C# UI + Rust backend**, with no web server,
WebView, cloud AI, telemetry, or background service. The original web app
remains unchanged.

Health is an independent native adaptation inspired by
[Vitals Command Center by 8tp](https://github.com/8tp/Vitals-Command-Center).
The original web app and its [MIT license](../LICENSE), copyright 2026 8tp,
are retained in this fork; the license is also included with the installed app.
The WinUI interface and Rust backend are new native implementations, not the
original React/Node app wrapped in a desktop window.

## Build and install

Requires Windows 11, .NET 8 SDK, Rust stable with `rustfmt`/`clippy`, Visual
Studio 2022 C++ ARM64 tools, and a Windows 11 SDK. Dependencies are pinned in
`core/Cargo.lock` and the WinUI project.

```powershell
.\native\Build.ps1 -Install
```

This tests/lints Rust, builds a self-contained ARM64 application, and creates
**Health** desktop/Start menu shortcuts. Close the app to stop it;
there is no console window to maintain. See [UI details](windows/README.md).
The acrylic backdrop follows Windows transparency/accessibility settings.

Check the installed dashboard, Settings, and two clean launch/close cycles:

```powershell
powershell.exe -NoProfile -File .\native\Test-Launch.ps1
```

The publish/install checks require the app's PRI and compiled XAML resources,
not just its executable and Rust DLL. Window-initialization failures display
an error dialog and write local details to
`%LOCALAPPDATA%\VitalsNative\startup-error.log`.
Internal binary names and `VitalsNative` storage paths are retained for
compatibility; renaming the app does not move or reset saved data.

## Connect Google Health

1. In your Google Cloud project, enable **Google Health API**, configure the
   consent screen/read-only fitness, health-metrics, and sleep scopes, and
   add yourself as a test user if the app is in Testing.
2. Create a **Desktop app** OAuth client, not the web client's fixed-port
   callback used by the original Vitals web app.
3. Enter the client ID and its client secret, if issued, in native Settings.
   Select **Connect Google** and approve in your system browser.
4. Sync the selected date range. Missing metrics remain missing; permissions,
   unsupported metrics, and failures are reported rather than shown as zeroes.

The app uses PKCE, random OAuth state, and a temporary loopback listener.
There is no sign-in embedded in the app. Testing-mode refresh tokens may
expire and require reconnection.

## Scope and privacy

- Reads steps, resting HR, HRV, SpO2, respiratory rate, sleep/stages, and active
  energy. Uses Google's **reconciled google-sources stream** to avoid simply
  summing duplicate device sources.
- Android Health Connect has no direct Windows API. Only Health Connect data
  exposed through your Google Health account/API can appear here.
- OAuth tokens, configuration, and cached health data are encrypted with
  **Windows DPAPI for the current user** under `%LOCALAPPDATA%\VitalsNative`.
  Theme preference is stored separately. No health data is stored in the repo.
- Outbound app traffic is limited to Google's OAuth/token and Health API
  services. Your system browser has its own behavior.
- Disconnect clears local tokens/cache; revoke the app's grant separately in
  your Google account if desired.

Tests cover documented Google response shapes, date handling, pagination,
permission/rate-limit errors, PKCE/state validation, FFI responses, and DPAPI.
Real-account sign-in and device-data parity still require validation with
authorized Google Health credentials; a successful build is not that validation.
