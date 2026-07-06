#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "error: ripgrep (rg) is required for Windows verification" >&2
  exit 127
fi

project="apps/windows/actro/Actro.Windows.csproj"
test_project="apps/windows/actro.tests/Actro.Windows.Tests.csproj"
required_files=(
  "$project"
  "$test_project"
  "apps/windows/actro/src/BrowserApplicationContext.cs"
  "apps/windows/actro/src/Program.cs"
  "apps/windows/actro/src/MainForm.cs"
  "apps/windows/actro/src/BrowserAddressInputPolicy.cs"
  "apps/windows/actro/src/BrowserBrowsingDataPolicy.cs"
  "apps/windows/actro/src/BrowserClosedTabStack.cs"
  "apps/windows/actro/src/BrowserContentBlockingPolicy.cs"
  "apps/windows/actro/src/BrowserContentBlockingStore.cs"
  "apps/windows/actro/src/BrowserDownloadManager.cs"
  "apps/windows/actro/src/BrowserDownloadPolicy.cs"
  "apps/windows/actro/src/BrowserExternalProtocolManager.cs"
  "apps/windows/actro/src/BrowserExternalProtocolPolicy.cs"
  "apps/windows/actro/src/BrowserFindBar.cs"
  "apps/windows/actro/src/BrowserFindPolicy.cs"
  "apps/windows/actro/src/BrowserBookmarksDocumentPolicy.cs"
  "apps/windows/actro/src/BrowserBookmarkStore.cs"
  "apps/windows/actro/src/BrowserHistoryDocumentPolicy.cs"
  "apps/windows/actro/src/BrowserHistoryStore.cs"
  "apps/windows/actro/src/BrowserHomeDocumentPolicy.cs"
  "apps/windows/actro/src/BrowserLocalListDocumentPolicy.cs"
  "apps/windows/actro/src/BrowserAppMenu.cs"
  "apps/windows/actro/src/BrowserEscapePolicy.cs"
  "apps/windows/actro/src/BrowserMenuCommand.cs"
  "apps/windows/actro/src/BrowserMenuModel.cs"
  "apps/windows/actro/src/BrowserNavigationErrorPolicy.cs"
  "apps/windows/actro/src/BrowserPageActionPolicy.cs"
  "apps/windows/actro/src/BrowserPdfPolicy.cs"
  "apps/windows/actro/src/BrowserPermissionManager.cs"
  "apps/windows/actro/src/BrowserPermissionPolicy.cs"
  "apps/windows/actro/src/BrowserProfilePolicy.cs"
  "apps/windows/actro/src/BrowserProcessFailurePolicy.cs"
  "apps/windows/actro/src/BrowserSearchEnginePolicy.cs"
  "apps/windows/actro/src/BrowserSearchSettingsStore.cs"
  "apps/windows/actro/src/BrowserSessionStore.cs"
  "apps/windows/actro/src/BrowserStartupSettingsStore.cs"
  "apps/windows/actro/src/BrowserStorageDatabase.cs"
  "apps/windows/actro/src/BrowserStrings.cs"
  "apps/windows/actro/src/BrowserSyncConnectionDialog.cs"
  "apps/windows/actro/src/BrowserSyncFolderPreflight.cs"
  "apps/windows/actro/src/BrowserSyncProviderConnectionPlanner.cs"
  "apps/windows/actro/src/BrowserSyncProviderDiagnosticsFormatter.cs"
  "apps/windows/actro/src/BrowserSyncProviderSettingsStore.cs"
  "apps/windows/actro/src/BrowserSyncProviderStatusFormatter.cs"
  "apps/windows/actro/src/BrowserSyncSettingsDialog.cs"
  "apps/windows/actro/src/BrowserWindowStateStore.cs"
  "apps/windows/actro/src/BrowserSidebarLayoutPolicy.cs"
  "apps/windows/actro/src/BrowserTab.cs"
  "apps/windows/actro/src/BrowserTabOverviewPanel.cs"
  "apps/windows/actro/src/BrowserTabOverviewPolicy.cs"
  "apps/windows/actro/src/BrowserTabSidebar.cs"
  "apps/windows/actro/src/BrowserTheme.cs"
  "apps/windows/actro/src/BrowserThemeManager.cs"
  "apps/windows/actro/src/BrowserThemeSelectionPolicy.cs"
  "apps/windows/actro/src/BrowserToolStripRenderer.cs"
  "apps/windows/actro/src/BrowserToolbar.cs"
  "apps/windows/actro/src/BrowserToolbarLayoutPolicy.cs"
  "apps/windows/actro/src/BrowserGlyphs.cs"
  "apps/windows/actro/src/BrowserDocumentStylePolicy.cs"
  "apps/windows/actro/src/BrowserZoomPolicy.cs"
  "apps/windows/actro/src/HostMessageBridge.cs"
  "apps/windows/actro/src/HostBridgeProtocol.cs"
  "apps/windows/actro/src/NavigationPolicy.cs"
  "apps/windows/actro/src/WindowsAppPaths.cs"
  "apps/windows/actro/src/WindowsStartupOptions.cs"
  "apps/windows/actro/src/ActroCore.cs"
  "apps/windows/actro.tests/Program.cs"
  "apps/windows/actro/native/README.md"
  "crates/actro-core/include/actro_core.h"
)

failures=0

record_failure() {
  failures=$((failures + 1))
}

print_ok() {
  echo "[ok] $1"
}

print_failure() {
  echo
  echo "[fail] $1"
  record_failure
}

check_dotnet_warnings() {
  local log_file="$1"
  local context="$2"
  local allow_known_windowsbase_warning="$3"
  local unexpected_warnings

  if [[ "$allow_known_windowsbase_warning" == "true" ]]; then
    unexpected_warnings="$(
      rg -i 'warning' "$log_file" \
        | rg -v 'warning MSB3277|Warning\(s\)' \
        || true
    )"
  else
    unexpected_warnings="$(rg -i 'warning' "$log_file" || true)"
  fi

  if [[ -n "$unexpected_warnings" ]]; then
    echo
    echo "error: unexpected dotnet warnings during $context" >&2
    printf '%s\n' "$unexpected_warnings" >&2
    exit 1
  fi

  if [[ "$allow_known_windowsbase_warning" == "true" ]] && rg -q 'warning MSB3277' "$log_file"; then
    print_ok "Windows build emitted only the known WebView2 WindowsBase warning"
  fi
}

missing_files=()
for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing_files+=("$file")
  fi
done

if [[ "${#missing_files[@]}" -ne 0 ]]; then
  print_failure "Windows project files are missing"
  printf '%s\n' "${missing_files[@]}"
else
  print_ok "Windows project files exist"
fi

if ! rg -q '<TargetFramework>net8\.0-windows10\.0\.19041\.0</TargetFramework>' "$project"; then
  print_failure "Windows project target framework is not the expected Windows TFM"
else
  print_ok "Windows project targets net8.0-windows10.0.19041.0"
fi

if ! rg -q '<UseWindowsForms>true</UseWindowsForms>' "$project"; then
  print_failure "Windows project does not enable Windows Forms"
else
  print_ok "Windows project enables Windows Forms"
fi

if ! rg -q '<EnableWindowsTargeting>true</EnableWindowsTargeting>' "$project"; then
  print_failure "Windows project does not enable Windows targeting"
else
  print_ok "Windows project enables Windows targeting"
fi

if ! rg -q 'Microsoft\.Web\.WebView2' "$project"; then
  print_failure "Windows project does not reference WebView2"
else
  print_ok "Windows project references WebView2"
fi

if ! rg -q 'Microsoft\.Data\.Sqlite' "$project" || ! rg -q 'Microsoft\.Data\.Sqlite' "$test_project"; then
  print_failure "Windows browser storage does not reference Microsoft.Data.Sqlite in app and tests"
else
  print_ok "Windows browser storage references Microsoft.Data.Sqlite"
fi

if ! rg -q 'native\\\*\.dll.*CopyToOutputDirectory' "$project"; then
  print_failure "Windows project does not copy native DLL artifacts"
else
  print_ok "Windows project copies native DLL artifacts"
fi

if ! rg -q 'WebView2' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not instantiate WebView2"
else
  print_ok "Windows shell instantiates WebView2"
fi

if ! rg -q 'BrowserApplicationContext' apps/windows/actro/src/Program.cs; then
  print_failure "Windows app does not run through the browser application context"
else
  print_ok "Windows app runs through the browser application context"
fi

