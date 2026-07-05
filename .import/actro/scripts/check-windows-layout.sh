#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

if ! command -v rg >/dev/null 2>&1; then
  echo "error: ripgrep (rg) is required for Windows layout checks" >&2
  exit 127
fi

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

check_pattern_in_each_file() {
  local pattern="$1"
  local success_message="$2"
  local failure_message="$3"
  shift 3

  local missing_matches=()
  for file in "$@"; do
    if [[ ! -f "$file" ]] || ! rg -q "$pattern" "$file"; then
      missing_matches+=("$file")
    fi
  done

  if [[ "${#missing_matches[@]}" -ne 0 ]]; then
    print_failure "$failure_message"
    printf '%s\n' "${missing_matches[@]}"
  else
    print_ok "$success_message"
  fi
}

required_files=(
  "apps/windows/README.md"
  "apps/windows/actro/README.md"
  "apps/windows/actro/Actro.Windows.csproj"
  "apps/windows/actro/native/README.md"
  "apps/windows/actro/src/ActroCore.cs"
  "apps/windows/actro/src/BrowserAddressInputPolicy.cs"
  "apps/windows/actro/src/BrowserApplicationContext.cs"
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
  "apps/windows/actro/src/BrowserStrings.cs"
  "apps/windows/actro/src/BrowserSyncProviderConnectionPlanner.cs"
  "apps/windows/actro/src/BrowserSyncProviderSettingsStore.cs"
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
  "apps/windows/actro/src/Program.cs"
  "apps/windows/actro/src/MainForm.cs"
  "apps/windows/actro/src/WindowsAppPaths.cs"
  "apps/windows/actro.tests/Actro.Windows.Tests.csproj"
  "apps/windows/actro.tests/Program.cs"
  "crates/actro-core/include/actro_core.h"
  "scripts/verify-windows.sh"
  "docs/migration/windows-client.md"
  "apps/windows/VERSION"
  "apps/windows/actro/packaging/actro-installer.iss"
  "scripts/package-windows.sh"
  "scripts/package-windows.ps1"
)

missing_files=()
for file in "${required_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    missing_files+=("$file")
  fi
done

if [[ "${#missing_files[@]}" -ne 0 ]]; then
  print_failure "Windows project/layout files are missing"
  printf '%s\n' "${missing_files[@]}"
else
  print_ok "Windows project/layout files exist"
fi

check_pattern_in_each_file \
  'apps/windows/actro/' \
  "Windows docs point at apps/windows/actro/" \
  "Windows docs do not point at apps/windows/actro/" \
  apps/windows/README.md \
  docs/migration/windows-client.md

check_pattern_in_each_file \
  'WebView2' \
  "Windows app docs record WebView2 as the expected shell host" \
  "Windows app docs do not record WebView2 as the expected shell host" \
  apps/windows/actro/README.md \
  docs/migration/windows-client.md

check_pattern_in_each_file \
  'third_party/rustdesk-actr' \
  "Windows docs keep rustdesk-actr separate from the browser client" \
  "Windows docs do not separate rustdesk-actr from the Windows browser client" \
  apps/windows/actro/README.md \
  docs/migration/windows-client.md

tracked_rustdesk_app_files="$(
  git ls-files apps/rustdesk-actr 2>/dev/null || true
)"

if [[ -n "$tracked_rustdesk_app_files" ]]; then
  print_failure "tracked rustdesk scratch files exist under apps/"
  printf '%s\n' "$tracked_rustdesk_app_files"
else
  print_ok "rustdesk scratch app is not tracked"
fi

check_pattern_in_each_file \
  'Microsoft\.Web\.WebView2' \
  "Windows project references WebView2" \
  "Windows project does not reference WebView2" \
  apps/windows/actro/Actro.Windows.csproj

check_pattern_in_each_file \
  'BrowserApplicationContext' \
  "Windows app runs through the browser application context" \
  "Windows app does not run through the browser application context" \
  apps/windows/actro/src/Program.cs

missing_application_context_symbols=()
for app_context_symbol in BrowserApplicationContext OpenWindow FormClosed ExitThread IsPrivate restoreSession persistSession restoreWindowState persistWindowState isPrivateWindow NewPrivateWindowRequested GetPrivateUserDataFolder TryDeleteUserDataFolder; do
  if ! rg -q "$app_context_symbol" apps/windows/actro/src/BrowserApplicationContext.cs; then
    missing_application_context_symbols+=("$app_context_symbol")
  fi
done

