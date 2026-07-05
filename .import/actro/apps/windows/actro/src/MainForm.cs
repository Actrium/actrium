using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Windows.Forms;
using Microsoft.Web.WebView2.Core;
using Microsoft.Web.WebView2.WinForms;

namespace Actro.Windows;

internal sealed class MainForm : Form
{
    private readonly WindowsStartupOptions startupOptions;
    private readonly BrowserSessionStore sessionStore;
    private readonly BrowserHistoryStore historyStore;
    private readonly BrowserBookmarkStore bookmarkStore;
    private readonly BrowserSearchSettingsStore searchSettingsStore;
    private readonly BrowserStartupSettingsStore startupSettingsStore;
    private readonly BrowserWindowStateStore windowStateStore;
    private readonly string userDataFolder;
    private readonly bool shouldRestoreSession;
    private readonly bool shouldPersistSession;
    private readonly bool shouldRestoreWindowState;
    private readonly bool shouldPersistWindowState;
    private readonly bool isPrivateWindow;
    private readonly BrowserAppMenu appMenu;
    private readonly BrowserTabSidebar tabSidebar;
    private readonly SplitContainer contentSplit;
    private readonly BrowserToolbar toolbar;
    private readonly BrowserFindBar findBar;
    private readonly BrowserDownloadManager downloadManager;
    private readonly BrowserPermissionManager permissionManager;
    private readonly BrowserExternalProtocolManager externalProtocolManager;
    private readonly BrowserContentBlockingStore contentBlockingStore;
    private readonly BrowserStorageSyncCredentialStore browserSyncCredentialStore;
    private readonly BrowserStorageSyncScheduler browserSyncScheduler;
    private readonly Panel webViewHost;
    private readonly List<BrowserTab> tabs = [];
    private readonly BrowserClosedTabStack closedTabs = new();
    private CoreWebView2Environment? webViewEnvironment;
    private BrowserTab? activeTab;
    private BrowserBookmarkSnapshot bookmarkSnapshot = BrowserBookmarkSnapshot.Empty();
    private BrowserSearchSettingsSnapshot searchSettingsSnapshot = BrowserSearchSettingsSnapshot.Default();
    private BrowserStartupSettingsSnapshot startupSettingsSnapshot = BrowserStartupSettingsSnapshot.Default();
    private FormBorderStyle windowedFormBorderStyle;
    private FormWindowState windowedWindowState;
    private bool windowedTopMost;
    private bool windowedSidebarVisible;
    private bool windowedToolbarVisible;
    private bool windowedFindBarVisible;
    private int sidebarWidth;
    private bool isApplyingSidebarLayout;
    private bool isFullScreen;
    private bool isBrowserFullScreen;
    private bool isPageFullScreen;
    private bool contentBlockingEnabled;
    private string lastFindQuery = string.Empty;
    private string? lastCompletedDownloadPath;

    public MainForm(WindowsStartupOptions startupOptions)
        : this(
            startupOptions,
            BrowserSessionStore.CreateDefault(),
            BrowserWindowStateStore.CreateDefault())
    {
    }

    internal MainForm(WindowsStartupOptions startupOptions, BrowserSessionStore sessionStore)
        : this(startupOptions, sessionStore, BrowserWindowStateStore.CreateDefault())
    {
    }

    internal MainForm(
        WindowsStartupOptions startupOptions,
        BrowserSessionStore sessionStore,
        BrowserWindowStateStore windowStateStore)
        : this(
            startupOptions,
            sessionStore,
            windowStateStore,
            BrowserProfilePolicy.GetDefaultUserDataFolder(),
            restoreSession: true,
            persistSession: true,
            restoreWindowState: true,
            persistWindowState: true,
            isPrivateWindow: false)
    {
    }

    internal MainForm(
        WindowsStartupOptions startupOptions,
        BrowserSessionStore sessionStore,
        BrowserWindowStateStore windowStateStore,
        string userDataFolder,
        bool restoreSession,
        bool persistSession,
        bool restoreWindowState,
        bool persistWindowState,
        bool isPrivateWindow)
    {
        this.startupOptions = startupOptions;
        this.sessionStore = sessionStore;
        historyStore = BrowserHistoryStore.CreateDefault();
        bookmarkStore = BrowserBookmarkStore.CreateDefault();
        bookmarkSnapshot = bookmarkStore.Load();
        searchSettingsStore = BrowserSearchSettingsStore.CreateDefault();
        searchSettingsSnapshot = searchSettingsStore.Load();
        startupSettingsStore = BrowserStartupSettingsStore.CreateDefault();
        startupSettingsSnapshot = startupSettingsStore.Load();
        this.windowStateStore = windowStateStore;
        this.userDataFolder = userDataFolder;
        shouldRestoreSession = restoreSession;
        shouldPersistSession = persistSession;
        shouldRestoreWindowState = restoreWindowState;
        shouldPersistWindowState = persistWindowState;
        this.isPrivateWindow = isPrivateWindow;
        contentBlockingStore = BrowserContentBlockingStore.CreateDefault();
        contentBlockingEnabled = contentBlockingStore.Load().IsEnabled;
        browserSyncCredentialStore = BrowserStorageSyncCredentialStore.CreateDefault();
        browserSyncScheduler = new BrowserStorageSyncScheduler(
            BrowserStorageSyncCoordinator.CreateDefault(browserSyncCredentialStore));
        Text = GetProductWindowTitle();
        Width = 1280;
        Height = 800;
        MinimumSize = new Size(960, 640);

        appMenu = new BrowserAppMenu();
        tabSidebar = new BrowserTabSidebar();
        toolbar = new BrowserToolbar();
        findBar = new BrowserFindBar();
        downloadManager = new BrowserDownloadManager(this, toolbar.SetStatus, SetLastCompletedDownload);
        permissionManager = new BrowserPermissionManager(this, toolbar.SetStatus);
        externalProtocolManager = new BrowserExternalProtocolManager(this, toolbar.SetStatus);
        webViewHost = new Panel
        {
            Dock = DockStyle.Fill,
        };
        // Structure follows the macOS app: the chrome bars sit above one
        // content area split into [vertical tab sidebar | web content host].
        // Width and collapse geometry come from BrowserSidebarLayoutPolicy.
        contentSplit = new SplitContainer
        {
            Dock = DockStyle.Fill,
            FixedPanel = FixedPanel.Panel1,
            TabStop = false,
        };
        contentSplit.Panel1.Controls.Add(tabSidebar);
        contentSplit.Panel2.Controls.Add(webViewHost);
        sidebarWidth = BrowserSidebarLayoutPolicy.DefaultWidth(DeviceDpi);

        Controls.Add(contentSplit);
        Controls.Add(findBar);
        Controls.Add(toolbar);
        // Compact app menu: every menu click funnels into the single
        // command router shared with keyboard shortcut handling.
        appMenu.CommandRequested += (_, args) => ExecuteCommand(args.Command, args.Argument);
        appMenu.SetSearchEngine(searchSettingsSnapshot.SearchEngineId);
        appMenu.SetRestorePreviousSession(startupSettingsSnapshot.RestorePreviousSession);
        appMenu.SetContentBlockingEnabled(contentBlockingEnabled);
        WireSidebar();
        WireToolbar();
        WireFindBar();
        contentSplit.SplitterMoved += (_, _) => OnSidebarSplitterMoved();
        Resize += (_, _) => ApplySidebarLayout();

        // Theme lifecycle: subscribe per form and unsubscribe on FormClosed so
        // closed windows are not kept alive by the singleton manager across
        // the multi-window application lifetime. A DPI change re-runs the
        // cascade because every ApplyTheme re-measures from DeviceDpi.
        BrowserThemeManager.Instance.Changed += OnSystemThemeChanged;
        FormClosed += OnFormClosedUnsubscribeTheme;
        // The app menu is a component, not a child control, so the form
        // does not dispose it implicitly.
        FormClosed += (_, _) => appMenu.Dispose();
        DpiChanged += (_, args) =>
        {
            if (args.DeviceDpiOld > 0)
            {
                // Keep the user's chosen sidebar width proportional when the
                // window moves across monitors with different DPIs.
                sidebarWidth = (int)Math.Round(
                    (double)sidebarWidth * args.DeviceDpiNew / args.DeviceDpiOld,
                    MidpointRounding.AwayFromZero);
            }

            ApplyTheme(BrowserThemeManager.Instance.Current);
        };
        ApplyTheme(BrowserThemeManager.Instance.Current);

        Load += OnLoad;
        FormClosing += OnFormClosing;
    }

    public event EventHandler<BrowserNewWindowRequestedEventArgs>? NewWindowRequested;
    public event EventHandler<BrowserNewWindowRequestedEventArgs>? NewPrivateWindowRequested;

    private async void OnLoad(object? sender, EventArgs args)
    {
        try
        {
            RestoreWindowState();
            // The split container only receives its real size once the form
            // is laid out, so the initial clamp runs here as well.
            ApplySidebarLayout();
            await RestoreInitialTabsAsync();
        }
        catch (Exception error)
        {
            ShowStartupError(error);
        }
    }

    private const int WM_SETTINGCHANGE = 0x001A;
    private const int DWMWA_USE_IMMERSIVE_DARK_MODE = 20;
    private const int DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1 = 19;

    [DllImport("dwmapi.dll", ExactSpelling = true)]
    private static extern int DwmSetWindowAttribute(IntPtr hwnd, int attribute, ref int value, int size);

    protected override void WndProc(ref Message message)
    {
        // The system broadcasts WM_SETTINGCHANGE with "ImmersiveColorSet"
        // when the light/dark app mode flips. The manager compares before
        // raising, so N open windows receiving the broadcast still produce
        // a single theme cascade.
        if (message.Msg == WM_SETTINGCHANGE
            && string.Equals(
                Marshal.PtrToStringUni(message.LParam),
                "ImmersiveColorSet",
                StringComparison.OrdinalIgnoreCase))
        {
            BrowserThemeManager.Instance.RefreshFromSystem();
        }

        base.WndProc(ref message);
    }

    protected override void OnHandleCreated(EventArgs args)
    {
        base.OnHandleCreated(args);
        ApplyImmersiveTitleBar(BrowserThemeManager.Instance.Current.IsDark);
    }

    private void OnSystemThemeChanged(object? sender, EventArgs args)
    {
        ApplyTheme(BrowserThemeManager.Instance.Current);
    }

    private void OnFormClosedUnsubscribeTheme(object? sender, FormClosedEventArgs args)
    {
        BrowserThemeManager.Instance.Changed -= OnSystemThemeChanged;
    }