for app_context_symbol in BrowserApplicationContext OpenWindow FormClosed ExitThread IsPrivate restoreSession persistSession restoreWindowState persistWindowState isPrivateWindow NewPrivateWindowRequested GetPrivateUserDataFolder TryDeleteUserDataFolder; do
  if ! rg -q "$app_context_symbol" apps/windows/actro/src/BrowserApplicationContext.cs; then
    print_failure "Windows browser application context is missing behavior: $app_context_symbol"
  fi
done

for startup_symbol in WindowsStartupOptions InitialAddresses InitialUrl IsPrivate FromArgs StartsWith '--private' '--private-window' '--incognito'; do
  if ! rg -Fq -- "$startup_symbol" apps/windows/actro/src/WindowsStartupOptions.cs; then
    print_failure "Windows startup options are missing behavior: $startup_symbol"
  fi
done

if ! rg -q 'new BrowserToolbar' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the browser toolbar"
else
  print_ok "Windows shell attaches the browser toolbar"
fi

if ! rg -q 'new BrowserAppMenu' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the compact app menu"
else
  print_ok "Windows shell attaches the compact app menu"
fi

if ! rg -q 'ExecuteCommand' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not route commands through the single command router"
else
  print_ok "Windows shell routes commands through the single command router"
fi

if ! rg -q 'BrowserEscapePolicy' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not resolve Escape through the escape policy"
else
  print_ok "Windows shell resolves Escape through the escape policy"
fi

if rg -q 'MenuStrip|BrowserMainMenu' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell still wires the removed classic menu strip"
else
  print_ok "Windows shell no longer wires a classic menu strip"
fi

if ! rg -q 'new BrowserTabSidebar' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the vertical tab sidebar"
else
  print_ok "Windows shell attaches the vertical tab sidebar"
fi

for sidebar_shell_symbol in SplitContainer Panel1Collapsed BrowserSidebarLayoutPolicy ToggleSidebar ApplySidebarLayout FaviconChanged GetFaviconAsync UpdateTabFaviconAsync; do
  if ! rg -q "$sidebar_shell_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing sidebar behavior: $sidebar_shell_symbol"
  fi
done

for sidebar_policy_symbol in MinWidth DefaultWidth MaxWidth ClampWidth CollapseThreshold ShouldCollapse; do
  if ! rg -q "$sidebar_policy_symbol" apps/windows/actro/src/BrowserSidebarLayoutPolicy.cs; then
    print_failure "Windows sidebar layout policy is missing symbol: $sidebar_policy_symbol"
  fi
done

if ! rg -Fq 'BrowserSidebarLayoutPolicy.cs' "$test_project"; then
  print_failure "Windows tests do not link the sidebar layout policy"
fi

if ! rg -q 'BrowserSidebarLayoutPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise sidebar layout policy"
fi

for menu_command in \
  NewTab \
  NewWindow \
  NewPrivateWindow \
  CloseTab \
  ReopenClosedTab \
  DuplicateTab \
  OpenFile \
  CopyAddress \
  OpenInDefaultBrowser \
  SavePdf \
  Print \
  Exit \
  Back \
  Forward \
  Home \
  ShowHistory \
  BookmarkPage \
  ShowBookmarks \
  Reload \
  Stop \
  ZoomIn \
  ZoomOut \
  ZoomReset \
  FullScreen \
  ToggleSidebar \
  ToggleTabOverview \
  Find \
  FindNext \
  FindPrevious \
  FocusAddress \
  DevTools \
  OpenDownloadsFolder \
  OpenLastDownload \
  RevealLastDownload \
  SelectSearchEngine \
  RestoreSessionToggle \
  BlockAdsToggle \
  ConfigureSync \
  SyncNow \
  SyncStatus \
  ClearBrowsingData
do
  if ! rg -q "$menu_command" apps/windows/actro/src/BrowserMenuCommand.cs \
    || ! rg -q "$menu_command" apps/windows/actro/src/BrowserMenuModel.cs; then
    print_failure "Windows compact menu model is missing command: $menu_command"
  fi
done

for menu_model_symbol in BrowserShortcut BrowserMenuShortcutAlias Aliases MenuOpenShortcuts Argument StringKey IsSeparator IsCheckable BrowserSearchEnginePolicy.Engines; do
  if ! rg -q "$menu_model_symbol" apps/windows/actro/src/BrowserMenuModel.cs; then
    print_failure "Windows compact menu model is missing structure: $menu_model_symbol"
  fi
done

for app_menu_symbol in ContextMenuStrip BrowserToolStripRenderer BrowserMenuModel ShortcutKeyDisplayString CommandRequested SetNavigationState ForeColor DisabledForeground KeyTokenMap; do
  if ! rg -q "$app_menu_symbol" apps/windows/actro/src/BrowserAppMenu.cs; then
    print_failure "Windows compact app menu is missing behavior: $app_menu_symbol"
  fi
done

for escape_policy_symbol in Resolve DismissOverview CloseFindBar ExitFullScreen Stop None overviewVisible; do
  if ! rg -q "$escape_policy_symbol" apps/windows/actro/src/BrowserEscapePolicy.cs; then
    print_failure "Windows escape policy is missing symbol: $escape_policy_symbol"
  fi
done

for menu_test_link in BrowserMenuCommand.cs BrowserMenuModel.cs BrowserEscapePolicy.cs; do
  if ! rg -Fq "$menu_test_link" "$test_project"; then
    print_failure "Windows tests do not link menu policy file: $menu_test_link"
  fi
done

for menu_test_symbol in 'BrowserMenuModel.BuildItems' 'BrowserMenuModel.Aliases' 'BrowserMenuModel.MenuOpenShortcuts' 'BrowserEscapePolicy.Resolve'; do
  if ! rg -Fq "$menu_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise menu behavior: $menu_test_symbol"
  fi
done

for tab_file_pattern in \
  'WebView2' \
  'HostMessageBridge' \
  'IDisposable' \
  'Favicon'
do
  if ! rg -q "$tab_file_pattern" apps/windows/actro/src/BrowserTab.cs; then
    print_failure "Windows browser tab state is missing pattern: $tab_file_pattern"
  fi
done

for tab_sidebar_event in NewTabRequested TabSelected TabCloseRequested UpdateTabs; do
  if ! rg -q "$tab_sidebar_event" apps/windows/actro/src/BrowserTabSidebar.cs; then
    print_failure "Windows vertical tab sidebar is missing behavior: $tab_sidebar_event"
  fi
done

# Tab overview surface: per-tab snapshots are captured on tab deactivation
# and refreshed for the active tab at overview open (CapturePreviewAsync),
# cached on the tab and rendered as a card grid overlaying the content host.
if ! rg -q 'new BrowserTabOverviewPanel' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the tab overview panel"
else
  print_ok "Windows shell attaches the tab overview panel"
fi

for overview_shell_symbol in CapturePreviewAsync CaptureTabSnapshotAsync ShowTabOverview HideTabOverview ToggleTabOverview SyncOverview; do
  if ! rg -q "$overview_shell_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing tab overview behavior: $overview_shell_symbol"
  fi
done

if ! rg -Fq 'Keys.Control | Keys.Shift | Keys.A' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell is missing the tab overview shortcut"
fi

if ! rg -q 'Snapshot' apps/windows/actro/src/BrowserTab.cs; then
  print_failure "Windows browser tab state does not cache the overview snapshot"
fi

for overview_panel_symbol in NewTabRequested TabSelected TabCloseRequested UpdateCards BrowserTabOverviewPolicy BrowserTabOverviewCard; do
  if ! rg -q "$overview_panel_symbol" apps/windows/actro/src/BrowserTabOverviewPanel.cs; then
    print_failure "Windows tab overview panel is missing behavior: $overview_panel_symbol"
  fi
done

for overview_policy_symbol in Columns CardRects CardWidth CardHeight ThumbnailHeight ContentHeight; do
  if ! rg -q "$overview_policy_symbol" apps/windows/actro/src/BrowserTabOverviewPolicy.cs; then
    print_failure "Windows tab overview policy is missing symbol: $overview_policy_symbol"
  fi
done

if ! rg -Fq 'BrowserTabOverviewPolicy.cs' "$test_project"; then
  print_failure "Windows tests do not link the tab overview policy"
fi

if ! rg -q 'BrowserTabOverviewPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise tab overview policy"
fi

for tab_lifecycle_call in CreateAndActivateTabAsync CloseTabAsync ReopenClosedTab DuplicateActiveTab RequestNewWindow BrowserClosedTabStack ActivateTab SyncSidebar ActivateNextTab ActivatePreviousTab ActivateTabAtIndex; do
  if ! rg -q "$tab_lifecycle_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing tab lifecycle behavior: $tab_lifecycle_call"
  fi
done

