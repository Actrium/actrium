# Actrium Windows

Native Windows WebView2 browser shell for Actrium.

## Current State

- This directory is the Windows app root.
- The current project is a .NET Windows Forms + WebView2 browser shell.
- `src/BrowserApplicationContext.cs` owns the Windows multi-window lifetime:
  first-window session restore/persist, `Ctrl+N` normal window creation,
  `Ctrl+Shift+N` private window creation, `--private`/`--incognito` startup,
  private profile cleanup, and app exit after the last window closes.
- `src/BrowserTab.cs` owns per-tab WebView2 state and host bridge ownership.
- `src/BrowserSessionStore.cs` owns Windows-local browser session snapshots used
  to restore tabs on the next launch, backed by the shared browser SQLite
  schema.
- `src/BrowserWindowStateStore.cs` owns Windows-local window bounds and
  maximized-state JSON snapshots used to restore the primary shell window.
- `src/BrowserAddressInputPolicy.cs` owns Windows-local address bar input
  resolution, including search fallback for user-entered queries.
- `src/BrowserSearchEnginePolicy.cs` adapts the Rust-owned search engine
  catalog and search URL builder to Windows, with local fallbacks when the
  native DLL is absent.
- `src/BrowserSearchSettingsStore.cs` owns the persisted Windows-local search
  engine preference.
- `src/BrowserStartupSettingsStore.cs` owns the persisted Windows-local startup
  session-restore preference.
- `src/BrowserBrowsingDataPolicy.cs` owns Windows-local clear-browsing-data
  prompt and status text.
- `src/BrowserContentBlockingPolicy.cs` owns the Windows-local lightweight
  ad/tracker resource-blocking rules and status text.
- `src/BrowserContentBlockingStore.cs` owns the persisted Windows-local content
  blocking toggle state.
- `src/BrowserBookmarkStore.cs` owns Windows-local browser bookmark snapshots for
  explicit page bookmarks, backed by the shared browser SQLite schema.
- `src/BrowserBookmarksDocumentPolicy.cs` owns safe generated bookmarks-page
  HTML plus in-page filtering and remove controls for the local
  `about:bookmarks` document.
- `src/BrowserLocalListDocumentPolicy.cs` owns shared generated-list chrome for
  Windows-local pages that need safe rows, filtering, and remove controls.
- `src/BrowserHistoryStore.cs` owns Windows-local browser history snapshots for
  successful non-private external navigations, backed by the shared browser
  SQLite schema.
- `src/BrowserStorageDatabase.cs` adapts the Rust-owned `actrium_browser.db`
  schema and repository SQL to Windows through `Microsoft.Data.Sqlite`, including
  one-time import from the older JSON files.
- `src/BrowserHistoryDocumentPolicy.cs` owns safe generated history-page HTML
  plus in-page filtering and remove controls for the local `about:history`
  document.
- `src/BrowserMenuCommand.cs`, `src/BrowserMenuModel.cs` and
  `src/BrowserAppMenu.cs` own the compact application menu (Chrome/Edge
  three-dot style): the command enum, the menu tree with shortcuts and aliases
  as pure data, and the themed ContextMenuStrip that renders it. Every command
  routes through the single `MainForm.ExecuteCommand` router; Escape
  precedence lives in `src/BrowserEscapePolicy.cs`.
- `src/BrowserNavigationErrorPolicy.cs` owns Windows-local navigation failure
  status text and safe error-page document generation.
- `src/BrowserDownloadManager.cs` owns WebView2 download prompts and per-download
  status updates plus last-completed-download tracking.
- `src/BrowserDownloadPolicy.cs` owns Windows-local download file-name
  sanitization, default downloads-folder resolution, and progress/status text
  used by the download manager.
- `src/BrowserPageActionPolicy.cs` owns Windows-local current-page address
  rules for copy and open-in-default-browser actions.
- `src/BrowserPdfPolicy.cs` owns Windows-local save-as-PDF file-name and status
  text rules.
- `src/BrowserExternalProtocolManager.cs` owns user-confirmed handoff from
  blocked WebView navigation to Windows default protocol handlers.