    /// <summary>
    /// Cascades the shared theme through the whole chrome stack, the caption
    /// bar and every tab's WebView2 profile (private-window profiles
    /// included). Runs at construction, on system theme change and on DPI
    /// change; consumers re-measure their metrics from DeviceDpi.
    /// </summary>
    private void ApplyTheme(BrowserTheme theme)
    {
        BackColor = theme.WindowBackground;
        webViewHost.BackColor = theme.WindowBackground;
        // Global renderer so menu drop-downs (which do not inherit a parent
        // strip's renderer) follow the palette as well.
        ToolStripManager.Renderer = new BrowserToolStripRenderer(theme);
        appMenu.ApplyTheme(theme);
        contentSplit.BackColor = theme.WindowBackground;
        tabSidebar.ApplyTheme(theme);
        toolbar.ApplyTheme(theme);
        findBar.ApplyTheme(theme);
        // Re-clamp the sidebar band: on a DPI change the scaled minimum and
        // the stored width both move.
        ApplySidebarLayout();
        ApplyImmersiveTitleBar(theme.IsDark);
        foreach (var tab in tabs)
        {
            ApplyWebViewColorScheme(tab, theme);
        }
    }

    /// <summary>Dark caption bar: attribute 20, with the pre-20H1 fallback 19.</summary>
    private void ApplyImmersiveTitleBar(bool isDark)
    {
        if (!IsHandleCreated)
        {
            return;
        }

        var value = isDark ? 1 : 0;
        if (DwmSetWindowAttribute(Handle, DWMWA_USE_IMMERSIVE_DARK_MODE, ref value, sizeof(int)) != 0)
        {
            _ = DwmSetWindowAttribute(Handle, DWMWA_USE_IMMERSIVE_DARK_MODE_BEFORE_20H1, ref value, sizeof(int));
        }
    }

    private static void ApplyWebViewColorScheme(BrowserTab tab, BrowserTheme theme)
    {
        if (tab.WebView.CoreWebView2 is CoreWebView2 coreWebView)
        {
            coreWebView.Profile.PreferredColorScheme = theme.IsDark
                ? CoreWebView2PreferredColorScheme.Dark
                : CoreWebView2PreferredColorScheme.Light;
        }
    }

    private void OnFormClosing(object? sender, FormClosingEventArgs args)
    {
        if (shouldPersistWindowState)
        {
            PersistWindowState();
        }

        if (shouldPersistSession)
        {
            PersistSession();
        }

        browserSyncScheduler.Dispose();
    }

    private static void ConfigureWebView(CoreWebView2 coreWebView)
    {
        coreWebView.Settings.AreDefaultContextMenusEnabled = true;
        coreWebView.Settings.AreDevToolsEnabled = true;
        coreWebView.Settings.IsStatusBarEnabled = false;
        coreWebView.Settings.IsZoomControlEnabled = true;
    }

    private void WireSidebar()
    {
        tabSidebar.NewTabRequested += (_, _) => RequestNewTab();
        tabSidebar.TabSelected += (_, args) => ActivateTab(args.TabId);
        tabSidebar.TabCloseRequested += async (_, args) => await CloseTabAsync(args.TabId);
    }

    /// <summary>
    /// Applies the policy-clamped sidebar width to the split container.
    /// Runs on construction, load, resize, theme/DPI cascade and after the
    /// user drags the splitter; no-ops while collapsed or mid-layout.
    /// </summary>
    private void ApplySidebarLayout()
    {
        if (contentSplit.Panel1Collapsed)
        {
            return;
        }

        var hostWidth = contentSplit.ClientSize.Width;
        if (hostWidth <= 0)
        {
            return;
        }

        var clampedWidth = BrowserSidebarLayoutPolicy.ClampWidth(sidebarWidth, hostWidth, DeviceDpi);
        if (clampedWidth > hostWidth - contentSplit.Panel2MinSize - contentSplit.SplitterWidth)
        {
            // Transient mid-layout size too narrow for the clamp band; the
            // next resize pass re-applies with real dimensions.
            return;
        }

        if (contentSplit.SplitterDistance == clampedWidth)
        {
            return;
        }

        isApplyingSidebarLayout = true;
        try
        {
            contentSplit.SplitterDistance = clampedWidth;
        }
        finally
        {
            isApplyingSidebarLayout = false;
        }
    }

    private void OnSidebarSplitterMoved()
    {
        if (isApplyingSidebarLayout || contentSplit.Panel1Collapsed)
        {
            return;
        }

        var dpi = DeviceDpi;
        if (BrowserSidebarLayoutPolicy.ShouldCollapse(contentSplit.SplitterDistance, dpi))
        {
            // Dragging the splitter (nearly) shut reads as a collapse
            // gesture; the toggle command or shortcut reopens the sidebar.
            contentSplit.Panel1Collapsed = true;
            return;
        }

        sidebarWidth = BrowserSidebarLayoutPolicy.ClampWidth(
            contentSplit.SplitterDistance,
            contentSplit.ClientSize.Width,
            dpi);
        ApplySidebarLayout();
    }

    private void ToggleSidebar()
    {
        if (isFullScreen)
        {
            // Full screen hides all chrome; only flip the preference that
            // ExitFullScreenMode restores.
            windowedSidebarVisible = !windowedSidebarVisible;
            return;
        }

        contentSplit.Panel1Collapsed = !contentSplit.Panel1Collapsed;
        if (!contentSplit.Panel1Collapsed)
        {
            ApplySidebarLayout();
        }
    }

    private void WireToolbar()
    {
        toolbar.BackRequested += (_, _) => NavigateBack();
        toolbar.ForwardRequested += (_, _) => NavigateForward();
        toolbar.ReloadRequested += (_, _) => ReloadOrStopActiveTab();
        toolbar.HomeRequested += (_, _) => NavigateHomeDocument();
        toolbar.MenuRequested += (_, _) => ShowAppMenu();
        toolbar.AddressRequested += (_, args) => NavigateAddress(args.Address);
    }

    private void ShowAppMenu()
    {
        toolbar.ShowAppMenu(appMenu);
    }

    /// <summary>
    /// Single command router: every compact-menu click and every keyboard
    /// shortcut lands here, so command behavior cannot fork between input
    /// paths. The argument channel carries per-item data that a bare enum
    /// cannot express — currently the search engine id.
    /// </summary>
    private void ExecuteCommand(BrowserMenuCommand command, string? argument = null)
    {
        switch (command)
        {
            case BrowserMenuCommand.NewTab:
                RequestNewTab();
                break;
            case BrowserMenuCommand.NewWindow:
                RequestNewWindow();
                break;
            case BrowserMenuCommand.NewPrivateWindow:
                RequestNewPrivateWindow();
                break;
            case BrowserMenuCommand.CloseTab:
                RequestCloseActiveTab();
                break;
            case BrowserMenuCommand.ReopenClosedTab:
                ReopenClosedTab();
                break;
            case BrowserMenuCommand.DuplicateTab:
                DuplicateActiveTab();
                break;
            case BrowserMenuCommand.OpenFile:
                OpenLocalFile();
                break;
            case BrowserMenuCommand.CopyAddress:
                CopyActivePageAddress();
                break;
            case BrowserMenuCommand.OpenInDefaultBrowser:
                OpenActivePageInDefaultBrowser();
                break;
            case BrowserMenuCommand.SavePdf:
                _ = SaveActiveTabAsPdfAsync();
                break;
            case BrowserMenuCommand.Print:
                PrintActiveTab();
                break;
            case BrowserMenuCommand.Exit:
                Close();
                break;
            case BrowserMenuCommand.Back:
                NavigateBack();
                break;
            case BrowserMenuCommand.Forward:
                NavigateForward();
                break;
            case BrowserMenuCommand.Home:
                NavigateHomeDocument();
                break;
            case BrowserMenuCommand.ShowHistory:
                NavigateHistoryDocument();
                break;
            case BrowserMenuCommand.BookmarkPage:
                ToggleActivePageBookmark();
                break;
            case BrowserMenuCommand.ShowBookmarks:
                NavigateBookmarksDocument();
                break;
            case BrowserMenuCommand.Reload:
                ReloadActiveTab();
                break;
            case BrowserMenuCommand.Stop:
                StopLoadingActiveTab();
                break;
            case BrowserMenuCommand.ZoomIn:
                ZoomInActiveTab();
                break;
            case BrowserMenuCommand.ZoomOut:
                ZoomOutActiveTab();
                break;
            case BrowserMenuCommand.ZoomReset:
                ResetActiveTabZoom();
                break;
            case BrowserMenuCommand.FullScreen:
                ToggleBrowserFullScreen();
                break;
            case BrowserMenuCommand.ToggleSidebar:
                ToggleSidebar();
                break;
            case BrowserMenuCommand.Find:
                OpenFindBar();
                break;
            case BrowserMenuCommand.FindNext:
                RepeatFind(forward: true);
                break;
            case BrowserMenuCommand.FindPrevious:
                RepeatFind(forward: false);
                break;
            case BrowserMenuCommand.FocusAddress:
                toolbar.FocusAddress();
                break;
            case BrowserMenuCommand.DevTools:
                OpenDeveloperTools();
                break;
            case BrowserMenuCommand.OpenDownloadsFolder:
                OpenDownloadsFolder();
                break;
            case BrowserMenuCommand.OpenLastDownload:
                OpenLastDownload();
                break;
            case BrowserMenuCommand.RevealLastDownload:
                RevealLastDownloadInFolder();
                break;
            case BrowserMenuCommand.SelectSearchEngine:
                if (!string.IsNullOrWhiteSpace(argument))
                {
                    SetSearchEngine(argument);
                }

                break;
            case BrowserMenuCommand.RestoreSessionToggle:
                SetRestorePreviousSession(!startupSettingsSnapshot.RestorePreviousSession);
                break;
            case BrowserMenuCommand.BlockAdsToggle:
                ToggleContentBlocking();
                break;
            case BrowserMenuCommand.ConfigureSync:
                ConfigureBrowserSyncConnection();
                break;
            case BrowserMenuCommand.SyncNow:
                _ = SyncBrowserDataNowAsync();
                break;
            case BrowserMenuCommand.SyncStatus:
                ShowBrowserSyncStatus();
                break;
            case BrowserMenuCommand.ClearBrowsingData:
                _ = ClearBrowsingDataAsync();
                break;
        }
    }

    private void WireFindBar()
    {
        findBar.FindRequested += async (_, args) => await FindInActiveTabAsync(args.Query, args.Forward);
        findBar.CloseRequested += (_, _) => CloseFindBar();
    }

