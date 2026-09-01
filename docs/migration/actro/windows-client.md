# Windows Client Migration

Windows is part of the initial Actro platform set. The current repository has a
.NET Windows Forms + WebView2 browser shell with native chrome, WebView2
profile handling, local browser storage, provider sync settings, and Rust
ActroCore integration. Browser-feature parity, OS background sync, packaging,
and non-local cloud provider integration are still in progress.

## Current Mapping

```text
windows/           Actro Windows WebView2 browser shell app root.
windows/Actro.Windows.csproj
                              .NET Windows project file.
windows/src/BrowserApplicationContext.cs
                              Windows multi-window lifetime, first-window
                              session restore/persist, Ctrl+N normal window
                              creation, Ctrl+Shift+N private window creation,
                              --private/--incognito startup, private profile
                              cleanup, and last-window app shutdown.
windows/src/BrowserTab.cs
                              Per-tab WebView2 state and host bridge owner.
windows/src/BrowserSessionStore.cs
                              Windows-local JSON session snapshots for tab
                              restore.
windows/src/BrowserWindowStateStore.cs
                              Windows-local JSON window bounds/maximized-state
                              snapshots for primary-window restore.
windows/src/BrowserAddressInputPolicy.cs
                              Windows-local address bar input resolution and
                              search fallback for user-entered queries.
windows/src/BrowserSearchEnginePolicy.cs
                              ActroCore-backed search engine catalog and search
                              URL construction adapter with local fallback.
windows/src/BrowserSearchSettingsStore.cs
                              Persisted Windows-local search engine preference.
windows/src/BrowserStartupSettingsStore.cs
                              Persisted Windows-local startup session-restore
                              preference.
windows/src/BrowserBrowsingDataPolicy.cs
                              Windows-local clear-browsing-data prompt and
                              status text.
windows/src/BrowserContentBlockingPolicy.cs
                              Windows-local lightweight ad/tracker
                              subresource-blocking rules and status text.
windows/src/BrowserContentBlockingStore.cs
                              Persisted Windows-local content blocking toggle
                              state.
windows/src/BrowserBookmarkStore.cs
                              Windows-local browser bookmark JSON snapshots for
                              explicit page bookmarks.
windows/src/BrowserBookmarksDocumentPolicy.cs
                              Safe generated bookmarks-page document and
                              in-page filter/remove controls for
                              actro://bookmarks.
windows/src/BrowserLocalListDocumentPolicy.cs
                              Shared generated-list chrome for Windows-local
                              pages with safe rows, filtering, and remove
                              controls.
windows/src/BrowserHistoryStore.cs
                              Windows-local browser history JSON snapshots for
                              successful non-private external navigations.
windows/src/BrowserHistoryDocumentPolicy.cs
                              Safe generated history-page document and
                              in-page filter/remove controls for
                              actro://history.
windows/src/BrowserMenuCommand.cs
                              Command enum shared by the compact app menu and
                              the keyboard shortcut router.
windows/src/BrowserMenuModel.cs
                              Compact app menu tree as pure data: items,
                              shortcuts, aliases, and the search-engine
                              argument channel.
windows/src/BrowserAppMenu.cs
                              Themed ContextMenuStrip rendering the menu model
                              (Chrome/Edge three-dot style).
windows/src/BrowserEscapePolicy.cs
                              Explicit Escape precedence: find bar, full
                              screen, then stop.
windows/src/BrowserNavigationErrorPolicy.cs
                              Windows-local navigation failure status text and
                              safe error-page document generation.
windows/src/BrowserDownloadManager.cs
                              WebView2 download prompts, per-download status
                              updates, and last-completed-download tracking.
windows/src/BrowserDownloadPolicy.cs
                              Windows-local download file-name sanitization and
                              default downloads-folder/progress/status text.
windows/src/BrowserPageActionPolicy.cs
                              Windows-local current-page address rules for copy
                              and open-in-default-browser actions.
windows/src/BrowserPdfPolicy.cs
                              Windows-local save-as-PDF file-name and status
                              text rules.
windows/src/BrowserExternalProtocolManager.cs
                              User-confirmed handoff to Windows default
                              protocol handlers.
windows/src/BrowserExternalProtocolPolicy.cs
                              Windows-local external protocol classification
                              and prompt text.
windows/src/BrowserFindBar.cs
                              Native Windows page-find bar and find navigation
                              events.
windows/src/BrowserFindPolicy.cs
                              Windows-local page-find script generation and
                              result parsing.
windows/src/BrowserHomeDocumentPolicy.cs
                              Windows-local native home-page document
                              generation and safe ActroCore status rendering.
windows/src/BrowserPermissionManager.cs
                              WebView2 permission prompts and allow/deny
                              decisions.
windows/src/BrowserPermissionPolicy.cs
                              Windows-local permission prompt text and origin
                              formatting.
windows/src/BrowserProfilePolicy.cs
                              Windows-local WebView2 user data folder policy
                              for normal and private profiles.
windows/src/BrowserProcessFailurePolicy.cs
                              Windows-local WebView2 process failure status
                              text and recoverable-tab reload classification.
windows/src/BrowserClosedTabStack.cs
                              Windows-local last-closed-tab state for
                              Ctrl+Shift+T reopening.
windows/src/BrowserSidebarLayoutPolicy.cs
                              Windows-local sidebar width clamp and collapse rules.
windows/src/BrowserTabSidebar.cs
                              Native Windows vertical tab sidebar and tab lifecycle UI.
windows/src/BrowserToolbar.cs
                              Native Windows browser toolbar, address entry,
                              address bar search fallback, and status display.
windows/src/BrowserZoomPolicy.cs
                              Windows-local WebView2 zoom bounds, step
                              behavior, command availability, and status text.
windows/src/MainForm.cs
                              Tab shortcut routing, WebView2 full-screen
                              element transitions, native F11 browser
                              full-screen, window chrome hide/restore, primary
                              window geometry restore/persist, and shell-level
                              browser orchestration.
windows/src/NavigationPolicy.cs
                              Windows-local URL/local file path normalization
                              and navigation allowlist policy for both
                              shell-entered and page-initiated navigation.
windows/src/ActroCore.cs
                              Centralized C# P/Invoke adapter for the Rust
                              ActroCore C ABI.
windows/src/HostBridgeProtocol.cs
                              Windows-local actro.host JSON protocol,
                              dispatcher, payload DTOs, and source allowlist.
windows/src/HostMessageBridge.cs
                              WebView2 wrapper for host/page JSON messages.
windows/src/WindowsAppPaths.cs
                              Shared Actro Windows local app data root helper
                              for session/window snapshots and profile data.
windows/tests/    Lightweight Windows shell behavior tests that do
                              not require third-party .NET test packages.
windows/native/    Ignored local Windows native DLL artifact
                              boundary.
crates/actrium-core/include/actro_core.h
                              Minimal C ABI header for Windows and other C
                              hosts.
```