- `src/BrowserExternalProtocolPolicy.cs` owns Windows-local external protocol
  classification and prompt text.
- `src/BrowserFindBar.cs` owns the native page-find bar and find navigation
  events.
- `src/BrowserFindPolicy.cs` owns Windows-local page-find script generation and
  result parsing.
- `src/BrowserHomeDocumentPolicy.cs` owns Windows-local native home-page
  document generation and safe ActriumCore status text rendering.
- `src/BrowserPermissionManager.cs` owns WebView2 permission prompts and
  allow/deny decisions.
- `src/BrowserPermissionPolicy.cs` owns Windows-local permission prompt text and
  origin formatting.
- `src/BrowserProfilePolicy.cs` owns the Windows-local WebView2 user data folder
  policy for normal and private profiles.
- `src/BrowserProcessFailurePolicy.cs` owns Windows-local WebView2 process
  failure status text and recoverable-tab reload classification.
- `src/BrowserClosedTabStack.cs` owns Windows-local last-closed-tab state for
  `Ctrl+Shift+T` reopening.
- `src/BrowserSidebarLayoutPolicy.cs` owns the vertical sidebar width clamp,
  collapse threshold, and DPI scaling rules.
- `src/BrowserTabSidebar.cs` owns the vertical tab sidebar: new tab, select
  tab, close tab, favicon/letter-avatar rendering, and tab title/loading
  state rendering.
- `src/BrowserToolbar.cs` owns the first Windows browser chrome: address entry,
  back, forward, reload/stop, home, address focus, loading/status display, and
  link-hover status text.
- `src/BrowserZoomPolicy.cs` owns Windows-local WebView2 zoom bounds, step
  behavior, command availability, and zoom percentage text.
- `src/MainForm.cs` owns tab shortcut routing, WebView2 full-screen element
  transitions, Windows window chrome restore, primary window geometry
  restore/persist, and the F11 browser full-screen command.
- `src/NavigationPolicy.cs` owns Windows-local browser URL normalization, home
  address recognition, local file path normalization, page-navigation
  allowlisting, and navigation scheme allowlisting.
- `src/ActriumCore.cs` is the only Windows C# entrypoint for the Rust ActriumCore C
  ABI.
- `src/HostBridgeProtocol.cs` owns the Windows-local `actrium.host.*` JSON
  protocol, source allowlist, request dispatch, response shape, and payload DTOs.
- `src/HostMessageBridge.cs` owns the WebView2 wrapper for that protocol: script
  injection, `WebMessageReceived`, and JSON responses back to the page.
- `src/WindowsAppPaths.cs` centralizes the Actrium Windows local app data root
  shared by session/window snapshots and WebView2 profile data.
- Browser-feature parity, OS background sync, packaging/CI automation, and
  non-local cloud provider integration are still in progress.

## Ownership Boundary

Keep Windows-owned behavior in this app:

- WebView2 lifecycle and browser host policy.
- WebView2 profile creation, user data folder selection, cookies/cache storage,
  and browser environment reuse.
- WebView2 private profile creation and best-effort cleanup after private
  windows close.
- WebView2 profile data clearing for cookies, site data, cache, browsing
  history, and download history.
- WebView2 host/page message routing and method allowlists.
- Browser tab lifecycle, active WebView host selection, closed-tab reopening,
  duplicate-tab creation, and per-tab page state.
- Browser session restore for Windows shell tabs.
- Browser primary-window size, position, maximized-state restore, and
  last-known geometry persistence.
- Browser multi-window lifecycle, first-window session persistence, secondary
  normal/private window creation, private profile cleanup, and last-window app
  shutdown.
- Browser startup options, including positional URL/file arguments, repeated
  `--url`, `--private`, `--private-window`, and `--incognito`.
- Browser chrome, Windows-local navigation policy, address bar search fallback,
  native home-page document generation, navigation failure pages, navigation
  state, page title/status sync, and generated local history/bookmarks-page
  rendering.
- Address-bar search engine selection and its persisted Windows-local setting.
- Browser bookmarks, explicit bookmark/unbookmark commands, bookmark menu state,
  and local bookmark-page rendering.
