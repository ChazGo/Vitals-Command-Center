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
Original Health artwork is embedded in the executable and used for window,
taskbar, and shortcut icons. To regenerate the 16-256px ICO and title-bar PNG,
run `powershell.exe -NoProfile -File .\native\windows\Assets\Generate-Icon.ps1`.

Check the installed dashboard, Settings, and two clean launch/close cycles:

```powershell
powershell.exe -NoProfile -File .\native\Test-Launch.ps1
```

Add `-InputChecks` to exercise real Windows mouse/touch/pen injection against
the test window. This temporarily brings it above other windows and moves the
pointer, restoring the cursor afterward.

Controls adapt live: compact 40-DIP main actions for mouse use and larger
48-DIP targets for touch/pen or Windows tablet posture. Settings uses a square
icon target instead of a wide button. Sizing changes follow completed taps,
pen hover, mouse movement, and Windows keyboard/posture notifications; there
is no device-wide input interception. Native scrolling and pen input remain
enabled. Attribution and links to this fork and the original project are in
**Settings > About Health**.

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

### Shared sign-in direction

The intended public flow is **Connect Google Health → read-only data disclosure
→ Google consent → linked-profile check → initial sync**, using one app-owned
Desktop OAuth client. Users authorize their own accounts; tokens/data remain
on their own PCs. Custom client setup would move to Advanced settings.
No shared registration is configured yet.

Before releasing that flow, register the app, publish its homepage/privacy
policy, handle partial consent, and check the Google Health identity endpoint
before declaring a connection successful. Testing is limited to 100 users.
Google Health documents verification and annual CASA assessment requirements;
general OAuth guidance ties assessments to server-side access. Confirm how
Google applies these rules to this local-only app rather than assuming an
exemption. See [Health verification](https://developers.google.com/health/app-verification),
[desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app),
and [Health setup](https://developers.google.com/health/setup).

This is Google Health cloud access, not direct access to Android's on-device
Health Connect database. A full Health Connect bridge would need an Android
companion; a shared OAuth client alone cannot provide that.

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