if [[ "${#missing_application_context_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser application context is missing expected behavior"
  printf '%s\n' "${missing_application_context_symbols[@]}"
else
  print_ok "Windows browser application context owns multi-window lifecycle"
fi

missing_startup_option_symbols=()
for startup_symbol in WindowsStartupOptions InitialAddresses InitialUrl IsPrivate FromArgs StartsWith '--private' '--private-window' '--incognito'; do
  if ! rg -Fq -- "$startup_symbol" apps/windows/actro/src/WindowsStartupOptions.cs; then
    missing_startup_option_symbols+=("$startup_symbol")
  fi
done

if [[ "${#missing_startup_option_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows startup options are missing private-window behavior"
  printf '%s\n' "${missing_startup_option_symbols[@]}"
else
  print_ok "Windows startup options expose private-window mode"
fi

check_pattern_in_each_file \
  'new BrowserToolbar' \
  "Windows shell attaches a browser toolbar" \
  "Windows shell does not attach a browser toolbar" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'new BrowserAppMenu' \
  "Windows shell attaches a compact app menu" \
  "Windows shell does not attach a compact app menu" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'ExecuteCommand' \
  "Windows shell routes commands through the single command router" \
  "Windows shell does not route commands through the single command router" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'BrowserEscapePolicy' \
  "Windows shell resolves Escape through the escape policy" \
  "Windows shell does not resolve Escape through the escape policy" \
  apps/windows/actro/src/MainForm.cs

if rg -q 'MenuStrip|BrowserMainMenu' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows shell still wires the removed classic menu strip"
else
  print_ok "Windows shell no longer wires a classic menu strip"
fi

check_pattern_in_each_file \
  'new BrowserTabSidebar' \
  "Windows shell attaches a vertical tab sidebar" \
  "Windows shell does not attach a vertical tab sidebar" \
  apps/windows/actro/src/MainForm.cs

missing_sidebar_shell_symbols=()
for sidebar_shell_symbol in SplitContainer Panel1Collapsed BrowserSidebarLayoutPolicy ToggleSidebar ApplySidebarLayout FaviconChanged GetFaviconAsync UpdateTabFaviconAsync; do
  if ! rg -q "$sidebar_shell_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_sidebar_shell_symbols+=("$sidebar_shell_symbol")
  fi
done

if [[ "${#missing_sidebar_shell_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing sidebar split/collapse/favicon behavior"
  printf '%s\n' "${missing_sidebar_shell_symbols[@]}"
else
  print_ok "Windows shell owns sidebar split, collapse and favicon behavior"
fi

missing_sidebar_policy_symbols=()
for sidebar_policy_symbol in MinWidth DefaultWidth MaxWidth ClampWidth CollapseThreshold ShouldCollapse; do
  if ! rg -q "$sidebar_policy_symbol" apps/windows/actro/src/BrowserSidebarLayoutPolicy.cs; then
    missing_sidebar_policy_symbols+=("$sidebar_policy_symbol")
  fi
done

if [[ "${#missing_sidebar_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows sidebar layout policy is missing expected symbols"
  printf '%s\n' "${missing_sidebar_policy_symbols[@]}"
else
  print_ok "Windows sidebar layout policy owns width clamp and collapse rules"
fi

check_pattern_in_each_file \
  'BrowserSidebarLayoutPolicy' \
  "Windows tests link and exercise the sidebar layout policy" \
  "Windows tests do not link and exercise the sidebar layout policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'WebView2' \
  "Windows browser tab state owns WebView2 instances" \
  "Windows browser tab state does not own WebView2 instances" \
  apps/windows/actro/src/BrowserTab.cs

check_pattern_in_each_file \
  'HostMessageBridge' \
  "Windows browser tab state carries host bridge ownership" \
  "Windows browser tab state does not carry host bridge ownership" \
  apps/windows/actro/src/BrowserTab.cs

missing_tab_sidebar_events=()
for tab_sidebar_event in NewTabRequested TabSelected TabCloseRequested UpdateTabs; do
  if ! rg -q "$tab_sidebar_event" apps/windows/actro/src/BrowserTabSidebar.cs; then
    missing_tab_sidebar_events+=("$tab_sidebar_event")
  fi
done

if [[ "${#missing_tab_sidebar_events[@]}" -ne 0 ]]; then
  print_failure "Windows vertical tab sidebar is missing tab events"
  printf '%s\n' "${missing_tab_sidebar_events[@]}"
else
  print_ok "Windows vertical tab sidebar exposes tab events"
fi

# Tab overview surface: per-tab snapshots are captured on tab deactivation
# and refreshed for the active tab at overview open (CapturePreviewAsync),
# cached on the tab and rendered as a card grid overlaying the content host.
check_pattern_in_each_file \
  'new BrowserTabOverviewPanel' \
  "Windows shell attaches the tab overview panel" \
  "Windows shell does not attach the tab overview panel" \
  apps/windows/actro/src/MainForm.cs

missing_overview_shell_symbols=()
for overview_shell_symbol in CapturePreviewAsync CaptureTabSnapshotAsync ShowTabOverview HideTabOverview ToggleTabOverview SyncOverview; do
  if ! rg -q "$overview_shell_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_overview_shell_symbols+=("$overview_shell_symbol")
  fi
done

if ! rg -Fq 'Keys.Control | Keys.Shift | Keys.A' apps/windows/actro/src/MainForm.cs; then
  missing_overview_shell_symbols+=("Keys.Control | Keys.Shift | Keys.A")
fi

if [[ "${#missing_overview_shell_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing tab overview snapshot/toggle behavior"
  printf '%s\n' "${missing_overview_shell_symbols[@]}"
else
  print_ok "Windows shell owns overview snapshots, toggle and shortcut"
fi

check_pattern_in_each_file \
  'Snapshot' \
  "Windows browser tab state caches the overview snapshot" \
  "Windows browser tab state does not cache the overview snapshot" \
  apps/windows/actro/src/BrowserTab.cs

missing_overview_panel_symbols=()
for overview_panel_symbol in NewTabRequested TabSelected TabCloseRequested UpdateCards BrowserTabOverviewPolicy BrowserTabOverviewCard; do
  if ! rg -q "$overview_panel_symbol" apps/windows/actro/src/BrowserTabOverviewPanel.cs; then
    missing_overview_panel_symbols+=("$overview_panel_symbol")
  fi
done

if [[ "${#missing_overview_panel_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows tab overview panel is missing expected behavior"
  printf '%s\n' "${missing_overview_panel_symbols[@]}"
else
  print_ok "Windows tab overview panel renders policy-laid-out snapshot cards"
fi

missing_overview_policy_symbols=()
for overview_policy_symbol in Columns CardRects CardWidth CardHeight ThumbnailHeight ContentHeight; do
  if ! rg -q "$overview_policy_symbol" apps/windows/actro/src/BrowserTabOverviewPolicy.cs; then
    missing_overview_policy_symbols+=("$overview_policy_symbol")
  fi
done

if [[ "${#missing_overview_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows tab overview policy is missing expected symbols"
  printf '%s\n' "${missing_overview_policy_symbols[@]}"
else
  print_ok "Windows tab overview policy owns the card grid geometry"
fi

check_pattern_in_each_file \
  'BrowserTabOverviewPolicy' \
  "Windows tests link and exercise the tab overview policy" \
  "Windows tests do not link and exercise the tab overview policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_tab_lifecycle_calls=()
for tab_lifecycle_call in CreateAndActivateTabAsync CloseTabAsync ReopenClosedTab DuplicateActiveTab RequestNewWindow BrowserClosedTabStack ActivateTab SyncSidebar ActivateNextTab ActivatePreviousTab ActivateTabAtIndex; do
  if ! rg -q "$tab_lifecycle_call" apps/windows/actro/src/MainForm.cs; then
    missing_tab_lifecycle_calls+=("$tab_lifecycle_call")
  fi
done

if [[ "${#missing_tab_lifecycle_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing tab lifecycle behavior"
  printf '%s\n' "${missing_tab_lifecycle_calls[@]}"
else
  print_ok "Windows shell implements tab lifecycle behavior"
fi

missing_tab_shortcut_symbols=()
for tab_shortcut_symbol in TryHandleTabShortcut TryResolveTabShortcutIndex Keys.Tab Keys.PageDown Keys.PageUp Keys.D9 Keys.NumPad9 'Keys.Control | Keys.N' 'Keys.Control | Keys.Shift | Keys.N' 'Keys.Shift | Keys.T'; do
  if ! rg -Fq "$tab_shortcut_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_tab_shortcut_symbols+=("$tab_shortcut_symbol")
  fi
done

if [[ "${#missing_tab_shortcut_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing tab keyboard shortcuts"
  printf '%s\n' "${missing_tab_shortcut_symbols[@]}"
else
  print_ok "Windows shell owns tab keyboard shortcuts"
fi

missing_closed_tab_symbols=()
for closed_tab_symbol in BrowserClosedTabStack CanReopen Push Pop MaxClosedTabs; do
  if ! rg -q "$closed_tab_symbol" apps/windows/actro/src/BrowserClosedTabStack.cs; then
    missing_closed_tab_symbols+=("$closed_tab_symbol")
  fi
done

if [[ "${#missing_closed_tab_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows closed-tab stack is missing expected behavior"
  printf '%s\n' "${missing_closed_tab_symbols[@]}"
else
  print_ok "Windows shell owns closed-tab reopening state"
fi

missing_menu_commands=()
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
    missing_menu_commands+=("$menu_command")
  fi
done

if [[ "${#missing_menu_commands[@]}" -ne 0 ]]; then
  print_failure "Windows compact menu model is missing expected commands"
  printf '%s\n' "${missing_menu_commands[@]}"
else
  print_ok "Windows compact menu model exposes desktop commands"
fi

missing_menu_model_symbols=()
for menu_model_symbol in BrowserShortcut BrowserMenuShortcutAlias Aliases MenuOpenShortcuts Argument StringKey IsSeparator IsCheckable BrowserSearchEnginePolicy.Engines; do
  if ! rg -q "$menu_model_symbol" apps/windows/actro/src/BrowserMenuModel.cs; then
    missing_menu_model_symbols+=("$menu_model_symbol")
  fi
done

if [[ "${#missing_menu_model_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows compact menu model is missing expected structure"
  printf '%s\n' "${missing_menu_model_symbols[@]}"
else
  print_ok "Windows compact menu model carries shortcuts, aliases and arguments as data"
fi

missing_app_menu_symbols=()
for app_menu_symbol in ContextMenuStrip BrowserToolStripRenderer BrowserMenuModel ShortcutKeyDisplayString CommandRequested SetNavigationState ForeColor DisabledForeground KeyTokenMap; do
  if ! rg -q "$app_menu_symbol" apps/windows/actro/src/BrowserAppMenu.cs; then
    missing_app_menu_symbols+=("$app_menu_symbol")
  fi
done

if [[ "${#missing_app_menu_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows compact app menu is missing expected behavior"
  printf '%s\n' "${missing_app_menu_symbols[@]}"
else
  print_ok "Windows compact app menu renders the model with themed items"
fi

missing_escape_policy_symbols=()
for escape_policy_symbol in Resolve DismissOverview CloseFindBar ExitFullScreen Stop None overviewVisible; do
  if ! rg -q "$escape_policy_symbol" apps/windows/actro/src/BrowserEscapePolicy.cs; then
    missing_escape_policy_symbols+=("$escape_policy_symbol")
  fi
done

if [[ "${#missing_escape_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows escape policy is missing expected symbols"
  printf '%s\n' "${missing_escape_policy_symbols[@]}"
else
  print_ok "Windows escape policy owns the Escape precedence order"
fi

check_pattern_in_each_file \
  'BrowserMenuModel' \
  "Windows tests link and exercise the compact menu model" \
  "Windows tests do not link and exercise the compact menu model" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserEscapePolicy' \
  "Windows tests link and exercise the escape policy" \
  "Windows tests do not link and exercise the escape policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_toolbar_events=()
for toolbar_event in AddressRequested BackRequested ForwardRequested ReloadRequested NewTabRequested OverviewToggleRequested SidebarToggleRequested BookmarkRequested MenuRequested; do
  if ! rg -q "$toolbar_event" apps/windows/actro/src/BrowserToolbar.cs; then
    missing_toolbar_events+=("$toolbar_event")
  fi
done

if [[ "${#missing_toolbar_events[@]}" -ne 0 ]]; then
  print_failure "Windows browser toolbar is missing navigation events"
  printf '%s\n' "${missing_toolbar_events[@]}"
else
  print_ok "Windows browser toolbar exposes navigation events"
fi

# Explicit decision (Chrome/Edge default): no toolbar Home button. Home
# stays a menu command plus Alt+Home, routed through the command router.
if rg -q 'HomeRequested' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar still exposes the removed Home affordance"
else
  print_ok "Windows browser toolbar has no Home button (home stays menu + Alt+Home)"
fi

check_pattern_in_each_file \
  'BrowserMenuCommand.Home' \
  "Windows shell routes the home menu command" \
  "Windows shell does not route the home menu command" \
  apps/windows/actro/src/MainForm.cs

missing_toolbar_shell_symbols=()
for toolbar_symbol in BrowserGlyphs BrowserToolbarLayoutPolicy SetSidebarLayout SetNavigationState SetBookmarkState SetOverviewActive; do
  if ! rg -q "$toolbar_symbol" apps/windows/actro/src/BrowserToolbar.cs; then
    missing_toolbar_shell_symbols+=("$toolbar_symbol")
  fi
done

if [[ "${#missing_toolbar_shell_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser toolbar is missing two-segment glyph behavior"
  printf '%s\n' "${missing_toolbar_shell_symbols[@]}"
else
  print_ok "Windows browser toolbar draws policy-laid-out MDL2 glyph segments"
fi

if rg -q 'SetStatus|statusLabel' apps/windows/actro/src/BrowserToolbar.cs; then
  print_failure "Windows browser toolbar still owns the removed status column"
else
  print_ok "Windows browser toolbar dropped the fixed status column"
fi

missing_glyph_symbols=()
for glyph_symbol in CommandGlyphs GlyphPointSize NewTab TabOverview ToggleSidebar Back Forward Reload Stop BookmarkStar BookmarkStarFilled AppMenu; do
  if ! rg -q "$glyph_symbol" apps/windows/actro/src/BrowserGlyphs.cs; then
    missing_glyph_symbols+=("$glyph_symbol")
  fi
done

if [[ "${#missing_glyph_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser glyph table is missing expected symbols"
  printf '%s\n' "${missing_glyph_symbols[@]}"
else
  print_ok "Windows browser glyph table owns the MDL2-safe command glyphs"
fi

missing_toolbar_layout_symbols=()
for toolbar_layout_symbol in Segments CollapsedStubWidth OverviewSlotWidth AddressMaxWidth AddressWidth ButtonWidth ButtonGap EdgePadding; do
  if ! rg -q "$toolbar_layout_symbol" apps/windows/actro/src/BrowserToolbarLayoutPolicy.cs; then
    missing_toolbar_layout_symbols+=("$toolbar_layout_symbol")
  fi
done

if [[ "${#missing_toolbar_layout_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows toolbar layout policy is missing expected symbols"
  printf '%s\n' "${missing_toolbar_layout_symbols[@]}"
else
  print_ok "Windows toolbar layout policy owns the two-segment geometry"
fi

check_pattern_in_each_file \
  'BrowserGlyphs' \
  "Windows tests link and exercise the glyph table" \
  "Windows tests do not link and exercise the glyph table" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserToolbarLayoutPolicy' \
  "Windows tests link and exercise the toolbar layout policy" \
  "Windows tests do not link and exercise the toolbar layout policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

# Statuses live in the bottom-left overlay: transient ones auto-hide,
# pinned (user-invoked) ones persist until Escape, click or navigation.
missing_status_overlay_symbols=()
for status_overlay_symbol in BrowserStatusOverlay BrowserStatusKind ShowTransientStatus ShowPinnedStatus HidePinnedStatus Transient Pinned; do
  if ! rg -q "$status_overlay_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_status_overlay_symbols+=("$status_overlay_symbol")
  fi
done

if [[ "${#missing_status_overlay_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing status overlay behavior"
  printf '%s\n' "${missing_status_overlay_symbols[@]}"
else
  print_ok "Windows shell owns the transient/pinned status overlay"
fi

check_pattern_in_each_file \
  '<ApplicationHighDpiMode>PerMonitorV2</ApplicationHighDpiMode>' \
  "Windows project enables per-monitor DPI v2" \
  "Windows project does not enable per-monitor DPI v2" \
  apps/windows/actro/Actro.Windows.csproj

missing_theme_lifecycle_symbols=()
for theme_symbol in WM_SETTINGCHANGE ImmersiveColorSet DwmSetWindowAttribute PreferredColorScheme ApplyTheme ApplyWebViewColorScheme DpiChanged OnFormClosedUnsubscribeTheme; do
  if ! rg -q "$theme_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_theme_lifecycle_symbols+=("$theme_symbol")
  fi
done

if [[ "${#missing_theme_lifecycle_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing theme lifecycle behavior"
  printf '%s\n' "${missing_theme_lifecycle_symbols[@]}"
else
  print_ok "Windows shell owns the system theme lifecycle"
fi

missing_theme_data_symbols=()
for theme_data_symbol in 'static BrowserTheme Light' 'static BrowserTheme Dark' WindowBackground BarBackground BarForeground Accent SidebarBackground InputBackground DisabledForeground StatusBackground UiFontFamily MonoFontFamilies GlyphFontFamilies ToolbarHeight FindBarHeight 'static int Scale'; do
  if ! rg -q "$theme_data_symbol" apps/windows/actro/src/BrowserTheme.cs; then
    missing_theme_data_symbols+=("$theme_data_symbol")
  fi
done

if [[ "${#missing_theme_data_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser theme is missing palette/typography/metric data"
  printf '%s\n' "${missing_theme_data_symbols[@]}"
else
  print_ok "Windows browser theme centralizes palette, typography and metrics"
fi

check_pattern_in_each_file \
  'Resolve' \
  "Windows theme selection policy maps the system app mode" \
  "Windows theme selection policy does not map the system app mode" \
  apps/windows/actro/src/BrowserThemeSelectionPolicy.cs

missing_theme_manager_symbols=()
for theme_manager_symbol in AppsUseLightTheme RefreshFromSystem Changed GetFont BrowserThemeSelectionPolicy; do
  if ! rg -q "$theme_manager_symbol" apps/windows/actro/src/BrowserThemeManager.cs; then
    missing_theme_manager_symbols+=("$theme_manager_symbol")
  fi
done

if [[ "${#missing_theme_manager_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows theme manager is missing expected behavior"
  printf '%s\n' "${missing_theme_manager_symbols[@]}"
else
  print_ok "Windows theme manager owns system reads and the font cache"
fi

missing_menu_renderer_symbols=()
for renderer_symbol in ProfessionalColorTable OnRenderItemText OnRenderItemCheck OnRenderArrow OnRenderSeparator; do
  if ! rg -q "$renderer_symbol" apps/windows/actro/src/BrowserToolStripRenderer.cs; then
    missing_menu_renderer_symbols+=("$renderer_symbol")
  fi
done

if [[ "${#missing_menu_renderer_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows themed menu renderer is missing render overrides"
  printf '%s\n' "${missing_menu_renderer_symbols[@]}"
else
  print_ok "Windows themed menu renderer paints text, checks, arrows and separators"
fi

check_pattern_in_each_file \
  'BrowserDocumentStylePolicy.RootCss' \
  "Windows internal documents embed the shared stylesheet" \
  "Windows internal documents do not embed the shared stylesheet" \
  apps/windows/actro/src/BrowserHomeDocumentPolicy.cs \
  apps/windows/actro/src/BrowserLocalListDocumentPolicy.cs \
  apps/windows/actro/src/BrowserNavigationErrorPolicy.cs

if rg -q 'FlatStyle.System' apps/windows/actro/src/BrowserToolbar.cs apps/windows/actro/src/BrowserFindBar.cs; then
  print_failure "Windows chrome bars still use system-drawn (theme-blind) buttons"
else
  print_ok "Windows chrome bars use owner-drawn themed buttons"
fi

check_pattern_in_each_file \
  'ApplyTheme' \
  "Windows chrome controls consume the shared theme" \
  "Windows chrome controls do not consume the shared theme" \
  apps/windows/actro/src/BrowserToolbar.cs \
  apps/windows/actro/src/BrowserFindBar.cs \
  apps/windows/actro/src/BrowserAppMenu.cs \
  apps/windows/actro/src/BrowserTabSidebar.cs

check_pattern_in_each_file \
  'BrowserTheme' \
  "Windows tests link and exercise the theme policies" \
  "Windows tests do not link and exercise the theme policies" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserDocumentStylePolicy' \
  "Windows tests link and exercise the shared document stylesheet" \
  "Windows tests do not link and exercise the shared document stylesheet" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_string_table_symbols=()
for strings_symbol in 'static string Get' 'static string Format' 'IReadOnlyDictionary<string, string> All'; do
  if ! rg -Fq "$strings_symbol" apps/windows/actro/src/BrowserStrings.cs; then
    missing_string_table_symbols+=("$strings_symbol")
  fi
done

if [[ "${#missing_string_table_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser string table is missing its lookup API"
  printf '%s\n' "${missing_string_table_symbols[@]}"
else
  print_ok "Windows browser string table exposes Get/Format/All"
fi

# The compact app menu consumes its labels as menu.* keys through the menu
# model's string keys.
check_pattern_in_each_file \
  'BrowserStrings\.' \
  "Windows chrome consumes the centralized string table" \
  "Windows chrome does not consume the centralized string table" \
  apps/windows/actro/src/MainForm.cs \
  apps/windows/actro/src/BrowserToolbar.cs \
  apps/windows/actro/src/BrowserFindBar.cs \
  apps/windows/actro/src/BrowserAppMenu.cs \
  apps/windows/actro/src/BrowserSyncConnectionDialog.cs

check_pattern_in_each_file \
  'BrowserStrings' \
  "Windows tests link and exercise the string table" \
  "Windows tests do not link and exercise the string table" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_webview_events=()
for webview_event in NavigationStarting NavigationCompleted HistoryChanged SourceChanged StatusBarTextChanged DocumentTitleChanged NewWindowRequested PermissionRequested ContainsFullScreenElementChanged ProcessFailed WebResourceRequested; do
  if ! rg -q "$webview_event" apps/windows/actro/src/MainForm.cs; then
    missing_webview_events+=("$webview_event")
  fi
done

if [[ "${#missing_webview_events[@]}" -ne 0 ]]; then
  print_failure "Windows shell does not synchronize all expected WebView2 navigation state"
  printf '%s\n' "${missing_webview_events[@]}"
else
  print_ok "Windows shell synchronizes WebView2 navigation state"
fi

missing_full_screen_symbols=()
for full_screen_symbol in \
  ContainsFullScreenElement \
  isBrowserFullScreen \
  isPageFullScreen \
  ToggleBrowserFullScreen \
  ApplyFullScreenState \
  EnterFullScreenMode \
  ExitFullScreenMode \
  RequestExitFullScreen \
  Keys.F11 \
  FormBorderStyle.None \
  FormWindowState.Maximized
do
  if ! rg -q "$full_screen_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_full_screen_symbols+=("$full_screen_symbol")
  fi
done

if [[ "${#missing_full_screen_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing WebView2 full-screen behavior"
  printf '%s\n' "${missing_full_screen_symbols[@]}"
else
  print_ok "Windows shell owns WebView2 full-screen behavior"
fi

missing_profile_symbols=()
for profile_symbol in GetDefaultUserDataFolder GetPrivateUserDataFolder EnsureUserDataFolder TryDeleteUserDataFolder UserDataFolderName PrivateUserDataParentFolderName; do
  if ! rg -q "$profile_symbol" apps/windows/actro/src/BrowserProfilePolicy.cs; then
    missing_profile_symbols+=("$profile_symbol")
  fi
done

if [[ "${#missing_profile_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser profile policy is missing expected symbols"
  printf '%s\n' "${missing_profile_symbols[@]}"
else
  print_ok "Windows browser profile policy owns WebView2 user data paths"
fi

check_pattern_in_each_file \
  'WindowsAppPaths' \
  "Windows local app data paths are centralized" \
  "Windows local app data paths are not centralized" \
  apps/windows/actro/src/BrowserProfilePolicy.cs \
  apps/windows/actro/src/BrowserSessionStore.cs \
  apps/windows/actro/src/BrowserWindowStateStore.cs

# Browser sync provider connection: the settings store owns the durable
# device identity next to the credential store, the planner owns snapshot
# assembly and routing, and the coordinator consumes both ahead of the
# endpoint-token branch.
check_pattern_in_each_file \
  'WindowsAppPaths' \
  "Windows browser sync provider settings live under the shared app data root" \
  "Windows browser sync provider settings do not use the shared app data root" \
  apps/windows/actro/src/BrowserSyncProviderSettingsStore.cs

check_pattern_in_each_file \
  'BrowserSyncProviderConnectionPlanner' \
  "Windows browser sync coordinator routes through the provider connection planner" \
  "Windows browser sync coordinator does not route through the provider connection planner" \
  apps/windows/actro/src/BrowserStorageSyncCoordinator.cs \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserSyncProviderSettingsStore' \
  "Windows browser sync provider settings store is wired and exercised" \
  "Windows browser sync provider settings store is not wired and exercised" \
  apps/windows/actro/src/MainForm.cs \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'CoreWebView2Environment.CreateAsync' \
  "Windows shell creates an explicit WebView2 environment" \
  "Windows shell does not create an explicit WebView2 environment" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'EnsureCoreWebView2Async\(await GetWebViewEnvironmentAsync\(\)\)' \
  "Windows tabs share the shell WebView2 profile environment" \
  "Windows tabs do not share the shell WebView2 profile environment" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'BrowserProfilePolicy' \
  "Windows tests exercise browser profile policy" \
  "Windows tests do not exercise browser profile policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'parses positional startup address|parses multiple startup addresses|parses private startup mode|parses private startup url|InitialAddresses|--private|--incognito|--private-window' \
  "Windows tests exercise startup address and private mode" \
  "Windows tests do not exercise startup address and private mode" \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'GetPrivateUserDataFolder|TryDeleteUserDataFolder|PrivateUserDataParentFolderName' \
  "Windows tests exercise private profile policy" \
  "Windows tests do not exercise private profile policy" \
  apps/windows/actro.tests/Program.cs

missing_browsing_data_symbols=()
for browsing_data_symbol in ConfirmationTitle ConfirmationMessage ClearingStatus ClearedStatus FailedStatus; do
  if ! rg -q "$browsing_data_symbol" apps/windows/actro/src/BrowserBrowsingDataPolicy.cs; then
    missing_browsing_data_symbols+=("$browsing_data_symbol")
  fi
done

if [[ "${#missing_browsing_data_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browsing-data policy is missing expected symbols"
  printf '%s\n' "${missing_browsing_data_symbols[@]}"
else
  print_ok "Windows browsing-data policy owns clear-data text rules"
fi

check_pattern_in_each_file \
  'BrowserClosedTabStack' \
  "Windows tests exercise closed-tab reopening" \
  "Windows tests do not exercise closed-tab reopening" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserBrowsingDataPolicy' \
  "Windows tests exercise browsing-data policy" \
  "Windows tests do not exercise browsing-data policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'ClearBrowsingDataAsync' \
  "Windows shell clears WebView2 browsing data" \
  "Windows shell does not clear WebView2 browsing data" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'CoreWebView2BrowsingDataKinds' \
  "Windows shell chooses explicit browsing data kinds" \
  "Windows shell does not choose explicit browsing data kinds" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'DownloadStarting' \
  "Windows shell handles WebView2 downloads" \
  "Windows shell does not handle WebView2 downloads" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'BrowserDownloadManager' \
  "Windows shell attaches the browser download manager" \
  "Windows shell does not attach the browser download manager" \
  apps/windows/actro/src/MainForm.cs

missing_download_manager_symbols=()
for download_symbol in HandleDownloadStarting SaveFileDialog CoreWebView2DownloadOperation TrackDownload setLastCompletedDownload; do
  if ! rg -q "$download_symbol" apps/windows/actro/src/BrowserDownloadManager.cs; then
    missing_download_manager_symbols+=("$download_symbol")
  fi
done

if [[ "${#missing_download_manager_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser download manager is missing expected behavior"
  printf '%s\n' "${missing_download_manager_symbols[@]}"
else
  print_ok "Windows browser download manager owns save-dialog download handling"
fi

missing_download_policy_symbols=()
for download_policy_symbol in GetSuggestedFileName SanitizeFileName FormatProgress FallbackFileName GetDefaultDownloadsDirectory ResolveDownloadsDirectory OpenDownloadsFolderStatus OpenDownloadsFolderFailedStatus TryGetAvailableDownloadFile FormatUnavailableDownloadStatus BuildExplorerSelectArguments OpenedLastDownloadStatus OpenLastDownloadFailedStatus RevealedLastDownloadStatus RevealLastDownloadFailedStatus; do
  if ! rg -q "$download_policy_symbol" apps/windows/actro/src/BrowserDownloadPolicy.cs; then
    missing_download_policy_symbols+=("$download_policy_symbol")
  fi
done

if [[ "${#missing_download_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser download policy is missing expected symbols"
  printf '%s\n' "${missing_download_policy_symbols[@]}"
else
  print_ok "Windows browser download policy owns file-name/progress rules"
fi

check_pattern_in_each_file \
  'BrowserDownloadPolicy' \
  "Windows tests exercise browser download policy" \
  "Windows tests do not exercise browser download policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserDownloadPolicy.GetDefaultDownloadsDirectory' \
  "Windows download manager uses the shared downloads-folder policy" \
  "Windows download manager does not use the shared downloads-folder policy" \
  apps/windows/actro/src/BrowserDownloadManager.cs

check_pattern_in_each_file \
  'BrowserExternalProtocolManager' \
  "Windows shell attaches the external protocol manager" \
  "Windows shell does not attach the external protocol manager" \
  apps/windows/actro/src/MainForm.cs

missing_external_manager_symbols=()
for external_manager_symbol in HandleExternalProtocol MessageBox ProcessStartInfo UseShellExecute; do
  if ! rg -q "$external_manager_symbol" apps/windows/actro/src/BrowserExternalProtocolManager.cs; then
    missing_external_manager_symbols+=("$external_manager_symbol")
  fi
done

if [[ "${#missing_external_manager_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows external protocol manager is missing expected behavior"
  printf '%s\n' "${missing_external_manager_symbols[@]}"
else
  print_ok "Windows external protocol manager owns OS handoff prompts"
fi

missing_external_policy_symbols=()
for external_policy_symbol in TryCreateExternalProtocolPrompt IsExternalProtocolAddress BrowserExternalProtocolPrompt; do
  if ! rg -q "$external_policy_symbol" apps/windows/actro/src/BrowserExternalProtocolPolicy.cs; then
    missing_external_policy_symbols+=("$external_policy_symbol")
  fi
done

if [[ "${#missing_external_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows external protocol policy is missing expected symbols"
  printf '%s\n' "${missing_external_policy_symbols[@]}"
else
  print_ok "Windows external protocol policy owns protocol classification"
fi

check_pattern_in_each_file \
  'BrowserExternalProtocolPolicy' \
  "Windows tests exercise external protocol policy" \
  "Windows tests do not exercise external protocol policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserPermissionManager' \
  "Windows shell attaches the browser permission manager" \
  "Windows shell does not attach the browser permission manager" \
  apps/windows/actro/src/MainForm.cs

missing_permission_manager_symbols=()
for permission_manager_symbol in HandlePermissionRequested MessageBox CoreWebView2PermissionState; do
  if ! rg -q "$permission_manager_symbol" apps/windows/actro/src/BrowserPermissionManager.cs; then
    missing_permission_manager_symbols+=("$permission_manager_symbol")
  fi
done

if [[ "${#missing_permission_manager_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser permission manager is missing expected behavior"
  printf '%s\n' "${missing_permission_manager_symbols[@]}"
else
  print_ok "Windows browser permission manager owns WebView2 permission prompts"
fi

missing_permission_policy_symbols=()
for permission_policy_symbol in BuildPrompt DescribePermission DescribeOrigin BrowserPermissionPrompt; do
  if ! rg -q "$permission_policy_symbol" apps/windows/actro/src/BrowserPermissionPolicy.cs; then
    missing_permission_policy_symbols+=("$permission_policy_symbol")
  fi
done

if [[ "${#missing_permission_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser permission policy is missing expected symbols"
  printf '%s\n' "${missing_permission_policy_symbols[@]}"
else
  print_ok "Windows browser permission policy owns prompt text rules"
fi

check_pattern_in_each_file \
  'BrowserPermissionPolicy' \
  "Windows tests exercise browser permission policy" \
  "Windows tests do not exercise browser permission policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_find_bar_symbols=()
for find_bar_symbol in BrowserFindBar FindRequested CloseRequested ShowAndFocus 'Query => queryBox.Text' BrowserFindRequestedEventArgs; do
  if ! rg -q "$find_bar_symbol" apps/windows/actro/src/BrowserFindBar.cs; then
    missing_find_bar_symbols+=("$find_bar_symbol")
  fi
done

if [[ "${#missing_find_bar_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser find bar is missing expected behavior"
  printf '%s\n' "${missing_find_bar_symbols[@]}"
else
  print_ok "Windows browser find bar exposes find commands"
fi

missing_find_policy_symbols=()
for find_policy_symbol in BuildWindowFindScript IsFoundResult; do
  if ! rg -q "$find_policy_symbol" apps/windows/actro/src/BrowserFindPolicy.cs; then
    missing_find_policy_symbols+=("$find_policy_symbol")
  fi
done

if [[ "${#missing_find_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser find policy is missing expected symbols"
  printf '%s\n' "${missing_find_policy_symbols[@]}"
else
  print_ok "Windows browser find policy owns page find script rules"
fi

check_pattern_in_each_file \
  'BrowserFindPolicy' \
  "Windows tests exercise browser find policy" \
  "Windows tests do not exercise browser find policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_zoom_policy_symbols=()
for zoom_policy_symbol in ZoomIn ZoomOut Reset FormatZoom CanZoomIn CanZoomOut CanReset Normalize; do
  if ! rg -q "$zoom_policy_symbol" apps/windows/actro/src/BrowserZoomPolicy.cs; then
    missing_zoom_policy_symbols+=("$zoom_policy_symbol")
  fi
done

if [[ "${#missing_zoom_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser zoom policy is missing expected symbols"
  printf '%s\n' "${missing_zoom_policy_symbols[@]}"
else
  print_ok "Windows browser zoom policy owns zoom bounds and formatting"
fi

check_pattern_in_each_file \
  'BrowserZoomPolicy' \
  "Windows tests exercise browser zoom policy" \
  "Windows tests do not exercise browser zoom policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_navigation_calls=()
for navigation_call in GoBack GoForward Reload Stop NavigationPolicy HandleNavigationStarting HandleWebResourceRequested SyncStatusBarText NavigateBack NavigateForward NavigateHistoryDocument NavigateBookmarksDocument OpenNewWindowRequestAsync ReloadOrStopActiveTab ReloadActiveTab StopLoadingActiveTab OpenDeveloperTools OpenDownloadsFolder OpenLastDownload RevealLastDownloadInFolder TryGetLastCompletedDownloadPath SetLastCompletedDownload HasCompletedDownload ToggleActivePageBookmark RemoveBookmarkFromHostAsync RemoveHistoryEntryFromHostAsync IsAddressBookmarked RecordHistoryVisit ClearLocalHistory CopyActivePageAddress OpenActivePageInDefaultBrowser OpenLocalFile PrintActiveTab ClearBrowsingDataAsync SaveActiveTabAsPdfAsync ToggleContentBlocking PersistContentBlockingSettings SetSearchEngine SetRestorePreviousSession ZoomInActiveTab ZoomOutActiveTab ResetActiveTabZoom ToggleBrowserFullScreen OpenFindBar CloseFindBar FindInActiveTabAsync RepeatFind ReopenClosedTab DuplicateActiveTab RequestNewWindow TryHandleTabShortcut ProcessCmdKey; do
  if ! rg -q "$navigation_call" apps/windows/actro/src/MainForm.cs; then
    missing_navigation_calls+=("$navigation_call")
  fi
done

if [[ "${#missing_navigation_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser navigation actions"
  printf '%s\n' "${missing_navigation_calls[@]}"
else
  print_ok "Windows shell implements browser navigation actions"
fi

missing_find_shortcut_symbols=()
for find_shortcut_symbol in 'Keys.F3' 'Keys.Shift | Keys.F3' lastFindQuery; do
  if ! rg -Fq "$find_shortcut_symbol" apps/windows/actro/src/MainForm.cs; then
    missing_find_shortcut_symbols+=("$find_shortcut_symbol")
  fi
done

if [[ "${#missing_find_shortcut_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser find shortcut behavior"
  printf '%s\n' "${missing_find_shortcut_symbols[@]}"
else
  print_ok "Windows shell supports repeated find shortcuts"
fi

check_pattern_in_each_file \
  'Keys.Control \| Keys.J' \
  "Windows shell exposes an open-downloads-folder shortcut" \
  "Windows shell does not expose an open-downloads-folder shortcut" \
  apps/windows/actro/src/MainForm.cs

missing_address_input_policy_symbols=()
for address_input_symbol in TryResolveAddressInput DefaultSearchUrlPrefix; do
  if ! rg -q "$address_input_symbol" apps/windows/actro/src/BrowserAddressInputPolicy.cs; then
    missing_address_input_policy_symbols+=("$address_input_symbol")
  fi
done

if [[ "${#missing_address_input_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows address input policy is missing expected symbols"
  printf '%s\n' "${missing_address_input_policy_symbols[@]}"
else
  print_ok "Windows address input policy owns search fallback rules"
fi

check_pattern_in_each_file \
  'BrowserAddressInputPolicy' \
  "Windows tests exercise address input policy" \
  "Windows tests do not exercise address input policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_search_engine_policy_symbols=()
for search_engine_symbol in Engines DefaultEngineId DefaultSearchUrlPrefix NormalizeEngineId TryGetEngine BuildSearchUrl BrowserSearchEngineInfo; do
  if ! rg -q "$search_engine_symbol" apps/windows/actro/src/BrowserSearchEnginePolicy.cs; then
    missing_search_engine_policy_symbols+=("$search_engine_symbol")
  fi
done

if [[ "${#missing_search_engine_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows search engine policy is missing expected symbols"
  printf '%s\n' "${missing_search_engine_policy_symbols[@]}"
else
  print_ok "Windows search engine policy owns search URL construction"
fi

missing_search_settings_store_symbols=()
for search_settings_symbol in CreateDefault Load Save CreateSnapshot BrowserSearchSettingsSnapshot; do
  if ! rg -q "$search_settings_symbol" apps/windows/actro/src/BrowserSearchSettingsStore.cs; then
    missing_search_settings_store_symbols+=("$search_settings_symbol")
  fi
done

if [[ "${#missing_search_settings_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows search settings store is missing expected symbols"
  printf '%s\n' "${missing_search_settings_store_symbols[@]}"
else
  print_ok "Windows search settings store owns persisted search preference"
fi

check_pattern_in_each_file \
  'BrowserSearch' \
  "Windows tests exercise search engine policy and settings" \
  "Windows tests do not exercise search engine policy and settings" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_startup_settings_store_symbols=()
for startup_settings_symbol in CreateDefault Load Save CreateSnapshot BrowserStartupSettingsSnapshot RestorePreviousSession; do
  if ! rg -q "$startup_settings_symbol" apps/windows/actro/src/BrowserStartupSettingsStore.cs; then
    missing_startup_settings_store_symbols+=("$startup_settings_symbol")
  fi
done

if [[ "${#missing_startup_settings_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows startup settings store is missing expected symbols"
  printf '%s\n' "${missing_startup_settings_store_symbols[@]}"
else
  print_ok "Windows startup settings store owns persisted restore preference"
fi

check_pattern_in_each_file \
  'BrowserStartupSettings' \
  "Windows tests exercise startup settings" \
  "Windows tests do not exercise startup settings" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_startup_settings_calls=()
for startup_settings_call in RestoreSessionToggle SetRestorePreviousSession BrowserStartupSettingsStore RestoreInitialTabsAsync; do
  if ! rg -q "$startup_settings_call" apps/windows/actro/src/MainForm.cs; then
    missing_startup_settings_calls+=("$startup_settings_call")
  fi
done

if [[ "${#missing_startup_settings_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing startup restore settings behavior"
  printf '%s\n' "${missing_startup_settings_calls[@]}"
else
  print_ok "Windows shell persists and applies startup restore settings"
fi

missing_search_settings_calls=()
for search_settings_call in SelectSearchEngine SetSearchEngine BrowserSearchEnginePolicy BrowserSearchSettingsStore TryResolveAddressInput; do
  if ! rg -q "$search_settings_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserAddressInputPolicy.cs; then
    missing_search_settings_calls+=("$search_settings_call")
  fi
done

if [[ "${#missing_search_settings_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing search engine settings behavior"
  printf '%s\n' "${missing_search_settings_calls[@]}"
else
  print_ok "Windows shell persists and applies search engine settings"
fi

check_pattern_in_each_file \
  'BrowserAddressInputPolicy.TryResolveAddressInput' \
  "Windows address bar resolves user input through address input policy" \
  "Windows address bar does not resolve user input through address input policy" \
  apps/windows/actro/src/MainForm.cs

missing_home_document_policy_symbols=()
for home_document_symbol in BuildDocument Title ActroCoreStatus; do
  if ! rg -q "$home_document_symbol" apps/windows/actro/src/BrowserHomeDocumentPolicy.cs; then
    missing_home_document_policy_symbols+=("$home_document_symbol")
  fi
done

if [[ "${#missing_home_document_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows home document policy is missing expected symbols"
  printf '%s\n' "${missing_home_document_policy_symbols[@]}"
else
  print_ok "Windows home document policy owns native home-page document rules"
fi

check_pattern_in_each_file \
  'BrowserHomeDocumentPolicy' \
  "Windows tests exercise home document policy" \
  "Windows tests do not exercise home document policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserHomeDocumentPolicy.BuildDocument' \
  "Windows shell delegates home-page HTML generation to the home document policy" \
  "Windows shell does not delegate home-page HTML generation to the home document policy" \
  apps/windows/actro/src/MainForm.cs

if rg -q 'BuildDefaultDocument' apps/windows/actro/src/MainForm.cs; then
  print_failure "Windows MainForm still owns inline home document generation"
else
  print_ok "Windows MainForm no longer owns inline home document generation"
fi

missing_bookmark_store_symbols=()
for bookmark_store_symbol in CreateDefault Load Save AddOrUpdate RemoveAddress ContainsAddress CreateSnapshot CanBookmarkAddress TryNormalizeBookmarkAddress BrowserBookmarkSnapshot BrowserBookmarkItemSnapshot; do
  if ! rg -q "$bookmark_store_symbol" apps/windows/actro/src/BrowserBookmarkStore.cs; then
    missing_bookmark_store_symbols+=("$bookmark_store_symbol")
  fi
done

if [[ "${#missing_bookmark_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser bookmark store is missing expected symbols"
  printf '%s\n' "${missing_bookmark_store_symbols[@]}"
else
  print_ok "Windows browser bookmark store owns local bookmark persistence"
fi

missing_bookmarks_document_symbols=()
for bookmarks_document_symbol in BuildDocument Title BrowserBookmarkItemSnapshot data-remove-bookmark; do
  if ! rg -q "$bookmarks_document_symbol" apps/windows/actro/src/BrowserBookmarksDocumentPolicy.cs; then
    missing_bookmarks_document_symbols+=("$bookmarks_document_symbol")
  fi
done

if [[ "${#missing_bookmarks_document_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows bookmarks document policy is missing expected symbols"
  printf '%s\n' "${missing_bookmarks_document_symbols[@]}"
else
  print_ok "Windows bookmarks document policy owns safe bookmarks-page rendering"
fi

missing_local_list_document_symbols=()
for local_list_document_symbol in BuildDocument BrowserLocalListDocumentOptions BrowserLocalListDocumentItem data-list-search data-filter removeHostMethod; do
  if ! rg -q "$local_list_document_symbol" apps/windows/actro/src/BrowserLocalListDocumentPolicy.cs; then
    missing_local_list_document_symbols+=("$local_list_document_symbol")
  fi
done

if [[ "${#missing_local_list_document_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows local list document policy is missing expected symbols"
  printf '%s\n' "${missing_local_list_document_symbols[@]}"
else
  print_ok "Windows local list document policy owns shared generated list-page chrome"
fi

check_pattern_in_each_file \
  'BrowserLocalListDocumentPolicy' \
  "Windows generated list pages use the shared local list policy" \
  "Windows generated list pages do not use the shared local list policy" \
  apps/windows/actro/src/BrowserBookmarksDocumentPolicy.cs \
  apps/windows/actro/src/BrowserHistoryDocumentPolicy.cs \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj

check_pattern_in_each_file \
  'BrowserBookmark' \
  "Windows tests exercise browser bookmark store and document policy" \
  "Windows tests do not exercise browser bookmark store and document policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_bookmark_calls=()
for bookmark_call in BookmarkPage ShowBookmarks NavigateBookmarksDocument ToggleActivePageBookmark RemoveBookmarkFromHostAsync BrowserBookmarksDocumentPolicy BrowserBookmarkStore IsAddressBookmarked; do
  if ! rg -q "$bookmark_call" apps/windows/actro/src/MainForm.cs; then
    missing_bookmark_calls+=("$bookmark_call")
  fi
done

if [[ "${#missing_bookmark_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser bookmark behavior"
  printf '%s\n' "${missing_bookmark_calls[@]}"
else
  print_ok "Windows shell records and renders bookmarks locally"
fi

check_pattern_in_each_file \
  'Keys.Control \| Keys.D' \
  "Windows shell exposes a bookmark-current-page shortcut" \
  "Windows shell does not expose a bookmark-current-page shortcut" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'Keys.Control \| Keys.Shift \| Keys.O' \
  "Windows shell exposes a show-bookmarks shortcut" \
  "Windows shell does not expose a show-bookmarks shortcut" \
  apps/windows/actro/src/MainForm.cs

missing_history_store_symbols=()
for history_store_symbol in CreateDefault Load Save AddVisit RemoveAddress Clear CreateSnapshot BrowserHistorySnapshot BrowserHistoryItemSnapshot; do
  if ! rg -q "$history_store_symbol" apps/windows/actro/src/BrowserHistoryStore.cs; then
    missing_history_store_symbols+=("$history_store_symbol")
  fi
done

if [[ "${#missing_history_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser history store is missing expected symbols"
  printf '%s\n' "${missing_history_store_symbols[@]}"
else
  print_ok "Windows browser history store owns local visit persistence"
fi

missing_history_document_symbols=()
for history_document_symbol in BuildDocument Title BrowserHistoryItemSnapshot data-remove-history; do
  if ! rg -q "$history_document_symbol" apps/windows/actro/src/BrowserHistoryDocumentPolicy.cs; then
    missing_history_document_symbols+=("$history_document_symbol")
  fi
done

if [[ "${#missing_history_document_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows history document policy is missing expected symbols"
  printf '%s\n' "${missing_history_document_symbols[@]}"
else
  print_ok "Windows history document policy owns safe history-page rendering"
fi

check_pattern_in_each_file \
  'BrowserHistory' \
  "Windows tests exercise browser history store and document policy" \
  "Windows tests do not exercise browser history store and document policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_history_calls=()
for history_call in ShowHistory NavigateHistoryDocument BrowserHistoryDocumentPolicy BrowserHistoryStore RecordHistoryVisit RemoveHistoryEntryFromHostAsync ClearLocalHistory; do
  if ! rg -q "$history_call" apps/windows/actro/src/MainForm.cs; then
    missing_history_calls+=("$history_call")
  fi
done

if [[ "${#missing_history_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser history behavior"
  printf '%s\n' "${missing_history_calls[@]}"
else
  print_ok "Windows shell records and renders browser history locally"
fi

missing_content_blocking_policy_symbols=()
for content_blocking_symbol in ShouldBlock FormatToggleStatus BlockedStatusCode BlockedReasonPhrase DefaultIsEnabled; do
  if ! rg -q "$content_blocking_symbol" apps/windows/actro/src/BrowserContentBlockingPolicy.cs; then
    missing_content_blocking_policy_symbols+=("$content_blocking_symbol")
  fi
done

if [[ "${#missing_content_blocking_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows content blocking policy is missing expected symbols"
  printf '%s\n' "${missing_content_blocking_policy_symbols[@]}"
else
  print_ok "Windows content blocking policy owns lightweight resource rules"
fi

missing_content_blocking_store_symbols=()
for content_blocking_store_symbol in CreateDefault Load Save CreateSnapshot BrowserContentBlockingSnapshot; do
  if ! rg -q "$content_blocking_store_symbol" apps/windows/actro/src/BrowserContentBlockingStore.cs; then
    missing_content_blocking_store_symbols+=("$content_blocking_store_symbol")
  fi
done

if [[ "${#missing_content_blocking_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows content blocking store is missing expected symbols"
  printf '%s\n' "${missing_content_blocking_store_symbols[@]}"
else
  print_ok "Windows content blocking store owns persisted toggle state"
fi

check_pattern_in_each_file \
  'BrowserContentBlocking' \
  "Windows tests exercise content blocking policy and store" \
  "Windows tests do not exercise content blocking policy and store" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_content_blocking_calls=()
for content_blocking_call in AddWebResourceRequestedFilter WebResourceRequested HandleWebResourceRequested CreateWebResourceResponse BrowserContentBlockingPolicy BrowserContentBlockingStore ToggleContentBlocking SetContentBlockingEnabled; do
  if ! rg -q "$content_blocking_call" apps/windows/actro/src/MainForm.cs; then
    missing_content_blocking_calls+=("$content_blocking_call")
  fi
done

if [[ "${#missing_content_blocking_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing content blocking behavior"
  printf '%s\n' "${missing_content_blocking_calls[@]}"
else
  print_ok "Windows shell filters blockable WebView2 resources"
fi

missing_process_failure_policy_symbols=()
for process_failure_symbol in FormatStatus FormatRecoveryStatus FormatRecoveryFailedStatus ShouldReloadTab; do
  if ! rg -q "$process_failure_symbol" apps/windows/actro/src/BrowserProcessFailurePolicy.cs; then
    missing_process_failure_policy_symbols+=("$process_failure_symbol")
  fi
done

if [[ "${#missing_process_failure_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows process failure policy is missing expected symbols"
  printf '%s\n' "${missing_process_failure_policy_symbols[@]}"
else
  print_ok "Windows process failure policy owns WebView2 recovery text rules"
fi

check_pattern_in_each_file \
  'BrowserProcessFailurePolicy' \
  "Windows tests exercise process failure policy" \
  "Windows tests do not exercise process failure policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_process_failure_calls=()
for process_failure_call in HandleProcessFailed BrowserProcessFailurePolicy ShouldReloadTab Reload; do
  if ! rg -q "$process_failure_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserProcessFailurePolicy.cs; then
    missing_process_failure_calls+=("$process_failure_call")
  fi
done

if [[ "${#missing_process_failure_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing process failure recovery behavior"
  printf '%s\n' "${missing_process_failure_calls[@]}"
else
  print_ok "Windows shell handles recoverable WebView2 process failures"
fi

missing_navigation_error_policy_symbols=()
for navigation_error_symbol in FormatStatus BuildErrorDocument DefaultTitle; do
  if ! rg -q "$navigation_error_symbol" apps/windows/actro/src/BrowserNavigationErrorPolicy.cs; then
    missing_navigation_error_policy_symbols+=("$navigation_error_symbol")
  fi
done

if [[ "${#missing_navigation_error_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows navigation error policy is missing expected symbols"
  printf '%s\n' "${missing_navigation_error_policy_symbols[@]}"
else
  print_ok "Windows navigation error policy owns error-page text rules"
fi

check_pattern_in_each_file \
  'BrowserNavigationErrorPolicy' \
  "Windows tests exercise navigation error policy" \
  "Windows tests do not exercise navigation error policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_navigation_error_calls=()
for navigation_error_call in ShowNavigationError BrowserNavigationErrorPolicy IsShowingErrorPage NavigateToString; do
  if ! rg -q "$navigation_error_call" apps/windows/actro/src/MainForm.cs apps/windows/actro/src/BrowserTab.cs; then
    missing_navigation_error_calls+=("$navigation_error_call")
  fi
done

if [[ "${#missing_navigation_error_calls[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing navigation error page behavior"
  printf '%s\n' "${missing_navigation_error_calls[@]}"
else
  print_ok "Windows shell shows navigation failures as retryable error pages"
fi

check_pattern_in_each_file \
  'NavigationPolicy.TryNormalizeAddress\(activeTab.Address, out var retryAddress\)' \
  "Windows reload retries the original address from a navigation error page" \
  "Windows reload does not retry the original address from a navigation error page" \
  apps/windows/actro/src/MainForm.cs

missing_page_action_policy_symbols=()
for page_action_symbol in TryGetCopyableAddress TryGetExternalAddress CopiedAddressStatus NoCopyableAddressStatus OpenedExternalStatus NoExternalAddressStatus OpenExternalFailedStatus; do
  if ! rg -q "$page_action_symbol" apps/windows/actro/src/BrowserPageActionPolicy.cs; then
    missing_page_action_policy_symbols+=("$page_action_symbol")
  fi
done

if [[ "${#missing_page_action_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows page action policy is missing expected symbols"
  printf '%s\n' "${missing_page_action_policy_symbols[@]}"
else
  print_ok "Windows page action policy owns copy/open address rules"
fi

check_pattern_in_each_file \
  'BrowserPageActionPolicy' \
  "Windows tests exercise page action policy" \
  "Windows tests do not exercise page action policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'Clipboard.SetText' \
  "Windows shell copies page addresses through the clipboard" \
  "Windows shell does not copy page addresses through the clipboard" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'ProcessStartInfo' \
  "Windows shell opens page addresses through the system shell" \
  "Windows shell does not open page addresses through the system shell" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'OpenFileDialog' \
  "Windows shell exposes native file opening" \
  "Windows shell does not expose native file opening" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'PrintToPdfAsync' \
  "Windows shell saves WebView2 pages as PDF" \
  "Windows shell does not save WebView2 pages as PDF" \
  apps/windows/actro/src/MainForm.cs

missing_pdf_policy_symbols=()
for pdf_policy_symbol in GetSuggestedFileName FormatSaveStatus DefaultPdfFileName; do
  if ! rg -q "$pdf_policy_symbol" apps/windows/actro/src/BrowserPdfPolicy.cs; then
    missing_pdf_policy_symbols+=("$pdf_policy_symbol")
  fi
done

if [[ "${#missing_pdf_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser PDF policy is missing expected symbols"
  printf '%s\n' "${missing_pdf_policy_symbols[@]}"
else
  print_ok "Windows browser PDF policy owns save-as-PDF text rules"
fi

check_pattern_in_each_file \
  'BrowserPdfPolicy' \
  "Windows tests exercise browser PDF policy" \
  "Windows tests do not exercise browser PDF policy" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'ShowPrintUI' \
  "Windows shell exposes WebView2 print UI" \
  "Windows shell does not expose WebView2 print UI" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'ExecuteScriptAsync' \
  "Windows shell executes page find scripts" \
  "Windows shell does not execute page find scripts" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'FocusAddress' \
  "Windows browser toolbar exposes address focus for shortcuts" \
  "Windows browser toolbar does not expose address focus for shortcuts" \
  apps/windows/actro/src/BrowserToolbar.cs

missing_navigation_policy_symbols=()
for policy_symbol in TryNormalizeAddress TryNormalizeLocalFilePath IsAllowedNavigationAddress IsAllowedNavigationUri IsHomeAddress IsHistoryAddress IsBookmarksAddress IsInternalAddress HomeAddress HistoryAddress BookmarksAddress; do
  if ! rg -q "$policy_symbol" apps/windows/actro/src/NavigationPolicy.cs; then
    missing_navigation_policy_symbols+=("$policy_symbol")
  fi
done

if [[ "${#missing_navigation_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows navigation policy is missing expected symbols"
  printf '%s\n' "${missing_navigation_policy_symbols[@]}"
else
  print_ok "Windows navigation policy owns URL normalization rules"
fi

check_pattern_in_each_file \
  'NavigationPolicy' \
  "Windows tests exercise navigation policy" \
  "Windows tests do not exercise navigation policy" \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'BrowserSessionStore' \
  "Windows tests exercise browser session persistence" \
  "Windows tests do not exercise browser session persistence" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_session_store_symbols=()
for session_symbol in BrowserSessionSnapshot BrowserSessionTabSnapshot CreateSnapshot Load Save; do
  if ! rg -q "$session_symbol" apps/windows/actro/src/BrowserSessionStore.cs; then
    missing_session_store_symbols+=("$session_symbol")
  fi
done

if [[ "${#missing_session_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser session store is missing expected symbols"
  printf '%s\n' "${missing_session_store_symbols[@]}"
else
  print_ok "Windows browser session store owns persisted tab snapshots"
fi

missing_session_lifecycle=()
for session_call in RestoreInitialTabsAsync PersistSession FormClosing BrowserSessionStore; do
  if ! rg -q "$session_call" apps/windows/actro/src/MainForm.cs; then
    missing_session_lifecycle+=("$session_call")
  fi
done

if [[ "${#missing_session_lifecycle[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser session lifecycle calls"
  printf '%s\n' "${missing_session_lifecycle[@]}"
else
  print_ok "Windows shell restores and persists browser sessions"
fi

check_pattern_in_each_file \
  'BrowserWindowStateStore' \
  "Windows tests exercise browser window state persistence" \
  "Windows tests do not exercise browser window state persistence" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj \
  apps/windows/actro.tests/Program.cs

missing_window_state_store_symbols=()
for window_state_symbol in BrowserWindowStateSnapshot CreateDefault CreateSnapshot Load Save IsMaximized MinimumWidth MinimumHeight; do
  if ! rg -q "$window_state_symbol" apps/windows/actro/src/BrowserWindowStateStore.cs; then
    missing_window_state_store_symbols+=("$window_state_symbol")
  fi
done

if [[ "${#missing_window_state_store_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows browser window state store is missing expected symbols"
  printf '%s\n' "${missing_window_state_store_symbols[@]}"
else
  print_ok "Windows browser window state store owns persisted window geometry"
fi

missing_window_state_lifecycle=()
for window_state_call in RestoreWindowState PersistWindowState BrowserWindowStateStore RestoreBounds WindowState; do
  if ! rg -q "$window_state_call" apps/windows/actro/src/MainForm.cs; then
    missing_window_state_lifecycle+=("$window_state_call")
  fi
done

if [[ "${#missing_window_state_lifecycle[@]}" -ne 0 ]]; then
  print_failure "Windows shell is missing browser window state lifecycle calls"
  printf '%s\n' "${missing_window_state_lifecycle[@]}"
else
  print_ok "Windows shell restores and persists browser window geometry"
fi

check_pattern_in_each_file \
  'HostBridgeProtocol' \
  "Windows tests link the host bridge protocol" \
  "Windows tests do not link the host bridge protocol" \
  apps/windows/actro.tests/Actro.Windows.Tests.csproj

check_pattern_in_each_file \
  'HostBridgeDispatcher' \
  "Windows tests exercise the host bridge dispatcher" \
  "Windows tests do not exercise the host bridge dispatcher" \
  apps/windows/actro.tests/Program.cs

check_pattern_in_each_file \
  'new HostMessageBridge' \
  "Windows shell attaches a host message bridge" \
  "Windows shell does not attach a host message bridge" \
  apps/windows/actro/src/MainForm.cs

check_pattern_in_each_file \
  'HostBridgeDispatcher' \
  "Windows WebView2 bridge delegates to the host bridge dispatcher" \
  "Windows WebView2 bridge does not delegate to the host bridge dispatcher" \
  apps/windows/actro/src/HostMessageBridge.cs

check_pattern_in_each_file \
  'WebMessageReceived' \
  "Windows host bridge receives WebView2 messages" \
  "Windows host bridge does not receive WebView2 messages" \
  apps/windows/actro/src/HostMessageBridge.cs

check_pattern_in_each_file \
  'PostWebMessageAsJson' \
  "Windows host bridge posts JSON responses to WebView2" \
  "Windows host bridge does not post JSON responses to WebView2" \
  apps/windows/actro/src/HostMessageBridge.cs

missing_host_bridge_methods=()
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
    missing_host_bridge_methods+=("$host_method")
  fi
done

if [[ "${#missing_host_bridge_methods[@]}" -ne 0 ]]; then
  print_failure "Windows host bridge is missing browser host methods"
  printf '%s\n' "${missing_host_bridge_methods[@]}"
else
  print_ok "Windows host bridge exposes browser host methods"
fi

missing_host_bridge_actions=()
for host_action in \
  HostBridgeActions \
  OpenTabFromHostAsync \
  CloseTabFromHostAsync \
  SelectTabFromHostAsync \
	  NavigateFromHostAsync \
	  GetTabsForHost \
	  GetActiveTabForHost \
	  RemoveBookmarkFromHostAsync \
	  RemoveHistoryEntryFromHostAsync
do
  if ! rg -q "$host_action" apps/windows/actro/src/HostBridgeProtocol.cs apps/windows/actro/src/MainForm.cs; then
    missing_host_bridge_actions+=("$host_action")
  fi
done

if [[ "${#missing_host_bridge_actions[@]}" -ne 0 ]]; then
  print_failure "Windows host bridge is missing browser action delegates"
  printf '%s\n' "${missing_host_bridge_actions[@]}"
else
  print_ok "Windows host bridge delegates browser actions to the shell"
fi

check_pattern_in_each_file \
  'forbidden_source' \
  "Windows host bridge enforces a source allowlist" \
  "Windows host bridge does not enforce a source allowlist" \
  apps/windows/actro/src/HostBridgeProtocol.cs

check_pattern_in_each_file \
  'actro_core_supported_contract_name' \
  "Windows C# adapter and Rust C API header share ActroCore metadata symbols" \
  "Windows C# adapter and Rust C API header do not share ActroCore metadata symbols" \
  apps/windows/actro/src/ActroCore.cs \
  crates/actro-core/include/actro_core.h

check_pattern_in_each_file \
  'native\\\*\.dll' \
  "Windows project copies native ActroCore DLL artifacts" \
  "Windows project does not copy native ActroCore DLL artifacts" \
  apps/windows/actro/Actro.Windows.csproj

check_pattern_in_each_file \
  'warning MSB3277' \
  "Windows verifier gates unexpected dotnet warnings" \
  "Windows verifier does not gate unexpected dotnet warnings" \
  scripts/verify-windows.sh

verify_windows_script="$(
  node -e "const p=require('./package.json'); console.log(p.scripts['verify:windows'] || '')"
)"

if [[ "$verify_windows_script" != "bash scripts/verify-windows.sh" ]]; then
  print_failure "verify:windows is not wired to scripts/verify-windows.sh"
  printf '%s\n' "$verify_windows_script"
else
  print_ok "verify:windows is wired to scripts/verify-windows.sh"
fi

if [[ -f apps/windows/actro/Actro.Windows.csproj ]]; then
  print_ok "Windows project scaffold exists"
else
  print_failure "Windows project scaffold is missing"
fi

# Packaging: apps/windows/VERSION is the single version source; the csproj
# reads it into <Version> and scripts/package-windows.ps1 reads it for
# artifact and installer stamping.
if [[ -f apps/windows/VERSION ]] && rg -q '^[0-9]+\.[0-9]+\.[0-9]+$' apps/windows/VERSION; then
  print_ok "Windows version file carries a MAJOR.MINOR.PATCH version"
else
  print_failure "apps/windows/VERSION is missing or not a MAJOR.MINOR.PATCH version"
fi

check_pattern_in_each_file \
  'ActroWindowsVersionFile' \
  "Windows project single-sources its version from apps/windows/VERSION" \
  "Windows project does not single-source its version from apps/windows/VERSION" \
  apps/windows/actro/Actro.Windows.csproj

missing_packaging_pipeline_symbols=()
for packaging_symbol in \
  'apps.windows.VERSION' \
  '[-][-]self-contained' \
  'actro_core\.dll' \
  'Compress-Archive' \
  'ISCC' \
  'actro-installer\.iss' \
  'LinkId=2124703' \
  'code-signing certificate'
do
  if ! rg -q "$packaging_symbol" scripts/package-windows.ps1; then
    missing_packaging_pipeline_symbols+=("$packaging_symbol")
  fi
done

if [[ "${#missing_packaging_pipeline_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows packaging pipeline is missing expected behavior"
  printf '%s\n' "${missing_packaging_pipeline_symbols[@]}"
else
  print_ok "Windows packaging pipeline publishes, gates actro_core.dll, zips and compiles the installer"
fi

check_pattern_in_each_file \
  'package-windows\.ps1' \
  "Windows packaging entry point delegates to the PowerShell pipeline" \
  "Windows packaging entry point does not delegate to the PowerShell pipeline" \
  scripts/package-windows.sh

missing_installer_policy_symbols=()
for installer_symbol in \
  'PrivilegesRequired=lowest' \
  '\{autopf\}' \
  '\{autoprograms\}' \
  'F3017226-FE2A-4295-8BDF-00C3A9A7E4C5' \
  'WOW6432Node' \
  'IsWebView2RuntimeInstalled' \
  '\{#AppVersion\}' \
  'unsigned'
do
  if ! rg -q "$installer_symbol" apps/windows/actro/packaging/actro-installer.iss; then
    missing_installer_policy_symbols+=("$installer_symbol")
  fi
done

if [[ "${#missing_installer_policy_symbols[@]}" -ne 0 ]]; then
  print_failure "Windows installer script is missing per-user/WebView2/version policy"
  printf '%s\n' "${missing_installer_policy_symbols[@]}"
else
  print_ok "Windows installer installs per-user, gates WebView2 and stamps the shared version"
fi

check_pattern_in_each_file \
  'package-windows' \
  "Windows app docs record the packaging entry point" \
  "Windows app docs do not record the packaging entry point" \
  apps/windows/actro/README.md \
  docs/migration/windows-client.md

if [[ "$failures" -ne 0 ]]; then
  echo
  echo "Windows layout check failed with $failures issue(s)"
  exit 1
fi

echo
echo "Windows layout check passed"