    /// <summary>
    /// Sole owner of keyboard command handling: primaries and aliases from
    /// the menu model route into ExecuteCommand (the app menu never
    /// registers ShortcutKeys, so key handling cannot fork between the menu
    /// and this method), Escape resolves through BrowserEscapePolicy, and
    /// Alt+E/Alt+F/F10 open the compact app menu (Chrome/Edge convention).
    /// </summary>
    protected override bool ProcessCmdKey(ref Message message, Keys keyData)
    {
        if (TryHandleTabShortcut(keyData))
        {
            return true;
        }

        switch (keyData)
        {
            case Keys.Alt | Keys.E:
            case Keys.Alt | Keys.F:
            case Keys.F10:
                ShowAppMenu();
                return true;
            case Keys.Control | Keys.L:
                ExecuteCommand(BrowserMenuCommand.FocusAddress);
                return true;
            case Keys.Control | Keys.T:
                ExecuteCommand(BrowserMenuCommand.NewTab);
                return true;
            case Keys.Control | Keys.N:
                ExecuteCommand(BrowserMenuCommand.NewWindow);
                return true;
            case Keys.Control | Keys.Shift | Keys.N:
                ExecuteCommand(BrowserMenuCommand.NewPrivateWindow);
                return true;
            case Keys.Control | Keys.W:
                ExecuteCommand(BrowserMenuCommand.CloseTab);
                return true;
            case Keys.Control | Keys.Shift | Keys.T:
                ExecuteCommand(BrowserMenuCommand.ReopenClosedTab);
                return true;
            case Keys.Control | Keys.F:
                ExecuteCommand(BrowserMenuCommand.Find);
                return true;
            case Keys.F3:
                ExecuteCommand(BrowserMenuCommand.FindNext);
                return true;
            case Keys.Shift | Keys.F3:
                ExecuteCommand(BrowserMenuCommand.FindPrevious);
                return true;
            case Keys.Control | Keys.O:
                ExecuteCommand(BrowserMenuCommand.OpenFile);
                return true;
            case Keys.Control | Keys.P:
                ExecuteCommand(BrowserMenuCommand.Print);
                return true;
            case Keys.Control | Keys.S:
                ExecuteCommand(BrowserMenuCommand.SavePdf);
                return true;
            case Keys.Control | Keys.R:
            case Keys.F5:
                ExecuteCommand(BrowserMenuCommand.Reload);
                return true;
            case Keys.Escape:
                switch (BrowserEscapePolicy.Resolve(
                    findBar.Visible,
                    isPageFullScreen,
                    isBrowserFullScreen,
                    activeTab?.IsLoading == true))
                {
                    case BrowserEscapeAction.CloseFindBar:
                        CloseFindBar();
                        break;
                    case BrowserEscapeAction.ExitFullScreen:
                        RequestExitFullScreen();
                        break;
                    case BrowserEscapeAction.Stop:
                        ExecuteCommand(BrowserMenuCommand.Stop);
                        break;
                }

                return true;
            case Keys.Control | Keys.Oemplus:
            case Keys.Control | Keys.Shift | Keys.Oemplus:
            case Keys.Control | Keys.Add:
                ExecuteCommand(BrowserMenuCommand.ZoomIn);
                return true;
            case Keys.Control | Keys.OemMinus:
            case Keys.Control | Keys.Subtract:
                ExecuteCommand(BrowserMenuCommand.ZoomOut);
                return true;
            case Keys.Control | Keys.D0:
            case Keys.Control | Keys.NumPad0:
                ExecuteCommand(BrowserMenuCommand.ZoomReset);
                return true;
            case Keys.Alt | Keys.Left:
                ExecuteCommand(BrowserMenuCommand.Back);
                return true;
            case Keys.Alt | Keys.Right:
                ExecuteCommand(BrowserMenuCommand.Forward);
                return true;
            case Keys.Alt | Keys.Home:
                ExecuteCommand(BrowserMenuCommand.Home);
                return true;
            case Keys.Control | Keys.H:
                ExecuteCommand(BrowserMenuCommand.ShowHistory);
                return true;
            case Keys.Control | Keys.D:
                ExecuteCommand(BrowserMenuCommand.BookmarkPage);
                return true;
            case Keys.Control | Keys.Shift | Keys.O:
                ExecuteCommand(BrowserMenuCommand.ShowBookmarks);
                return true;
            case Keys.F11:
                ExecuteCommand(BrowserMenuCommand.FullScreen);
                return true;
            case Keys.Control | Keys.Shift | Keys.Oemcomma:
                ExecuteCommand(BrowserMenuCommand.ToggleSidebar);
                return true;
            case Keys.Control | Keys.Shift | Keys.I:
                ExecuteCommand(BrowserMenuCommand.DevTools);
                return true;
            case Keys.Control | Keys.J:
                ExecuteCommand(BrowserMenuCommand.OpenDownloadsFolder);
                return true;
            case Keys.Control | Keys.Shift | Keys.Delete:
                ExecuteCommand(BrowserMenuCommand.ClearBrowsingData);
                return true;
        }

        return base.ProcessCmdKey(ref message, keyData);
    }

    private void WireWebViewEvents(BrowserTab tab, CoreWebView2 coreWebView)
    {
        coreWebView.AddWebResourceRequestedFilter(
            "*",
            CoreWebView2WebResourceContext.All,
            CoreWebView2WebResourceRequestSourceKinds.Document);
        coreWebView.WebResourceRequested += (_, args) =>
            HandleWebResourceRequested(coreWebView, args);

        coreWebView.NavigationStarting += (_, args) =>
            HandleNavigationStarting(tab, args);

        coreWebView.NavigationCompleted += (_, args) =>
        {
            tab.IsLoading = false;
            if (!args.IsSuccess)
            {
                ShowNavigationError(tab, args.WebErrorStatus.ToString());
                if (activeTab == tab)
                {
                    toolbar.SetStatus(BrowserNavigationErrorPolicy.FormatStatus(args.WebErrorStatus.ToString()));
                }

                return;
            }

            SyncActiveTabUi();
            SyncSidebar();
            RecordHistoryVisit(tab);
        };

        coreWebView.HistoryChanged += (_, _) =>
            SyncActiveTabUi();

        coreWebView.DownloadStarting += (_, args) =>
            downloadManager.HandleDownloadStarting(args);

        coreWebView.PermissionRequested += (_, args) =>
            permissionManager.HandlePermissionRequested(args);

        coreWebView.ContainsFullScreenElementChanged += (_, _) =>
            SyncFullScreenState(tab);

        coreWebView.NewWindowRequested += (_, args) =>
        {
            args.Handled = true;
            _ = OpenNewWindowRequestAsync(args.Uri);
        };

        coreWebView.SourceChanged += (_, _) =>
        {
            if (!string.IsNullOrWhiteSpace(coreWebView.Source)
                && coreWebView.Source != "about:blank")
            {
                tab.Address = coreWebView.Source;
            }

            SyncActiveTabUi();
        };

        coreWebView.StatusBarTextChanged += (_, _) =>
            SyncStatusBarText(tab);

        coreWebView.FaviconChanged += async (_, _) =>
            await UpdateTabFaviconAsync(tab);

        coreWebView.DocumentTitleChanged += (_, _) =>
        {
            tab.Title = string.IsNullOrWhiteSpace(coreWebView.DocumentTitle)
                ? BrowserStrings.Get("tab.defaultTitle")
                : coreWebView.DocumentTitle;
            SyncActiveTabUi();
            SyncSidebar();
        };

        coreWebView.ProcessFailed += (_, args) =>
            HandleProcessFailed(tab, args.ProcessFailedKind.ToString());
    }

    /// <summary>
    /// Refreshes the sidebar favicon for a tab from WebView2. The tab owns
    /// the decoded bitmap (disposed on replacement and with the tab); on a
    /// failed fetch or undecodable data the previous icon is kept, and the
    /// sidebar's letter avatar covers tabs that never produce one.
    /// </summary>
    private async Task UpdateTabFaviconAsync(BrowserTab tab)
    {
        if (tab.WebView.CoreWebView2 is not CoreWebView2 coreWebView)
        {
            return;
        }

        try
        {
            using var faviconStream = await coreWebView.GetFaviconAsync(CoreWebView2FaviconImageFormat.Png);
            if (faviconStream is null || faviconStream.Length == 0)
            {
                tab.Favicon = null;
            }
            else
            {
                using var decodedImage = Image.FromStream(faviconStream);
                // Copy the pixels: GDI+ images keep their source stream
                // alive, and the WebView2 stream is disposed right here.
                tab.Favicon = new Bitmap(decodedImage);
            }
        }
        catch (ArgumentException)
        {
            return;
        }
        catch (NotSupportedException)
        {
            return;
        }
        catch (ObjectDisposedException)
        {
            return;
        }
        catch (InvalidOperationException)
        {
            return;
        }
        catch (COMException)
        {
            return;
        }

        if (tabs.Contains(tab))
        {
            SyncSidebar();
        }
    }

    private void HandleWebResourceRequested(CoreWebView2 coreWebView, CoreWebView2WebResourceRequestedEventArgs args)
    {
        if (!contentBlockingEnabled
            || !BrowserContentBlockingPolicy.ShouldBlock(args.Request.Uri, args.ResourceContext.ToString()))
        {
            return;
        }

        args.Response = coreWebView.Environment.CreateWebResourceResponse(
            new MemoryStream(Array.Empty<byte>()),
            BrowserContentBlockingPolicy.BlockedStatusCode,
            BrowserContentBlockingPolicy.BlockedReasonPhrase,
            BrowserContentBlockingPolicy.BlockedResponseHeaders);
    }

    private void HandleProcessFailed(BrowserTab tab, string processFailedKind)
    {
        tab.IsLoading = false;
        if (!BrowserProcessFailurePolicy.ShouldReloadTab(processFailedKind))
        {
            if (activeTab == tab)
            {
                toolbar.SetStatus(BrowserProcessFailurePolicy.FormatStatus(processFailedKind));
            }

            SyncActiveTabUi();
            SyncSidebar();
            return;
        }

        if (activeTab == tab)
        {
            toolbar.SetStatus(BrowserProcessFailurePolicy.FormatRecoveryStatus(processFailedKind));
        }

        try
        {
            if (NavigationPolicy.IsHomeAddress(tab.Address))
            {
                NavigateHomeDocument(tab);
            }
            else
            {
                tab.WebView.CoreWebView2.Reload();
                SyncActiveTabUi();
                SyncSidebar();
            }
        }
        catch (ObjectDisposedException)
        {
            if (activeTab == tab)
            {
                toolbar.SetStatus(BrowserProcessFailurePolicy.FormatRecoveryFailedStatus(processFailedKind));
            }
        }
        catch (InvalidOperationException)
        {
            if (activeTab == tab)
            {
                toolbar.SetStatus(BrowserProcessFailurePolicy.FormatRecoveryFailedStatus(processFailedKind));
            }
        }
    }

