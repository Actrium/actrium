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

            // Browser data sync status line (BrowserSyncStatusPolicy renders
            // scheduler status JSON through these; result.* keys map the
            // classification tokens the policy appends as "(last result: X)",
            // and unknown tokens render raw so contract growth stays visible)
            ["status.sync.syncing"] = "Syncing browser data...",
            ["status.sync.failed"] = "Browser data sync failed",
            ["status.sync.unavailable"] = "Browser data sync status unavailable",
            ["status.sync.idle"] = "Browser data sync is up to date",
            ["status.sync.pending"] = "Browser data sync is pending",
            ["status.sync.retryScheduledFormat"] = "Browser data sync retry scheduled in {0}s",
            ["status.sync.retryScheduled"] = "Browser data sync retry scheduled",
            ["status.sync.needsSignIn"] = "Browser data sync needs sign-in",
            ["status.sync.notConfigured"] = "Browser data sync is not configured",
            ["status.sync.requiresAppUpdate"] = "Browser data sync requires an app update",
            ["status.sync.needsAttention"] = "Browser data sync needs attention",
            ["status.sync.lastResultFormat"] = "{0} (last result: {1})",
            ["status.sync.result.credentialsUnavailable"] = "credentials unavailable",
            ["status.sync.result.serverUnavailable"] = "server unavailable",
            ["status.sync.result.clientUpgradeRequired"] = "app update required",
            ["status.sync.result.validationError"] = "validation error",

            // Page actions
            ["status.zoomLevel"] = "Zoom {0}",
            ["status.printUnavailable"] = "Print is not available",
            ["status.savingPdf"] = "Saving PDF...",
            ["status.savePdfFailed"] = "Save as PDF failed",
            ["status.fileOpenRejected"] = "Selected file cannot be opened",

            // Browser data sync settings dialog (the product sync surface;
            // the endpoint-token dialog below is the development-only
            // reference path reachable solely from this dialog's advanced
            // link)
            ["dialog.syncSettings.title"] = "Browser Data Sync",
            ["dialog.syncSettings.providerSection"] = "Sync provider",
            ["dialog.syncSettings.providerLocalFolder"] = "Local Folder",
            ["dialog.syncSettings.providerICloudDrive"] = "iCloud Drive",
            ["dialog.syncSettings.providerGoogleDrive"] = "Google Drive",
            ["dialog.syncSettings.providerOneDrive"] = "OneDrive",
            ["dialog.syncSettings.providerComingLater"] = "coming later",
            ["dialog.syncSettings.providerDisabledFormat"] = "{0} ({1})",
            ["dialog.syncSettings.providerFooter"] = "Actro syncs through storage you own. Choose a folder that your cloud drive app already keeps in sync (for example an iCloud Drive or OneDrive folder).",
            ["dialog.syncSettings.folderSection"] = "Sync folder",
            ["dialog.syncSettings.folderRow"] = "Folder",
            ["dialog.syncSettings.folderNotSelected"] = "No folder selected",
            ["dialog.syncSettings.chooseFolder"] = "Choose Folder...",
            ["dialog.syncSettings.chooseFolderDescription"] = "Choose the folder Actro syncs browser data through.",
            ["dialog.syncSettings.disconnect"] = "Disconnect",
            ["dialog.syncSettings.statusSection"] = "Status",
            ["dialog.syncSettings.connectionRow"] = "Connection",
            ["dialog.syncSettings.lastSyncRow"] = "Last sync",
            ["dialog.syncSettings.deviceRow"] = "This device",
            ["dialog.syncSettings.syncNow"] = "Sync Now",
            ["dialog.syncSettings.refreshStatus"] = "Refresh Status",
            ["dialog.syncSettings.showDiagnostics"] = "Show diagnostics",
            ["dialog.syncSettings.hideDiagnostics"] = "Hide diagnostics",
            ["dialog.syncSettings.close"] = "Close",
            ["dialog.syncSettings.referenceServerLink"] = "Reference server (development)...",
            ["dialog.syncSettings.folderConnected"] = "Browser data sync folder connected",
            ["dialog.syncSettings.disconnected"] = "Browser data sync disconnected",
            // Folder acceptance error lines, keyed by the preflight's
            // blocked-reason token (BrowserSyncFolderPreflight).
            ["dialog.syncSettings.folderBlockedDetailFormat"] = "{0} ({1})",
            ["dialog.syncSettings.folderBlocked.folderMissing"] = "The selected folder does not exist.",
            ["dialog.syncSettings.folderBlocked.folderNotWritable"] = "Actro cannot write to the selected folder.",
            ["dialog.syncSettings.folderBlocked.capabilityBlocked"] = "The Local Folder provider is not ready to sync.",
            ["dialog.syncSettings.folderBlocked.adapterBlocked"] = "This folder cannot host the Actro browser sync storage.",
            ["dialog.syncSettings.folderBlocked.compatibilityBlocked"] = "The sync data already in this folder is not compatible with this app.",

            // Connection row states. Classification and user-action keys
            // mirror the Rust provider connection contract tokens; the
            // formatter falls back to the raw token for unknown ones.
            ["status.syncProvider.notConfigured"] = "Not configured",
            ["status.syncProvider.referenceServerActive"] = "Reference server connection active (development)",
            ["status.syncProvider.ready"] = "Connected",
            ["status.syncProvider.providerNotSelected"] = "No sync provider selected",
            ["status.syncProvider.signInRequired"] = "Provider sign-in required",
            ["status.syncProvider.authExpired"] = "Provider sign-in expired",
            ["status.syncProvider.storageSetupRequired"] = "Sync folder setup required",
            ["status.syncProvider.quotaExceeded"] = "Cloud storage is full",
            ["status.syncProvider.providerUnavailable"] = "Provider temporarily unavailable",
            ["status.syncProvider.adapterUnsupported"] = "Provider storage is not supported",
            ["status.syncProvider.membershipRequired"] = "Joining the sync storage on the next sync",
            ["status.syncProvider.recoveryRequired"] = "Cloud sync data needs recovery",
            ["status.syncProvider.withActionFormat"] = "{0} ({1})",
            ["status.syncProvider.action.chooseSyncProvider"] = "choose a sync provider",
            ["status.syncProvider.action.connectProvider"] = "connect the provider",
            ["status.syncProvider.action.reconnectProvider"] = "reconnect the provider",
            ["status.syncProvider.action.chooseProviderFolder"] = "choose a sync folder",
            ["status.syncProvider.action.freeCloudStorageOrReduceRetention"] = "free cloud storage or reduce retention",
            ["status.syncProvider.action.reportProviderIssue"] = "report a provider issue",
            ["status.syncProvider.action.recoverCloudData"] = "recover the cloud sync data",
            ["status.syncProvider.action.reviewSyncStatus"] = "review the sync status",

            // Diagnostics detail (BrowserSyncProviderDiagnosticsFormatter
            // renders the last provider round envelope's Rust diagnostics;
            // labels live here, contract tokens render raw by design)
            ["diagnostics.syncProvider.empty"] = "No provider sync round has run in this session.",
            ["diagnostics.syncProvider.summarySection"] = "Provider diagnostics",
            ["diagnostics.syncProvider.errorSection"] = "Provider errors",
            ["diagnostics.syncProvider.rowFormat"] = "{0}: {1}",
            ["diagnostics.syncProvider.listHeaderFormat"] = "{0}:",
            ["diagnostics.syncProvider.listItemFormat"] = "- {0}",
            ["diagnostics.syncProvider.classificationRow"] = "Classification",
            ["diagnostics.syncProvider.severityRow"] = "Severity",
            ["diagnostics.syncProvider.providerRow"] = "Provider",
            ["diagnostics.syncProvider.syncAllowedRow"] = "Sync allowed",
            ["diagnostics.syncProvider.retryRow"] = "Retry planned",
            ["diagnostics.syncProvider.rereadRow"] = "Cloud reread planned",
            ["diagnostics.syncProvider.userActionRow"] = "User action",
            ["diagnostics.syncProvider.reasonsRow"] = "Reasons",
            ["diagnostics.syncProvider.warningsRow"] = "Warnings",
            ["diagnostics.syncProvider.failedOperationsRow"] = "Failed operations",
            ["diagnostics.syncProvider.failedOperationsFormat"] = "{0} of {1} (retryable {2})",
            ["diagnostics.syncProvider.errorsRow"] = "Errors",
            ["diagnostics.syncProvider.errorKindRow"] = "Kind",
            ["diagnostics.syncProvider.errorActionRow"] = "Action",
            ["diagnostics.syncProvider.errorReasonRow"] = "Reason",
            ["diagnostics.syncProvider.errorMessageRow"] = "Message",
            ["diagnostics.syncProvider.yes"] = "yes",
            ["diagnostics.syncProvider.no"] = "no",

            // Reference server connection dialog (development-only endpoint
            // path; it stays the protocol probe for the reference sync
            // server transport contract)
            ["status.syncCredentialsUpdated"] = "Reference server sync credentials updated (development)",
            ["dialog.sync.title"] = "Reference Server Connection (Development)",
            ["dialog.sync.environment"] = "Environment",
            ["dialog.sync.baseUrl"] = "Base URL",
            ["dialog.sync.accessToken"] = "Access Token",
            ["dialog.sync.save"] = "Save",
            ["dialog.sync.clear"] = "Clear",
            ["dialog.sync.cancel"] = "Cancel",
            ["dialog.sync.accessTokenRequired"] = "Enter an access token before saving reference server sync credentials.",
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