for tab_shortcut_symbol in TryHandleTabShortcut TryResolveTabShortcutIndex Keys.Tab Keys.PageDown Keys.PageUp Keys.D9 Keys.NumPad9 'Keys.Control | Keys.N' 'Keys.Control | Keys.Shift | Keys.N' 'Keys.Shift | Keys.T'; do
  if ! rg -Fq "$tab_shortcut_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing tab shortcut behavior: $tab_shortcut_symbol"
  fi
done

for closed_tab_symbol in BrowserClosedTabStack CanReopen Push Pop MaxClosedTabs; do
  if ! rg -q "$closed_tab_symbol" apps/windows/actro/src/BrowserClosedTabStack.cs; then
    print_failure "Windows closed-tab stack is missing behavior: $closed_tab_symbol"
  fi
done

if ! rg -q 'AddressRequested' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar does not expose address navigation"
else
  print_ok "Windows browser toolbar exposes address navigation"
fi

for event_name in BackRequested ForwardRequested ReloadRequested NewTabRequested OverviewToggleRequested SidebarToggleRequested BookmarkRequested MenuRequested; do
  if ! rg -q "$event_name" apps/windows/actro/src/BrowserToolbar.cs; then
    print_failure "Windows browser toolbar is missing event: $event_name"
  fi
done

# Explicit decision (Chrome/Edge default): no toolbar Home button. Home
# stays a menu command plus Alt+Home, routed through the command router.
if rg -q 'HomeRequested' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar still exposes the removed Home affordance"
else
  print_ok "Windows browser toolbar has no Home button (home stays menu + Alt+Home)"
fi

if ! rg -q 'BrowserMenuCommand.Home' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell no longer routes the home menu command"
fi

# The toolbar draws MDL2-safe glyphs and lays out its two segments through
# the pure policy; statuses moved to the MainForm overlay.
for toolbar_symbol in BrowserGlyphs BrowserToolbarLayoutPolicy SetSidebarLayout SetNavigationState SetBookmarkState SetOverviewActive; do
  if ! rg -q "$toolbar_symbol" apps/windows/actro/src/BrowserToolbar.cs; then
    print_failure "Windows browser toolbar is missing behavior: $toolbar_symbol"
  fi
done

if rg -q 'SetStatus|statusLabel' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar still owns the removed status column"
else
  print_ok "Windows browser toolbar dropped the fixed status column"
fi

for glyph_symbol in CommandGlyphs GlyphPointSize NewTab TabOverview ToggleSidebar Back Forward Reload Stop BookmarkStar BookmarkStarFilled AppMenu; do
  if ! rg -q "$glyph_symbol" apps/windows/actro/src/BrowserGlyphs.cs; then
    print_failure "Windows browser glyph table is missing symbol: $glyph_symbol"
  fi
done

for toolbar_layout_symbol in Segments CollapsedStubWidth OverviewSlotWidth AddressMaxWidth AddressWidth ButtonWidth ButtonGap EdgePadding; do
  if ! rg -q "$toolbar_layout_symbol" apps/windows/actro/src/BrowserToolbarLayoutPolicy.cs; then
    print_failure "Windows toolbar layout policy is missing symbol: $toolbar_layout_symbol"
  fi
done

for toolbar_policy_test_link in BrowserGlyphs.cs BrowserToolbarLayoutPolicy.cs; do
  if ! rg -Fq "$toolbar_policy_test_link" "$test_project"; then
    print_failure "Windows tests do not link toolbar policy file: $toolbar_policy_test_link"
  fi
done

for toolbar_policy_test_symbol in BrowserGlyphs BrowserToolbarLayoutPolicy; do
  if ! rg -q "$toolbar_policy_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise toolbar policy: $toolbar_policy_test_symbol"
  fi
done

# Statuses live in the bottom-left overlay: transient ones auto-hide,
# pinned (user-invoked) ones persist until Escape, click or navigation.
for status_overlay_symbol in BrowserStatusOverlay BrowserStatusKind ShowTransientStatus ShowPinnedStatus HidePinnedStatus Transient Pinned; do
  if ! rg -q "$status_overlay_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing status overlay behavior: $status_overlay_symbol"
  fi
done

if ! rg -q '<ApplicationHighDpiMode>PerMonitorV2</ApplicationHighDpiMode>' "$project"; then
  print_failure "Windows project does not enable per-monitor DPI v2"
else
  print_ok "Windows project enables per-monitor DPI v2"
fi

for theme_symbol in WM_SETTINGCHANGE ImmersiveColorSet DwmSetWindowAttribute PreferredColorScheme ApplyTheme ApplyWebViewColorScheme DpiChanged OnFormClosedUnsubscribeTheme; do
  if ! rg -q "$theme_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing theme lifecycle behavior: $theme_symbol"
  fi
done

for theme_data_symbol in 'static BrowserTheme Light' 'static BrowserTheme Dark' WindowBackground BarBackground BarForeground Accent SidebarBackground InputBackground DisabledForeground StatusBackground UiFontFamily MonoFontFamilies GlyphFontFamilies ToolbarHeight FindBarHeight 'static int Scale'; do
  if ! rg -q "$theme_data_symbol" apps/windows/actro/src/BrowserTheme.cs; then
    print_failure "Windows browser theme is missing data: $theme_data_symbol"
  fi
done

if ! rg -q 'Resolve' apps/windows/actro/src/BrowserThemeSelectionPolicy.cs; then
  print_failure "Windows theme selection policy is missing the Resolve mapping"
fi

for theme_manager_symbol in AppsUseLightTheme RefreshFromSystem Changed GetFont BrowserThemeSelectionPolicy; do
  if ! rg -q "$theme_manager_symbol" apps/windows/actro/src/BrowserThemeManager.cs; then
    print_failure "Windows theme manager is missing behavior: $theme_manager_symbol"
  fi
done

for renderer_symbol in ProfessionalColorTable OnRenderItemText OnRenderItemCheck OnRenderArrow OnRenderSeparator; do
  if ! rg -q "$renderer_symbol" apps/windows/actro/src/BrowserToolStripRenderer.cs; then
    print_failure "Windows themed menu renderer is missing override: $renderer_symbol"
  fi
done

for document_style_symbol in RootCss 'prefers-color-scheme: dark' ':root'; do
  if ! rg -Fq "$document_style_symbol" apps/windows/actro/src/BrowserDocumentStylePolicy.cs; then
    print_failure "Windows document style policy is missing symbol: $document_style_symbol"
  fi
done

if ! rg -q 'BrowserDocumentStylePolicy.RootCss' \
  apps/windows/actro/src/BrowserHomeDocumentPolicy.cs; then
  print_failure "Windows home document does not embed the shared stylesheet"
fi

if ! rg -q 'BrowserDocumentStylePolicy.RootCss' \
  apps/windows/actro/src/BrowserLocalListDocumentPolicy.cs; then
  print_failure "Windows local list documents do not embed the shared stylesheet"
fi

if ! rg -q 'BrowserDocumentStylePolicy.RootCss' \
  apps/windows/actro/src/BrowserNavigationErrorPolicy.cs; then
  print_failure "Windows navigation error document does not embed the shared stylesheet"
fi

if rg -q 'FlatStyle.System' apps/windows/actro/src/BrowserToolbar.cs apps/windows/actro/src/BrowserFindBar.cs; then
  print_failure "Windows chrome bars still use system-drawn (theme-blind) buttons"
fi

for themed_chrome_symbol in ApplyTheme BrowserThemedButton BrowserThemedBorderPanel; do
  if ! rg -q "$themed_chrome_symbol" apps/windows/actro/src/BrowserToolbar.cs \
    || ! rg -q "$themed_chrome_symbol" apps/windows/actro/src/BrowserFindBar.cs; then
    print_failure "Windows chrome bars are missing themed drawing: $themed_chrome_symbol"
  fi
done

for theme_test_link in BrowserTheme.cs BrowserThemeSelectionPolicy.cs BrowserDocumentStylePolicy.cs; do
  if ! rg -Fq "$theme_test_link" "$test_project"; then
    print_failure "Windows tests do not link theme policy file: $theme_test_link"
  fi
done

for theme_test_symbol in BrowserThemeSelectionPolicy 'BrowserTheme.Metrics.Scale' 'BrowserDocumentStylePolicy.RootCss'; do
  if ! rg -Fq "$theme_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise theme behavior: $theme_test_symbol"
  fi
done

for strings_symbol in 'static string Get' 'static string Format' 'IReadOnlyDictionary<string, string> All'; do
  if ! rg -Fq "$strings_symbol" apps/windows/actro/src/BrowserStrings.cs; then
    print_failure "Windows browser string table is missing API: $strings_symbol"
  fi