## First Implementation Shape

The first Windows client is a native Windows shell using WebView2. It should
consume shared Actro runtime contracts where appropriate, but keep Windows-owned
shell behavior local to the app:

- WebView2 lifecycle, browser host policy, and process model.
- WebView2 profile environment creation, user data folder selection, cookies,
  cache, local storage, and browser environment reuse.
- WebView2 private profile creation and best-effort cleanup after private
  windows close.
- WebView2 profile data clearing for cookies, site data, cache, browsing
  history, and download history.
- WebView2 host/page message routing, method allowlists, and bootstrap script.
- Browser tab lifecycle, active WebView host selection, closed-tab reopening,
  duplicate-tab creation, direct tab-selection shortcuts, per-tab
  title/address/loading state, and close-last-tab behavior.
- Browser session restore for Windows shell tabs when no startup URL override is
  provided.
- Browser primary-window bounds and maximized-state restore/persist for the
  first normal Windows shell window.
- Browser multi-window lifetime: first-window session restore/persist,
  secondary normal/private fresh-window creation, private profile cleanup, and
  app shutdown after the last window closes.
- Browser startup options, including positional URL/file arguments, repeated
  `--url`, `--private`, `--private-window`, and `--incognito`.
- Browser chrome, keyboard shortcuts, tab cycling, closed-tab reopen,
  address normalization, address bar search fallback, navigation state,
  navigation failure pages, local history/bookmarks-page rendering, document
  title sync, link-hover status text, and WebView2 process-failure status.