    private void HandleNavigationStarting(BrowserTab tab, CoreWebView2NavigationStartingEventArgs args)
    {
        if (!NavigationPolicy.IsAllowedNavigationAddress(args.Uri))
        {
            args.Cancel = true;
            tab.IsLoading = false;
            if (activeTab == tab)
            {
                if (!externalProtocolManager.HandleExternalProtocol(args.Uri))
                {
                    toolbar.SetStatus(BrowserStrings.Get("status.navigationBlocked"));
                }
            }

            SyncActiveTabUi();
            SyncSidebar();
            return;
        }

        tab.IsLoading = true;
        if (!string.Equals(args.Uri, "about:blank", StringComparison.OrdinalIgnoreCase))
        {
            tab.Address = args.Uri;
            tab.IsShowingErrorPage = false;
        }

        SyncActiveTabUi();
        SyncSidebar();
    }

    private void SyncStatusBarText(BrowserTab tab)
    {
        if (activeTab != tab || tab.IsLoading)
        {
            return;
        }

        toolbar.SetStatus(tab.WebView.CoreWebView2?.StatusBarText ?? string.Empty);
    }

    private async Task<BrowserTab> CreateAndActivateTabAsync(string? initialAddress, Guid? tabId = null)
    {
        var tab = new BrowserTab(tabId);
        tabs.Add(tab);
        ActivateTab(tab.Id);

        await tab.WebView.EnsureCoreWebView2Async(await GetWebViewEnvironmentAsync());
        ConfigureWebView(tab.WebView.CoreWebView2);
        // Applied per tab at creation so tabs and windows opened after a
        // theme flip (including private-window throwaway profiles) start on
        // the current scheme; the Changed cascade re-applies it live.
        ApplyWebViewColorScheme(tab, BrowserThemeManager.Instance.Current);
        WireWebViewEvents(tab, tab.WebView.CoreWebView2);
        tab.HostMessageBridge = new HostMessageBridge(tab.Id, tab.WebView.CoreWebView2, CreateHostBridgeActions());
        await tab.HostMessageBridge.AttachAsync();

        NavigateInitialDocument(tab, initialAddress);
        SyncSidebar();
        return tab;
    }

    private async Task<CoreWebView2Environment> GetWebViewEnvironmentAsync()
    {
        if (webViewEnvironment is not null)
        {
            return webViewEnvironment;
        }

        webViewEnvironment = await CoreWebView2Environment.CreateAsync(
            userDataFolder: BrowserProfilePolicy.EnsureUserDataFolder(userDataFolder));
        return webViewEnvironment;
    }

    private async Task OpenNewWindowRequestAsync(string requestedAddress)
    {
        if (string.IsNullOrWhiteSpace(requestedAddress))
        {
            await CreateAndActivateTabAsync(null);
            return;
        }

        if (!NavigationPolicy.TryNormalizeAddress(requestedAddress, out var normalizedAddress))
        {
            toolbar.SetStatus(BrowserStrings.Get("status.newWindowBlocked"));
            return;
        }

        await CreateAndActivateTabAsync(normalizedAddress);
    }

    private async Task RestoreInitialTabsAsync()
    {
        if (startupOptions.InitialAddresses.Count > 0)
        {
            var initialTabs = new List<BrowserTab>();
            foreach (var initialAddress in startupOptions.InitialAddresses)
            {
                if (!string.IsNullOrWhiteSpace(initialAddress))
                {
                    initialTabs.Add(await CreateAndActivateTabAsync(initialAddress));
                }
            }

            if (initialTabs.Count > 0)
            {
                ActivateTab(initialTabs[0].Id);
                return;
            }
        }

        if (!string.IsNullOrWhiteSpace(startupOptions.InitialUrl))
        {
            await CreateAndActivateTabAsync(startupOptions.InitialUrl);
            return;
        }

        if (!shouldRestoreSession || !startupSettingsSnapshot.RestorePreviousSession)
        {
            await CreateAndActivateTabAsync(null);
            return;
        }

        var snapshot = sessionStore.Load();
        if (snapshot is null)
        {
            await CreateAndActivateTabAsync(null);
            return;
        }

        var restoredTabs = new List<BrowserTab>();
        foreach (var tabSnapshot in snapshot.Tabs)
        {
            var tab = await CreateAndActivateTabAsync(
                tabSnapshot.Address,
                Guid.TryParse(tabSnapshot.Id, out var snapshotTabId) ? snapshotTabId : null);
            tab.Title = tabSnapshot.Title;
            restoredTabs.Add(tab);
        }

        if (restoredTabs.Count == 0)
        {
            await CreateAndActivateTabAsync(null);
            return;
        }

        ActivateTab(restoredTabs[Math.Clamp(snapshot.ActiveTabIndex, 0, restoredTabs.Count - 1)].Id);
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void RequestNewTab()
    {
        _ = CreateAndActivateTabAsync(null);
    }

    private void RequestCloseActiveTab()
    {
        if (activeTab is not null)
        {
            _ = CloseTabAsync(activeTab.Id);
        }
    }

    private void RequestNewWindow(string? initialUrl = null)
    {
        if (NewWindowRequested is null)
        {
            RequestNewTab();
            toolbar.SetStatus(BrowserStrings.Get("status.newWindowUnavailable"));
            return;
        }

        NewWindowRequested.Invoke(this, new BrowserNewWindowRequestedEventArgs(initialUrl));
    }

    private void RequestNewPrivateWindow(string? initialUrl = null)
    {
        if (NewPrivateWindowRequested is null)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.privateWindowUnavailable"));
            return;
        }

        NewPrivateWindowRequested.Invoke(this, new BrowserNewWindowRequestedEventArgs(initialUrl));
    }

    private void ReopenClosedTab()
    {
        var snapshot = closedTabs.Pop();
        if (snapshot is null)
        {
            SyncActiveTabUi();
            return;
        }

        _ = ReopenClosedTabAsync(snapshot);
    }