done

# The compact app menu consumes its labels as menu.* keys through the menu
# model's string keys. BrowserSyncConnectionDialog stays in the list as the
# development-only reference-server path (reachable solely from the sync
# settings dialog's advanced link). BrowserSyncStatusPolicy and the
# diagnostics formatter render sync status/diagnostics wording exclusively
# through the table.
for strings_consumer in \
  apps/windows/actro/src/MainForm.cs \
  apps/windows/actro/src/BrowserToolbar.cs \
  apps/windows/actro/src/BrowserFindBar.cs \
  apps/windows/actro/src/BrowserAppMenu.cs \
  apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  apps/windows/actro/src/BrowserSyncConnectionDialog.cs \
  apps/windows/actro/src/BrowserSyncStatusPolicy.cs \
  apps/windows/actro/src/BrowserSyncProviderDiagnosticsFormatter.cs
do
  if ! rg -q 'BrowserStrings\.' "$strings_consumer"; then
    print_failure "Windows chrome does not consume the centralized string table: $strings_consumer"
  fi
done

if ! rg -q 'BrowserStrings' "$test_project"; then
  print_failure "Windows tests do not link the browser string table"
fi

for strings_test_symbol in 'BrowserStrings.All' 'BrowserStrings.Get' 'BrowserStrings.Format' CollectFormatPlaceholderIndices; do
  if ! rg -Fq "$strings_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise the browser string table: $strings_test_symbol"
  fi
done

for webview_event in NavigationStarting NavigationCompleted HistoryChanged SourceChanged StatusBarTextChanged DocumentTitleChanged NewWindowRequested PermissionRequested ContainsFullScreenElementChanged ProcessFailed WebResourceRequested; do
  if ! rg -q "$webview_event" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell does not handle WebView2 event: $webview_event"
  fi
done

for full_screen_symbol in ContainsFullScreenElement isBrowserFullScreen isPageFullScreen ToggleBrowserFullScreen ApplyFullScreenState EnterFullScreenMode ExitFullScreenMode RequestExitFullScreen Keys.F11 FormBorderStyle.None FormWindowState.Maximized; do
  if ! rg -q "$full_screen_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing WebView2 full-screen behavior: $full_screen_symbol"
  fi
done

for profile_symbol in GetDefaultUserDataFolder GetPrivateUserDataFolder EnsureUserDataFolder TryDeleteUserDataFolder UserDataFolderName PrivateUserDataParentFolderName; do
  if ! rg -q "$profile_symbol" apps/windows/actro/src/BrowserProfilePolicy.cs; then
    print_failure "Windows browser profile policy is missing symbol: $profile_symbol"
  fi
done

if ! rg -q 'CoreWebView2Environment.CreateAsync' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not create an explicit WebView2 environment"
fi

if ! rg -q 'EnsureCoreWebView2Async\(await GetWebViewEnvironmentAsync\(\)\)' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not initialize WebView2 tabs with the shared profile environment"
fi

if ! rg -q 'WindowsAppPaths' apps/windows/actro/src/BrowserStorageDatabase.cs apps/windows/actro/src/BrowserProfilePolicy.cs; then
  print_failure "Windows local app data paths are not centralized"
fi

# The durable state root honors an absolute-path override so concurrent
# multi-instance runs keep independent state trees; the --user-data-root
# argument feeds the same single choke point.
if ! rg -q 'ACTRO_USER_DATA_ROOT' apps/windows/actro/src/WindowsAppPaths.cs \
  || ! rg -q 'UserDataRootEnvironmentVariable' apps/windows/actro/src/Program.cs \
  || ! rg -q -- '--user-data-root' apps/windows/actro/src/WindowsStartupOptions.cs; then
  print_failure "Windows user data root override is not wired through the single app data path choke point"
else
  print_ok "Windows user data root override routes through WindowsAppPaths"
fi

if ! rg -q 'DownloadStarting' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not handle WebView2 downloads"
else
  print_ok "Windows shell handles WebView2 downloads"
fi

if ! rg -q 'BrowserDownloadManager' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the browser download manager"
else
  print_ok "Windows shell attaches the browser download manager"
fi

for download_symbol in HandleDownloadStarting SaveFileDialog CoreWebView2DownloadOperation TrackDownload setLastCompletedDownload; do
  if ! rg -q "$download_symbol" apps/windows/actro/src/BrowserDownloadManager.cs; then
    print_failure "Windows browser download manager is missing behavior: $download_symbol"
  fi
done

for download_policy_symbol in GetSuggestedFileName SanitizeFileName FormatProgress FallbackFileName GetDefaultDownloadsDirectory ResolveDownloadsDirectory OpenDownloadsFolderStatus OpenDownloadsFolderFailedStatus TryGetAvailableDownloadFile FormatUnavailableDownloadStatus BuildExplorerSelectArguments OpenedLastDownloadStatus OpenLastDownloadFailedStatus RevealedLastDownloadStatus RevealLastDownloadFailedStatus; do
  if ! rg -q "$download_policy_symbol" apps/windows/actro/src/BrowserDownloadPolicy.cs; then
    print_failure "Windows browser download policy is missing symbol: $download_policy_symbol"
  fi
done

if ! rg -q 'BrowserDownloadPolicy.GetDefaultDownloadsDirectory' apps/windows/actro/src/BrowserDownloadManager.cs; then
  print_failure "Windows download manager does not use the shared downloads-folder policy"
fi

if ! rg -q 'BrowserDownloadPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser download policy"
fi

if ! rg -q 'BrowserDownloadPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser download policy"
fi

if ! rg -q 'BrowserExternalProtocolManager' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the external protocol manager"
else
  print_ok "Windows shell attaches the external protocol manager"
fi

for external_manager_symbol in HandleExternalProtocol MessageBox ProcessStartInfo UseShellExecute; do
  if ! rg -q "$external_manager_symbol" apps/windows/actro/src/BrowserExternalProtocolManager.cs; then
    print_failure "Windows external protocol manager is missing behavior: $external_manager_symbol"
  fi
done

for external_policy_symbol in TryCreateExternalProtocolPrompt IsExternalProtocolAddress BrowserExternalProtocolPrompt; do
  if ! rg -q "$external_policy_symbol" apps/windows/actro/src/BrowserExternalProtocolPolicy.cs; then
    print_failure "Windows external protocol policy is missing symbol: $external_policy_symbol"
  fi
done

if ! rg -q 'BrowserExternalProtocolPolicy' "$test_project"; then
  print_failure "Windows tests do not link the external protocol policy"
fi

if ! rg -q 'BrowserExternalProtocolPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise external protocol policy"
fi

if ! rg -q 'BrowserPermissionManager' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the browser permission manager"
else
  print_ok "Windows shell attaches the browser permission manager"
fi

for permission_manager_symbol in HandlePermissionRequested MessageBox CoreWebView2PermissionState; do
  if ! rg -q "$permission_manager_symbol" apps/windows/actro/src/BrowserPermissionManager.cs; then
    print_failure "Windows browser permission manager is missing behavior: $permission_manager_symbol"
  fi
done

for permission_policy_symbol in BuildPrompt DescribePermission DescribeOrigin BrowserPermissionPrompt; do
  if ! rg -q "$permission_policy_symbol" apps/windows/actro/src/BrowserPermissionPolicy.cs; then
    print_failure "Windows browser permission policy is missing symbol: $permission_policy_symbol"
  fi
done

if ! rg -q 'BrowserPermissionPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser permission policy"
fi

if ! rg -q 'BrowserPermissionPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser permission policy"
fi

if ! rg -q 'BrowserClosedTabStack' "$test_project"; then
  print_failure "Windows tests do not link the browser closed-tab stack"
fi

if ! rg -q 'BrowserClosedTabStack' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser closed-tab reopening"
fi

if ! rg -q 'BrowserProfilePolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser profile policy"
fi

if ! rg -q 'BrowserProfilePolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser profile policy"
fi

for startup_test_symbol in 'parses positional startup address' 'parses multiple startup addresses' 'parses private startup mode' 'parses private startup url' InitialAddresses '--private' '--incognito' '--private-window'; do
  if ! rg -Fq -- "$startup_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise private startup behavior: $startup_test_symbol"
  fi
done

for private_profile_test_symbol in GetPrivateUserDataFolder TryDeleteUserDataFolder PrivateUserDataParentFolderName; do
  if ! rg -q "$private_profile_test_symbol" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not exercise private profile behavior: $private_profile_test_symbol"
  fi
done