- Browser history recording for non-private external navigations and local
  history clearing from normal windows.
- Lightweight content blocking for WebView2 subresources and its persisted
  Windows-local toggle.
- Page-initiated navigation interception for unsafe schemes.
- Browser menu commands and their Windows keyboard accelerators.
- Browser keyboard shortcuts for address focus, new/close/reopen tab, tab
  cycling, direct tab selection, reload, and back/forward navigation.
- Native file-open dialog routing and local file path navigation.
- Current page address copy/open actions, Windows Clipboard integration, and
  Windows default-browser handoff for shell-safe page addresses.
- WebView2 save-as-PDF, save-file dialogs, and Windows-local PDF status text.
- WebView2 print UI and the native print keyboard command.
- WebView2 downloads, save-file dialogs, and Windows-local download status.
- Opening the Windows downloads folder, the most recent completed download, and
  the most recent completed download's folder location through the native shell.
- External protocol prompts and Windows default-app handoff for non-browser
  schemes.
- WebView2 permission prompts for browser capabilities such as camera,
  microphone, location, notifications, clipboard, and related browser features.
- WebView2 page find, the native find bar, and find keyboard commands.
- WebView2 zoom commands, zoom keyboard accelerators, and zoom status text.
- WebView2 process-failure status and recoverable-tab reload attempts.
- WebView2 `window.open`/new-window requests and the Windows tab-opening policy.
- WebView2 full-screen element transitions, native window border changes, and
  browser chrome hide/restore.
- Browser full-screen command routing, including F11 and the native View menu.
- Windowing, tabs, menus, tray, keyboard shortcuts, and OS integration.
- Credential prompts and platform account UX.
- File dialogs, notifications, clipboard, and desktop-specific permissions.

Use shared Rust crates for stable non-UI behavior:

- ACTR manifest parsing, integrity checks, and install planning.
- Persistence schemas and deterministic data migrations.
- Auth/session contract shapes and cross-platform validation rules.
- Text, parsing, crypto, and other non-UI algorithms when they become shared or
  materially cleaner as a single native boundary.

Do not move UI routing or thin event forwarding into Rust just to centralize it;
that would create unnecessary native round-trips and a fragmented call graph.

The current shell reads shared ActriumCore metadata through `actrium_core.dll` when
that DLL is present. Build and copy it on a Windows Rust host with:

```text
scripts/actrium/build-actrium-rust-bindings.sh windows
```

Generated DLLs live under `windows/native/` locally and are ignored by
git.

The Windows process runs through `BrowserApplicationContext` rather than a
single `MainForm`. The first normal window restores and persists the browser
session plus the last-known window bounds/maximized state; windows opened with
`Ctrl+N` or File > New Window start from a fresh home tab and do not overwrite
those saved primary-window snapshots. The application exits after the last
window closes. This is native window lifetime management and should stay out of
Rust.

Private windows are also Windows-owned. `Ctrl+Shift+N` or File > New Private
Window creates a secondary shell window backed by a separate WebView2 user data
folder under the ignored local app-data profile area. Private windows do not
restore or persist browser sessions, and their profile folder is deleted on close
on a best-effort basis. This is profile and window lifecycle orchestration, not
a shared Rust-core rule.

The same private mode can be used at startup with `--private`,
`--private-window`, or `--incognito`. When one of those flags is present, the
first window uses an isolated private WebView2 profile, skips session restore,
and does not write the browser session snapshot on close. `--url <address>` can
be combined with private startup to open a specific initial page.

Startup addresses are Windows-owned as well. `--url <address>` can be repeated,
and non-flag positional arguments are treated as URL or local-file candidates so
shell shortcuts, file associations, and protocol launches can open one or more
initial tabs. When startup addresses are present they take precedence over
session restore.

Window geometry restore is Windows-owned. `BrowserWindowStateStore` writes a
small JSON snapshot under the Actrium Windows local app data root for the first
normal window only. Secondary normal windows and private windows do not restore
or persist this snapshot, which keeps temporary window placement from replacing
the user's primary browser window state.

