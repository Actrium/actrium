using System.Globalization;

namespace Actro.Windows;

/// <summary>
/// Single flat table for user-visible chrome strings: window titles, status
/// line messages, dialog labels, toolbar and find bar texts. English only —
/// no resx, no satellite assemblies — but centralizing the literals here is
/// the groundwork for future localization. Strings owned by tested policy
/// classes (Browser*Policy) intentionally stay in those policies, which are
/// already centralized and covered by the harness. Menu labels are added as
/// menu.* keys together with the menu model, not migrated from the doomed
/// classic menu bar. Harness-linked: this file must stay WinForms-free.
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

            // Toolbar
            ["toolbar.back"] = "Back",
            ["toolbar.forward"] = "Forward",
            ["toolbar.reload"] = "Reload",
            ["toolbar.stop"] = "Stop",
            ["toolbar.home"] = "Home",
            ["toolbar.go"] = "Go",
            ["toolbar.status.loading"] = "Loading",

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