for browsing_data_symbol in ConfirmationTitle ConfirmationMessage ClearingStatus ClearedStatus FailedStatus; do
  if ! rg -q "$browsing_data_symbol" apps/windows/actro/src/BrowserBrowsingDataPolicy.cs; then
    print_failure "Windows browsing-data policy is missing symbol: $browsing_data_symbol"
  fi
done

if ! rg -q 'BrowserBrowsingDataPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browsing-data policy"
fi

if ! rg -q 'BrowserBrowsingDataPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browsing-data policy"
fi

if ! rg -q 'ClearBrowsingDataAsync' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not clear WebView2 browsing data"
fi

if ! rg -q 'CoreWebView2BrowsingDataKinds' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not choose explicit browsing data kinds"
fi

for find_bar_symbol in BrowserFindBar FindRequested CloseRequested ShowAndFocus 'Query => queryBox.Text' BrowserFindRequestedEventArgs; do
  if ! rg -q "$find_bar_symbol" apps/windows/actro/src/BrowserFindBar.cs; then
    print_failure "Windows browser find bar is missing behavior: $find_bar_symbol"
  fi
done

for find_policy_symbol in BuildWindowFindScript IsFoundResult; do
  if ! rg -q "$find_policy_symbol" apps/windows/actro/src/BrowserFindPolicy.cs; then
    print_failure "Windows browser find policy is missing symbol: $find_policy_symbol"
  fi
done

if ! rg -q 'BrowserFindPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser find policy"
fi

if ! rg -q 'BrowserFindPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser find policy"
fi

for zoom_policy_symbol in ZoomIn ZoomOut Reset FormatZoom CanZoomIn CanZoomOut CanReset Normalize; do
  if ! rg -q "$zoom_policy_symbol" apps/windows/actro/src/BrowserZoomPolicy.cs; then
    print_failure "Windows browser zoom policy is missing symbol: $zoom_policy_symbol"
  fi
done

if ! rg -q 'BrowserZoomPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser zoom policy"
fi

if ! rg -q 'BrowserZoomPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser zoom policy"
fi

for navigation_call in GoBack GoForward Reload Stop NavigationPolicy HandleNavigationStarting HandleWebResourceRequested SyncStatusBarText NavigateBack NavigateForward NavigateHistoryDocument NavigateBookmarksDocument OpenNewWindowRequestAsync ReloadOrStopActiveTab ReloadActiveTab StopLoadingActiveTab OpenDeveloperTools OpenDownloadsFolder OpenLastDownload RevealLastDownloadInFolder TryGetLastCompletedDownloadPath SetLastCompletedDownload HasCompletedDownload ToggleActivePageBookmark RemoveBookmarkFromHostAsync RemoveHistoryEntryFromHostAsync IsAddressBookmarked RecordHistoryVisit ClearLocalHistory CopyActivePageAddress OpenActivePageInDefaultBrowser OpenLocalFile PrintActiveTab ClearBrowsingDataAsync SaveActiveTabAsPdfAsync ToggleContentBlocking PersistContentBlockingSettings SetSearchEngine SetRestorePreviousSession ZoomInActiveTab ZoomOutActiveTab ResetActiveTabZoom ToggleBrowserFullScreen OpenFindBar CloseFindBar FindInActiveTabAsync RepeatFind ReopenClosedTab DuplicateActiveTab RequestNewWindow TryHandleTabShortcut ProcessCmdKey; do
  if ! rg -q "$navigation_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser navigation behavior: $navigation_call"
  fi
done

if ! rg -Fq 'Keys.Control | Keys.J' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell is missing the open-downloads-folder shortcut"
fi

for find_shortcut_symbol in 'Keys.F3' 'Keys.Shift | Keys.F3' lastFindQuery; do
  if ! rg -Fq "$find_shortcut_symbol" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser find shortcut behavior: $find_shortcut_symbol"
  fi
done

for address_input_symbol in TryResolveAddressInput DefaultSearchUrlPrefix; do
  if ! rg -q "$address_input_symbol" apps/windows/actro/src/BrowserAddressInputPolicy.cs; then
    print_failure "Windows address input policy is missing symbol: $address_input_symbol"
  fi
done

if ! rg -q 'BrowserAddressInputPolicy' "$test_project"; then
  print_failure "Windows tests do not link the address input policy"
fi

if ! rg -q 'BrowserAddressInputPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise address input policy"
fi

for search_engine_symbol in Engines DefaultEngineId DefaultSearchUrlPrefix NormalizeEngineId TryGetEngine BuildSearchUrl BrowserSearchEngineInfo; do
  if ! rg -q "$search_engine_symbol" apps/windows/actro/src/BrowserSearchEnginePolicy.cs; then
    print_failure "Windows search engine policy is missing symbol: $search_engine_symbol"
  fi
done

for search_settings_symbol in CreateDefault Load Save CreateSnapshot BrowserSearchSettingsSnapshot; do
  if ! rg -q "$search_settings_symbol" apps/windows/actro/src/BrowserSearchSettingsStore.cs; then
    print_failure "Windows search settings store is missing symbol: $search_settings_symbol"
  fi
done

if ! rg -q 'BrowserSearch' "$test_project"; then
  print_failure "Windows tests do not link search engine policy and settings"
fi

if ! rg -q 'BrowserSearch' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise search engine policy and settings"
fi

for startup_settings_symbol in CreateDefault Load Save CreateSnapshot BrowserStartupSettingsSnapshot RestorePreviousSession; do
  if ! rg -q "$startup_settings_symbol" apps/windows/actro/src/BrowserStartupSettingsStore.cs; then
    print_failure "Windows startup settings store is missing symbol: $startup_settings_symbol"
  fi
done

if ! rg -q 'BrowserStartupSettings' "$test_project"; then
  print_failure "Windows tests do not link startup settings"
fi

if ! rg -q 'BrowserStartupSettings' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise startup settings"
fi

for startup_settings_call in RestoreSessionToggle SetRestorePreviousSession BrowserStartupSettingsStore RestoreInitialTabsAsync; do
  if ! rg -q "$startup_settings_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing startup restore settings behavior: $startup_settings_call"
  fi
done

for search_settings_call in SelectSearchEngine SetSearchEngine BrowserSearchEnginePolicy BrowserSearchSettingsStore TryResolveAddressInput; do
  if ! rg -q "$search_settings_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserAddressInputPolicy.cs; then
    print_failure "Windows shell is missing search engine settings behavior: $search_settings_call"
  fi
done

if ! rg -q 'BrowserAddressInputPolicy.TryResolveAddressInput' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows address bar does not resolve user input through the address input policy"
fi

for home_document_symbol in BuildDocument Title ActroCoreStatus; do
  if ! rg -q "$home_document_symbol" apps/windows/actro/src/BrowserHomeDocumentPolicy.cs; then
    print_failure "Windows home document policy is missing symbol: $home_document_symbol"
  fi
done

if ! rg -q 'BrowserHomeDocumentPolicy' "$test_project"; then
  print_failure "Windows tests do not link the home document policy"
fi

if ! rg -q 'BrowserHomeDocumentPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise home document policy"
fi

if ! rg -q 'BrowserHomeDocumentPolicy.BuildDocument' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not delegate home-page HTML generation to the home document policy"
fi

if rg -q 'BuildDefaultDocument' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows MainForm still owns inline home document generation"
fi

for bookmark_store_symbol in CreateDefault Load Save AddOrUpdate RemoveAddress ContainsAddress CreateSnapshot CanBookmarkAddress TryNormalizeBookmarkAddress BrowserBookmarkSnapshot BrowserBookmarkItemSnapshot; do
  if ! rg -q "$bookmark_store_symbol" apps/windows/actro/src/BrowserBookmarkStore.cs; then
    print_failure "Windows browser bookmark store is missing symbol: $bookmark_store_symbol"
  fi
done

if ! rg -q 'BrowserStorageDatabase' apps/windows/actro/src/BrowserBookmarkStore.cs; then
  print_failure "Windows browser bookmark store does not use the shared browser storage database"
fi

for bookmarks_document_symbol in BuildDocument Title BrowserBookmarkItemSnapshot data-remove-bookmark; do
  if ! rg -q "$bookmarks_document_symbol" apps/windows/actro/src/BrowserBookmarksDocumentPolicy.cs; then
    print_failure "Windows bookmarks document policy is missing symbol: $bookmarks_document_symbol"
  fi
done

for local_list_document_symbol in BuildDocument BrowserLocalListDocumentOptions BrowserLocalListDocumentItem data-list-search data-filter removeHostMethod; do
  if ! rg -q "$local_list_document_symbol" apps/windows/actro/src/BrowserLocalListDocumentPolicy.cs; then
    print_failure "Windows local list document policy is missing symbol: $local_list_document_symbol"
  fi