- Address-bar search engine selection and persisted Windows-local search
  preference, backed by the shared ActroCore engine catalog/URL builder.
- Browser bookmarks, explicit bookmark/unbookmark commands, bookmark menu state,
  and Windows-local bookmark-page rendering.
- Browser history recording for non-private external navigations and
  Windows-local history clearing from normal windows.
- Native menu commands and Windows keyboard accelerators.
- Native file-open dialog routing and local file path navigation.
- Current page address copy/open actions, Windows Clipboard integration, and
  Windows default-browser handoff for shell-safe page addresses.
- WebView2 save-as-PDF, save-file dialogs, and PDF status.
- WebView2 print UI and native print command routing.
- WebView2 download prompts, save-file dialogs, and download status.
- Open-downloads-folder and recent-download shell integration.
- External protocol prompts and Windows default-app handoff for non-browser
  schemes.
- WebView2 permission prompts and allow/deny decisions.
- WebView2 page find bar, find commands, and find status.
- WebView2 zoom commands, keyboard accelerators, and status text.
- WebView2 new-window requests and Windows tab-opening policy.
- WebView2 page full-screen, F11 browser full-screen, and native window chrome
  restore.
- Windows-local URL normalization and browser-safe navigation scheme policy.
- WebView2 page-initiated navigation interception for unsafe schemes.
- OS windowing, tabs, menus, shortcuts, notifications, and tray behavior.
- Credential prompts, file dialogs, clipboard, and permission UX.
- Platform packaging, signing, installer, and update flow.

Stable non-UI behavior should use the Rust native boundary in `crates/` when it
reduces duplication or keeps cross-platform rules deterministic:

- ACTR manifest parsing, integrity checks, and install plans.
- Persistence schema definitions and migrations.
- Auth/session DTO shapes and validation.
- Text, parsing, crypto, and other non-UI algorithms.

The first concrete Windows/Rust connection is intentionally small:
`ActroCore.cs` reads stable metadata from the `actro_core` native DLL through
the C ABI declared in `crates/actrium-core/include/actro_core.h`. That proves the
Windows shell can attach to the shared core without moving WebView2 lifecycle or
UI routing into Rust.

`BrowserApplicationContext.cs` owns the Windows process/window lifetime. The
first normal window can restore and persist the shell session plus the
last-known window bounds/maximized state; windows opened through `Ctrl+N` or
File > New Window start fresh and do not overwrite those persisted
primary-window snapshots. The process exits after the last open window closes.
This is WinForms application orchestration, not a Rust-core candidate.

Private windows are part of that same Windows-owned lifecycle. `Ctrl+Shift+N` or
File > New Private Window opens a secondary shell window with a separate WebView2
user data folder. Private windows do not restore or save the browser session,
and the private profile folder is deleted after close on a best-effort basis.
The implementation deliberately stays in the Windows shell because it is profile
and OS-window orchestration.

Private startup uses the same Windows-owned path. `--private`,
`--private-window`, or `--incognito` makes the first window private; `--url
<address>` can still provide its initial page. A private startup window does not
restore session tabs and does not persist a session snapshot when it closes.
`--url <address>` may be repeated, and non-flag positional arguments are treated
as URL or local-file candidates for shell shortcuts, file associations, and
protocol launches. Startup addresses open as initial tabs and take precedence
over restoring the saved tab snapshot.

The first concrete page/host connection is split between
`HostBridgeProtocol.cs` and `HostMessageBridge.cs`. The protocol file owns the
`actro.host.*` JSON request/response shape, dispatcher, payload DTOs, and source
allowlist; the WebView2 bridge injects `window.actroHost`, handles
`WebMessageReceived`, and writes JSON responses through `PostWebMessageAsJson`.
The allowlist contains core status, tab lifecycle, tab selection, tab
enumeration, and tab-scoped navigation methods, all restricted to trusted
local/dev document sources. Core status reads through the local C# `ActroCore`
adapter; tab and navigation requests delegate back to `MainForm`. Keep this
bridge as Windows-owned orchestration; only stable non-UI payload rules should
move down into Rust.

