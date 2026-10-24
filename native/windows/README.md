# Vitals for Windows ARM64

A native WinUI 3 / C# dashboard. No WebView, Electron, hosted frontend, AI, or
telemetry. Google Health is the only integration. The upstream repository's
MIT license applies; see `..\..\LICENSE`.

## Frontend prerequisites

- Windows 10 version 2004 (build 19041) or newer; Windows 11 ARM64 recommended.
- .NET 8 SDK and Visual Studio 2022 Build Tools (17.8 or newer), including
  MSBuild, C++ ARM64 build tools and a Windows 10/11 SDK. WinUI's XAML compiler
  also needs the Visual Studio build environment; a .NET SDK alone is not enough.
- NuGet access for `Microsoft.WindowsAppSDK` **1.8.260921001** (stable 1.8)
  and `Microsoft.Windows.SDK.BuildTools` **10.0.26100.9169**.
- A matching ARM64 `vitals_core.dll` exposing the agreed C ABI. Building the
  Rust core is outside this frontend's scope.

## Build and publish

From the repository root, in a Visual Studio Developer PowerShell with the
ARM64 toolchain available:

```powershell
& "$env:LOCALAPPDATA\Programs\VitalsNative\dotnet\dotnet.exe" build .\native\windows\Vitals.Windows.csproj -c Release -p:Platform=ARM64 -r win-arm64
& "$env:LOCALAPPDATA\Programs\VitalsNative\dotnet\dotnet.exe" publish .\native\windows\Vitals.Windows.csproj -c Release -p:Platform=ARM64 -r win-arm64 --self-contained true -o .\native\windows\artifacts\publish
```

If using a different .NET 8 SDK installation, substitute its `dotnet.exe`.
Depending on the installed WinUI XAML compiler host, Visual Studio's
`MSBuild.exe /restore` may be required in place of `dotnet build`:

```powershell
MSBuild.exe .\native\windows\Vitals.Windows.csproj /restore /t:Build /p:Configuration=Release /p:Platform=ARM64 /p:RuntimeIdentifier=win-arm64
MSBuild.exe .\native\windows\Vitals.Windows.csproj /restore /t:Publish /p:Configuration=Release /p:Platform=ARM64 /p:RuntimeIdentifier=win-arm64 /p:SelfContained=true /p:PublishDir=artifacts\publish\
```

The project conditionally copies the repository-root-relative
`native\core\target\aarch64-pc-windows-msvc\release\vitals_core.dll` into build
and publish output (`..\core\target\aarch64-pc-windows-msvc\release\vitals_core.dll`
relative to this project). Build the core **before** evaluating/building this project.
If the core is built elsewhere, place its DLL beside `Vitals.Windows.exe`.
Distribute the entire publish directory, not just the executable. This is an
unpackaged app, with self-contained Windows App SDK and .NET deployment;
there is no MSIX or Windows App Runtime installation requirement. Native DLL
dependencies must also be available.

Start `.\native\windows\artifacts\publish\Vitals.Windows.exe` on Windows ARM64.
A missing, incompatible, or unloadable core displays an actionable error with
a local-status retry button. It never displays synthetic data.

## First run

1. In Google Cloud, enable the Google Health API for your own project.
2. Configure the OAuth consent screen and add your account as a test user if
   the application is in testing. Create a **Desktop app** OAuth client.
3. Open Vitals **Settings**, save the client ID and optional secret, then choose
   **Connect Google**. Finish authorization in your default browser.
4. Choose **Sync last 30 days**. Vitals displays the latest date with available
   data, seven metric cards, sleep stages, and up to 30 dated history rows.

Google Cloud configuration and API/account eligibility are the user's
responsibility. Google Health cloud access does **not** imply access to all
Android Health Connect data. Missing values appear as `—`, including sleep
stages. No readiness/medical scores are inferred. A metric value of zero is
displayed as zero only when the backend actually supplies it.

## Privacy and behavior

- Launch reads local status only. All native calls run on a worker thread;
  connect, configure, sync, and disconnect are serialized by the UI.
- Connect may wait up to three minutes for browser authorization; sync may
  take several minutes. Controls are disabled during an operation.
- No automatic sync, background service, startup changes, or web server is
  introduced by this frontend.
- Secrets are not logged or saved by C#. They are passed to the Rust core.
  A blank secret preserves the existing secret for the same client ID.
  Replacing an existing client ID requires confirmation before the core clears
  cached data/tokens. Cancel leaves the saved configuration and data unchanged.
- **Disconnect & clear** requires confirmation and removes local tokens and
  cached health data. Google authorization must be revoked separately in
  your Google Account's third-party connections.
- Appearance defaults to the system theme. Light/Dark overrides are saved in
  `%LOCALAPPDATA%\VitalsNative\appearance.json`; this file contains no credentials.
  The core owns health cache and credential storage under the same app data
  root and protects tokens with DPAPI.

## Frontend verification checklist

After the core and frontend have been built, verify on ARM64:

- Missing DLL: explicit startup error; Retry succeeds after restoring it.
- First run: setup guidance, unavailable values, no fabricated history.
- Settings: invalid client ID remains in the dialog; password is masked;
  cancel restores the prior theme; a saved theme survives restart.
- Client change: replacing an existing ID asks for confirmation before calling
  configure; cancel preserves tokens/cache; confirm clears them. Saving the same
  ID with a blank secret does not prompt or replace its saved secret.
- Connect/sync: window stays responsive, overlapping operations are disabled,
  errors/warnings are shown, and partial data retains `—` rather than zeros.
- Resize: metric cards reflow; long history and settings scroll; keyboard and
  screen-reader labels remain usable; theme changes update cards and dialogs.
- Disconnect: cancel preserves data; confirmation clears the dashboard;
  Google revocation is explicitly separate.

The FFI result pointer is decoded as UTF-8 JSON and freed in `finally` using
`vitals_free`, including error and invalid-JSON paths. The frontend expects
camelCase status/day fields and the `{"ok":true,"data":...}` /
`{"ok":false,"error":"..."}` envelopes.