done

for local_list_document_call in BrowserLocalListDocumentPolicy BrowserLocalListDocumentOptions; do
  if ! rg -q "$local_list_document_call" apps/windows/actro/src/BrowserBookmarksDocumentPolicy.cs apps/windows/actro/src/BrowserHistoryDocumentPolicy.cs "$test_project"; then
    print_failure "Windows generated list pages do not use shared local list behavior: $local_list_document_call"
  fi
done

if ! rg -q 'BrowserBookmark' "$test_project"; then
  print_failure "Windows tests do not link browser bookmark store and document policy"
fi

if ! rg -q 'BrowserBookmark' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser bookmark store and document policy"
fi

if ! rg -q 'migrates legacy browser bookmark json to sqlite' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not cover bookmark JSON-to-SQLite migration"
fi

for bookmark_call in BookmarkPage ShowBookmarks NavigateBookmarksDocument ToggleActivePageBookmark RemoveBookmarkFromHostAsync BrowserBookmarksDocumentPolicy BrowserBookmarkStore IsAddressBookmarked; do
  if ! rg -q "$bookmark_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser bookmark behavior: $bookmark_call"
  fi
done

if ! rg -Fq 'Keys.Control | Keys.D' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell is missing the bookmark-current-page shortcut"
fi

if ! rg -Fq 'Keys.Control | Keys.Shift | Keys.O' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell is missing the show-bookmarks shortcut"
fi

for history_store_symbol in CreateDefault Load Save AddVisit RemoveAddress Clear CreateSnapshot BrowserHistorySnapshot BrowserHistoryItemSnapshot; do
  if ! rg -q "$history_store_symbol" apps/windows/actro/src/BrowserHistoryStore.cs; then
    print_failure "Windows browser history store is missing symbol: $history_store_symbol"
  fi
done

if ! rg -q 'BrowserStorageDatabase' apps/windows/actro/src/BrowserHistoryStore.cs; then
  print_failure "Windows browser history store does not use the shared browser storage database"
fi

for history_document_symbol in BuildDocument Title BrowserHistoryItemSnapshot data-remove-history; do
  if ! rg -q "$history_document_symbol" apps/windows/actro/src/BrowserHistoryDocumentPolicy.cs; then
    print_failure "Windows history document policy is missing symbol: $history_document_symbol"
  fi
done

if ! rg -q 'BrowserHistory' "$test_project"; then
  print_failure "Windows tests do not link browser history store and document policy"
fi

if ! rg -q 'BrowserHistory' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser history store and document policy"
fi

if ! rg -q 'migrates legacy browser history json to sqlite' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not cover history JSON-to-SQLite migration"
fi

for history_call in ShowHistory NavigateHistoryDocument BrowserHistoryDocumentPolicy BrowserHistoryStore RecordHistoryVisit RemoveHistoryEntryFromHostAsync ClearLocalHistory; do
  if ! rg -q "$history_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser history behavior: $history_call"
  fi
done

for content_blocking_symbol in ShouldBlock FormatToggleStatus BlockedStatusCode BlockedReasonPhrase DefaultIsEnabled; do
  if ! rg -q "$content_blocking_symbol" apps/windows/actro/src/BrowserContentBlockingPolicy.cs; then
    print_failure "Windows content blocking policy is missing symbol: $content_blocking_symbol"
  fi
done

for content_blocking_store_symbol in CreateDefault Load Save CreateSnapshot BrowserContentBlockingSnapshot; do
  if ! rg -q "$content_blocking_store_symbol" apps/windows/actro/src/BrowserContentBlockingStore.cs; then
    print_failure "Windows content blocking store is missing symbol: $content_blocking_store_symbol"
  fi
done

if ! rg -q 'BrowserContentBlocking' "$test_project"; then
  print_failure "Windows tests do not link content blocking policy and store"
fi

if ! rg -q 'BrowserContentBlocking' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise content blocking policy and store"
fi

for content_blocking_call in AddWebResourceRequestedFilter WebResourceRequested HandleWebResourceRequested CreateWebResourceResponse BrowserContentBlockingPolicy BrowserContentBlockingStore ToggleContentBlocking SetContentBlockingEnabled; do
  if ! rg -q "$content_blocking_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing content blocking behavior: $content_blocking_call"
  fi
done

for process_failure_symbol in FormatStatus FormatRecoveryStatus FormatRecoveryFailedStatus ShouldReloadTab; do
  if ! rg -q "$process_failure_symbol" apps/windows/actro/src/BrowserProcessFailurePolicy.cs; then
    print_failure "Windows process failure policy is missing symbol: $process_failure_symbol"
  fi
done

if ! rg -q 'BrowserProcessFailurePolicy' "$test_project"; then
  print_failure "Windows tests do not link the process failure policy"
fi

if ! rg -q 'BrowserProcessFailurePolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise process failure policy"
fi

for process_failure_call in HandleProcessFailed BrowserProcessFailurePolicy ShouldReloadTab Reload; do
  if ! rg -q "$process_failure_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserProcessFailurePolicy.cs; then
    print_failure "Windows shell is missing process failure recovery behavior: $process_failure_call"
  fi
done

for navigation_error_symbol in FormatStatus BuildErrorDocument DefaultTitle; do
  if ! rg -q "$navigation_error_symbol" apps/windows/actro/src/BrowserNavigationErrorPolicy.cs; then
    print_failure "Windows navigation error policy is missing symbol: $navigation_error_symbol"
  fi
done

if ! rg -q 'BrowserNavigationErrorPolicy' "$test_project"; then
  print_failure "Windows tests do not link the navigation error policy"
fi

if ! rg -q 'BrowserNavigationErrorPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise navigation error policy"
fi

for navigation_error_call in ShowNavigationError BrowserNavigationErrorPolicy IsShowingErrorPage NavigateToString; do
  if ! rg -q "$navigation_error_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserTab.cs; then
    print_failure "Windows shell is missing navigation error behavior: $navigation_error_call"
  fi
done

if ! rg -q 'NavigationPolicy.TryNormalizeAddress\(activeTab.Address, out var retryAddress\)' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows reload does not retry the original address from a navigation error page"
fi

for page_action_symbol in TryGetCopyableAddress TryGetExternalAddress CopiedAddressStatus NoCopyableAddressStatus OpenedExternalStatus NoExternalAddressStatus OpenExternalFailedStatus; do
  if ! rg -q "$page_action_symbol" apps/windows/actro/src/BrowserPageActionPolicy.cs; then
    print_failure "Windows page action policy is missing symbol: $page_action_symbol"
  fi
done

if ! rg -q 'BrowserPageActionPolicy' "$test_project"; then
  print_failure "Windows tests do not link the page action policy"
fi

if ! rg -q 'BrowserPageActionPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise page action policy"
fi

if ! rg -q 'Clipboard.SetText' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not copy page addresses through the clipboard"
fi

if ! rg -q 'ProcessStartInfo' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not open page addresses through the system shell"
fi

if ! rg -q 'OpenFileDialog' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not expose native file opening"
fi

if ! rg -q 'PrintToPdfAsync' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not save WebView2 pages as PDF"
fi

for pdf_policy_symbol in GetSuggestedFileName FormatSaveStatus DefaultPdfFileName; do
  if ! rg -q "$pdf_policy_symbol" apps/windows/actro/src/BrowserPdfPolicy.cs; then
    print_failure "Windows browser PDF policy is missing symbol: $pdf_policy_symbol"
  fi
done

if ! rg -q 'BrowserPdfPolicy' "$test_project"; then
  print_failure "Windows tests do not link the browser PDF policy"
fi

if ! rg -q 'BrowserPdfPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser PDF policy"
fi

if ! rg -q 'ShowPrintUI' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not expose WebView2 print UI"
fi

if ! rg -q 'ExecuteScriptAsync' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not execute page find scripts"
else
  print_ok "Windows shell executes page find scripts"
fi

if ! rg -q 'FocusAddress' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar does not expose address focus"
fi

for policy_symbol in TryNormalizeAddress TryNormalizeLocalFilePath IsAllowedNavigationAddress IsAllowedNavigationUri IsHomeAddress IsHistoryAddress IsBookmarksAddress IsInternalAddress HomeAddress HistoryAddress BookmarksAddress; do
  if ! rg -q "$policy_symbol" apps/windows/actro/src/NavigationPolicy.cs; then
    print_failure "Windows navigation policy is missing symbol: $policy_symbol"
  fi
done

if ! rg -q 'NavigationPolicy' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise navigation policy"
fi

