; Actro Windows per-user installer (Inno Setup 6).
;
; Compiled by scripts/actrium/package-windows.ps1, which passes:
;   /DAppVersion=<version>        from windows/VERSION (single source)
;   /DPublishDir=<folder>         self-contained dotnet publish output
;   /DBootstrapperPath=<exe>      optional: official WebView2 Evergreen
;                                 bootstrapper to embed (~2 MB)
;
; Policy:
;   - Per-user install (PrivilegesRequired=lowest): lands in
;     %LOCALAPPDATA%\Programs\Actro, never requests for administrator rights,
;     never touches HKLM or Program Files.
;   - Start Menu shortcut goes to the per-user Programs folder.
;   - WebView2 Evergreen runtime is detected through the EdgeUpdate Clients
;     registry keys (WOW6432Node first: that is where the machine-wide
;     runtime registers on 64-bit Windows); the embedded official
;     bootstrapper runs only when no runtime is found.
;   - Uninstall removes the installed binaries, the shortcut and the
;     uninstaller itself, but deliberately leaves user profile data in
;     place (%LOCALAPPDATA%\Actrium\Actro\Windows: browser SQLite store,
;     settings, plus the WebView2 browsing profile), so reinstalling
;     restores the user's session, bookmarks and history. It also leaves
;     the WebView2 runtime installed: it is machine/user infrastructure
;     shared with other applications.
;
; KNOWN GAP - code signing: the installer and the payload are unsigned (no
; certificate yet); SmartScreen warns on first run. Sign both with signtool
; once a certificate exists.
;
; No SetupIconFile / shortcut IconFilename yet: the repo has no app icon
; asset to convert (the Apple icon sets carry only Contents.json). Wire an
; .ico here and via ApplicationIcon in Actro.Windows.csproj once authored.

#ifndef AppVersion
  #error AppVersion must be defined, e.g. ISCC /DAppVersion=0.1.0
#endif
#ifndef PublishDir
  #error PublishDir must be defined to the dotnet publish output folder
#endif

[Setup]
AppId={{6D0C8B48-51B2-4E60-9E5F-3A8C21D9B7A4}
AppName=Actro
AppVersion={#AppVersion}
AppPublisher=Actrium
DefaultDirName={autopf}\Actro
DefaultGroupName=Actro
DisableProgramGroupPage=yes
DisableDirPage=yes
; Per-user: {autopf} resolves to %LOCALAPPDATA%\Programs with lowest
; privileges; no UAC prompt, no machine-wide state.
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0.19041
UninstallDisplayName=Actro
UninstallDisplayIcon={app}\Actro.exe
; lzma2/fast: the self-contained .NET payload is ~200 MB; normal lzma2
; roughly doubles compile time for a marginal size win.
Compression=lzma2/fast
SolidCompression=yes
WizardStyle=modern

[Files]
Source: "{#PublishDir}\*"; DestDir: "{app}"; Flags: recursesubdirs ignoreversion
#ifdef BootstrapperPath
; Staged to {tmp} only when the runtime is missing; deleted after setup.
Source: "{#BootstrapperPath}"; DestDir: "{tmp}"; DestName: "MicrosoftEdgeWebView2Setup.exe"; Flags: deleteafterinstall; Check: not IsWebView2RuntimeInstalled
#endif

[Icons]
Name: "{autoprograms}\Actro"; Filename: "{app}\Actro.exe"

[Run]
#ifdef BootstrapperPath
; /silent /install: official Evergreen bootstrapper downloads and installs
; the current runtime; requires network on the target machine.
Filename: "{tmp}\MicrosoftEdgeWebView2Setup.exe"; Parameters: "/silent /install"; Check: not IsWebView2RuntimeInstalled; StatusMsg: "Installing Microsoft Edge WebView2 Runtime..."; Flags: waituntilterminated
#endif
Filename: "{app}\Actro.exe"; Description: "Launch Actro"; Flags: nowait postinstall skipifsilent

[Code]
const
  // Machine-wide Evergreen runtime on 64-bit Windows registers under
  // WOW6432Node (confirmed on real hardware); the non-WOW path covers
  // 32-bit Windows and the HKCU path covers per-user runtime installs.
  WebView2ClientsKeyWow =
    'SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';
  WebView2ClientsKey =
    'SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}';

function IsWebView2VersionValue(const RootKey: Integer; const SubKey: String): Boolean;
var
  Version: String;
begin
  // An uninstalled runtime can leave pv = 0.0.0.0 behind; treat it as absent.
  Result := RegQueryStringValue(RootKey, SubKey, 'pv', Version)
    and (Version <> '') and (Version <> '0.0.0.0');
end;

function IsWebView2RuntimeInstalled(): Boolean;
begin
  Result := IsWebView2VersionValue(HKLM, WebView2ClientsKeyWow)
    or IsWebView2VersionValue(HKLM, WebView2ClientsKey)
    or IsWebView2VersionValue(HKCU, WebView2ClientsKey);
end;

#ifndef BootstrapperPath
function InitializeSetup(): Boolean;
begin
  // Built without the embedded bootstrapper (offline packaging): install
  // proceeds, but warn that the app cannot start without the runtime.
  Result := True;
  if not IsWebView2RuntimeInstalled() then
    SuppressibleMsgBox(
      'The Microsoft Edge WebView2 Runtime was not found. Actro needs it to run. '
        + 'Install it from https://developer.microsoft.com/microsoft-edge/webview2/ after setup.',
      mbInformation, MB_OK, IDOK);
end;
#endif