    private async Task ReopenClosedTabAsync(BrowserSessionTabSnapshot snapshot)
    {
        var tab = await CreateAndActivateTabAsync(snapshot.Address);
        tab.Title = snapshot.Title;
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void DuplicateActiveTab()
    {
        if (activeTab is null)
        {
            SyncActiveTabUi();
            return;
        }

        var snapshot = new BrowserSessionTabSnapshot(activeTab.DisplayTitle, activeTab.Address);
        _ = DuplicateActiveTabAsync(snapshot);
    }

    private async Task DuplicateActiveTabAsync(BrowserSessionTabSnapshot snapshot)
    {
        var tab = await CreateAndActivateTabAsync(snapshot.Address);
        tab.Title = snapshot.Title;
        toolbar.SetStatus(BrowserStrings.Get("status.tabDuplicated"));
        SyncActiveTabUi();
        SyncSidebar();
    }

    private bool TryHandleTabShortcut(Keys keyData)
    {
        var modifiers = keyData & Keys.Modifiers;
        var keyCode = keyData & Keys.KeyCode;

        if (modifiers == (Keys.Control | Keys.Shift) && keyCode == Keys.Tab)
        {
            ActivatePreviousTab();
            return true;
        }

        if (modifiers != Keys.Control)
        {
            return false;
        }

        if (keyCode == Keys.Tab || keyCode == Keys.PageDown)
        {
            ActivateNextTab();
            return true;
        }

        if (keyCode == Keys.PageUp)
        {
            ActivatePreviousTab();
            return true;
        }

        if (TryResolveTabShortcutIndex(keyCode, out var shortcutIndex))
        {
            ActivateTabShortcutIndex(shortcutIndex);
            return true;
        }

        return false;
    }

    private static bool TryResolveTabShortcutIndex(Keys keyCode, out int shortcutIndex)
    {
        shortcutIndex = keyCode switch
        {
            Keys.D1 or Keys.NumPad1 => 0,
            Keys.D2 or Keys.NumPad2 => 1,
            Keys.D3 or Keys.NumPad3 => 2,
            Keys.D4 or Keys.NumPad4 => 3,
            Keys.D5 or Keys.NumPad5 => 4,
            Keys.D6 or Keys.NumPad6 => 5,
            Keys.D7 or Keys.NumPad7 => 6,
            Keys.D8 or Keys.NumPad8 => 7,
            Keys.D9 or Keys.NumPad9 => 8,
            _ => -1,
        };
        return shortcutIndex >= 0;
    }

    private void ActivateTabShortcutIndex(int shortcutIndex)
    {
        if (tabs.Count == 0)
        {
            return;
        }

        var tabIndex = shortcutIndex == 8 ? tabs.Count - 1 : shortcutIndex;
        ActivateTabAtIndex(tabIndex);
    }

    private void ActivateNextTab()
    {
        if (tabs.Count <= 1)
        {
            return;
        }

        var activeIndex = activeTab is null ? -1 : tabs.IndexOf(activeTab);
        ActivateTabAtIndex(activeIndex < 0 ? 0 : (activeIndex + 1) % tabs.Count);
    }

    private void ActivatePreviousTab()
    {
        if (tabs.Count <= 1)
        {
            return;
        }

        var activeIndex = activeTab is null ? 0 : tabs.IndexOf(activeTab);
        ActivateTabAtIndex(activeIndex <= 0 ? tabs.Count - 1 : activeIndex - 1);
    }

    private void ActivateTabAtIndex(int tabIndex)
    {
        if (tabIndex < 0 || tabIndex >= tabs.Count)
        {
            return;
        }

        ActivateTab(tabs[tabIndex].Id);
    }

    private async Task CloseTabAsync(Guid tabId)
    {
        var tabIndex = tabs.FindIndex(tab => tab.Id == tabId);
        if (tabIndex < 0)
        {
            return;
        }

        var tab = tabs[tabIndex];
        closedTabs.Push(new BrowserSessionTabSnapshot(tab.DisplayTitle, tab.Address));
        var wasActive = activeTab == tab;
        if (wasActive)
        {
            isPageFullScreen = false;
            ApplyFullScreenState();
        }

        tabs.RemoveAt(tabIndex);

        if (wasActive)
        {
            webViewHost.Controls.Remove(tab.WebView);
            activeTab = null;
        }

        // Drop the closing tab (and its favicon image, disposed just below)
        // from the sidebar before the resources go away.
        SyncSidebar();
        tab.Dispose();

        if (tabs.Count == 0)
        {
            await CreateAndActivateTabAsync(null);
            return;
        }

        if (wasActive)
        {
            var nextIndex = Math.Min(tabIndex, tabs.Count - 1);
            ActivateTab(tabs[nextIndex].Id);
        }
        else
        {
            SyncActiveTabUi();
            SyncSidebar();
        }
    }

    private void ActivateTab(Guid tabId)
    {
        var tab = tabs.FirstOrDefault(candidate => candidate.Id == tabId);
        if (tab is null)
        {
            return;
        }

        if (activeTab == tab)
        {
            SyncActiveTabUi();
            SyncFullScreenState(tab);
            SyncSidebar();
            return;
        }

        webViewHost.Controls.Clear();
        webViewHost.Controls.Add(tab.WebView);
        activeTab = tab;
        SyncActiveTabUi();
        SyncFullScreenState(tab);
        SyncSidebar();
    }

    private void NavigateInitialDocument(BrowserTab tab, string? initialAddress)
    {
        if (!string.IsNullOrWhiteSpace(initialAddress)
            && NavigationPolicy.IsHistoryAddress(initialAddress))
        {
            NavigateHistoryDocument(tab);
            return;
        }

        if (!string.IsNullOrWhiteSpace(initialAddress)
            && NavigationPolicy.IsBookmarksAddress(initialAddress))
        {
            NavigateBookmarksDocument(tab);
            return;
        }

        if (!string.IsNullOrWhiteSpace(initialAddress)
            && NavigationPolicy.TryNormalizeAddress(initialAddress, out var normalizedAddress))
        {
            tab.Address = normalizedAddress;
            tab.IsShowingErrorPage = false;
            tab.WebView.CoreWebView2.Navigate(normalizedAddress);
            return;
        }

        NavigateHomeDocument(tab);
    }

    private void NavigateHomeDocument()
    {
        if (activeTab is not null)
        {
            NavigateHomeDocument(activeTab);
        }
    }

    private void NavigateHomeDocument(BrowserTab tab)
    {
        tab.Address = NavigationPolicy.HomeAddress;
        tab.Title = BrowserHomeDocumentPolicy.Title;
        tab.IsShowingErrorPage = false;
        tab.WebView.NavigateToString(BrowserHomeDocumentPolicy.BuildDocument(ActroCore.LoadStatus()));
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void NavigateHistoryDocument()
    {
        if (activeTab is not null)
        {
            NavigateHistoryDocument(activeTab);
        }
    }

    private void NavigateHistoryDocument(BrowserTab tab)
    {
        tab.Address = NavigationPolicy.HistoryAddress;
        tab.Title = BrowserHistoryDocumentPolicy.Title;
        tab.IsShowingErrorPage = false;
        tab.WebView.NavigateToString(BrowserHistoryDocumentPolicy.BuildDocument(historyStore.Load().Items));
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void NavigateBookmarksDocument()
    {
        if (activeTab is not null)
        {
            NavigateBookmarksDocument(activeTab);
        }
    }

    private void NavigateBookmarksDocument(BrowserTab tab)
    {
        bookmarkSnapshot = bookmarkStore.Load();
        tab.Address = NavigationPolicy.BookmarksAddress;
        tab.Title = BrowserBookmarksDocumentPolicy.Title;
        tab.IsShowingErrorPage = false;
        tab.WebView.NavigateToString(BrowserBookmarksDocumentPolicy.BuildDocument(bookmarkSnapshot.Items));
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void NavigateBack()
    {
        if (activeTab?.WebView.CoreWebView2?.CanGoBack == true)
        {
            activeTab.WebView.CoreWebView2.GoBack();
        }
    }

    private void NavigateForward()
    {
        if (activeTab?.WebView.CoreWebView2?.CanGoForward == true)
        {
            activeTab.WebView.CoreWebView2.GoForward();
        }
    }

    private void ReloadOrStopActiveTab()
    {
        if (activeTab?.WebView.CoreWebView2 is null)
        {
            return;
        }

        if (activeTab.IsLoading)
        {
            StopLoadingActiveTab();
            return;
        }

        ReloadActiveTab();
    }

    private void ReloadActiveTab()
    {
        if (activeTab?.WebView.CoreWebView2 is null)
        {
            return;
        }

        if (NavigationPolicy.IsHomeAddress(activeTab.Address))
        {
            NavigateHomeDocument(activeTab);
            return;
        }

        if (NavigationPolicy.IsHistoryAddress(activeTab.Address))
        {
            NavigateHistoryDocument(activeTab);
            return;
        }

        if (NavigationPolicy.IsBookmarksAddress(activeTab.Address))
        {
            NavigateBookmarksDocument(activeTab);
            return;
        }

        if (activeTab.IsShowingErrorPage
            && NavigationPolicy.TryNormalizeAddress(activeTab.Address, out var retryAddress))
        {
            activeTab.Address = retryAddress;
            activeTab.IsShowingErrorPage = false;
            activeTab.WebView.CoreWebView2.Navigate(retryAddress);
            SyncActiveTabUi();
            SyncSidebar();
            return;
        }

        activeTab.WebView.CoreWebView2.Reload();
    }

    private void StopLoadingActiveTab()
    {
        if (activeTab?.WebView.CoreWebView2 is null || !activeTab.IsLoading)
        {
            return;
        }

        activeTab.WebView.CoreWebView2.Stop();
        activeTab.IsLoading = false;
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void OpenDeveloperTools()
    {
        activeTab?.WebView.CoreWebView2?.OpenDevToolsWindow();
    }

    private void OpenDownloadsFolder()
    {
        var downloadsDirectory = BrowserDownloadPolicy.GetDefaultDownloadsDirectory();
        if (string.IsNullOrWhiteSpace(downloadsDirectory))
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenDownloadsFolderFailedStatus);
            return;
        }

        try
        {
            Process.Start(new ProcessStartInfo(downloadsDirectory)
            {
                UseShellExecute = true,
            });
            toolbar.SetStatus(BrowserDownloadPolicy.OpenDownloadsFolderStatus);
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenDownloadsFolderFailedStatus);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenDownloadsFolderFailedStatus);
        }
        catch (Win32Exception)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenDownloadsFolderFailedStatus);
        }
    }

    private void ToggleContentBlocking()
    {
        contentBlockingEnabled = !contentBlockingEnabled;
        appMenu.SetContentBlockingEnabled(contentBlockingEnabled);
        PersistContentBlockingSettings();
        toolbar.SetStatus(BrowserContentBlockingPolicy.FormatToggleStatus(contentBlockingEnabled));
    }

    private void PersistContentBlockingSettings()
    {
        try
        {
            contentBlockingStore.Save(contentBlockingStore.CreateSnapshot(contentBlockingEnabled));
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private async Task SyncBrowserDataNowAsync()
    {
        toolbar.SetStatus(BrowserSyncStatusPolicy.SyncingStatus);
        try
        {
            await browserSyncScheduler.RunNowAsync();
            toolbar.SetStatus(BrowserSyncStatusPolicy.FormatStatusJson(ReadBrowserSyncStatusJson()));
        }
        catch (Exception error)
        {
            toolbar.SetStatus(BrowserSyncStatusPolicy.FailedStatus);
            MessageBox.Show(
                this,
                FormatBrowserSyncError(error),
                BrowserStrings.Get("dialog.sync.title"),
                MessageBoxButtons.OK,
                MessageBoxIcon.Warning);
        }
    }

    private void ConfigureBrowserSyncConnection()
    {
        using var dialog = new BrowserSyncConnectionDialog(browserSyncCredentialStore.Load());
        if (dialog.ShowDialog(this) != DialogResult.OK)
        {
            return;
        }

        if (dialog.ShouldClear)
        {
            browserSyncCredentialStore.ClearSession(dialog.Environment);
            browserSyncScheduler.CancelPendingSync();
            toolbar.SetStatus(BrowserSyncStatusPolicy.FormatStatusJson(ReadBrowserSyncStatusJson()));
            return;
        }

        browserSyncCredentialStore.SaveSession(
            dialog.Environment,
            dialog.AccessToken,
            dialog.BaseUrl);
        ScheduleBrowserDataSyncAfterCredentialChange();
        toolbar.SetStatus(BrowserStrings.Get("status.syncCredentialsUpdated"));
    }

    private void ShowBrowserSyncStatus()
    {
        MessageBox.Show(
            this,
            BrowserSyncStatusPolicy.FormatStatusJson(ReadBrowserSyncStatusJson()),
            BrowserStrings.Get("dialog.sync.title"),
            MessageBoxButtons.OK,
            MessageBoxIcon.Information);
    }

    private void ScheduleBrowserDataSyncAfterCredentialChange()
    {
        try
        {
            browserSyncScheduler.ScheduleAfterLocalChange(TimeSpan.Zero);
        }
        catch (ObjectDisposedException)
        {
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserSyncStatusPolicy.FailedStatus);
        }
    }

    private void ScheduleBrowserDataSyncAfterLocalChange()
    {
        try
        {
            browserSyncScheduler.ScheduleAfterLocalChange();
        }
        catch (ObjectDisposedException)
        {
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserSyncStatusPolicy.FailedStatus);
        }
    }

    private string? ReadBrowserSyncStatusJson()
    {
        try
        {
            return browserSyncScheduler.StatusJson;
        }
        catch (ObjectDisposedException)
        {
            return null;
        }
        catch (InvalidOperationException)
        {
            return null;
        }
        catch (JsonException)
        {
            return null;
        }
    }

    private static string FormatBrowserSyncError(Exception error)
    {
        var message = string.IsNullOrWhiteSpace(error.Message)
            ? error.GetType().Name
            : error.Message;
        return BrowserStrings.Format("dialog.sync.errorFormat", BrowserSyncStatusPolicy.FailedStatus, message);
    }

    private void SetSearchEngine(string engineId)
    {
        try
        {
            searchSettingsSnapshot = searchSettingsStore.CreateSnapshot(engineId);
            searchSettingsStore.Save(searchSettingsSnapshot);
            appMenu.SetSearchEngine(searchSettingsSnapshot.SearchEngineId);
            BrowserSearchEnginePolicy.TryGetEngine(searchSettingsSnapshot.SearchEngineId, out var engine);
            toolbar.SetStatus(BrowserStrings.Format("status.searchEngineChanged", engine.DisplayName));
        }
        catch (IOException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.searchEngineUpdateFailed"));
        }
        catch (UnauthorizedAccessException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.searchEngineUpdateFailed"));
        }
    }

    private void SetRestorePreviousSession(bool restorePreviousSession)
    {
        try
        {
            startupSettingsSnapshot = startupSettingsStore.CreateSnapshot(restorePreviousSession);
            startupSettingsStore.Save(startupSettingsSnapshot);
            appMenu.SetRestorePreviousSession(startupSettingsSnapshot.RestorePreviousSession);
            toolbar.SetStatus(BrowserStrings.Get(
                startupSettingsSnapshot.RestorePreviousSession
                    ? "status.startupRestoresSession"
                    : "status.startupOpensHome"));
        }
        catch (IOException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.startupSettingUpdateFailed"));
        }
        catch (UnauthorizedAccessException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.startupSettingUpdateFailed"));
        }
    }

    private void OpenLastDownload()
    {
        if (!TryGetLastCompletedDownloadPath(out var filePath))
        {
            return;
        }

        try
        {
            Process.Start(new ProcessStartInfo(filePath)
            {
                UseShellExecute = true,
            });
            toolbar.SetStatus(BrowserDownloadPolicy.OpenedLastDownloadStatus);
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenLastDownloadFailedStatus);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenLastDownloadFailedStatus);
        }
        catch (Win32Exception)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.OpenLastDownloadFailedStatus);
        }
    }

    private void RevealLastDownloadInFolder()
    {
        if (!TryGetLastCompletedDownloadPath(out var filePath))
        {
            return;
        }

        try
        {
            Process.Start(new ProcessStartInfo(
                "explorer.exe",
                BrowserDownloadPolicy.BuildExplorerSelectArguments(filePath))
            {
                UseShellExecute = true,
            });
            toolbar.SetStatus(BrowserDownloadPolicy.RevealedLastDownloadStatus);
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.RevealLastDownloadFailedStatus);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.RevealLastDownloadFailedStatus);
        }
        catch (Win32Exception)
        {
            toolbar.SetStatus(BrowserDownloadPolicy.RevealLastDownloadFailedStatus);
        }
    }

    private bool TryGetLastCompletedDownloadPath(out string filePath)
    {
        if (BrowserDownloadPolicy.TryGetAvailableDownloadFile(
            lastCompletedDownloadPath,
            File.Exists,
            out filePath))
        {
            return true;
        }

        toolbar.SetStatus(BrowserDownloadPolicy.FormatUnavailableDownloadStatus(lastCompletedDownloadPath));
        return false;
    }

    private void SetLastCompletedDownload(string filePath)
    {
        lastCompletedDownloadPath = filePath;
        SyncActiveTabUi();
    }

    private void CopyActivePageAddress()
    {
        if (!BrowserPageActionPolicy.TryGetCopyableAddress(activeTab?.Address, out var address))
        {
            toolbar.SetStatus(BrowserPageActionPolicy.NoCopyableAddressStatus);
            return;
        }

        try
        {
            Clipboard.SetText(address);
            toolbar.SetStatus(BrowserPageActionPolicy.CopiedAddressStatus);
        }
        catch (ExternalException)
        {
            toolbar.SetStatus(BrowserPageActionPolicy.NoCopyableAddressStatus);
        }
    }

    private void OpenActivePageInDefaultBrowser()
    {
        if (!BrowserPageActionPolicy.TryGetExternalAddress(activeTab?.Address, out var address))
        {
            toolbar.SetStatus(BrowserPageActionPolicy.NoExternalAddressStatus);
            return;
        }

        try
        {
            Process.Start(new ProcessStartInfo(address)
            {
                UseShellExecute = true,
            });
            toolbar.SetStatus(BrowserPageActionPolicy.OpenedExternalStatus);
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserPageActionPolicy.OpenExternalFailedStatus);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserPageActionPolicy.OpenExternalFailedStatus);
        }
        catch (Win32Exception)
        {
            toolbar.SetStatus(BrowserPageActionPolicy.OpenExternalFailedStatus);
        }
    }

    private void ToggleActivePageBookmark()
    {
        if (activeTab is null
            || !BrowserBookmarkStore.TryNormalizeBookmarkAddress(activeTab.Address, out _))
        {
            toolbar.SetStatus(BrowserStrings.Get("status.pageNotBookmarkable"));
            return;
        }

        try
        {
            var didChangeBookmarks = false;
            if (IsAddressBookmarked(activeTab.Address))
            {
                if (bookmarkStore.RemoveAddress(activeTab.Address))
                {
                    toolbar.SetStatus(BrowserStrings.Get("status.bookmarkRemoved"));
                    didChangeBookmarks = true;
                }
            }
            else
            {
                var title = activeTab.WebView.CoreWebView2?.DocumentTitle;
                if (bookmarkStore.AddOrUpdate(
                        string.IsNullOrWhiteSpace(title) ? activeTab.DisplayTitle : title,
                        activeTab.Address,
                        DateTimeOffset.UtcNow))
                {
                    toolbar.SetStatus(BrowserStrings.Get("status.bookmarkAdded"));
                    didChangeBookmarks = true;
                }
            }

            bookmarkSnapshot = bookmarkStore.Load();
            SyncActiveTabUi();
            if (didChangeBookmarks)
            {
                ScheduleBrowserDataSyncAfterLocalChange();
            }
        }
        catch (IOException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.bookmarkUpdateFailed"));
        }
        catch (UnauthorizedAccessException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.bookmarkUpdateFailed"));
        }
    }

    private void OpenLocalFile()
    {
        if (activeTab is null)
        {
            return;
        }

        using var dialog = new OpenFileDialog
        {
            AddExtension = false,
            CheckFileExists = true,
            CheckPathExists = true,
            Filter = BrowserStrings.Get("dialog.openFile.filter"),
            RestoreDirectory = true,
            Title = BrowserStrings.Get("dialog.openFile.title"),
        };

        if (dialog.ShowDialog(this) != DialogResult.OK)
        {
            return;
        }

        if (!NavigationPolicy.TryNormalizeLocalFilePath(dialog.FileName, out var fileAddress))
        {
            toolbar.SetStatus(BrowserStrings.Get("status.fileOpenRejected"));
            return;
        }

        if (!TryNavigateTabAddress(activeTab, fileAddress, out var errorMessage))
        {
            toolbar.SetStatus(errorMessage ?? BrowserStrings.Get("status.fileOpenRejected"));
        }
    }

    private void PrintActiveTab()
    {
        if (activeTab?.WebView.CoreWebView2 is not CoreWebView2 coreWebView)
        {
            return;
        }

        try
        {
            coreWebView.ShowPrintUI(CoreWebView2PrintDialogKind.System);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.printUnavailable"));
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.printUnavailable"));
        }
    }

    private async Task ClearBrowsingDataAsync()
    {
        if (activeTab?.WebView.CoreWebView2?.Profile is not CoreWebView2Profile profile)
        {
            return;
        }

        var result = MessageBox.Show(
            this,
            BrowserBrowsingDataPolicy.ConfirmationMessage,
            BrowserBrowsingDataPolicy.ConfirmationTitle,
            MessageBoxButtons.OKCancel,
            MessageBoxIcon.Warning,
            MessageBoxDefaultButton.Button2);
        if (result != DialogResult.OK)
        {
            return;
        }

        try
        {
            toolbar.SetStatus(BrowserBrowsingDataPolicy.ClearingStatus);
            await profile.ClearBrowsingDataAsync(
                CoreWebView2BrowsingDataKinds.AllSite
                | CoreWebView2BrowsingDataKinds.DiskCache
                | CoreWebView2BrowsingDataKinds.BrowsingHistory
                | CoreWebView2BrowsingDataKinds.DownloadHistory);
            ClearLocalHistory();
            toolbar.SetStatus(BrowserBrowsingDataPolicy.ClearedStatus);
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserBrowsingDataPolicy.FailedStatus);
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserBrowsingDataPolicy.FailedStatus);
        }
    }

    private async Task SaveActiveTabAsPdfAsync()
    {
        if (activeTab?.WebView.CoreWebView2 is not CoreWebView2 coreWebView)
        {
            return;
        }

        using var dialog = new SaveFileDialog
        {
            AddExtension = true,
            DefaultExt = "pdf",
            FileName = BrowserPdfPolicy.GetSuggestedFileName(activeTab.DisplayTitle, activeTab.Address),
            Filter = BrowserStrings.Get("dialog.savePdf.filter"),
            OverwritePrompt = true,
            RestoreDirectory = true,
            Title = BrowserStrings.Get("dialog.savePdf.title"),
        };

        if (dialog.ShowDialog(this) != DialogResult.OK)
        {
            return;
        }

        try
        {
            toolbar.SetStatus(BrowserStrings.Get("status.savingPdf"));
            var printSettings = webViewEnvironment?.CreatePrintSettings();
            if (printSettings is not null)
            {
                printSettings.ShouldPrintBackgrounds = true;
            }

            var succeeded = await coreWebView.PrintToPdfAsync(dialog.FileName, printSettings);
            toolbar.SetStatus(BrowserPdfPolicy.FormatSaveStatus(succeeded, dialog.FileName));
        }
        catch (InvalidOperationException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.savePdfFailed"));
        }
        catch (ArgumentException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.savePdfFailed"));
        }
        catch (IOException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.savePdfFailed"));
        }
        catch (UnauthorizedAccessException)
        {
            toolbar.SetStatus(BrowserStrings.Get("status.savePdfFailed"));
        }
    }

    private void ZoomInActiveTab()
    {
        ApplyActiveTabZoom(BrowserZoomPolicy.ZoomIn(GetActiveTabZoomFactor()));
    }

    private void ZoomOutActiveTab()
    {
        ApplyActiveTabZoom(BrowserZoomPolicy.ZoomOut(GetActiveTabZoomFactor()));
    }

    private void ResetActiveTabZoom()
    {
        ApplyActiveTabZoom(BrowserZoomPolicy.Reset());
    }

    private double GetActiveTabZoomFactor()
    {
        return activeTab is null
            ? BrowserZoomPolicy.DefaultZoomFactor
            : BrowserZoomPolicy.Normalize(activeTab.WebView.ZoomFactor);
    }

    private void ApplyActiveTabZoom(double zoomFactor)
    {
        if (activeTab is null)
        {
            return;
        }

        var normalizedZoom = BrowserZoomPolicy.Normalize(zoomFactor);
        activeTab.WebView.ZoomFactor = normalizedZoom;
        toolbar.SetStatus(BrowserStrings.Format(
            "status.zoomLevel",
            BrowserZoomPolicy.FormatZoom(normalizedZoom)));
        SyncActiveTabUi();
    }

    private void OpenFindBar()
    {
        if (activeTab is null)
        {
            return;
        }

        findBar.ShowAndFocus();
    }

    private void CloseFindBar()
    {
        findBar.Visible = false;
        activeTab?.WebView.Focus();
    }

    private void SyncFullScreenState(BrowserTab tab)
    {
        if (activeTab != tab)
        {
            return;
        }

        if (tab.WebView.CoreWebView2?.ContainsFullScreenElement == true)
        {
            isPageFullScreen = true;
        }
        else
        {
            isPageFullScreen = false;
        }

        ApplyFullScreenState();
    }

    private void RequestExitFullScreen()
    {
        if (isPageFullScreen && activeTab?.WebView.CoreWebView2 is CoreWebView2 coreWebView)
        {
            _ = TryExitDocumentFullScreenAsync(coreWebView);
            isPageFullScreen = false;
        }
        else if (isBrowserFullScreen)
        {
            isBrowserFullScreen = false;
        }

        ApplyFullScreenState();
    }

    private static async Task TryExitDocumentFullScreenAsync(CoreWebView2 coreWebView)
    {
        try
        {
            await coreWebView.ExecuteScriptAsync(
                "if (document.fullscreenElement) document.exitFullscreen();");
        }
        catch (InvalidOperationException)
        {
        }
        catch (ArgumentException)
        {
        }
    }

    private void ToggleBrowserFullScreen()
    {
        if (activeTab is null)
        {
            return;
        }

        isBrowserFullScreen = !isBrowserFullScreen;
        ApplyFullScreenState();
    }

    private void ApplyFullScreenState()
    {
        if (isBrowserFullScreen || isPageFullScreen)
        {
            EnterFullScreenMode();
        }
        else
        {
            ExitFullScreenMode();
        }
    }

    private void EnterFullScreenMode()
    {
        if (isFullScreen)
        {
            return;
        }

        windowedFormBorderStyle = FormBorderStyle;
        windowedWindowState = WindowState;
        windowedTopMost = TopMost;
        windowedSidebarVisible = !contentSplit.Panel1Collapsed;
        windowedToolbarVisible = toolbar.Visible;
        windowedFindBarVisible = findBar.Visible;

        SuspendLayout();
        contentSplit.Panel1Collapsed = true;
        toolbar.Visible = false;
        findBar.Visible = false;
        FormBorderStyle = FormBorderStyle.None;
        WindowState = FormWindowState.Maximized;
        TopMost = true;
        isFullScreen = true;
        ResumeLayout(performLayout: true);
        activeTab?.WebView.Focus();
    }

    private void ExitFullScreenMode()
    {
        if (!isFullScreen)
        {
            return;
        }

        SuspendLayout();
        FormBorderStyle = windowedFormBorderStyle;
        WindowState = windowedWindowState;
        TopMost = windowedTopMost;
        contentSplit.Panel1Collapsed = !windowedSidebarVisible;
        toolbar.Visible = windowedToolbarVisible;
        findBar.Visible = windowedFindBarVisible;
        isFullScreen = false;
        ResumeLayout(performLayout: true);
        ApplySidebarLayout();

        if (findBar.Visible)
        {
            findBar.ShowAndFocus();
        }
        else
        {
            activeTab?.WebView.Focus();
        }

        SyncActiveTabUi();
    }

    private async Task FindInActiveTabAsync(string query, bool forward)
    {
        if (activeTab?.WebView.CoreWebView2 is null)
        {
            return;
        }

        if (string.IsNullOrWhiteSpace(query))
        {
            toolbar.SetStatus(BrowserStrings.Get("status.find.emptyQuery"));
            return;
        }

        lastFindQuery = query;
        var result = await activeTab.WebView.CoreWebView2.ExecuteScriptAsync(
            BrowserFindPolicy.BuildWindowFindScript(query, forward));
        toolbar.SetStatus(BrowserStrings.Get(
            BrowserFindPolicy.IsFoundResult(result) ? "status.find.matchFound" : "status.find.noMatch"));
    }

    private void RepeatFind(bool forward)
    {
        var query = findBar.Visible ? findBar.Query : lastFindQuery;
        if (string.IsNullOrWhiteSpace(query))
        {
            OpenFindBar();
            return;
        }

        _ = FindInActiveTabAsync(query, forward);
    }

    private void NavigateAddress(string address)
    {
        if (activeTab is null)
        {
            return;
        }

        if (!BrowserAddressInputPolicy.TryResolveAddressInput(
            address,
            searchSettingsSnapshot.SearchEngineId,
            out var resolvedAddress,
            out _))
        {
            toolbar.SetStatus(BrowserStrings.Get("status.addressInvalid"));
            return;
        }

        if (!TryNavigateTabAddress(activeTab, resolvedAddress, out var errorMessage))
        {
            toolbar.SetStatus(errorMessage ?? BrowserStrings.Get("status.addressInvalid"));
            return;
        }
    }

    private bool TryNavigateTabAddress(BrowserTab tab, string address, out string? errorMessage)
    {
        if (tab.WebView.CoreWebView2 is null)
        {
            errorMessage = BrowserStrings.Get("status.tabNotReady");
            return false;
        }

        if (NavigationPolicy.IsHomeAddress(address))
        {
            NavigateHomeDocument(tab);
            errorMessage = null;
            return true;
        }

        if (NavigationPolicy.IsHistoryAddress(address))
        {
            NavigateHistoryDocument(tab);
            errorMessage = null;
            return true;
        }

        if (NavigationPolicy.IsBookmarksAddress(address))
        {
            NavigateBookmarksDocument(tab);
            errorMessage = null;
            return true;
        }

        if (!NavigationPolicy.TryNormalizeAddress(address, out var normalizedAddress))
        {
            errorMessage = BrowserStrings.Get("status.addressNotNormalizable");
            return false;
        }

        tab.Address = normalizedAddress;
        tab.IsShowingErrorPage = false;
        if (activeTab == tab)
        {
            toolbar.SetAddress(normalizedAddress);
        }

        tab.WebView.CoreWebView2.Navigate(normalizedAddress);
        SyncActiveTabUi();
        SyncSidebar();
        errorMessage = null;
        return true;
    }

    private void ShowNavigationError(BrowserTab tab, string errorStatus)
    {
        tab.IsShowingErrorPage = true;
        tab.IsLoading = false;
        tab.Title = BrowserNavigationErrorPolicy.DefaultTitle;
        tab.WebView.NavigateToString(BrowserNavigationErrorPolicy.BuildErrorDocument(tab.Address, errorStatus));
        SyncActiveTabUi();
        SyncSidebar();
    }

    private void UpdateWindowTitle(string documentTitle)
    {
        var productTitle = GetProductWindowTitle();
        Text = string.IsNullOrWhiteSpace(documentTitle)
            || documentTitle == BrowserStrings.Get("window.title.product")
            ? productTitle
            : BrowserStrings.Format("window.title.documentFormat", documentTitle, productTitle);
    }

    private string GetProductWindowTitle()
    {
        return BrowserStrings.Get(
            isPrivateWindow ? "window.title.privateProduct" : "window.title.product");
    }

    private void SyncActiveTabUi()
    {
        if (activeTab is null)
        {
            toolbar.SetAddress(string.Empty);
            toolbar.SetNavigationState(canGoBack: false, canGoForward: false, isLoading: false);
            appMenu.SetNavigationState(
                canGoBack: false,
                canGoForward: false,
                isLoading: false,
                hasActiveTab: false,
                canCopyPageAddress: false,
                canOpenInDefaultBrowser: false,
                canReopenClosedTab: closedTabs.CanReopen,
                canBookmarkPage: false,
                isActivePageBookmarked: false,
                canOpenLastDownload: HasCompletedDownload(),
                canZoomIn: false,
                canZoomOut: false,
                canResetZoom: false);
            UpdateWindowTitle(BrowserStrings.Get("window.title.product"));
            return;
        }

        var canGoBack = activeTab.WebView.CoreWebView2?.CanGoBack == true;
        var canGoForward = activeTab.WebView.CoreWebView2?.CanGoForward == true;
        var zoomFactor = GetActiveTabZoomFactor();
        var canCopyPageAddress = BrowserPageActionPolicy.TryGetCopyableAddress(activeTab.Address, out _);
        var canOpenInDefaultBrowser = BrowserPageActionPolicy.TryGetExternalAddress(activeTab.Address, out _);
        var canBookmarkPage = BrowserBookmarkStore.TryNormalizeBookmarkAddress(activeTab.Address, out _);
        toolbar.SetAddress(activeTab.Address);
        toolbar.SetNavigationState(
            canGoBack,
            canGoForward,
            activeTab.IsLoading);
        appMenu.SetNavigationState(
            canGoBack,
            canGoForward,
            activeTab.IsLoading,
            hasActiveTab: true,
            canCopyPageAddress,
            canOpenInDefaultBrowser,
            canReopenClosedTab: closedTabs.CanReopen,
            canBookmarkPage,
            isActivePageBookmarked: canBookmarkPage && IsAddressBookmarked(activeTab.Address),
            canOpenLastDownload: HasCompletedDownload(),
            canZoomIn: BrowserZoomPolicy.CanZoomIn(zoomFactor),
            canZoomOut: BrowserZoomPolicy.CanZoomOut(zoomFactor),
            canResetZoom: BrowserZoomPolicy.CanReset(zoomFactor));
        UpdateWindowTitle(activeTab.DisplayTitle);
    }

    private bool HasCompletedDownload()
    {
        return !string.IsNullOrWhiteSpace(lastCompletedDownloadPath);
    }

    private bool IsAddressBookmarked(string? address)
    {
        if (!BrowserBookmarkStore.TryNormalizeBookmarkAddress(address, out var normalizedAddress))
        {
            return false;
        }

        return bookmarkSnapshot.Items.Any(item =>
            string.Equals(item.Address, normalizedAddress, StringComparison.OrdinalIgnoreCase));
    }

    private void RecordHistoryVisit(BrowserTab tab)
    {
        if (isPrivateWindow)
        {
            return;
        }

        try
        {
            var title = tab.WebView.CoreWebView2?.DocumentTitle;
            historyStore.AddVisit(
                string.IsNullOrWhiteSpace(title) ? tab.DisplayTitle : title,
                tab.Address,
                DateTimeOffset.UtcNow);
            ScheduleBrowserDataSyncAfterLocalChange();
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private void ClearLocalHistory()
    {
        if (isPrivateWindow)
        {
            return;
        }

        try
        {
            historyStore.Clear();
            ScheduleBrowserDataSyncAfterLocalChange();
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private void SyncSidebar()
    {
        tabSidebar.UpdateTabs(
            tabs.Select(tab => new BrowserTabHeader(tab.Id, tab.DisplayTitle, tab.IsLoading, tab.Favicon)).ToList(),
            activeTab?.Id);
    }

    private HostBridgeActions CreateHostBridgeActions()
    {
        return new HostBridgeActions(
            ActroCore.LoadStatus,
            OpenTabFromHostAsync,
            CloseTabFromHostAsync,
            SelectTabFromHostAsync,
            NavigateFromHostAsync,
            GetTabsForHost,
            GetActiveTabForHost,
            RemoveBookmarkFromHostAsync,
            RemoveHistoryEntryFromHostAsync);
    }

    private async Task<HostOpenTabPayload> OpenTabFromHostAsync(Guid sourceTabId, HostOpenTabRequest request)
    {
        var previousActiveTabId = activeTab?.Id;
        var requestedAddress = FirstNonBlank(request.Url, request.Address, request.AppUrl);
        var initialAddress = NormalizeOptionalHostAddress(requestedAddress);
        var tab = await CreateAndActivateTabAsync(initialAddress);

        if (!string.IsNullOrWhiteSpace(request.Title))
        {
            tab.Title = request.Title.Trim();
            SyncActiveTabUi();
            SyncSidebar();
        }

        if (request.Activate == false
            && previousActiveTabId is Guid previousId
            && previousId != tab.Id)
        {
            ActivateTab(previousId);
        }

        var payload = BuildHostTabPayload(tab);
        return new HostOpenTabPayload(payload.TabId, payload);
    }

    private async Task<bool> CloseTabFromHostAsync(Guid sourceTabId, HostCloseTabRequest request)
    {
        var targetTabId = ResolveOptionalHostTabId(request.TabId, sourceTabId);
        if (!tabs.Any(tab => tab.Id == targetTabId))
        {
            return false;
        }

        if (targetTabId == sourceTabId)
        {
            BeginInvoke(new MethodInvoker(async () => await CloseTabAsync(targetTabId)));
            return true;
        }

        await CloseTabAsync(targetTabId);
        return true;
    }

    private Task<bool> SelectTabFromHostAsync(HostSelectTabRequest request)
    {
        var targetTabId = ResolveRequiredHostTabId(request.TabId);
        if (!tabs.Any(tab => tab.Id == targetTabId))
        {
            return Task.FromResult(false);
        }

        ActivateTab(targetTabId);
        return Task.FromResult(true);
    }

    private Task<HostTabPayload> NavigateFromHostAsync(Guid sourceTabId, HostNavigateRequest request)
    {
        var targetTabId = ResolveOptionalHostTabId(request.TabId, sourceTabId);
        var targetTab = tabs.FirstOrDefault(tab => tab.Id == targetTabId);
        if (targetTab is null)
        {
            throw new HostBridgeException("unknown_tab", "The requested tab does not exist.");
        }

        var address = FirstNonBlank(request.Url, request.Address);
        if (string.IsNullOrWhiteSpace(address))
        {
            throw new HostBridgeException("invalid_url", "Navigate requires a URL or address.");
        }

        if (!TryNavigateTabAddress(targetTab, address, out var errorMessage))
        {
            throw new HostBridgeException("invalid_url", errorMessage ?? "Enter a valid URL.");
        }

        return Task.FromResult(BuildHostTabPayload(targetTab));
    }

    private HostTabsPayload GetTabsForHost()
    {
        return new HostTabsPayload(
            activeTab?.Id.ToString("D"),
            tabs.Select(BuildHostTabPayload).ToList());
    }

    private HostTabPayload? GetActiveTabForHost()
    {
        return activeTab is null ? null : BuildHostTabPayload(activeTab);
    }

    private Task<bool> RemoveBookmarkFromHostAsync(HostRemoveBookmarkRequest request)
    {
        var address = FirstNonBlank(request.Url, request.Address);
        if (string.IsNullOrWhiteSpace(address))
        {
            throw new HostBridgeException("invalid_url", "Remove bookmark requires a URL or address.");
        }

        try
        {
            var removed = bookmarkStore.RemoveAddress(address);
            bookmarkSnapshot = bookmarkStore.Load();
            SyncActiveTabUi();
            if (removed)
            {
                toolbar.SetStatus(BrowserStrings.Get("status.bookmarkRemoved"));
                ScheduleBrowserDataSyncAfterLocalChange();
            }

            return Task.FromResult(removed);
        }
        catch (IOException)
        {
            throw new HostBridgeException("bookmark_update_failed", "Bookmark update failed.");
        }
        catch (UnauthorizedAccessException)
        {
            throw new HostBridgeException("bookmark_update_failed", "Bookmark update failed.");
        }
    }

    private Task<bool> RemoveHistoryEntryFromHostAsync(HostRemoveHistoryEntryRequest request)
    {
        var address = FirstNonBlank(request.Url, request.Address);
        if (string.IsNullOrWhiteSpace(address))
        {
            throw new HostBridgeException("invalid_url", "Remove history entry requires a URL or address.");
        }

        if (isPrivateWindow)
        {
            return Task.FromResult(false);
        }

        try
        {
            var removed = historyStore.RemoveAddress(address);
            if (removed)
            {
                toolbar.SetStatus(BrowserStrings.Get("status.historyEntryRemoved"));
                ScheduleBrowserDataSyncAfterLocalChange();
            }

            return Task.FromResult(removed);
        }
        catch (IOException)
        {
            throw new HostBridgeException("history_update_failed", "History update failed.");
        }
        catch (UnauthorizedAccessException)
        {
            throw new HostBridgeException("history_update_failed", "History update failed.");
        }
    }

    private HostTabPayload BuildHostTabPayload(BrowserTab tab)
    {
        return new HostTabPayload(
            tab.Id.ToString("D"),
            tab.DisplayTitle,
            tab.Address,
            activeTab == tab,
            tab.IsLoading,
            tab.WebView.CoreWebView2?.CanGoBack == true,
            tab.WebView.CoreWebView2?.CanGoForward == true);
    }

    private static string? NormalizeOptionalHostAddress(string? requestedAddress)
    {
        if (string.IsNullOrWhiteSpace(requestedAddress)
            || NavigationPolicy.IsHomeAddress(requestedAddress))
        {
            return null;
        }

        if (NavigationPolicy.IsHistoryAddress(requestedAddress))
        {
            return NavigationPolicy.HistoryAddress;
        }

        if (NavigationPolicy.IsBookmarksAddress(requestedAddress))
        {
            return NavigationPolicy.BookmarksAddress;
        }

        if (NavigationPolicy.TryNormalizeAddress(requestedAddress, out var normalizedAddress))
        {
            return normalizedAddress;
        }

        throw new HostBridgeException("invalid_url", "Open tab requires a browser-safe URL or address.");
    }

    private static string? FirstNonBlank(params string?[] values)
    {
        return values.FirstOrDefault(value => !string.IsNullOrWhiteSpace(value));
    }

    private static Guid ResolveOptionalHostTabId(string? rawTabId, Guid defaultTabId)
    {
        if (string.IsNullOrWhiteSpace(rawTabId))
        {
            return defaultTabId;
        }

        return ResolveRequiredHostTabId(rawTabId);
    }

    private static Guid ResolveRequiredHostTabId(string? rawTabId)
    {
        if (!Guid.TryParse(rawTabId, out var tabId))
        {
            throw new HostBridgeException("invalid_tab_id", "A valid tabId is required.");
        }

        return tabId;
    }

    private void RestoreWindowState()
    {
        if (!shouldRestoreWindowState)
        {
            return;
        }

        var snapshot = windowStateStore.Load();
        if (snapshot is null)
        {
            return;
        }

        StartPosition = FormStartPosition.Manual;
        Bounds = EnsureVisibleWindowBounds(new Rectangle(
            snapshot.X,
            snapshot.Y,
            snapshot.Width,
            snapshot.Height));

        if (snapshot.IsMaximized)
        {
            WindowState = FormWindowState.Maximized;
        }
    }

    private void PersistWindowState()
    {
        try
        {
            var effectiveWindowState = isFullScreen ? windowedWindowState : WindowState;
            var bounds = effectiveWindowState == FormWindowState.Normal ? Bounds : RestoreBounds;
            windowStateStore.Save(windowStateStore.CreateSnapshot(
                bounds.X,
                bounds.Y,
                bounds.Width,
                bounds.Height,
                effectiveWindowState == FormWindowState.Maximized));
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private static Rectangle EnsureVisibleWindowBounds(Rectangle bounds)
    {
        foreach (var screen in Screen.AllScreens)
        {
            if (screen.WorkingArea.IntersectsWith(bounds))
            {
                return bounds;
            }
        }

        var workingArea = Screen.PrimaryScreen?.WorkingArea ?? bounds;
        var width = workingArea.Width > 0 ? Math.Min(bounds.Width, workingArea.Width) : bounds.Width;
        var height = workingArea.Height > 0 ? Math.Min(bounds.Height, workingArea.Height) : bounds.Height;
        return new Rectangle(
            workingArea.Left + Math.Max(0, (workingArea.Width - width) / 2),
            workingArea.Top + Math.Max(0, (workingArea.Height - height) / 2),
            width,
            height);
    }

    private void PersistSession()
    {
        try
        {
            var activeIndex = Math.Max(0, tabs.FindIndex(tab => tab == activeTab));
        var tabSnapshots = tabs
                .Select(tab => new BrowserSessionTabSnapshot(
                    tab.DisplayTitle,
                    tab.Address,
                    tab.Id.ToString("D")))
                .ToList();
            sessionStore.Save(sessionStore.CreateSnapshot(tabSnapshots, activeIndex));
        }
        catch (IOException)
        {
        }
        catch (UnauthorizedAccessException)
        {
        }
    }

    private void ShowStartupError(Exception error)
    {
        Controls.Clear();
        Controls.Add(new Label
        {
            AutoSize = false,
            Dock = DockStyle.Fill,
            Padding = new Padding(24),
            TextAlign = ContentAlignment.MiddleCenter,
            Text = BrowserStrings.Get("error.startupWebView") + Environment.NewLine + error.Message,
        });
    }
}

internal sealed class BrowserNewWindowRequestedEventArgs : EventArgs
{
    public BrowserNewWindowRequestedEventArgs(string? initialUrl)
    {
        InitialUrl = initialUrl;
    }

    public string? InitialUrl { get; }
}