if ! rg -q 'BrowserSessionStore' "$test_project"; then
  print_failure "Windows tests do not link the browser session store"
fi

if ! rg -q 'BrowserSessionStore' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser session persistence"
fi

if ! rg -q 'migrates legacy browser session json to sqlite' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not cover session JSON-to-SQLite migration"
fi

if ! rg -q 'migrates browser storage sqlite v1 v2 v3 to v6' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not cover browser storage v1/v2/v3 SQLite migration"
fi

for session_symbol in BrowserSessionSnapshot BrowserSessionTabSnapshot CreateSnapshot Load Save; do
  if ! rg -q "$session_symbol" apps/windows/actro/src/BrowserSessionStore.cs; then
    print_failure "Windows browser session store is missing symbol: $session_symbol"
  fi
done

if ! rg -q 'BrowserStorageDatabase' apps/windows/actro/src/BrowserSessionStore.cs; then
  print_failure "Windows browser session store does not use the shared browser storage database"
fi

for storage_symbol in LoadBrowserStorageSchema LoadBrowserStorageSyncSqlJson LoadBrowserStorageSyncCloudStoreContractJson LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson BuildBrowserStorageSyncCloudPushPlanJson BuildBrowserStorageSyncCloudPushResultPlanJson BuildBrowserStorageSyncCloudPullPlanJson BuildBrowserStorageSyncCloudPullApplyPlanJson BuildBrowserStorageSyncCloudCompactionPlanJson BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson NormalizeBrowserStorageSyncBatchJson BuildBrowserStorageSyncBatchFromDirtyRowsJson BuildBrowserStorageSyncPushRequestFromDirtyRowsJson NormalizeBrowserStorageSyncAckBatchJson NormalizeBrowserStorageSyncCursorsJson BuildBrowserStorageSyncPushRequestJson BuildBrowserStorageSyncPullRequestJson BuildBrowserStorageSyncCycleResponseJson BuildBrowserStorageSyncAckPlanJson BuildBrowserStorageSyncRemoteApplyPlanJson BuildBrowserStorageSyncRemoteApplyResultJson ApplyRemoteSyncPlan ApplySyncAckPlan ApplyCloudCycleClientPlan ExportDirtySyncBatch ExportDirtySyncPushRequest SelectDirtyStatements BrowserStorageSyncSqlContract Microsoft.Data.Sqlite browser_history_pages browser_bookmarks browser_sessions browser_sync_cursors; do
  if ! rg -q "$storage_symbol" apps/windows/actro/src/BrowserStorageDatabase.cs apps/windows/actro/src/ActroCore.cs; then
    print_failure "Windows browser storage database is missing shared storage behavior: $storage_symbol"
  fi
done

if ! rg -q 'RunLocalFolderSyncCycleAsync' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q 'BuildBrowserStorageSyncCloudLocalFolderCapabilityPreflightJson' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q 'BuildBrowserStorageSyncCloudLocalFolderCycleRoundJson' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q 'BuildBrowserStorageSyncCloudCycleAttemptPlanJson' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q 'runs browser storage local-folder cloud sync client cycle' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows local-folder cloud sync client is not wired through the Rust cycle helper"
else
  print_ok "Windows local-folder cloud sync client consumes the Rust local-folder cycle helper"
fi

# The provider connection branch owns browser sync when a cloud provider is
# configured: settings store + planner feed the Rust connection status policy
# and the coordinator runs the local-folder rounds from that routing.
if ! rg -q 'BrowserSyncProviderSettingsStore' apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  || ! rg -q 'BrowserSyncProviderConnectionPlanner' apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  || ! rg -q 'RunLocalFolderSyncCycleAsync' apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  || ! rg -q 'RunLocalFolderMembershipRoundAsync' apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  || ! rg -q 'BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson' apps/windows/actro/src/BrowserSyncProviderConnectionPlanner.cs \
  || ! rg -q 'MembershipJoinedAtUnixSeconds' apps/windows/actro/src/BrowserSyncProviderSettingsStore.cs \
  || ! rg -q 'BrowserSyncProviderSettingsStore' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows browser sync coordinator does not route through the provider connection"
else
  print_ok "Windows browser sync coordinator routes through the provider connection"
fi

for provider_test_link in BrowserSyncProviderSettingsStore.cs BrowserSyncProviderConnectionPlanner.cs BrowserSyncFolderPreflight.cs BrowserSyncProviderStatusFormatter.cs BrowserSyncProviderDiagnosticsFormatter.cs; do
  if ! rg -Fq "$provider_test_link" "$test_project"; then
    print_failure "Windows tests do not link provider connection file: $provider_test_link"
  fi
done

for provider_test_case in \
  'persists browser sync provider settings' \
  'plans browser sync provider connection routing' \
  'translates blocked provider connection policy into scheduler state' \
  'routes browser sync coordinator through the provider connection' \
  'shapes blocked browser sync provider coordinator results' \
  'browser sync scheduler consumes cloud and blocked provider schedule states' \
  'gates browser sync folder acceptance through preflight' \
  'formats browser sync provider connection status for the settings dialog' \
  'formats provider sync diagnostics from local-folder round envelopes' \
  'centralizes browser sync status display strings' \
  'disconnects browser sync provider and endpoint session'
do
  if ! rg -Fq "$provider_test_case" apps/windows/actro.tests/Program.cs; then
    print_failure "Windows tests do not cover provider connection behavior: $provider_test_case"
  fi
done

# The provider settings dialog is the product ConfigureSync surface: folder
# acceptance is gated by the pure preflight over the Rust capability /
# adapter / compatibility contracts, the Connection row renders through the
# pure status formatter (including the explicit reference-server-active
# state), and Disconnect stops browser sync entirely (provider fields plus
# the endpoint credential session). The endpoint-token dialog remains
# development-only, constructed in MainForm but reachable solely from the
# settings dialog's reference-server link.
if ! rg -q 'new BrowserSyncSettingsDialog' apps/windows/actro/src/MainForm.cs \
  || ! rg -q 'ConfigureReferenceServerConnection' apps/windows/actro/src/MainForm.cs \
  || ! rg -q 'new BrowserSyncConnectionDialog' apps/windows/actro/src/MainForm.cs \
  || ! rg -q 'BrowserSyncFolderPreflight' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'BrowserSyncProviderStatusFormatter' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'FolderBrowserDialog' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'DisconnectAndClearEndpointSession' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'BuildBrowserStorageSyncCloudLocalFolderCompatibilityPreflightJson' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'referenceServerLink' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'FormatReferenceServerActive' apps/windows/actro/src/BrowserSyncProviderStatusFormatter.cs \
  || ! rg -q 'DisconnectAndClearEndpointSession' apps/windows/actro/src/BrowserSyncProviderSettingsStore.cs; then
  print_failure "Windows browser sync settings dialog does not front the provider connection"
else
  print_ok "Windows browser sync settings dialog fronts the provider connection"
fi

# The diagnostics surface: every local-folder round envelope embeds the Rust
# provider error classification next to the provider diagnostics, the
# coordinator keeps the last round's summary in memory (nothing persisted),
# and the settings dialog renders it through the pure diagnostics formatter
# behind the collapsible detail. Status line display strings are centralized
# in BrowserStrings (status.sync.*) with the policy reading through them.
if ! rg -q 'BuildBrowserStorageSyncCloudProviderErrorClassificationJson' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q '"providerErrorClassification"' apps/windows/actro/src/BrowserStorageSyncClient.cs \
  || ! rg -q 'providerErrorClassification' apps/windows/actro/src/BrowserSyncProviderDiagnosticsFormatter.cs \
  || ! rg -q 'providerDiagnostics' apps/windows/actro/src/BrowserSyncProviderDiagnosticsFormatter.cs \
  || ! rg -q 'LastProviderRoundDiagnosticsJson' apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  || ! rg -q 'LastProviderRoundDiagnosticsJson' apps/windows/actro/src/MainForm.cs \
  || ! rg -q 'BrowserSyncProviderDiagnosticsFormatter' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'showDiagnostics' apps/windows/actro/src/BrowserSyncSettingsDialog.cs \
  || ! rg -q 'status\.sync\.' apps/windows/actro/src/BrowserSyncStatusPolicy.cs \
  || ! rg -q 'status\.sync\.notConfigured' apps/windows/actro/src/BrowserSyncStatusPolicy.cs; then
  print_failure "Windows browser sync diagnostics surface is not wired through the Rust round envelopes"
else
  print_ok "Windows browser sync diagnostics surface renders the Rust round envelopes"
fi

