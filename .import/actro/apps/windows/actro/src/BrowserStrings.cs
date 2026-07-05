using System.Globalization;

namespace Actro.Windows;

/// <summary>
/// Single flat table for user-visible chrome strings: window titles, status
/// line messages, dialog labels, toolbar and find bar texts. English only —
/// no resx, no satellite assemblies — but centralizing the literals here is
/// the groundwork for future localization. Strings owned by tested policy
/// classes (Browser*Policy) intentionally stay in those policies, which are
/// already centralized and covered by the harness. Menu labels live as
/// menu.* keys consumed by the compact app menu through the menu model.
/// Harness-linked: this file must stay WinForms-free.
/// </summary>
internal static class BrowserStrings
{
    private static readonly IReadOnlyDictionary<string, string> Table =
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            // Window titles
            ["window.title.product"] = "Actro",
            ["window.title.privateProduct"] = "Actro Private",
            ["window.title.documentFormat"] = "{0} - {1}",

            // Tabs
            ["tab.defaultTitle"] = "New Tab",

            // Vertical tab sidebar
            ["sidebar.newTab"] = "New tab",
            ["sidebar.loadingTitleFormat"] = "{0} ...",

            // Toolbar (glyph buttons carry tooltips, not captions; the
            // reload/stop pair flips with the loading state and the
            // bookmark star with the bookmarked state)
            ["toolbar.tooltip.newTab"] = "New tab (Ctrl+T)",
            ["toolbar.tooltip.toggleSidebar"] = "Show or hide sidebar (Ctrl+Shift+,)",
            ["toolbar.tooltip.back"] = "Go back (Alt+Left)",
            ["toolbar.tooltip.forward"] = "Go forward (Alt+Right)",
            ["toolbar.tooltip.reload"] = "Reload (Ctrl+R)",
            ["toolbar.tooltip.stop"] = "Stop loading (Esc)",
            ["toolbar.tooltip.bookmark"] = "Bookmark this page (Ctrl+D)",
            ["toolbar.tooltip.removeBookmark"] = "Remove bookmark (Ctrl+D)",
            ["toolbar.tooltip.tabOverview"] = "Tab overview (Ctrl+Shift+A)",
            ["toolbar.tooltip.menu"] = "Menu (Alt+E)",

            // Tab overview surface
            ["overview.newTab"] = "New tab",

            // Compact app menu (mnemonics apply inside the opened menu)
            ["menu.newTab"] = "&New Tab",
            ["menu.newWindow"] = "New &Window",
            ["menu.newPrivateWindow"] = "New &Private Window",
            ["menu.closeTab"] = "&Close Tab",
            ["menu.reopenClosedTab"] = "&Reopen Closed Tab",
            ["menu.duplicateTab"] = "&Duplicate Tab",
            ["menu.back"] = "&Back",
            ["menu.forward"] = "&Forward",
            ["menu.home"] = "&Home",
            ["menu.reload"] = "Re&load",
            ["menu.stop"] = "S&top",
            ["menu.zoomIn"] = "Zoom &In",
            ["menu.zoomOut"] = "Zoom &Out",
            ["menu.zoomReset"] = "Reset &Zoom",
            ["menu.fullScreen"] = "F&ull Screen",
            ["menu.toggleSidebar"] = "Show or Hide Side&bar",
            ["menu.toggleTabOverview"] = "Tab O&verview",
            ["menu.find"] = "&Find",
            ["menu.findNext"] = "Find Ne&xt",
            ["menu.findPrevious"] = "Find Pre&vious",
            ["menu.showHistory"] = "H&istory",
            ["menu.bookmarkPage"] = "Book&mark This Page",
            ["menu.showBookmarks"] = "Show Bookmark&s",
            ["menu.page"] = "Pa&ge",
            ["menu.openFile"] = "&Open File",
            ["menu.copyAddress"] = "Copy Page &Address",
            ["menu.openInDefaultBrowser"] = "Open in Default &Browser",
            ["menu.savePdf"] = "&Save as PDF",
            ["menu.print"] = "&Print",
            ["menu.downloads"] = "Do&wnloads",
            ["menu.openDownloadsFolder"] = "Open &Downloads Folder",
            ["menu.openLastDownload"] = "Open &Last Download",
            ["menu.revealLastDownload"] = "Show Last Download in &Folder",
            ["menu.moreTools"] = "More &Tools",
            ["menu.focusAddress"] = "Focus &Address Bar",
            ["menu.devTools"] = "&Developer Tools",
            ["menu.searchEngine"] = "Search &Engine",
            ["menu.restoreSession"] = "Restore Previous Session at Start&up",
            ["menu.blockAds"] = "Bloc&k Ads and Trackers",
            ["menu.sync"] = "Browser Data S&ync",
            ["menu.configureSync"] = "&Configure Browser Data Sync...",
            ["menu.syncNow"] = "Sync &Now",
            ["menu.syncStatus"] = "Show Sync &Status",
            ["menu.clearBrowsingData"] = "Clear Bro&wsing Data",
            ["menu.exit"] = "E&xit",