The first browser-client UI slice is `BrowserToolbar.cs` plus the WebView2
navigation event loop in `MainForm.cs`. It adds address entry, back/forward,
reload/stop, home, loading status, URL synchronization, title synchronization,
link-hover status text, process-failure status, keyboard shortcuts for common
browser actions, `Ctrl+Tab`/`Ctrl+Shift+Tab`/`Ctrl+PageUp`/`Ctrl+PageDown` tab
cycling, `Ctrl+1..9` direct tab selection, `Ctrl+Shift+T` closed-tab reopening,
address bar search fallback for user-entered free text, and a Windows-local
allowlist for browser-safe navigation schemes without introducing another shared
abstraction.

`BrowserMenuCommand.cs`, `BrowserMenuModel.cs` and `BrowserAppMenu.cs` form the
compact application menu (Chrome/Edge three-dot style, replacing the classic
menu bar). The model carries the menu tree, shortcuts and aliases as pure data;
the app menu renders it, and every command executes through the single
`MainForm.ExecuteCommand` router shared with keyboard shortcuts. Escape
precedence is owned by `BrowserEscapePolicy.cs`. Keep this menu layer
Windows-owned: it is desktop shell orchestration and should not move to Rust.

`BrowserPageActionPolicy.cs` adds current-page address rules for File menu
actions. The active page address can be copied to the Windows clipboard, while
shell-safe `http`, `https`, and `file` addresses can be opened through the
user's default browser via `ProcessStartInfo`. This is Windows desktop
integration, not a shared Rust-core algorithm.

`BrowserDownloadManager.cs` and `BrowserDownloadPolicy.cs` add the first download
slice. WebView2 `DownloadStarting` is handled inside the Windows shell, which
prompts with `SaveFileDialog`, applies a Windows-local file-name policy, tracks
download progress, opens the default downloads folder through `Ctrl+J`/Tools >
Open Downloads Folder, tracks the last completed download, can open that file,
can reveal it in Explorer, and reports completion/interruption through the
toolbar status area. This is intentionally not a Rust boundary: file-save UX,
dialog ownership, recent-file handoff, folder handoff, and local
Downloads-directory behavior are platform shell concerns.

`BrowserExternalProtocolManager.cs` and `BrowserExternalProtocolPolicy.cs` add
confirmed Windows default-app handoff for non-browser schemes. Page navigations
are canceled before any handoff, then schemes such as `mailto:` or `tel:` can be
opened through Windows only after user confirmation. Browser-internal and unsafe
schemes remain blocked.

`BrowserFindBar.cs` and `BrowserFindPolicy.cs` add the first page-find slice.
`Ctrl+F` opens a native find bar, Enter/Shift+Enter and `F3`/`Shift+F3` move
between matches, and Escape closes the bar. `MainForm.cs` executes a small
`window.find` script through WebView2; query escaping and result parsing are
testable in the policy file. This is Windows-owned page workflow rather than a
Rust-core candidate.

`BrowserPermissionManager.cs` and `BrowserPermissionPolicy.cs` add browser
permission handling. WebView2 `PermissionRequested` is handled by the Windows
shell, which prompts with a native confirmation dialog and sets Allow/Deny on the
WebView2 event. Prompt text and origin formatting are testable, but the dialog
and permission state assignment remain platform-owned.

`BrowserZoomPolicy.cs` adds WebView2 zoom command policy. `MainForm.cs` owns the
active-tab WebView2 `ZoomFactor`; menu items and keyboard accelerators call the
same shell command methods for zoom in, zoom out, and reset. This stays
Windows-owned because zoom factors, accelerators, and status text are desktop
browser shell behavior, not a durable cross-platform schema.

WebView2 `NewWindowRequested` is also handled in `MainForm.cs`. The Windows
shell intercepts target-blank/window-open requests, normalizes browser-safe
addresses through `NavigationPolicy`, and opens them as Actro tabs. Unsafe
new-window URLs are blocked locally.