The host message bridge is deliberately small. It injects `window.actriumHost`,
accepts only allowlisted `actrium.host.*` requests from trusted local/dev WebView2
sources, returns the core status payload through the C# `ActriumCore` adapter, and
delegates `openTab`, `closeTab`, `selectTab`, `navigate`, `getTabs`, and
`getActiveTab` to `MainForm`. The protocol dispatcher is separated from the
WebView2 wrapper so it can be tested without launching WebView2. Do not push tab
routing, page navigation, WebView lifecycle, or high-volume UI events through
Rust just to centralize them.

The native home page is Windows-owned. `BrowserHomeDocumentPolicy` builds the
generated `about:home` HTML and safely renders the ActriumCore availability
summary before the page requests the host bridge for live status. `MainForm` only
decides when to show that document. This keeps start-page UX and WebView2
document rendering in the shell; Rust only provides stable core metadata through
`ActriumCore.cs`.

The toolbar, main menu, and WebView navigation loop are Windows-owned. They keep
address entry, back/forward history, reload/stop, home navigation, page zoom,
loading status, source changes, document title changes, local file opening,
save-as-PDF, native print UI, developer tools, and WebView2 process failures
local to this app. Navigation failures render a safe local error page through
`BrowserNavigationErrorPolicy`; reload from that state retries the original tab
address instead of refreshing the generated error document. Address entry first
tries browser-safe local/file/http/https/about targets, then treats user-entered
free text as a search query through `BrowserAddressInputPolicy`. Browser
keyboard shortcuts for tab cycling/direct selection and menu accelerators are
also routed through this same shell layer. WebView2 link-hover
`StatusBarTextChanged` events update the toolbar status area for the active tab
only.

Browsing data clearing is Windows-owned. It prompts through the native shell,
then uses the current WebView2 profile's `ClearBrowsingDataAsync` API for site
data, cookies, disk cache, browsing history, and download history. It
also clears the Actrium Windows-local history store for normal windows. Private
windows do not write that history and do not clear the normal-window history
store. The operation intentionally does not clear profile settings, autofill, or
saved passwords. Prompt and status text stay testable through
`BrowserBrowsingDataPolicy`; the profile operation remains in `MainForm`.

Browser history is Windows-owned in this first slice. `MainForm` records
successful non-private external navigations and handles the generated page's
remove action, `BrowserHistoryStore` persists through `BrowserStorageDatabase`
into the Rust-owned `actrium_browser.db` schema, and
`BrowserHistoryDocumentPolicy` renders the generated `about:history` page with
local filtering through `BrowserLocalListDocumentPolicy`. Older
`browser-history.json` files are imported once. WebView2 event timing,
private-window rules, generated document routing, and host-bridge action
dispatch stay in the Windows shell.

Browser bookmarks are Windows-owned in this first slice too. `MainForm` handles
the explicit bookmark/unbookmark command and the generated page's remove action,
`BrowserBookmarkStore` persists through `BrowserStorageDatabase` into the
Rust-owned `actrium_browser.db` schema, and `BrowserBookmarksDocumentPolicy`
renders the generated `about:bookmarks` page. Older `browser-bookmarks.json`
files are imported once. Menu routing, bookmark-page filtering/rendering through
`BrowserLocalListDocumentPolicy`, host-bridge action dispatch, and active-tab
checked state stay in the Windows shell.

The navigation policy is also Windows-owned. It is extracted from `MainForm` so
toolbar entry, local file opening, startup URLs, host-bridge navigation, and
future menu commands share one local rule without moving browser shell policy
into Rust.

Address bar search fallback is Windows-owned and deliberately sits above
`NavigationPolicy`. Only user-entered toolbar text can become a search URL; the
engine catalog, engine-id normalization, and search URL construction come from
ActriumCore through `BrowserSearchEnginePolicy`, while the selected search engine
is persisted locally through `BrowserSearchSettingsStore` and applied by
`BrowserAddressInputPolicy`. Startup arguments, host-bridge navigation, session
restore, and page-initiated navigation remain strict browser-safe address flows.
This keeps invalid bridge or page URLs from silently becoming search requests.