for session_call in RestoreInitialTabsAsync PersistSession FormClosing BrowserSessionStore; do
  if ! rg -q "$session_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser session lifecycle call: $session_call"
  fi
done

if ! rg -q 'BrowserWindowStateStore' "$test_project"; then
  print_failure "Windows tests do not link the browser window state store"
fi

if ! rg -q 'BrowserWindowStateStore' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise browser window state persistence"
fi

for window_state_symbol in BrowserWindowStateSnapshot CreateDefault CreateSnapshot Load Save IsMaximized MinimumWidth MinimumHeight; do
  if ! rg -q "$window_state_symbol" apps/windows/actro/src/BrowserWindowStateStore.cs; then
    print_failure "Windows browser window state store is missing symbol: $window_state_symbol"
  fi
done

for window_state_call in RestoreWindowState PersistWindowState BrowserWindowStateStore RestoreBounds WindowState; do
  if ! rg -q "$window_state_call" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing browser window state lifecycle call: $window_state_call"
  fi
done

if ! rg -q 'HostBridgeProtocol' "$test_project"; then
  print_failure "Windows tests do not link the host bridge protocol"
fi

if ! rg -q 'HostBridgeDispatcher' apps/windows/actro.tests/Program.cs; then
  print_failure "Windows tests do not exercise the host bridge dispatcher"
fi

if ! rg -q 'new HostMessageBridge' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell does not attach the host message bridge"
else
  print_ok "Windows shell attaches the host message bridge"
fi

if ! rg -q 'HostBridgeDispatcher' apps/windows/actro/src/HostMessageBridge.cs; then
  print_failure "Windows WebView2 bridge does not delegate to the host bridge dispatcher"
fi

if ! rg -q 'WebMessageReceived' apps/windows/actro/src/HostMessageBridge.cs; then
  print_failure "Windows host bridge does not receive WebView2 messages"
else
  print_ok "Windows host bridge receives WebView2 messages"
fi

if ! rg -q 'PostWebMessageAsJson' apps/windows/actro/src/HostMessageBridge.cs; then
  print_failure "Windows host bridge does not post JSON responses to WebView2"
else
  print_ok "Windows host bridge posts JSON responses to WebView2"
fi

if ! rg -q 'AddScriptToExecuteOnDocumentCreatedAsync' apps/windows/actro/src/HostMessageBridge.cs; then
  print_failure "Windows host bridge does not inject the page bootstrap"
else
  print_ok "Windows host bridge injects the page bootstrap"
fi

for host_method in \
  'actro\.host\.getCoreStatus' \
  'actro\.host\.openTab' \
  'actro\.host\.closeTab' \
  'actro\.host\.selectTab' \
	  'actro\.host\.navigate' \
	  'actro\.host\.getTabs' \
	  'actro\.host\.getActiveTab' \
	  'actro\.host\.removeBookmark' \
	  'actro\.host\.removeHistoryEntry'
do
  if ! rg -q "$host_method" apps/windows/actro/src/HostMessageBridge.cs apps/windows/actro/src/HostBridgeProtocol.cs; then
    print_failure "Windows host bridge does not expose method: $host_method"
  fi
done

if ! rg -q 'HostBridgeActions' apps/windows/actro/src/HostBridgeProtocol.cs; then
  print_failure "Windows host bridge does not delegate host actions"
else
  print_ok "Windows host bridge delegates host actions"
fi

for host_action in \
  OpenTabFromHostAsync \
  CloseTabFromHostAsync \
  SelectTabFromHostAsync \
	  NavigateFromHostAsync \
	  GetTabsForHost \
	  GetActiveTabForHost \
	  RemoveBookmarkFromHostAsync \
	  RemoveHistoryEntryFromHostAsync
do
  if ! rg -q "$host_action" apps/windows/actro/src/MainForm.cs; then
    print_failure "Windows shell is missing host bridge action: $host_action"
  fi
done

if ! rg -q 'HostOpenTabPayload' apps/windows/actro/src/HostBridgeProtocol.cs; then
  print_failure "Windows host bridge does not return tab payloads"
else
  print_ok "Windows host bridge exposes tab payloads"
fi

if ! rg -q 'forbidden_source' apps/windows/actro/src/HostBridgeProtocol.cs; then
  print_failure "Windows host bridge does not enforce a source allowlist"
else
  print_ok "Windows host bridge enforces a source allowlist"
fi

if ! rg -q 'DllImport\(LibraryName' apps/windows/actro/src/ActroCore.cs; then
  print_failure "Windows ActroCore adapter does not use the centralized native library import"
else
  print_ok "Windows ActroCore adapter centralizes native library imports"
fi

for symbol in \
  actro_core_c_api_version \
  actro_core_product_name \
  actro_core_supported_contract_name \
  actro_core_supported_contract_version \
  actro_core_schema_version \
  actro_core_runtime_schema_version \
  actro_core_browser_storage_sync_sql_json \
  actro_core_browser_storage_sync_batch_normalize_json \
  actro_core_browser_storage_sync_ack_batch_normalize_json \
  actro_core_browser_storage_sync_cursors_normalize_json \
  actro_core_browser_storage_sync_batch_empty_json \
  actro_core_browser_storage_sync_push_request_json \
  actro_core_browser_storage_sync_pull_request_json \
  actro_core_browser_storage_sync_cycle_response_json \
  actro_core_browser_storage_sync_cloud_store_contract_json \
  actro_core_browser_storage_sync_cloud_data_model_json \
  actro_core_browser_storage_sync_cloud_default_manifest_json \
  actro_core_browser_storage_sync_cloud_manifest_normalize_json \
  actro_core_browser_storage_sync_cloud_device_normalize_json \
  actro_core_browser_storage_sync_cloud_log_object_json \
  actro_core_browser_storage_sync_cloud_log_object_normalize_json \
  actro_core_browser_storage_sync_cloud_snapshot_object_json \
  actro_core_browser_storage_sync_cloud_snapshot_object_normalize_json \
  actro_core_browser_storage_sync_cloud_push_plan_json \
  actro_core_browser_storage_sync_cloud_push_result_plan_json \
  actro_core_browser_storage_sync_cloud_pull_plan_json \
  actro_core_browser_storage_sync_cloud_pull_apply_plan_json \
  actro_core_browser_storage_sync_cloud_compaction_plan_json \
  actro_core_browser_storage_sync_cloud_compaction_result_plan_json \
  actro_core_browser_storage_sync_cycle_request_plan_json \
  actro_core_browser_storage_sync_cycle_transport_result_json \
  actro_core_browser_storage_sync_status_policy_json \
  actro_core_browser_storage_sync_schedule_state_json \
  actro_core_browser_storage_sync_scheduler_status_json \
  actro_core_browser_storage_sync_ack_plan_json \
  actro_core_browser_storage_sync_remote_apply_plan_json \
  actro_core_browser_storage_sync_remote_apply_result_json
do
  if ! rg -q "$symbol" crates/actro-core/include/actro_core.h; then
    print_failure "Rust C ABI header is missing symbol: $symbol"
  fi
  if ! rg -q "$symbol" apps/windows/actro/src/ActroCore.cs; then
    print_failure "Windows ActroCore adapter is missing symbol: $symbol"
  fi
done

is_windows_host=false
case "$(uname -s 2>/dev/null || echo unknown)" in
  MINGW*|MSYS*|CYGWIN*)
    is_windows_host=true
    ;;
esac

if [[ "${OS:-}" == "Windows_NT" ]]; then
  is_windows_host=true
fi

if [[ "$failures" -ne 0 ]]; then
  echo
  echo "Windows verification failed before build with $failures issue(s)"
  exit 1
fi

dotnet_cmd="${DOTNET:-dotnet}"

if command -v "$dotnet_cmd" >/dev/null 2>&1; then
  test_log="$(mktemp -t actro-windows-tests.XXXXXX)"
  build_log="$(mktemp -t actro-windows-build.XXXXXX)"
  trap 'rm -f "$test_log" "$build_log"' EXIT

  "$dotnet_cmd" run --project "$test_project" --configuration Debug -p:NuGetAudit=false 2>&1 | tee "$test_log"
  check_dotnet_warnings "$test_log" "Windows tests" false

  "$dotnet_cmd" build "$project" --configuration Debug -p:NuGetAudit=false 2>&1 | tee "$build_log"
  check_dotnet_warnings "$build_log" "Windows app build" true
elif [[ "$is_windows_host" == "true" ]]; then
  echo "error: dotnet SDK is required to build Actro Windows on Windows hosts" >&2
  exit 127
else
  echo
  echo "Windows project metadata passed; dotnet SDK not found, skipping build on non-Windows host"
fi