`NavigationPolicy.cs` keeps that URL/home/scheme rule testable without turning
it into Rust ownership. It is reused by startup URL handling, toolbar
navigation, host-bridge navigation, and home-page reload handling.

`BrowserAddressInputPolicy.cs` sits above `NavigationPolicy` for toolbar input
only. It keeps URL/local-file/home entries strict, turns free text typed by the
user into a search URL through the selected engine, and refuses explicit unsafe
schemes. `BrowserSearchEnginePolicy.cs` adapts the ActroCore engine catalog,
engine-id normalization, and URL builder to Windows, while
`BrowserSearchSettingsStore.cs` persists the menu selection. Startup arguments,
host-bridge navigation, session restore, and page-initiated navigation remain
strict URL flows so invalid external input is not silently converted to search.

`BrowserStartupSettingsStore.cs` owns the first startup restore preference. The
Tools menu can choose whether a normal launch without explicit startup URLs reads
the last `BrowserSessionStore` snapshot or opens the home tab. This is kept in
the Windows shell because it controls startup UX around `MainForm.cs` and
WebView2 tab creation; a future shared preferences schema can move the durable
setting to Rust if all platforms need the same preference contract.

`BrowserNavigationErrorPolicy.cs` adds a safe generated error page for failed
WebView2 navigations. `MainForm.cs` records the attempted tab address, renders
the local error document after `NavigationCompleted` failures, and makes reload
retry the original address instead of refreshing the generated `about:blank`
document. This remains Windows shell UX: Rust may later own durable URL/domain
rules, but not WebView2 error-page rendering or reload routing.

`BrowserProcessFailurePolicy.cs` adds testable status text and reload
classification for WebView2 `ProcessFailed`. `MainForm.cs` keeps the lifecycle
handling local: renderer/frame failures can reload the tab, while broader
browser-process failures surface a status message. This is WebView2 shell
recovery behavior, not a Rust boundary.

`BrowserContentBlockingPolicy.cs` and `BrowserContentBlockingStore.cs` add the
first Windows content-blocking slice. `MainForm.cs` installs a WebView2
`WebResourceRequested` filter, blocks lightweight ad/tracker subresources with a
local response, and persists the Tools menu toggle under local app data. This is
Windows shell behavior for now. A future complete rule-list parser or shared
privacy model may become Rust-owned, but WebView2 interception and menu routing
stay platform-owned.

`BrowserHomeDocumentPolicy.cs` owns the generated native `actro://home`
document. It safely renders the local ActroCore availability summary and leaves
live status refresh to the existing `window.actroHost.getCoreStatus` bridge.
`MainForm.cs` only decides when to show the document. This stays Windows-owned:
start-page HTML, WebView2 document rendering, and browser chrome state are shell
UX, while Rust only provides stable core metadata through `ActroCore.cs`.

`MainForm.cs` also checks WebView2 `NavigationStarting`. Toolbar and host bridge
requests can normalize bare hosts into HTTPS, but page-initiated navigations must
already be absolute and browser-safe. Unsafe schemes such as `javascript:`,
`data:`, or unsupported external protocols are canceled locally.

The next browser-client UI slice is `BrowserTab.cs` and `BrowserTabSidebar.cs`.
Each tab owns its own WebView2 control and host message bridge. `MainForm.cs`
creates, activates, closes, and swaps active tabs while preserving toolbar state
against the active tab only. Closing the final tab creates a fresh home tab, and
File > Duplicate Tab opens the active tab's address through the existing tab
creation/navigation path. This keeps WebView ownership and tab switching readable
inside the Windows app rather than routing UI lifecycle through Rust.

`MainForm.cs` now also provides the page bridge actions:
`OpenTabFromHostAsync`, `CloseTabFromHostAsync`, `SelectTabFromHostAsync`,
`NavigateFromHostAsync`, `GetTabsForHost`, and `GetActiveTabForHost`. These are
intentionally shell actions, not Rust-core calls. The bridge protocol can be
refined later, but WebView ownership, activation policy, close-current-tab
scheduling, and address normalization stay in the Windows app.