Startup restore behavior is Windows-owned. `BrowserStartupSettingsStore` persists
the Tools menu preference that decides whether a normal launch without explicit
startup URLs restores `BrowserSessionStore` or opens the home tab. This is shell
startup UX, not a Rust-core algorithm; a future product-wide preferences schema
can own the durable setting if cross-platform parity demands it.

Current-page actions are Windows-owned. The File menu can copy the active page
address to the Windows clipboard and open shell-safe `http`, `https`, or `file`
addresses in the user's default browser. `BrowserPageActionPolicy` keeps the
copy/open address rules testable, while `MainForm` owns Clipboard and
`ProcessStartInfo` shell integration. This is desktop shell behavior and should
not move into Rust.

The WebView2 profile is Windows-owned. `MainForm` creates one explicit
`CoreWebView2Environment` per shell instance and initializes each tab with it, so
cookies, cache, local storage, and related browser data live under the Actrium
Windows local app data root instead of an implicit WebView2 default location.
`BrowserProfilePolicy` owns that user data folder path, and `WindowsAppPaths`
keeps it aligned with session storage.

Page-initiated navigation is also checked in `MainForm.cs`. Address bar and host
bridge inputs may normalize bare hosts, but WebView2 `NavigationStarting` only
allows already-absolute browser-safe targets. Unsafe page redirects or links are
canceled before the WebView navigates.

External protocol handoff is Windows-owned. When a page tries to navigate to a
non-browser scheme such as `mailto:` or `tel:`, the WebView navigation is
canceled first. `BrowserExternalProtocolManager` then requests the user before
opening the target through Windows' default protocol handler. Browser-internal or
unsafe schemes such as `javascript:`, `data:`, `blob:`, and `actrium:` are never
handed off.

The tab sidebar is also Windows-owned. Each tab owns a separate WebView2 control
and host message bridge; selecting a tab swaps the active WebView host, closing
the last tab creates a fresh home tab, `Ctrl+Shift+T` reopens the last closed
tab from a Windows-local stack, File > Duplicate Tab opens the active tab's
address as a new tab through the existing navigation path, and browser shortcuts
such as `Ctrl+Tab`, `Ctrl+Shift+Tab`, `Ctrl+PageUp`, `Ctrl+PageDown`, and
`Ctrl+1..9` select tabs through `MainForm`. Do not move tab switching,
duplicate-tab creation, closed-tab reopening, or WebView ownership into Rust.

The session store is Windows-owned for now. It writes the first-window session
into the Rust-owned `actrium_browser.db` schema through `BrowserStorageDatabase`,
restores tabs when no `--url` startup override is provided, ignores corrupt
legacy session files, and drops unsafe navigation entries instead of restoring
them. Older `browser-session.json` files are imported once. This launch/restore
workflow remains Windows shell orchestration.

The download manager is Windows-owned. It handles WebView2 `DownloadStarting`,
prompts with a native save-file dialog, applies a Windows-local file-name policy,
opens the default downloads folder through `Ctrl+J`/Tools > Open Downloads
Folder, tracks the last completed download, can open that file, can reveal it in
Explorer, and reports progress/completion through the toolbar status area.
Download prompts, filesystem locations, recent-file shell handoff, folder
handoff, and dialog ownership are desktop shell behavior; do not move them to
Rust.

Save-as-PDF is Windows-owned. It uses a native save-file dialog and WebView2
`PrintToPdfAsync`, while `BrowserPdfPolicy` keeps file-name suggestions and
status text testable. This is browser shell UX and filesystem interaction, not a
shared native algorithm.

The find bar is Windows-owned. It provides the `Ctrl+F` surface, `F3`/
`Shift+F3` repeated find shortcuts, local previous/next/close command handling,
and a small WebView2 script execution boundary for `window.find`. The script
string is built through `BrowserFindPolicy` so query escaping and result parsing
remain testable without moving the page workflow into Rust.

The permission manager is Windows-owned. It handles WebView2
`PermissionRequested`, prompts with a native Windows confirmation dialog, and
sets `Allow` only after explicit user approval. Prompt wording is testable
through `BrowserPermissionPolicy`, but the permission dialog, OS UX, and
WebView2 state assignment stay in this shell.