            // Find bar and find results
            ["find.previous"] = "Previous",
            ["find.next"] = "Next",
            ["find.close"] = "Close",
            ["status.find.emptyQuery"] = "Enter text to find",
            ["status.find.matchFound"] = "Match found",
            ["status.find.noMatch"] = "No match",

            // Navigation and address input
            ["status.navigationBlocked"] = "Blocked unsafe navigation",
            ["status.newWindowBlocked"] = "Blocked unsafe new window",
            ["status.addressInvalid"] = "Enter a valid URL or search query",
            ["status.addressNotNormalizable"] = "Enter a valid URL",
            ["status.tabNotReady"] = "Tab is not ready",

            // Windows and tabs
            ["status.newWindowUnavailable"] = "Opened a new tab because window management is unavailable",
            ["status.privateWindowUnavailable"] = "Private window management is unavailable",
            ["status.tabDuplicated"] = "Duplicated tab",

            // Bookmarks and history
            ["status.pageNotBookmarkable"] = "This page cannot be bookmarked",
            ["status.bookmarkAdded"] = "Page bookmarked",
            ["status.bookmarkRemoved"] = "Bookmark removed",
            ["status.bookmarkUpdateFailed"] = "Bookmark update failed",
            ["status.historyEntryRemoved"] = "History entry removed",

            // Settings
            ["status.searchEngineChanged"] = "Search engine: {0}",
            ["status.searchEngineUpdateFailed"] = "Search engine update failed",
            ["status.startupRestoresSession"] = "Startup restores previous session",
            ["status.startupOpensHome"] = "Startup opens home",
            ["status.startupSettingUpdateFailed"] = "Startup setting update failed",

            // Page actions
            ["status.zoomLevel"] = "Zoom {0}",
            ["status.printUnavailable"] = "Print is not available",
            ["status.savingPdf"] = "Saving PDF...",
            ["status.savePdfFailed"] = "Save as PDF failed",
            ["status.fileOpenRejected"] = "Selected file cannot be opened",

            // Browser data sync
            ["status.syncCredentialsUpdated"] = "Browser data sync credentials updated",
            ["dialog.sync.title"] = "Browser Data Sync",
            ["dialog.sync.environment"] = "Environment",
            ["dialog.sync.baseUrl"] = "Base URL",
            ["dialog.sync.accessToken"] = "Access Token",
            ["dialog.sync.deviceId"] = "Device ID",
            ["dialog.sync.save"] = "Save",
            ["dialog.sync.clear"] = "Clear",
            ["dialog.sync.cancel"] = "Cancel",
            ["dialog.sync.accessTokenRequired"] = "Enter an access token before saving browser data sync credentials.",
            ["dialog.sync.errorFormat"] = "{0}: {1}",

            // File dialogs
            ["dialog.openFile.title"] = "Open File",
            ["dialog.openFile.filter"] = "Web and text files|*.html;*.htm;*.xhtml;*.svg;*.txt;*.md;*.pdf|All files|*.*",
            ["dialog.savePdf.title"] = "Save as PDF",
            ["dialog.savePdf.filter"] = "PDF files|*.pdf|All files|*.*",

            // Startup failure surface
            ["error.startupWebView"] = "Actro Windows failed to start WebView2.",
        };

    /// <summary>Every table entry, exposed for exhaustive harness checks.</summary>
    public static IReadOnlyDictionary<string, string> All => Table;

    /// <summary>
    /// Fail-soft lookup: a missing key returns the key itself, so a typo
    /// degrades to a visible-but-harmless label instead of crashing chrome
    /// code, and the miss stays observable in tests and on screen.
    /// </summary>
    public static string Get(string key)
    {
        return Table.TryGetValue(key, out var value) ? value : key;
    }

    /// <summary>
    /// Formats a parameterized table string ({0}, {1}, ...). Culture is
    /// invariant: arguments arrive pre-formatted by the owning policy.
    /// </summary>
    public static string Format(string key, params object[] args)
    {
        return string.Format(CultureInfo.InvariantCulture, Get(key), args);
    }
}