`BrowserSessionStore.cs` adds a first Windows-local restore path. On normal
startup, the shell can load a JSON tab snapshot from local app data; `--url`
overrides session restore, and `BrowserStartupSettingsStore.cs` lets the user
turn that normal-launch restore behavior off. On close, `MainForm.cs` writes the
current tab title, address, and active index. The store normalizes addresses
through `NavigationPolicy`, ignores corrupt files, and drops unsafe entries. This
is not a Rust boundary yet: it is shell startup state. Shared Rust persistence
schemas can be introduced later if Windows gains history/bookmark/session
database surfaces that need cross-platform parity.

`BrowserHistoryStore.cs` and `BrowserHistoryDocumentPolicy.cs` add the first
Windows-local history slice. `MainForm.cs` records successful non-private
external navigations, stores a compact JSON history list under local app data,
serves it through the generated `actro://history` document with local filtering
via `BrowserLocalListDocumentPolicy`, removes individual entries through the
local host bridge, and clears that store when normal-window browsing data is
cleared. Private windows do not write, remove, or clear the normal history store.
This should stay Windows-owned until there is a real shared
history/bookmark/session database schema worth moving into Rust.

`BrowserBookmarkStore.cs` and `BrowserBookmarksDocumentPolicy.cs` add the first
Windows-local bookmark slice. `MainForm.cs` handles explicit bookmark/unbookmark
commands, stores a compact JSON bookmark list under local app data, tracks the
active page's checked state in the Bookmarks menu, and serves the list through
the generated `actro://bookmarks` document with local filtering via
`BrowserLocalListDocumentPolicy`. The generated page can remove a bookmark
through the local host bridge; `MainForm.cs` still owns the store write and
active-tab UI synchronization. This should stay Windows-owned until there is a
real shared bookmarks/history database schema worth moving into Rust.

`BrowserWindowStateStore.cs` adds Windows-local primary-window geometry restore.
It persists a compact JSON snapshot for the first normal window's bounds and
maximized state, ignores corrupt state files, and normalizes too-small saved
sizes. Secondary normal windows and private windows do not write this file.
This stays in the Windows shell because it is OS-window placement and launch UX,
not a durable cross-platform data model.

Avoid pushing thin UI event forwarding, route transitions, or WebView2 lifecycle
details into Rust. Those are platform orchestration concerns and would make the
call graph more fragmented without improving reuse.

## Verification

On any host:

```text
npm run verify:windows
```

This validates the Windows project metadata, WebView2 browser shell source,
native tab sidebar, native navigation toolbar, host bridge protocol and browser
methods, local navigation/session/bridge behavior tests, and ActroCore C ABI
adapter. Whenever a .NET SDK is available, it also runs:

```text
dotnet run --project windows/tests/Actro.Windows.Tests.csproj --configuration Debug
dotnet build windows/Actro.Windows.csproj --configuration Debug
```

The verifier allows only the WebView2 package's known `MSB3277` `WindowsBase`
assembly-conflict warning during the Windows app build; other dotnet warnings
fail verification. Non-Windows hosts without a .NET SDK keep the project in the
root verification chain with metadata-only checks. Windows hosts require the .NET
SDK.

To build and copy the Rust DLL on a Windows Rust host:

```text
scripts/build-actro-rust-bindings.sh windows
```

## Packaging

`scripts/actrium/package-windows.sh` (delegating to
`scripts/actrium/package-windows.ps1` on a Windows host) turns the tree under
`windows/` into distributable
artifacts in `dist/windows/`: a portable self-contained win-x64 zip and, when
Inno Setup 6 is present, a per-user installer built from
`windows/packaging/actro-installer.iss`. `windows/VERSION` is
the single version source for the assembly/file version stamps and the
installer version. The installer requires no administrator rights, creates a
per-user Start Menu shortcut, installs the WebView2 Evergreen runtime through
the official bootstrapper only when it is missing, and on uninstall leaves the
user's profile data in `%LOCALAPPDATA%\Actrium\Actro\Windows` untouched.
Artifacts are currently unsigned (no code-signing certificate yet).