The zoom policy is Windows-owned. It clamps WebView2 zoom factors, formats zoom
status, and controls menu availability for Windows desktop commands. It is a
local browser shell rule rather than a shared product schema.

WebView2 process failures are Windows-owned. `MainForm` receives
`ProcessFailed`, updates the active tab state, and reloads recoverable renderer
or frame failures through the existing WebView2 control.
`BrowserProcessFailurePolicy` keeps the status text and reload classification
testable. This is browser engine lifecycle recovery, not a Rust-core algorithm.

Content blocking is Windows-owned in this first slice. `MainForm` installs a
WebView2 `WebResourceRequested` filter and returns a local blocked response for
lightweight ad/tracker subresource matches. `BrowserContentBlockingPolicy` keeps
the current rule set testable, while `BrowserContentBlockingStore` persists the
Tools menu toggle under the Actrium Windows local app data root. Do not move this
WebView2 interception flow into Rust; a future full rule-list parser or shared
privacy schema can be considered separately.

WebView2 new-window requests are Windows-owned. The shell marks
`NewWindowRequested` as handled and opens safe requests in a new Actrium tab. This
keeps target-blank/window-open behavior inside the browser shell instead of
allowing detached WebView windows or routing page lifecycle through Rust.

WebView2 and browser full-screen are Windows-owned. `MainForm` listens for
`ContainsFullScreenElementChanged`, routes F11 through the native menu command,
and keeps page full-screen separate from user-requested browser full-screen.
Both modes share the same WinForms chrome transition: browser chrome is hidden
while full-screen is active, then the previous border, window state, topmost
state, toolbar, tab sidebar, menu, and find bar are restored when neither mode is
active. `Esc` requests the document to leave page full-screen first, or exits browser
full-screen when the page is not full-screen. This is local window orchestration
and should not move into Rust.

Local reference or scratch checkouts should live under `internal/`, not under
`apps/`. `apps/` is reserved for the Actrium platform clients.

## Verification

The root Windows verifier always validates project metadata:

```text
npm run verify:windows
```

Whenever a .NET SDK is available, the same command also runs:

```text
dotnet run --project windows/tests/Actrium.Windows.Tests.csproj --configuration Debug
dotnet build windows/Actrium.Windows.csproj --configuration Debug
```

The verifier treats the WebView2 package's `MSB3277` `WindowsBase` assembly
conflict as the only allowed Windows app build warning; any other dotnet warning
fails verification. Windows hosts require the .NET SDK. Non-Windows hosts without
it keep the root verification chain metadata-only for this app.

Run the shell manually with:

```text
dotnet run --project windows/Actrium.Windows.csproj -- --url https://example.com
```

## Packaging

`VERSION` (`windows/VERSION`) is the single version source: the csproj
reads it into `<Version>` (which drives the assembly, file and product
versions) and the packaging pipeline reads it for artifact names and the
installer version, so the stamps cannot drift.

On a Windows host, `scripts/actrium/package-windows.sh` (a thin wrapper around
`scripts/actrium/package-windows.ps1`) produces, under the gitignored `dist/windows/`:

- `Actrium-<version>-win-x64.zip` — portable self-contained publish
  (`dotnet publish -c Release -r win-x64 --self-contained true`) with
  `actrium_core.dll` verified at the application root; extract and run
  `Actrium.exe` in place.
- `Actrium-<version>-win-x64-setup.exe` — per-user installer compiled with Inno
  Setup 6 from `packaging/actrium-installer.iss` when `ISCC.exe` is installed;
  otherwise the pipeline warns and ships the zip only. The installer needs no
  administrator rights, installs to `%LOCALAPPDATA%\Programs\Actrium`, creates a
  per-user Start Menu shortcut, and runs the embedded official WebView2
  Evergreen bootstrapper only when the runtime is missing. Uninstall removes
  binaries and shortcuts but leaves `%LOCALAPPDATA%\Actrium\Actrium\Windows`
  (profile data) in place.

Known gaps: artifacts are unsigned (no code-signing certificate yet), and the
app has no icon asset — the repo contains no convertible icon source, so the
executable and shortcut keep default icons until one is authored.
