using System.Drawing;
using System.Text.Json;
using System.Text.Json.Nodes;
using Microsoft.Data.Sqlite;

using Actro.Windows;

var tests = new (string Name, Func<Task> Run)[]
{
    ("normalizes bare host to https", () =>
    {
        AssertTrue(NavigationPolicy.TryNormalizeAddress("example.com", out var normalized));
        AssertEqual("https://example.com/", normalized);
        return Task.CompletedTask;
    }),
    ("preserves allowed absolute url", () =>
    {
        AssertTrue(NavigationPolicy.TryNormalizeAddress("https://example.com/path?q=1", out var normalized));
        AssertEqual("https://example.com/path?q=1", normalized);
        return Task.CompletedTask;
    }),
    ("allows file and about urls", () =>
    {
        AssertTrue(NavigationPolicy.TryNormalizeAddress("about:blank", out var aboutUrl));
        AssertEqual("about:blank", aboutUrl);

        AssertTrue(NavigationPolicy.TryNormalizeAddress("file:///C:/Actro/index.html", out var fileUrl));
        AssertTrue(fileUrl.StartsWith("file:///", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("normalizes absolute local file paths", () =>
    {
        var localPath = Path.Combine(Path.GetTempPath(), $"actro windows test {Guid.NewGuid():N}.html");

        AssertTrue(NavigationPolicy.TryNormalizeLocalFilePath(localPath, out var fileAddress));
        AssertTrue(fileAddress.StartsWith("file:", StringComparison.Ordinal));
        AssertFalse(fileAddress.Contains(' ', StringComparison.Ordinal));

        AssertTrue(NavigationPolicy.TryNormalizeAddress(localPath, out var normalizedAddress));
        AssertEqual(fileAddress, normalizedAddress);
        AssertFalse(NavigationPolicy.TryNormalizeLocalFilePath("relative-file.html", out _));
        return Task.CompletedTask;
    }),
    ("rejects unsafe or ambiguous urls", () =>
    {
        AssertFalse(NavigationPolicy.TryNormalizeAddress("javascript:alert(1)", out _));
        AssertFalse(NavigationPolicy.TryNormalizeAddress("ftp://example.com", out _));
        AssertFalse(NavigationPolicy.TryNormalizeAddress("not a url", out _));
        AssertFalse(NavigationPolicy.TryNormalizeAddress("   ", out _));
        return Task.CompletedTask;
    }),
    ("allows only browser-safe page navigation addresses", () =>
    {
        AssertTrue(NavigationPolicy.IsAllowedNavigationAddress("https://example.com/path"));
        AssertTrue(NavigationPolicy.IsAllowedNavigationAddress("http://localhost:3000"));
        AssertTrue(NavigationPolicy.IsAllowedNavigationAddress("about:blank"));
        AssertTrue(NavigationPolicy.IsAllowedNavigationAddress("file:///C:/Actro/index.html"));
        AssertFalse(NavigationPolicy.IsAllowedNavigationAddress("javascript:alert(1)"));
        AssertFalse(NavigationPolicy.IsAllowedNavigationAddress("data:text/html,hello"));
        AssertFalse(NavigationPolicy.IsAllowedNavigationAddress("example.com"));
        return Task.CompletedTask;
    }),
    ("recognizes generated document navigation uris", () =>
    {
        // WebView2 NavigateToString surfaces app-generated documents as
        // data:text/html (current runtimes) or about:blank (older ones).
        AssertTrue(NavigationPolicy.IsGeneratedDocumentUri(
            "data:text/html;charset=utf-8;base64,PCFkb2N0eXBlIGh0bWw+"));
        AssertTrue(NavigationPolicy.IsGeneratedDocumentUri("data:text/html,hello"));
        AssertTrue(NavigationPolicy.IsGeneratedDocumentUri("about:blank"));
        AssertTrue(NavigationPolicy.IsGeneratedDocumentUri("DATA:TEXT/HTML,hello"));

        // Everything else stays subject to the address allow-list.
        AssertFalse(NavigationPolicy.IsGeneratedDocumentUri("data:image/png;base64,AAAA"));
        AssertFalse(NavigationPolicy.IsGeneratedDocumentUri("data:text/plain,hello"));
        AssertFalse(NavigationPolicy.IsGeneratedDocumentUri("https://example.com"));
        AssertFalse(NavigationPolicy.IsGeneratedDocumentUri("actro://home"));
        AssertFalse(NavigationPolicy.IsGeneratedDocumentUri(""));
        return Task.CompletedTask;
    }),
    ("classifies external protocol addresses", () =>
    {
        AssertTrue(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("mailto:hello@example.com"));
        AssertTrue(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("tel:+15551234567"));
        AssertTrue(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("ftp://example.com/file.txt"));
        AssertFalse(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("https://example.com"));
        AssertFalse(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("file:///C:/Actro/index.html"));
        AssertFalse(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("javascript:alert(1)"));
        AssertFalse(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("data:text/html,hello"));
        AssertFalse(BrowserExternalProtocolPolicy.IsExternalProtocolAddress("actro://home"));
        return Task.CompletedTask;
    }),
    ("builds external protocol prompt", () =>
    {
        AssertTrue(BrowserExternalProtocolPolicy.TryCreateExternalProtocolPrompt(
            "mailto:hello@example.com",
            out var prompt));

        AssertEqual("Open external app?", prompt.Title);
        AssertEqual("This page wants to open a mailto link in another app.", prompt.Message);
        AssertEqual("mailto", prompt.Scheme);
        AssertEqual("mailto:hello@example.com", prompt.Address);

        AssertFalse(BrowserExternalProtocolPolicy.TryCreateExternalProtocolPrompt(
            "javascript:alert(1)",
            out _));
        return Task.CompletedTask;
    }),
    ("recognizes home address", () =>
    {
        AssertTrue(NavigationPolicy.IsHomeAddress(" actro://HOME "));
        AssertFalse(NavigationPolicy.IsHomeAddress("https://actro.local"));
        return Task.CompletedTask;
    }),
    ("recognizes history address", () =>
    {
        AssertTrue(NavigationPolicy.IsHistoryAddress(" actro://HISTORY "));
        AssertTrue(NavigationPolicy.IsInternalAddress("actro://history"));
        AssertTrue(NavigationPolicy.IsBookmarksAddress(" actro://BOOKMARKS "));
        AssertTrue(NavigationPolicy.IsInternalAddress("actro://bookmarks"));
        AssertFalse(NavigationPolicy.IsInternalAddress("https://actro.local/history"));
        return Task.CompletedTask;
    }),
    ("resolves address bar urls without search", () =>
    {
        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput("example.com/docs", out var resolved, out var isSearch));
        AssertEqual("https://example.com/docs", resolved);
        AssertFalse(isSearch);

        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput(" actro://HOME ", out var homeAddress, out var isHomeSearch));
        AssertEqual(NavigationPolicy.HomeAddress, homeAddress);
        AssertFalse(isHomeSearch);

        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput(" actro://HISTORY ", out var historyAddress, out var isHistorySearch));
        AssertEqual(NavigationPolicy.HistoryAddress, historyAddress);
        AssertFalse(isHistorySearch);

        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput(" actro://BOOKMARKS ", out var bookmarksAddress, out var isBookmarksSearch));
        AssertEqual(NavigationPolicy.BookmarksAddress, bookmarksAddress);
        AssertFalse(isBookmarksSearch);
        return Task.CompletedTask;
    }),
    ("resolves address bar search queries", () =>
    {
        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput("actro browser client", out var resolved, out var isSearch));
        AssertEqual($"{BrowserAddressInputPolicy.DefaultSearchUrlPrefix}actro%20browser%20client", resolved);
        AssertTrue(isSearch);

        AssertTrue(BrowserAddressInputPolicy.TryResolveAddressInput("actro browser client", "bing", out var bingResolved, out var isBingSearch));
        AssertEqual("https://www.bing.com/search?q=actro%20browser%20client", bingResolved);
        AssertTrue(isBingSearch);
        return Task.CompletedTask;
    }),
    ("does not search explicit unsafe address schemes", () =>
    {
        AssertFalse(BrowserAddressInputPolicy.TryResolveAddressInput("javascript:alert(1)", out _, out _));
        AssertFalse(BrowserAddressInputPolicy.TryResolveAddressInput("data:text/html,hello", out _, out _));
        AssertFalse(BrowserAddressInputPolicy.TryResolveAddressInput(" ", out _, out _));
        return Task.CompletedTask;
    }),
    ("formats browser navigation error status", () =>
    {
        AssertEqual("Navigation failed: Timeout", BrowserNavigationErrorPolicy.FormatStatus(" Timeout "));
        AssertEqual("Navigation failed", BrowserNavigationErrorPolicy.FormatStatus(" "));
        return Task.CompletedTask;
    }),
    ("suppresses error documents for superseded navigations", () =>
    {
        // Chrome behavior: aborted or canceled navigations (superseded by
        // a newer one, stopped, or blocked) keep the current document.
        AssertFalse(BrowserNavigationErrorPolicy.ShouldShowErrorDocument("ConnectionAborted"));
        AssertFalse(BrowserNavigationErrorPolicy.ShouldShowErrorDocument("OperationCanceled"));
        AssertFalse(BrowserNavigationErrorPolicy.ShouldShowErrorDocument(" connectionaborted "));

        AssertTrue(BrowserNavigationErrorPolicy.ShouldShowErrorDocument("HostNameNotResolved"));
        AssertTrue(BrowserNavigationErrorPolicy.ShouldShowErrorDocument("Timeout"));
        AssertTrue(BrowserNavigationErrorPolicy.ShouldShowErrorDocument(null));
        AssertTrue(BrowserNavigationErrorPolicy.ShouldShowErrorDocument(""));
        return Task.CompletedTask;
    }),
    ("builds safe browser navigation error document", () =>
    {
        var document = BrowserNavigationErrorPolicy.BuildErrorDocument(
            "https://example.com/?q=<x>",
            "NameNotResolved<script>");

        AssertTrue(document.Contains("https://example.com/?q=&lt;x&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("NameNotResolved&lt;script&gt;", StringComparison.Ordinal));
        AssertFalse(document.Contains("NameNotResolved<script>", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("builds available browser home document", () =>
    {
        var document = BrowserHomeDocumentPolicy.BuildDocument(
            ActroCoreStatus.Available(
                "Actro <Beta>",
                7,
                "ask<script>",
                2,
                3,
                4));

        AssertTrue(document.Contains("<title>Actro &lt;Beta&gt;</title>", StringComparison.Ordinal));
        AssertTrue(document.Contains("<h1>Actro &lt;Beta&gt;</h1>", StringComparison.Ordinal));
        AssertTrue(document.Contains("Rust core ready: C API v7, contract ask&lt;script&gt;@v2, schemas core/runtime 3/4.", StringComparison.Ordinal));
        AssertTrue(document.Contains("window.actroHost.getCoreStatus", StringComparison.Ordinal));
        AssertFalse(document.Contains("Actro <Beta>", StringComparison.Ordinal));
        AssertFalse(document.Contains("ask<script>", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("builds fallback browser home document", () =>
    {
        var document = BrowserHomeDocumentPolicy.BuildDocument(
            ActroCoreStatus.Unavailable("missing <actro_core.dll>"));

        AssertTrue(document.Contains("Rust core DLL is not loaded yet", StringComparison.Ordinal));
        AssertTrue(document.Contains("missing &lt;actro_core.dll&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-core-status", StringComparison.Ordinal));
        AssertFalse(document.Contains("missing <actro_core.dll>", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("formats browser process failures", () =>
    {
        AssertEqual(
            "WebView2 process failed: RenderProcessExited",
            BrowserProcessFailurePolicy.FormatStatus(" RenderProcessExited "));
        AssertEqual(
            "WebView2 process failed: Unknown",
            BrowserProcessFailurePolicy.FormatStatus(" "));
        AssertEqual(
            "WebView2 process failed: RenderProcessExited. Reloading tab...",
            BrowserProcessFailurePolicy.FormatRecoveryStatus("RenderProcessExited"));
        AssertEqual(
            "WebView2 recovery failed: RenderProcessExited",
            BrowserProcessFailurePolicy.FormatRecoveryFailedStatus("RenderProcessExited"));
        return Task.CompletedTask;
    }),
    ("classifies recoverable browser process failures", () =>
    {
        AssertTrue(BrowserProcessFailurePolicy.ShouldReloadTab("RenderProcessExited"));
        AssertTrue(BrowserProcessFailurePolicy.ShouldReloadTab("RenderProcessUnresponsive"));
        AssertTrue(BrowserProcessFailurePolicy.ShouldReloadTab("FrameRenderProcessExited"));
        AssertFalse(BrowserProcessFailurePolicy.ShouldReloadTab("BrowserProcessExited"));
        AssertFalse(BrowserProcessFailurePolicy.ShouldReloadTab(""));
        return Task.CompletedTask;
    }),
    ("blocks common browser ad and tracker resources", () =>
    {
        AssertTrue(BrowserContentBlockingPolicy.ShouldBlock(
            "https://pagead2.googlesyndication.com/pagead/js/adsbygoogle.js",
            "Script"));
        AssertTrue(BrowserContentBlockingPolicy.ShouldBlock(
            "https://cdn.example.com/ads/banner.js",
            "Image"));
        AssertTrue(BrowserContentBlockingPolicy.ShouldBlock(
            "https://metrics.example.com/pixel/collect.gif",
            "Image"));
        AssertFalse(BrowserContentBlockingPolicy.ShouldBlock(
            "https://ads.example.com/",
            "Document"));
        AssertFalse(BrowserContentBlockingPolicy.ShouldBlock(
            "https://notdoubleclick.net/script.js",
            "Script"));
        AssertFalse(BrowserContentBlockingPolicy.ShouldBlock(
            "http://localhost:5173/ads/dev.js",
            "Script"));
        AssertFalse(BrowserContentBlockingPolicy.ShouldBlock(
            "file:///C:/Actro/ads/banner.js",
            "Image"));
        return Task.CompletedTask;
    }),
    ("formats browser content blocking state", () =>
    {
        AssertEqual("Content blocking enabled", BrowserContentBlockingPolicy.FormatToggleStatus(true));
        AssertEqual("Content blocking disabled", BrowserContentBlockingPolicy.FormatToggleStatus(false));
        return Task.CompletedTask;
    }),
    ("persists browser content blocking settings", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserContentBlockingStore(filePath);

        AssertTrue(store.Load().IsEnabled);
        store.Save(store.CreateSnapshot(false));

        var restored = store.Load();
        AssertFalse(restored.IsEnabled);
        AssertEqual(1, restored.Version);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser content blocking settings", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserContentBlockingStore(filePath);

        AssertTrue(store.Load().IsEnabled);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("builds browser search engine urls", () =>
    {
        AssertEqual(BrowserSearchEnginePolicy.DefaultEngineId, BrowserSearchEnginePolicy.NormalizeEngineId("missing"));
        AssertEqual("duckduckgo", BrowserSearchEnginePolicy.NormalizeEngineId(" DuckDuckGo "));
        AssertEqual(
            "https://duckduckgo.com/?q=actro%20browser",
            BrowserSearchEnginePolicy.BuildSearchUrl("actro browser", "duckduckgo"));
        AssertTrue(BrowserSearchEnginePolicy.TryGetEngine("bing", out var engine));
        AssertEqual("Bing", engine.DisplayName);
        return Task.CompletedTask;
    }),
    ("persists browser search settings", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserSearchSettingsStore(filePath);

        AssertEqual(BrowserSearchEnginePolicy.DefaultEngineId, store.Load().SearchEngineId);
        store.Save(store.CreateSnapshot("bing"));

        var restored = store.Load();
        AssertEqual("bing", restored.SearchEngineId);
        AssertEqual(1, restored.Version);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser search settings", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserSearchSettingsStore(filePath);

        AssertEqual(BrowserSearchEnginePolicy.DefaultEngineId, store.Load().SearchEngineId);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("persists browser startup settings", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserStartupSettingsStore(filePath);

        AssertTrue(store.Load().RestorePreviousSession);
        store.Save(store.CreateSnapshot(false));

        var restored = store.Load();
        AssertFalse(restored.RestorePreviousSession);
        AssertEqual(1, restored.Version);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser startup settings", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserStartupSettingsStore(filePath);

        AssertTrue(store.Load().RestorePreviousSession);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("suggests download file name from result path", () =>
    {
        var fileName = BrowserDownloadPolicy.GetSuggestedFileName(
            @"C:\Users\leo\Downloads\report.pdf",
            "https://example.com/fallback.txt");

        AssertEqual("report.pdf", fileName);
        return Task.CompletedTask;
    }),
    ("suggests download file name from uri", () =>
    {
        var fileName = BrowserDownloadPolicy.GetSuggestedFileName(
            null,
            "https://example.com/files/report%20final.pdf?token=1");

        AssertEqual("report final.pdf", fileName);
        return Task.CompletedTask;
    }),
    ("sanitizes Windows download file names", () =>
    {
        AssertEqual("bad_name_.txt", BrowserDownloadPolicy.SanitizeFileName("bad:name?.txt"));
        AssertEqual(BrowserDownloadPolicy.FallbackFileName, BrowserDownloadPolicy.SanitizeFileName("..."));
        AssertEqual(BrowserDownloadPolicy.FallbackFileName, BrowserDownloadPolicy.SanitizeFileName(""));
        return Task.CompletedTask;
    }),
    ("formats download progress", () =>
    {
        AssertEqual("50% (512 B of 1.0 KB)", BrowserDownloadPolicy.FormatProgress(512, 1024));
        AssertEqual("1.5 KB", BrowserDownloadPolicy.FormatProgress(1536, -1));
        return Task.CompletedTask;
    }),
    ("resolves Windows downloads folder", () =>
    {
        var userProfile = Path.Combine("C:", "Users", "leo");
        var downloads = Path.Combine(userProfile, "Downloads");

        AssertEqual(downloads, BrowserDownloadPolicy.ResolveDownloadsDirectory(
            userProfile,
            path => path == downloads));
        AssertEqual(userProfile, BrowserDownloadPolicy.ResolveDownloadsDirectory(
            userProfile,
            _ => false));
        AssertEqual(string.Empty, BrowserDownloadPolicy.ResolveDownloadsDirectory(
            " ",
            _ => true));
        return Task.CompletedTask;
    }),
    ("describes opening downloads folder", () =>
    {
        AssertEqual("Opened Downloads folder", BrowserDownloadPolicy.OpenDownloadsFolderStatus);
        AssertEqual("Open Downloads folder failed", BrowserDownloadPolicy.OpenDownloadsFolderFailedStatus);
        return Task.CompletedTask;
    }),
    ("detects available completed download files", () =>
    {
        var filePath = Path.Combine("C:", "Users", "leo", "Downloads", "report.pdf");

        AssertTrue(BrowserDownloadPolicy.TryGetAvailableDownloadFile(
            filePath,
            path => path == filePath,
            out var availableFilePath));
        AssertEqual(filePath, availableFilePath);
        AssertFalse(BrowserDownloadPolicy.TryGetAvailableDownloadFile(
            filePath,
            _ => false,
            out _));
        AssertFalse(BrowserDownloadPolicy.TryGetAvailableDownloadFile(
            " ",
            _ => true,
            out _));
        return Task.CompletedTask;
    }),
    ("formats completed download actions", () =>
    {
        AssertEqual("No completed download yet", BrowserDownloadPolicy.FormatUnavailableDownloadStatus(null));
        AssertEqual("Last download is not available", BrowserDownloadPolicy.FormatUnavailableDownloadStatus("C:\\Downloads\\missing.pdf"));
        AssertEqual("Opened last download", BrowserDownloadPolicy.OpenedLastDownloadStatus);
        AssertEqual("Open last download failed", BrowserDownloadPolicy.OpenLastDownloadFailedStatus);
        AssertEqual("Shown last download in folder", BrowserDownloadPolicy.RevealedLastDownloadStatus);
        AssertEqual("Show last download in folder failed", BrowserDownloadPolicy.RevealLastDownloadFailedStatus);
        return Task.CompletedTask;
    }),
    ("builds explorer select arguments", () =>
    {
        AssertEqual(
            "/select,\"C:\\Downloads\\report.pdf\"",
            BrowserDownloadPolicy.BuildExplorerSelectArguments("C:\\Downloads\\report.pdf"));
        AssertEqual(
            "/select,\"C:\\Downloads\\badname.pdf\"",
            BrowserDownloadPolicy.BuildExplorerSelectArguments("C:\\Downloads\\bad\"name.pdf"));
        return Task.CompletedTask;
    }),
    ("suggests PDF file names", () =>
    {
        AssertEqual("Example Docs.pdf", BrowserPdfPolicy.GetSuggestedFileName("Example Docs", "https://example.com/docs"));
        AssertEqual("report.pdf", BrowserPdfPolicy.GetSuggestedFileName("report.pdf", "https://example.com/report"));
        AssertEqual("file_name.pdf", BrowserPdfPolicy.GetSuggestedFileName("file:name", null));
        AssertEqual("page.html.pdf", BrowserPdfPolicy.GetSuggestedFileName("New Tab", "https://example.com/page.html"));
        AssertEqual(BrowserPdfPolicy.DefaultPdfFileName, BrowserPdfPolicy.GetSuggestedFileName("New Tab", null));
        return Task.CompletedTask;
    }),
    ("formats PDF save status", () =>
    {
        AssertEqual("Saved page.pdf", BrowserPdfPolicy.FormatSaveStatus(true, Path.Combine("C:", "Actro", "page.pdf")));
        AssertEqual("Save as PDF failed", BrowserPdfPolicy.FormatSaveStatus(false, "page.pdf"));
        return Task.CompletedTask;
    }),
    ("describes browsing data clearing", () =>
    {
        AssertEqual("Clear browsing data?", BrowserBrowsingDataPolicy.ConfirmationTitle);
        AssertTrue(BrowserBrowsingDataPolicy.ConfirmationMessage.Contains("cookies", StringComparison.OrdinalIgnoreCase));
        AssertTrue(BrowserBrowsingDataPolicy.ConfirmationMessage.Contains("cache", StringComparison.OrdinalIgnoreCase));
        AssertEqual("Clearing browsing data...", BrowserBrowsingDataPolicy.ClearingStatus);
        AssertEqual("Browsing data cleared", BrowserBrowsingDataPolicy.ClearedStatus);
        AssertEqual("Clear browsing data failed", BrowserBrowsingDataPolicy.FailedStatus);
        return Task.CompletedTask;
    }),
    ("normalizes browser zoom factors", () =>
    {
        AssertEqual(BrowserZoomPolicy.DefaultZoomFactor, BrowserZoomPolicy.Normalize(double.NaN));
        AssertEqual(BrowserZoomPolicy.MinimumZoomFactor, BrowserZoomPolicy.Normalize(0.01));
        AssertEqual(BrowserZoomPolicy.MaximumZoomFactor, BrowserZoomPolicy.Normalize(9.0));
        AssertEqual(1.23, BrowserZoomPolicy.Normalize(1.234));
        return Task.CompletedTask;
    }),
    ("steps browser zoom factors", () =>
    {
        AssertEqual(1.1, BrowserZoomPolicy.ZoomIn(1.0));
        AssertEqual(0.9, BrowserZoomPolicy.ZoomOut(1.0));
        AssertEqual(BrowserZoomPolicy.MaximumZoomFactor, BrowserZoomPolicy.ZoomIn(5.0));
        AssertEqual(BrowserZoomPolicy.MinimumZoomFactor, BrowserZoomPolicy.ZoomOut(0.25));
        return Task.CompletedTask;
    }),
    ("reports browser zoom command availability", () =>
    {
        AssertTrue(BrowserZoomPolicy.CanZoomIn(1.0));
        AssertTrue(BrowserZoomPolicy.CanZoomOut(1.0));
        AssertFalse(BrowserZoomPolicy.CanZoomIn(BrowserZoomPolicy.MaximumZoomFactor));
        AssertFalse(BrowserZoomPolicy.CanZoomOut(BrowserZoomPolicy.MinimumZoomFactor));
        AssertFalse(BrowserZoomPolicy.CanReset(BrowserZoomPolicy.DefaultZoomFactor));
        AssertTrue(BrowserZoomPolicy.CanReset(1.25));
        return Task.CompletedTask;
    }),
    ("formats browser zoom percentage", () =>
    {
        AssertEqual("100%", BrowserZoomPolicy.FormatZoom(1.0));
        AssertEqual("125%", BrowserZoomPolicy.FormatZoom(1.25));
        return Task.CompletedTask;
    }),
    ("normalizes copyable page addresses", () =>
    {
        AssertTrue(BrowserPageActionPolicy.TryGetCopyableAddress(NavigationPolicy.HomeAddress, out var homeAddress));
        AssertEqual(NavigationPolicy.HomeAddress, homeAddress);

        AssertTrue(BrowserPageActionPolicy.TryGetCopyableAddress(NavigationPolicy.HistoryAddress, out var historyAddress));
        AssertEqual(NavigationPolicy.HistoryAddress, historyAddress);

        AssertTrue(BrowserPageActionPolicy.TryGetCopyableAddress(NavigationPolicy.BookmarksAddress, out var bookmarksAddress));
        AssertEqual(NavigationPolicy.BookmarksAddress, bookmarksAddress);

        AssertTrue(BrowserPageActionPolicy.TryGetCopyableAddress("example.com/docs", out var webAddress));
        AssertEqual("https://example.com/docs", webAddress);

        AssertFalse(BrowserPageActionPolicy.TryGetCopyableAddress("javascript:alert(1)", out _));
        AssertFalse(BrowserPageActionPolicy.TryGetCopyableAddress(" ", out _));
        return Task.CompletedTask;
    }),
    ("allows only shell-safe external page addresses", () =>
    {
        AssertTrue(BrowserPageActionPolicy.TryGetExternalAddress("https://example.com/docs", out var webAddress));
        AssertEqual("https://example.com/docs", webAddress);

        AssertTrue(BrowserPageActionPolicy.TryGetExternalAddress("example.com", out var normalizedWebAddress));
        AssertEqual("https://example.com/", normalizedWebAddress);

        AssertTrue(BrowserPageActionPolicy.TryGetExternalAddress("file:///C:/Actro/index.html", out var fileAddress));
        AssertTrue(fileAddress.StartsWith("file:///", StringComparison.Ordinal));

        AssertFalse(BrowserPageActionPolicy.TryGetExternalAddress(NavigationPolicy.HomeAddress, out _));
        AssertFalse(BrowserPageActionPolicy.TryGetExternalAddress(NavigationPolicy.HistoryAddress, out _));
        AssertFalse(BrowserPageActionPolicy.TryGetExternalAddress(NavigationPolicy.BookmarksAddress, out _));
        AssertFalse(BrowserPageActionPolicy.TryGetExternalAddress("about:blank", out _));
        AssertFalse(BrowserPageActionPolicy.TryGetExternalAddress("javascript:alert(1)", out _));
        return Task.CompletedTask;
    }),
    ("builds browser find script", () =>
    {
        AssertEqual(
            "window.find(\"alpha\", false, false, true, false, true, false);",
            BrowserFindPolicy.BuildWindowFindScript("alpha", forward: true));
        AssertEqual(
            "window.find(\"alpha\", false, true, true, false, true, false);",
            BrowserFindPolicy.BuildWindowFindScript("alpha", forward: false));
        return Task.CompletedTask;
    }),
    ("escapes browser find query script", () =>
    {
        var script = BrowserFindPolicy.BuildWindowFindScript("a\";alert(1)//", forward: true);

        AssertTrue(script.StartsWith("window.find(", StringComparison.Ordinal));
        AssertTrue(script.EndsWith(", false, false, true, false, true, false);", StringComparison.Ordinal));
        AssertFalse(script.Contains("\";alert(1)//", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("parses browser find result", () =>
    {
        AssertTrue(BrowserFindPolicy.IsFoundResult(" true "));
        AssertFalse(BrowserFindPolicy.IsFoundResult("false"));
        return Task.CompletedTask;
    }),
    ("describes browser permissions", () =>
    {
        AssertEqual("microphone", BrowserPermissionPolicy.DescribePermission("Microphone"));
        AssertEqual("file read/write access", BrowserPermissionPolicy.DescribePermission("FileReadWrite"));
        AssertEqual("custom feature", BrowserPermissionPolicy.DescribePermission("CustomFeature"));
        AssertEqual("a browser permission", BrowserPermissionPolicy.DescribePermission(""));
        return Task.CompletedTask;
    }),
    ("describes browser permission origins", () =>
    {
        AssertEqual("https://example.com", BrowserPermissionPolicy.DescribeOrigin("https://example.com/path"));
        AssertEqual("https://example.com:8443", BrowserPermissionPolicy.DescribeOrigin("https://example.com:8443/path"));
        AssertEqual("A local file", BrowserPermissionPolicy.DescribeOrigin("file:///C:/Actro/index.html"));
        AssertEqual("This page", BrowserPermissionPolicy.DescribeOrigin("not a uri"));
        return Task.CompletedTask;
    }),
    ("builds browser permission prompt", () =>
    {
        var prompt = BrowserPermissionPolicy.BuildPrompt("Camera", "https://example.com/app");

        AssertEqual("Allow camera?", prompt.Title);
        AssertEqual("https://example.com wants to use camera.", prompt.Message);
        AssertEqual("camera", prompt.PermissionLabel);
        AssertEqual("https://example.com", prompt.Origin);
        return Task.CompletedTask;
    }),
    ("parses startup url", () =>
    {
        var options = WindowsStartupOptions.FromArgs(["--url", "https://example.com"]);
        AssertEqual("https://example.com", options.InitialUrl);
        AssertEqual(1, options.InitialAddresses.Count);
        AssertEqual("https://example.com", options.InitialAddresses[0]);
        AssertFalse(options.IsPrivate);
        return Task.CompletedTask;
    }),
    ("parses positional startup address", () =>
    {
        var options = WindowsStartupOptions.FromArgs(["https://example.com/from-shell"]);

        AssertEqual("https://example.com/from-shell", options.InitialUrl);
        AssertEqual(1, options.InitialAddresses.Count);
        AssertEqual("https://example.com/from-shell", options.InitialAddresses[0]);
        return Task.CompletedTask;
    }),
    ("parses multiple startup addresses", () =>
    {
        var options = WindowsStartupOptions.FromArgs([
            "--url",
            "https://example.com/one",
            "C:\\Actro\\two.html",
            "--url",
            "https://example.com/three",
        ]);

        AssertEqual(3, options.InitialAddresses.Count);
        AssertEqual("https://example.com/one", options.InitialAddresses[0]);
        AssertEqual("C:\\Actro\\two.html", options.InitialAddresses[1]);
        AssertEqual("https://example.com/three", options.InitialAddresses[2]);
        return Task.CompletedTask;
    }),
    ("parses private startup mode", () =>
    {
        var privateOptions = WindowsStartupOptions.FromArgs(["--private"]);
        var incognitoOptions = WindowsStartupOptions.FromArgs(["--incognito"]);
        var privateWindowOptions = WindowsStartupOptions.FromArgs(["--private-window"]);

        AssertTrue(privateOptions.IsPrivate);
        AssertTrue(incognitoOptions.IsPrivate);
        AssertTrue(privateWindowOptions.IsPrivate);
        AssertEqual(null, privateOptions.InitialUrl);
        return Task.CompletedTask;
    }),
    ("parses private startup url", () =>
    {
        var options = WindowsStartupOptions.FromArgs(["--private", "--url", "https://example.com/private"]);

        AssertTrue(options.IsPrivate);
        AssertEqual("https://example.com/private", options.InitialUrl);
        AssertEqual(1, options.InitialAddresses.Count);
        return Task.CompletedTask;
    }),
    ("ignores incomplete startup url option", () =>
    {
        var options = WindowsStartupOptions.FromArgs(["--url"]);
        AssertEqual(null, options.InitialUrl);
        AssertFalse(options.IsPrivate);
        return Task.CompletedTask;
    }),
    ("builds Windows app data root", () =>
    {
        var root = WindowsAppPaths.GetLocalAppDataRoot();
        AssertTrue(root.EndsWith(Path.Combine("Actrium", "Actro", "Windows"), StringComparison.OrdinalIgnoreCase));
        return Task.CompletedTask;
    }),
    ("builds WebView2 profile folder", () =>
    {
        var userDataFolder = BrowserProfilePolicy.GetDefaultUserDataFolder();
        AssertTrue(userDataFolder.EndsWith(
            Path.Combine("Actrium", "Actro", "Windows", BrowserProfilePolicy.UserDataFolderName),
            StringComparison.OrdinalIgnoreCase));
        return Task.CompletedTask;
    }),
    ("builds private WebView2 profile folder", () =>
    {
        var windowId = Guid.Parse("aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee");
        var userDataFolder = BrowserProfilePolicy.GetPrivateUserDataFolder(windowId);

        AssertTrue(userDataFolder.EndsWith(
            Path.Combine(
                "Actrium",
                "Actro",
                "Windows",
                BrowserProfilePolicy.PrivateUserDataParentFolderName,
                "aaaaaaaabbbbccccddddeeeeeeeeeeee"),
            StringComparison.OrdinalIgnoreCase));
        return Task.CompletedTask;
    }),
    ("ensures WebView2 profile folder", () =>
    {
        var userDataFolder = CreateTempDirectoryPath();
        AssertFalse(Directory.Exists(userDataFolder));
        AssertEqual(userDataFolder, BrowserProfilePolicy.EnsureUserDataFolder(userDataFolder));
        AssertTrue(Directory.Exists(userDataFolder));

        try
        {
            BrowserProfilePolicy.EnsureUserDataFolder(" ");
            throw new InvalidOperationException("Expected empty profile folders to be rejected.");
        }
        catch (ArgumentException)
        {
        }

        Directory.Delete(userDataFolder);
        return Task.CompletedTask;
    }),
    ("deletes WebView2 profile folder best effort", () =>
    {
        var userDataFolder = CreateTempDirectoryPath();
        BrowserProfilePolicy.EnsureUserDataFolder(userDataFolder);
        File.WriteAllText(Path.Combine(userDataFolder, "marker.txt"), "private profile");

        AssertTrue(BrowserProfilePolicy.TryDeleteUserDataFolder(userDataFolder));
        AssertFalse(Directory.Exists(userDataFolder));
        AssertFalse(BrowserProfilePolicy.TryDeleteUserDataFolder(userDataFolder));
        return Task.CompletedTask;
    }),
    ("round trips browser session snapshots", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserSessionStore(filePath);
        var snapshot = new BrowserSessionSnapshot(
            1,
            1,
            [
                new BrowserSessionTabSnapshot("Home", NavigationPolicy.HomeAddress),
                new BrowserSessionTabSnapshot("Docs", "https://example.com/docs", "tab-docs"),
            ]);

        store.Save(snapshot);
        var restored = store.Load();

        AssertEqual(1, restored?.ActiveTabIndex);
        AssertEqual(2, restored?.Tabs.Count);
        AssertEqual(NavigationPolicy.HomeAddress, restored?.Tabs[0].Address);
        AssertEqual("tab-docs", restored?.Tabs[1].Id);
        AssertEqual("https://example.com/docs", restored?.Tabs[1].Address);
        AssertFalse(File.Exists(filePath));
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser session files", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserSessionStore(filePath);

        AssertEqual(null, store.Load());
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("migrates legacy browser session json to sqlite", () =>
    {
        var filePath = CreateTempPath();
        var legacySnapshot = new BrowserSessionSnapshot(
            1,
            1,
            [
                new BrowserSessionTabSnapshot("Home", NavigationPolicy.HomeAddress),
                new BrowserSessionTabSnapshot("Docs", "https://example.com/docs"),
            ]);
        File.WriteAllText(filePath, JsonSerializer.Serialize(legacySnapshot));
        var store = new BrowserSessionStore(filePath);

        var restored = store.Load();

        AssertEqual(1, restored?.ActiveTabIndex);
        AssertEqual("https://example.com/docs", restored?.Tabs[1].Address);
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        store.Save(BrowserSessionSnapshot.Home());
        AssertEqual(NavigationPolicy.HomeAddress, store.Load()?.Tabs[0].Address);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("drops invalid browser session tabs", () =>
    {
        var store = new BrowserSessionStore(CreateTempPath());
        var snapshot = store.CreateSnapshot(
            [
                new BrowserSessionTabSnapshot("Bad", "javascript:alert(1)"),
                new BrowserSessionTabSnapshot("History", NavigationPolicy.HistoryAddress),
                new BrowserSessionTabSnapshot("Bookmarks", NavigationPolicy.BookmarksAddress),
                new BrowserSessionTabSnapshot("", "https://example.com"),
            ],
            5);

        AssertEqual(2, snapshot.ActiveTabIndex);
        AssertEqual(3, snapshot.Tabs.Count);
        AssertEqual("History", snapshot.Tabs[0].Title);
        AssertEqual(NavigationPolicy.HistoryAddress, snapshot.Tabs[0].Address);
        AssertEqual("Bookmarks", snapshot.Tabs[1].Title);
        AssertEqual(NavigationPolicy.BookmarksAddress, snapshot.Tabs[1].Address);
        AssertEqual("New Tab", snapshot.Tabs[2].Title);
        AssertEqual("https://example.com/", snapshot.Tabs[2].Address);
        return Task.CompletedTask;
    }),
    ("adopts rust session snapshot normalization verbatim", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserSessionStore(filePath);
        var longTitle = new string('t', 200);
        var snapshot = store.CreateSnapshot(
            [new BrowserSessionTabSnapshot(longTitle, "https://user:secret@example.com:443/private")],
            0);

        AssertEqual(1, snapshot.Tabs.Count);
        AssertEqual("https://example.com/private", snapshot.Tabs[0].Address);
        AssertEqual(160, snapshot.Tabs[0].Title.Length);
        AssertFalse(string.IsNullOrWhiteSpace(snapshot.Tabs[0].Id));

        var recreated = store.CreateSnapshot(
            [new BrowserSessionTabSnapshot("Other", "https://example.com/private")],
            0);
        AssertEqual(snapshot.Tabs[0].Id, recreated.Tabs[0].Id);

        store.Save(snapshot);
        var restored = store.Load();
        AssertEqual("https://example.com/private", restored?.Tabs[0].Address);
        AssertEqual(snapshot.Tabs[0].Id, restored?.Tabs[0].Id);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("strips credentials from persisted history and bookmark snapshots", () =>
    {
        var historyPath = CreateTempPath();
        var historyStore = new BrowserHistoryStore(historyPath);
        historyStore.Save(new BrowserHistorySnapshot(
            1,
            [new BrowserHistoryItemSnapshot("Private", "https://user:secret@example.com/private", 100, 1)]));
        var restoredHistory = historyStore.Load();
        AssertEqual(1, restoredHistory.Items.Count);
        AssertEqual("https://example.com/private", restoredHistory.Items[0].Address);
        DeleteBrowserStoreFiles(historyPath);

        var bookmarkPath = CreateTempPath();
        var bookmarkStore = new BrowserBookmarkStore(bookmarkPath);
        bookmarkStore.Save(new BrowserBookmarkSnapshot(
            1,
            [new BrowserBookmarkItemSnapshot("Private", "https://user:secret@example.com/private", 100)]));
        var restoredBookmarks = bookmarkStore.Load();
        AssertEqual(1, restoredBookmarks.Items.Count);
        AssertEqual("https://example.com/private", restoredBookmarks.Items[0].Address);
        DeleteBrowserStoreFiles(bookmarkPath);
        return Task.CompletedTask;
    }),
    ("records browser bookmarks", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserBookmarkStore(filePath);
        var firstBookmark = DateTimeOffset.FromUnixTimeSeconds(100);
        var secondBookmark = DateTimeOffset.FromUnixTimeSeconds(200);

        AssertTrue(BrowserBookmarkStore.CanBookmarkAddress("example.com/docs"));
        AssertFalse(BrowserBookmarkStore.CanBookmarkAddress(NavigationPolicy.HomeAddress));
        AssertTrue(store.AddOrUpdate("Example Docs", "example.com/docs", firstBookmark));
        AssertTrue(store.AddOrUpdate("Example Docs Updated", "https://example.com/docs", secondBookmark));
        AssertFalse(store.AddOrUpdate("Home", NavigationPolicy.HomeAddress, secondBookmark));
        AssertFalse(store.AddOrUpdate("Unsafe", "javascript:alert(1)", secondBookmark));

        var restored = store.Load();
        AssertEqual(1, restored.Items.Count);
        AssertEqual("Example Docs Updated", restored.Items[0].Title);
        AssertEqual("https://example.com/docs", restored.Items[0].Address);
        AssertEqual(100, restored.Items[0].CreatedAtUnixSeconds);
        AssertTrue(store.ContainsAddress("example.com/docs"));
        AssertTrue(store.RemoveAddress("https://example.com/docs"));
        AssertFalse(store.ContainsAddress("example.com/docs"));
        AssertEqual(0, store.Load().Items.Count);
        AssertFalse(File.Exists(filePath));
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser bookmark files", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserBookmarkStore(filePath);

        AssertEqual(0, store.Load().Items.Count);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("migrates legacy browser bookmark json to sqlite", () =>
    {
        var filePath = CreateTempPath();
        var legacySnapshot = new BrowserBookmarkSnapshot(
            1,
            [new BrowserBookmarkItemSnapshot("Docs", "https://example.com/docs", 100)]);
        File.WriteAllText(filePath, JsonSerializer.Serialize(legacySnapshot));
        var store = new BrowserBookmarkStore(filePath);

        var restored = store.Load();

        AssertEqual(1, restored.Items.Count);
        AssertEqual("https://example.com/docs", restored.Items[0].Address);
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        store.RemoveAddress("https://example.com/docs");
        AssertEqual(0, store.Load().Items.Count);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("normalizes browser bookmark snapshots", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserBookmarkStore(filePath);
        var snapshot = store.CreateSnapshot(
            [
                new BrowserBookmarkItemSnapshot("Future", "https://future.example", long.MaxValue),
                new BrowserBookmarkItemSnapshot("Past", "https://past.example", long.MinValue),
            ]);

        AssertEqual(2, snapshot.Items.Count);
        AssertEqual(253402300799, snapshot.Items[0].CreatedAtUnixSeconds);
        AssertEqual(0, snapshot.Items[1].CreatedAtUnixSeconds);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("builds safe browser bookmarks document", () =>
    {
        var document = BrowserBookmarksDocumentPolicy.BuildDocument(
            [
                new BrowserBookmarkItemSnapshot(
                    "Example <Docs>",
                    "https://example.com/?q=<x>",
                    100),
            ]);

        AssertTrue(document.Contains("Example &lt;Docs&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("https://example.com/?q=&lt;x&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-list-search", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-filter=\"Example &lt;Docs&gt; https://example.com/?q=&lt;x&gt;\"", StringComparison.Ordinal));
        AssertTrue(document.Contains("No matching bookmarks.", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-remove-bookmark=\"https://example.com/?q=&lt;x&gt;\"", StringComparison.Ordinal));
        AssertTrue(document.Contains("removeHostMethod = \"removeBookmark\"", StringComparison.Ordinal));
        AssertFalse(document.Contains("Example <Docs>", StringComparison.Ordinal));
        AssertFalse(document.Contains("q=<x>", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("records browser history visits", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserHistoryStore(filePath);
        var firstVisit = DateTimeOffset.FromUnixTimeSeconds(100);
        var secondVisit = DateTimeOffset.FromUnixTimeSeconds(200);

        store.AddVisit("Example Docs", "example.com/docs", firstVisit);
        store.AddVisit("Example Docs Updated", "https://example.com/docs", secondVisit);
        store.AddVisit("Home", NavigationPolicy.HomeAddress, secondVisit);
        store.AddVisit("Unsafe", "javascript:alert(1)", secondVisit);

        var restored = store.Load();
        AssertEqual(1, restored.Items.Count);
        AssertEqual("Example Docs Updated", restored.Items[0].Title);
        AssertEqual("https://example.com/docs", restored.Items[0].Address);
        AssertEqual(200, restored.Items[0].VisitedAtUnixSeconds);
        AssertEqual(2, restored.Items[0].VisitCount);
        AssertTrue(store.RemoveAddress("example.com/docs"));
        AssertEqual(0, store.Load().Items.Count);
        AssertFalse(File.Exists(filePath));
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser history files", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserHistoryStore(filePath);

        AssertEqual(0, store.Load().Items.Count);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("migrates legacy browser history json to sqlite", () =>
    {
        var filePath = CreateTempPath();
        var legacySnapshot = new BrowserHistorySnapshot(
            1,
            [new BrowserHistoryItemSnapshot("Docs", "https://example.com/docs", 100, 3)]);
        File.WriteAllText(filePath, JsonSerializer.Serialize(legacySnapshot));
        var store = new BrowserHistoryStore(filePath);

        var restored = store.Load();

        AssertEqual(1, restored.Items.Count);
        AssertEqual("https://example.com/docs", restored.Items[0].Address);
        AssertEqual(3, restored.Items[0].VisitCount);
        AssertTrue(File.Exists(BrowserStorageDatabase.GetDatabasePathForLegacyFile(filePath)));
        store.Clear();
        AssertEqual(0, store.Load().Items.Count);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("normalizes browser history snapshots", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserHistoryStore(filePath);
        var snapshot = store.CreateSnapshot(
            [
                new BrowserHistoryItemSnapshot("Future", "https://future.example", long.MaxValue, 0),
                new BrowserHistoryItemSnapshot("Past", "https://past.example", long.MinValue, -1),
            ]);

        AssertEqual(2, snapshot.Items.Count);
        AssertEqual(253402300799, snapshot.Items[0].VisitedAtUnixSeconds);
        AssertEqual(1, snapshot.Items[0].VisitCount);
        AssertEqual(0, snapshot.Items[1].VisitedAtUnixSeconds);
        AssertEqual(1, snapshot.Items[1].VisitCount);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("clears browser history", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserHistoryStore(filePath);

        store.AddVisit("Example", "https://example.com", DateTimeOffset.FromUnixTimeSeconds(100));
        AssertEqual(1, store.Load().Items.Count);
        store.Clear();
        AssertEqual(0, store.Load().Items.Count);
        DeleteBrowserStoreFiles(filePath);
        return Task.CompletedTask;
    }),
    ("migrates browser storage sqlite v1 v2 v3 to v6", () =>
    {
        for (var legacyVersion = 1; legacyVersion <= 3; legacyVersion++)
        {
            var databasePath = CreateTempDatabasePath();
            try
            {
                CreateLegacyBrowserStorageDatabase(databasePath, legacyVersion);
                var database = new BrowserStorageDatabase(databasePath);

                var history = database.LoadHistoryItems(10);
                AssertEqual(1, history.Count);
                AssertEqual($"https://example.com/history-v{legacyVersion}", history[0].Address);
                AssertEqual(2, history[0].VisitCount);

                var bookmarks = database.LoadBookmarkItems(10);
                AssertEqual(1, bookmarks.Count);
                AssertEqual($"https://example.com/bookmark-v{legacyVersion}", bookmarks[0].Address);

                var session = database.LoadSessionSnapshot(BrowserStorageDatabase.DefaultSessionId);
                AssertEqual(1, session?.Tabs.Count);
                AssertEqual($"https://example.com/session-v{legacyVersion}", session?.Tabs[0].Address);

                AssertBrowserStorageV6Shape(databasePath, legacyVersion);
                using (var batch = JsonDocument.Parse(database.ExportDirtySyncBatch("profile", "device-win", 1234, 20)))
                {
                    AssertEqual("profile", batch.RootElement.GetProperty("scope").GetString());
                    AssertEqual("device-win", batch.RootElement.GetProperty("deviceId").GetString());
                    AssertTrue(BrowserSyncBatchHasRecord(batch, "historyPage", $"https://example.com/history-v{legacyVersion}"));
                    AssertTrue(BrowserSyncBatchHasRecord(batch, "historyVisit", $"visit-v{legacyVersion}"));
                    AssertTrue(BrowserSyncBatchHasRecord(batch, "bookmark", $"bookmark-v{legacyVersion}"));
                    AssertTrue(BrowserSyncBatchHasBookmarkOwner(batch, $"bookmark-v{legacyVersion}", "user", string.Empty));
                    AssertTrue(BrowserSyncBatchHasRecord(batch, "session", BrowserStorageDatabase.DefaultSessionId));
                    AssertTrue(BrowserSyncBatchHasRecord(batch, "sessionTab"));
                }

                using (var pushRequest = JsonDocument.Parse(database.ExportDirtySyncPushRequest("profile", "device-win", 1235, 20)))
                {
                    AssertEqual("push", pushRequest.RootElement.GetProperty("requestType").GetString());
                    AssertTrue(pushRequest.RootElement.TryGetProperty("pushBatch", out _));
                }

                AssertTrue(database.ApplySyncAckBatch($$"""
                    {
                        "version": 1,
                        "storageSchemaVersion": 6,
                        "scope": "profile",
                        "deviceId": "device-win",
                        "syncedAtUnixSeconds": 1240,
                        "records": [
                            {
                                "recordType": "historyPage",
                                "recordId": "https://example.com/history-v{{legacyVersion}}",
                                "syncRevision": 999
                            }
                        ],
                        "cursors": []
                    }
                    """));
                AssertEqual(
                    "synced",
                    QueryScalarString(
                        databasePath,
                        $"SELECT sync_state FROM browser_history_pages WHERE address = 'https://example.com/history-v{legacyVersion}'"));
                AssertEqual(
                    "999",
                    QueryScalarString(
                        databasePath,
                        $"SELECT sync_revision FROM browser_history_pages WHERE address = 'https://example.com/history-v{legacyVersion}'"));

                var remoteBatchJson = ActroCore.BuildBrowserStorageSyncBatchFromDirtyRowsJson(
                    "profile",
                    "device-win",
                    1250,
                    $$"""
                    [
                        {
                            "record_type": "historyPage",
                            "record_id": "https://example.com/remote-v{{legacyVersion}}",
                            "sync_revision": {{1200 + legacyVersion}},
                            "sync_state": "remote",
                            "origin_device_id": "device-remote",
                            "updated_at": 1250,
                            "address": "https://example.com/remote-v{{legacyVersion}}",
                            "title": "Remote {{legacyVersion}}",
                            "visit_count": 3,
                            "last_visited_at": 1250,
                            "created_at": 1240
                        }
                    ]
                    """);
                var cycleResponseJson = ActroCore.BuildBrowserStorageSyncCycleResponseJson(
                    remoteBatchJson,
                    """
                    {
                        "version": 1,
                        "storageSchemaVersion": 6,
                        "scope": "profile",
                        "deviceId": "device-win",
                        "syncedAtUnixSeconds": 1251,
                        "records": [],
                        "cursors": []
                    }
                    """);
                using (var cycleResult = JsonDocument.Parse(database.ApplySyncCycleResponse(cycleResponseJson)))
                {
                    AssertEqual("applyRemoteThenAck", cycleResult.RootElement.GetProperty("action").GetString());
                    AssertTrue(cycleResult.RootElement.GetProperty("ackApplied").GetBoolean());
                    AssertFalse(cycleResult.RootElement.GetProperty("cursorAckDeferred").GetBoolean());
                }
                AssertEqual(
                    $"Remote {legacyVersion}",
                    QueryScalarString(
                        databasePath,
                        $"SELECT title FROM browser_history_pages WHERE address = 'https://example.com/remote-v{legacyVersion}'"));
                AssertEqual(
                    $"{1200 + legacyVersion}",
                    QueryScalarString(
                        databasePath,
                        "SELECT cursor FROM browser_sync_cursors WHERE scope = 'profile' AND collection = 'historyPages'"));

                using (var requestPlan = JsonDocument.Parse(database.ExportSyncCycleRequestPlan(
                    "http://127.0.0.1:8787/",
                    "token-win-123",
                    "profile",
                    "device-win",
                    1260,
                    20)))
                {
                    var root = requestPlan.RootElement;
                    AssertEqual("POST", root.GetProperty("method").GetString());
                    AssertEqual("/v1/browser-storage/sync/cycle", root.GetProperty("path").GetString());
                    AssertEqual(
                        "http://127.0.0.1:8787/v1/browser-storage/sync/cycle",
                        root.GetProperty("url").GetString());
                    AssertTrue(root.GetProperty("headers").EnumerateArray().Any(header =>
                        string.Equals(header.GetProperty("name").GetString(), "Authorization", StringComparison.Ordinal)
                        && string.Equals(header.GetProperty("value").GetString(), "Bearer token-win-123", StringComparison.Ordinal)));

                    using var requestBody = JsonDocument.Parse(root.GetProperty("bodyJson").GetString() ?? "{}");
                    AssertEqual("push", requestBody.RootElement.GetProperty("pushRequest").GetProperty("requestType").GetString());
                    AssertTrue(requestBody.RootElement.GetProperty("cursors").EnumerateArray().Any(cursor =>
                        string.Equals(cursor.GetProperty("collection").GetString(), "historyPages", StringComparison.Ordinal)
                        && string.Equals(cursor.GetProperty("cursor").GetString(), $"{1200 + legacyVersion}", StringComparison.Ordinal)));
                }

                AssertTrue(database.DeleteBookmarkByAddress($"https://example.com/bookmark-v{legacyVersion}"));
                AssertEqual(
                    "1",
                    QueryScalarString(
                        databasePath,
                        $"SELECT is_deleted FROM browser_bookmarks WHERE id = 'bookmark-v{legacyVersion}'"));
                AssertFalse(string.IsNullOrWhiteSpace(QueryScalarString(
                    databasePath,
                    $"SELECT deleted_at FROM browser_bookmarks WHERE id = 'bookmark-v{legacyVersion}'")));

                using var tombstoneBatch = JsonDocument.Parse(database.ExportDirtySyncBatch("profile", "device-win", 1236, 20));
                AssertTrue(BrowserSyncBatchHasDeletedBookmark(tombstoneBatch, $"bookmark-v{legacyVersion}"));
            }
            finally
            {
                DeleteDatabaseFiles(databasePath);
            }
        }

        return Task.CompletedTask;
    }),
    ("runs browser storage sync client cycle", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            database.UpsertHistoryItem(new BrowserHistoryItemSnapshot(
                "Local",
                "https://example.com/local",
                1300,
                1));
            database.InsertHistoryVisit(new BrowserHistoryItemSnapshot(
                "Local",
                "https://example.com/local",
                1300,
                1));

            var remoteBatchJson = ActroCore.BuildBrowserStorageSyncBatchFromDirtyRowsJson(
                "profile",
                "device-win",
                1310,
                """
                [
                    {
                        "record_type": "historyPage",
                        "record_id": "https://example.com/sync-client-remote",
                        "sync_revision": 2001,
                        "sync_state": "remote",
                        "origin_device_id": "device-remote",
                        "updated_at": 1310,
                        "address": "https://example.com/sync-client-remote",
                        "title": "Remote From Client",
                        "visit_count": 2,
                        "last_visited_at": 1310,
                        "created_at": 1309
                    }
                ]
                """);
            var cycleResponseJson = ActroCore.BuildBrowserStorageSyncCycleResponseJson(
                remoteBatchJson,
                """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-win",
                    "syncedAtUnixSeconds": 1311,
                    "records": [],
                    "cursors": []
                }
                """);
            var transport = new FakeBrowserStorageSyncTransport(cycleResponseJson);
            var client = new BrowserStorageSyncClient(database, transport);

            using var result = JsonDocument.Parse(await client.RunSyncCycleAsync(
                "http://127.0.0.1:8787/",
                "token-win-client",
                "profile",
                "device-win",
                1312,
                20));

            AssertTrue(result.RootElement.GetProperty("applied").GetBoolean());
            AssertEqual("success", result.RootElement.GetProperty("classification").GetString());
            AssertEqual("POST", transport.LastRequest?.Method);
            AssertEqual(
                "http://127.0.0.1:8787/v1/browser-storage/sync/cycle",
                transport.LastRequest?.Url.ToString());
            AssertEqual("Bearer token-win-client", transport.LastRequest?.Headers["Authorization"]);

            using var requestBody = JsonDocument.Parse(transport.LastRequest?.BodyJson ?? "{}");
            AssertEqual("push", requestBody.RootElement.GetProperty("pushRequest").GetProperty("requestType").GetString());
            AssertEqual(
                "Remote From Client",
                QueryScalarString(
                    databasePath,
                    "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/sync-client-remote'"));
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("plans browser sync cloud provider adapter object space through Rust", () =>
    {
        using (var contract = JsonDocument.Parse(ActroCore.LoadBrowserStorageSyncCloudProviderAdapterContractJson()))
        {
            AssertEqual(
                "cloudProviderAdapterPlan",
                contract.RootElement.GetProperty("planType").GetString());
            AssertEqual(
                "Actro/BrowserSync",
                contract.RootElement.GetProperty("objectSpace").GetProperty("rootPath").GetString());
            AssertTrue(contract.RootElement.GetProperty("operationBoundary").GetString()!.Contains(
                "coarse rounds",
                StringComparison.Ordinal));
        }

        var localFolderCapabilities = ActroCore.LoadBrowserStorageSyncCloudLocalFolderCapabilitiesJson();
        using (var ready = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderAdapterPlanJson(
            localFolderCapabilities,
            1400)))
        {
            AssertEqual("cloudProviderAdapterPlan", ready.RootElement.GetProperty("planType").GetString());
            AssertEqual("ready", ready.RootElement.GetProperty("classification").GetString());
            AssertEqual("localFolder", ready.RootElement.GetProperty("adapterFamily").GetString());
            AssertTrue(ready.RootElement.GetProperty("productionReady").GetBoolean());
            AssertTrue(ready.RootElement.GetProperty("shouldAllowSync").GetBoolean());
            AssertFalse(ready.RootElement.GetProperty("shouldBlockSync").GetBoolean());
            AssertEqual(
                "Actro/BrowserSync/manifest.json",
                ready.RootElement.GetProperty("objectLayout").GetProperty("manifestPath").GetString());
        }

        using (var setup = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderAdapterPlanJson(
            """
            {
                "providerKind": "localFolder",
                "rootPath": "Wrong/BrowserSync",
                "revisionKind": "contentHash",
                "supportsPrefixList": true,
                "supportsPagedList": true,
                "supportsConditionalWrite": true,
                "supportsConditionalDelete": true,
                "supportsReadIfChanged": true,
                "supportsAtomicRename": false,
                "supportsAppFolder": true,
                "backgroundSync": "platformScheduled",
                "conflictResolution": "optimisticRevision",
                "maxObjectBytes": 8388608,
                "maxPageSize": 1000
            }
            """,
            1401)))
        {
            AssertEqual("providerSetupRequired", setup.RootElement.GetProperty("classification").GetString());
            AssertTrue(setup.RootElement.GetProperty("shouldBlockSync").GetBoolean());
            AssertEqual("configureActroBrowserSyncRoot", setup.RootElement.GetProperty("rootPathAction").GetString());
        }

        return Task.CompletedTask;
    }),
    ("classifies browser sync cloud provider connection state through Rust", () =>
    {
        using (var contract = JsonDocument.Parse(ActroCore.LoadBrowserStorageSyncCloudProviderConnectionContractJson()))
        {
            AssertEqual(
                "cloudProviderConnectionStatusPolicy",
                contract.RootElement.GetProperty("planType").GetString());
            AssertTrue(contract.RootElement.GetProperty("productConstraint").GetString()!.Contains(
                "No Actro product server",
                StringComparison.Ordinal));
        }

        using (var ready = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            """
            {
                "providerKind": "localFolder",
                "scope": "profile",
                "deviceId": "device-win",
                "connectionState": "connected",
                "credentialState": "available",
                "membershipStatus": "active",
                "cloudStoreReady": true
            }
            """,
            0,
            1700)))
        {
            AssertEqual("ready", ready.RootElement.GetProperty("classification").GetString());
            AssertTrue(ready.RootElement.GetProperty("shouldAttemptSync").GetBoolean());
            AssertFalse(ready.RootElement.GetProperty("shouldAttemptMembership").GetBoolean());
            AssertEqual("idle", ready.RootElement.GetProperty("schedulerAction").GetString());
        }

        using (var missingMembership = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            """
            {
                "providerKind": "googleDrive",
                "providerAccountId": "user@example.com",
                "scope": "profile",
                "deviceId": "device-win",
                "connectionState": "connected",
                "credentialState": "available",
                "membershipStatus": "missing",
                "cloudStoreReady": true
            }
            """,
            0,
            1701)))
        {
            AssertEqual("membershipRequired", missingMembership.RootElement.GetProperty("classification").GetString());
            AssertTrue(missingMembership.RootElement.GetProperty("shouldAttemptMembership").GetBoolean());
            AssertFalse(missingMembership.RootElement.GetProperty("shouldAttemptSync").GetBoolean());
            AssertEqual("runCloudMembership", missingMembership.RootElement.GetProperty("schedulerAction").GetString());
            AssertEqual("writeCloudMembership", missingMembership.RootElement.GetProperty("providerAction").GetString());
        }

        using (var retry = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderConnectionStatusPolicyJson(
            """
            {
                "providerKind": "oneDrive",
                "providerAccountId": "onedrive-user",
                "connectionState": "connecting",
                "credentialState": "available",
                "membershipStatus": "active",
                "cloudStoreReady": true,
                "lastProviderErrorKind": "network"
            }
            """,
            1,
            1702)))
        {
            AssertEqual("providerUnavailable", retry.RootElement.GetProperty("classification").GetString());
            AssertTrue(retry.RootElement.GetProperty("shouldRetry").GetBoolean());
            AssertEqual(10, retry.RootElement.GetProperty("retryDelaySeconds").GetInt32());
            AssertEqual("scheduleRetry", retry.RootElement.GetProperty("schedulerAction").GetString());
        }

        return Task.CompletedTask;
    }),
    ("classifies browser sync cloud provider errors through Rust", () =>
    {
        using (var policy = JsonDocument.Parse(ActroCore.LoadBrowserStorageSyncCloudProviderErrorPolicyJson()))
        {
            AssertEqual(
                "cloudProviderErrorClassification",
                policy.RootElement.GetProperty("planType").GetString());
            AssertTrue(policy.RootElement.GetProperty("policyBoundary").GetString()!.Contains(
                "coarse native/provider rounds",
                StringComparison.Ordinal));
        }

        using (var reread = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderErrorClassificationJson(
            """
            {
                "providerKind": "googleDrive",
                "errors": [
                    {
                        "operationIndex": 1,
                        "operationType": "writeManifest",
                        "path": "Actro/BrowserSync/manifest.json",
                        "providerStatusCode": 412,
                        "message": "etag mismatch"
                    }
                ]
            }
            """,
            0,
            1703)))
        {
            AssertEqual("cloudStateRereadRequired", reread.RootElement.GetProperty("classification").GetString());
            AssertTrue(reread.RootElement.GetProperty("shouldRereadCloud").GetBoolean());
            AssertEqual("runCloudReread", reread.RootElement.GetProperty("schedulerAction").GetString());
            AssertEqual(
                "conflict",
                reread.RootElement.GetProperty("errors")[0].GetProperty("canonicalErrorKind").GetString());

            using var diagnostics = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderDiagnosticsJson(
                string.Empty,
                string.Empty,
                reread.RootElement.GetRawText(),
                string.Empty,
                string.Empty,
                1705));
            AssertEqual("cloudConflict", diagnostics.RootElement.GetProperty("classification").GetString());
            AssertEqual(
                "cloudStateRereadRequired",
                diagnostics.RootElement.GetProperty("providerErrorClassification").GetString());
            AssertTrue(diagnostics.RootElement.GetProperty("pendingCloudReread").GetBoolean());
        }

        using (var retry = JsonDocument.Parse(ActroCore.BuildBrowserStorageSyncCloudProviderErrorClassificationJson(
            """
            {
                "providerKind": "oneDrive",
                "operationResults": [
                    {
                        "operationIndex": 2,
                        "operationType": "listLogs",
                        "success": false,
                        "providerStatusCode": 503
                    }
                ]
            }
            """,
            1,
            1704)))
        {
            AssertEqual("providerUnavailable", retry.RootElement.GetProperty("classification").GetString());
            AssertTrue(retry.RootElement.GetProperty("shouldRetry").GetBoolean());
            AssertEqual(10, retry.RootElement.GetProperty("retryDelaySeconds").GetInt32());
            AssertEqual("retryProviderOperation", retry.RootElement.GetProperty("providerAction").GetString());
        }

        return Task.CompletedTask;
    }),
    ("runs browser storage local-folder cloud sync client cycle", async () =>
    {
        var remoteDatabasePath = CreateTempDatabasePath();
        var localDatabasePath = CreateTempDatabasePath();
        var cloudRoot = CreateTempDirectoryPath();
        try
        {
            var remoteDatabase = new BrowserStorageDatabase(remoteDatabasePath);
            remoteDatabase.UpsertHistoryItem(new BrowserHistoryItemSnapshot(
                "Remote Local Folder",
                "https://example.com/local-folder-remote",
                1700,
                1));
            remoteDatabase.InsertHistoryVisit(new BrowserHistoryItemSnapshot(
                "Remote Local Folder",
                "https://example.com/local-folder-remote",
                1700,
                1));

            var remoteClient = new BrowserStorageSyncClient(remoteDatabase);
            using (var seedResult = JsonDocument.Parse(await remoteClient.RunLocalFolderSyncCycleAsync(
                cloudRoot,
                "profile",
                "device-remote",
                1710,
                20,
                observedAtUnixSeconds: 1710)))
            {
                AssertEqual("cloudLocalFolderCycle", seedResult.RootElement.GetProperty("endpointType").GetString());
                AssertEqual("localFolder", seedResult.RootElement.GetProperty("providerKind").GetString());
                AssertEqual(
                    "cloudStoreCapabilityPreflight",
                    seedResult.RootElement.GetProperty("capabilityPreflight").GetProperty("planType").GetString());
                AssertEqual(
                    "ready",
                    seedResult.RootElement.GetProperty("capabilityPreflight").GetProperty("classification").GetString());
                AssertEqual(
                    "localfolder",
                    seedResult.RootElement.GetProperty("capabilityPreflight").GetProperty("capabilities").GetProperty("providerKind").GetString());
                AssertEqual(
                    "contenthash",
                    seedResult.RootElement.GetProperty("capabilityPreflight").GetProperty("capabilities").GetProperty("revisionKind").GetString());
                AssertTrue(seedResult.RootElement.GetProperty("capabilityPreflight").GetProperty("shouldAllowSync").GetBoolean());
                AssertEqual(
                    "cloudCompatibilityPreflight",
                    seedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("planType").GetString());
                AssertEqual(
                    "compatible",
                    seedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("classification").GetString());
                AssertEqual(0, seedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("documentCount").GetInt32());
                AssertEqual(JsonValueKind.Null, seedResult.RootElement.GetProperty("schemaRolloutPlan").ValueKind);
                AssertEqual(JsonValueKind.Null, seedResult.RootElement.GetProperty("schemaMigrationPlaybook").ValueKind);
                AssertEqual(JsonValueKind.Null, seedResult.RootElement.GetProperty("recoverySnapshotRound").ValueKind);
                AssertEqual("success", seedResult.RootElement.GetProperty("classification").GetString());
                AssertTrue(seedResult.RootElement.GetProperty("applyResult").GetProperty("ackApplied").GetBoolean());
                AssertEqual(
                    "compactionRound",
                    seedResult.RootElement.GetProperty("compactionRound").GetProperty("planType").GetString());
                AssertTrue(seedResult.RootElement.GetProperty("compactionRound").GetProperty("completed").GetBoolean());
                AssertEqual(
                    "nothingToCompact",
                    seedResult.RootElement.GetProperty("compactionRound").GetProperty("clientAction").GetString());
                AssertEqual(
                    "cleanupRound",
                    seedResult.RootElement.GetProperty("cleanupRound").GetProperty("planType").GetString());
                AssertTrue(seedResult.RootElement.GetProperty("cleanupRound").GetProperty("cleanupCompleted").GetBoolean());
                AssertEqual(
                    "nothingToCleanup",
                    seedResult.RootElement.GetProperty("cleanupRound").GetProperty("clientAction").GetString());
                AssertEqual(
                    "applyLocalAck",
                    seedResult.RootElement.GetProperty("attemptPlan").GetProperty("attemptAction").GetString());
                AssertEqual(
                    "cloudCycleScheduleState",
                    seedResult.RootElement.GetProperty("scheduleState").GetProperty("planType").GetString());
                AssertEqual(
                    "cloudCycleLoopPlan",
                    seedResult.RootElement.GetProperty("loopPlan").GetProperty("planType").GetString());
                AssertEqual(1, seedResult.RootElement.GetProperty("loopPlan").GetProperty("roundCount").GetInt32());
                AssertEqual(
                    "applyLocalAck",
                    seedResult.RootElement.GetProperty("loopPlan").GetProperty("finalAction").GetString());
                AssertEqual(
                    "cloudProviderDiagnostics",
                    seedResult.RootElement.GetProperty("providerDiagnostics").GetProperty("planType").GetString());
                AssertEqual(
                    "success",
                    seedResult.RootElement.GetProperty("providerDiagnostics").GetProperty("cycleClassification").GetString());
                AssertEqual(
                    "compatible",
                    seedResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
                AssertEqual(
                    "cloudObservabilityEvent",
                    seedResult.RootElement.GetProperty("observabilityEvent").GetProperty("planType").GetString());
                AssertEqual(
                    "success",
                    seedResult.RootElement.GetProperty("observabilityEvent").GetProperty("classification").GetString());
                AssertTrue(seedResult.RootElement.GetProperty("observabilityEvent").GetProperty("redactedExportAllowed").GetBoolean());
            }

            AssertTrue(File.Exists(Path.Combine(cloudRoot, "Actro", "BrowserSync", "manifest.json")));

            var localDatabase = new BrowserStorageDatabase(localDatabasePath);
            var localClient = new BrowserStorageSyncClient(localDatabase);
            using var pullResult = JsonDocument.Parse(await localClient.RunLocalFolderSyncCycleAsync(
                cloudRoot,
                "profile",
                "device-win",
                1720,
                20,
                observedAtUnixSeconds: 1720));

            AssertEqual("cloudLocalFolderCycle", pullResult.RootElement.GetProperty("endpointType").GetString());
            AssertEqual(
                "ready",
                pullResult.RootElement.GetProperty("capabilityPreflight").GetProperty("classification").GetString());
            AssertEqual(
                "cloudCompatibilityPreflight",
                pullResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("planType").GetString());
            AssertEqual(
                "compatible",
                pullResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("classification").GetString());
            AssertTrue(pullResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("documentCount").GetInt32() > 0);
            AssertEqual(JsonValueKind.Null, pullResult.RootElement.GetProperty("schemaRolloutPlan").ValueKind);
            AssertEqual(JsonValueKind.Null, pullResult.RootElement.GetProperty("schemaMigrationPlaybook").ValueKind);
            AssertEqual(JsonValueKind.Null, pullResult.RootElement.GetProperty("recoverySnapshotRound").ValueKind);
            AssertEqual("success", pullResult.RootElement.GetProperty("classification").GetString());
            AssertTrue(pullResult.RootElement.GetProperty("applied").GetBoolean());
            AssertTrue(pullResult.RootElement.GetProperty("applyResult").GetProperty("remoteApplied").GetBoolean());
            AssertEqual(
                "compactionRound",
                pullResult.RootElement.GetProperty("compactionRound").GetProperty("planType").GetString());
            AssertTrue(pullResult.RootElement.GetProperty("compactionRound").GetProperty("completed").GetBoolean());
            AssertEqual(
                "nothingToCompact",
                pullResult.RootElement.GetProperty("compactionRound").GetProperty("clientAction").GetString());
            AssertEqual(
                "cleanupRound",
                pullResult.RootElement.GetProperty("cleanupRound").GetProperty("planType").GetString());
            AssertTrue(pullResult.RootElement.GetProperty("cleanupRound").GetProperty("cleanupCompleted").GetBoolean());
            AssertEqual(
                "nothingToCleanup",
                pullResult.RootElement.GetProperty("cleanupRound").GetProperty("clientAction").GetString());
            AssertEqual(
                "applyRemoteBatch",
                pullResult.RootElement.GetProperty("attemptPlan").GetProperty("attemptAction").GetString());
            AssertEqual(
                "cloudCycleScheduleState",
                pullResult.RootElement.GetProperty("scheduleState").GetProperty("planType").GetString());
            AssertEqual(
                "cloudCycleLoopPlan",
                pullResult.RootElement.GetProperty("loopPlan").GetProperty("planType").GetString());
            AssertEqual(
                "applyRemoteBatch",
                pullResult.RootElement.GetProperty("loopPlan").GetProperty("finalAction").GetString());
            AssertEqual(
                "cloudProviderDiagnostics",
                pullResult.RootElement.GetProperty("providerDiagnostics").GetProperty("planType").GetString());
            AssertEqual(
                "success",
                pullResult.RootElement.GetProperty("providerDiagnostics").GetProperty("cycleClassification").GetString());
            AssertEqual(
                "compatible",
                pullResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
            AssertEqual(
                "cloudObservabilityEvent",
                pullResult.RootElement.GetProperty("observabilityEvent").GetProperty("planType").GetString());
            AssertEqual(
                "success",
                pullResult.RootElement.GetProperty("observabilityEvent").GetProperty("classification").GetString());
            AssertTrue(pullResult.RootElement.GetProperty("observabilityEvent").GetProperty("redactedExportAllowed").GetBoolean());
            AssertEqual(
                "Remote Local Folder",
                QueryScalarString(
                    localDatabasePath,
                    "SELECT title FROM browser_history_pages WHERE address = 'https://example.com/local-folder-remote'"));
            AssertEqual(
                "1",
                QueryScalarString(
                    localDatabasePath,
                    "SELECT COUNT(*) FROM browser_history_visits WHERE address = 'https://example.com/local-folder-remote'"));

            var manifestPath = Path.Combine(cloudRoot, "Actro", "BrowserSync", "manifest.json");
            var manifest = JsonNode.Parse(File.ReadAllText(manifestPath))!.AsObject();
            manifest["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion + 1;
            File.WriteAllText(manifestPath, manifest.ToJsonString());

            using var blockedResult = JsonDocument.Parse(await localClient.RunLocalFolderSyncCycleAsync(
                cloudRoot,
                "profile",
                "device-win",
                1730,
                20,
                observedAtUnixSeconds: 1730));

            AssertEqual("clientUpgradeRequired", blockedResult.RootElement.GetProperty("classification").GetString());
            AssertFalse(blockedResult.RootElement.GetProperty("applied").GetBoolean());
            AssertEqual(JsonValueKind.Null, blockedResult.RootElement.GetProperty("cycleRound").ValueKind);
            AssertEqual(JsonValueKind.Null, blockedResult.RootElement.GetProperty("compactionRound").ValueKind);
            AssertEqual(JsonValueKind.Null, blockedResult.RootElement.GetProperty("cleanupRound").ValueKind);
            AssertEqual(
                "cloudCompatibilityPreflight",
                blockedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("classification").GetString());
            AssertTrue(blockedResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("shouldBlockSync").GetBoolean());
            AssertEqual(
                "cloudSchemaRolloutPlan",
                blockedResult.RootElement.GetProperty("schemaRolloutPlan").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedResult.RootElement.GetProperty("schemaRolloutPlan").GetProperty("classification").GetString());
            AssertTrue(blockedResult.RootElement.GetProperty("schemaRolloutPlan").GetProperty("shouldBlockSync").GetBoolean());
            AssertEqual(
                "cloudSchemaMigrationPlaybook",
                blockedResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("classification").GetString());
            AssertTrue(blockedResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("requiresClientUpgrade").GetBoolean());
            AssertFalse(blockedResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("requiresProviderBackup").GetBoolean());
            AssertEqual(
                "recoverySnapshotRound",
                blockedResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("planType").GetString());
            AssertTrue(blockedResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("completed").GetBoolean());
            AssertEqual(
                "upgradeClient",
                blockedResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("clientAction").GetString());
            AssertEqual(
                "cloudRecoverySnapshotResult",
                blockedResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("resultPlan").GetProperty("planType").GetString());
            AssertFalse(blockedResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("resultPlan").GetProperty("snapshotRequested").GetBoolean());
            AssertEqual(
                "clientUpgradeRequired",
                blockedResult.RootElement.GetProperty("providerDiagnostics").GetProperty("classification").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
        }
        finally
        {
            DeleteDatabaseFiles(remoteDatabasePath);
            DeleteDatabaseFiles(localDatabasePath);
            if (Directory.Exists(cloudRoot))
            {
                Directory.Delete(cloudRoot, recursive: true);
            }
        }
    }),
    ("runs browser storage local-folder cloud membership client round", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        var cloudRoot = CreateTempDirectoryPath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            var client = new BrowserStorageSyncClient(database);
            var joinRequestJson = JsonSerializer.Serialize(new Dictionary<string, object>
            {
                ["scope"] = "profile",
                ["actorDeviceId"] = "device-win",
                ["memberDeviceId"] = "device-win",
                ["displayName"] = "Actro Windows",
                ["platform"] = "Windows",
                ["membershipAction"] = "connect",
                ["generatedAtUnixSeconds"] = 1810,
            });

            using (var joinResult = JsonDocument.Parse(await client.RunLocalFolderMembershipRoundAsync(
                cloudRoot,
                joinRequestJson,
                1810)))
            {
                AssertEqual("cloudLocalFolderMembership", joinResult.RootElement.GetProperty("endpointType").GetString());
                AssertEqual("localFolder", joinResult.RootElement.GetProperty("providerKind").GetString());
                AssertEqual(
                    "ready",
                    joinResult.RootElement.GetProperty("capabilityPreflight").GetProperty("classification").GetString());
                AssertEqual(
                    "compatible",
                    joinResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("classification").GetString());
                AssertEqual(0, joinResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("documentCount").GetInt32());
                AssertEqual(JsonValueKind.Null, joinResult.RootElement.GetProperty("schemaRolloutPlan").ValueKind);
                AssertEqual(JsonValueKind.Null, joinResult.RootElement.GetProperty("schemaMigrationPlaybook").ValueKind);
                AssertEqual(JsonValueKind.Null, joinResult.RootElement.GetProperty("recoverySnapshotRound").ValueKind);
                AssertEqual(
                    "cloudMembershipStatusPolicy",
                    joinResult.RootElement.GetProperty("statusPolicy").GetProperty("planType").GetString());
                AssertEqual(
                    "success",
                    joinResult.RootElement.GetProperty("statusPolicy").GetProperty("classification").GetString());
                AssertEqual(
                    "finishCloudDeviceJoined",
                    joinResult.RootElement.GetProperty("statusPolicy").GetProperty("clientAction").GetString());
                AssertEqual(
                    "cloudMembershipScheduleState",
                    joinResult.RootElement.GetProperty("scheduleState").GetProperty("planType").GetString());
                AssertEqual(
                    "success",
                    joinResult.RootElement.GetProperty("scheduleState").GetProperty("classification").GetString());
                AssertEqual(
                    "idle",
                    joinResult.RootElement.GetProperty("scheduleState").GetProperty("schedulerAction").GetString());
                AssertEqual(0, joinResult.RootElement.GetProperty("scheduleState").GetProperty("nextRetryAttempt").GetInt32());
                AssertEqual(
                    "cloudMembershipAttemptPlan",
                    joinResult.RootElement.GetProperty("attemptPlan").GetProperty("planType").GetString());
                AssertEqual(
                    "success",
                    joinResult.RootElement.GetProperty("attemptPlan").GetProperty("classification").GetString());
                AssertEqual(
                    "finishCloudDeviceJoined",
                    joinResult.RootElement.GetProperty("attemptPlan").GetProperty("attemptAction").GetString());
                AssertTrue(joinResult.RootElement.GetProperty("attemptPlan").GetProperty("shouldFinishMembership").GetBoolean());
                AssertEqual(
                    "cloudMembershipLoopPlan",
                    joinResult.RootElement.GetProperty("loopPlan").GetProperty("planType").GetString());
                AssertEqual(1, joinResult.RootElement.GetProperty("loopPlan").GetProperty("roundCount").GetInt32());
                AssertEqual(
                    "finishCloudDeviceJoined",
                    joinResult.RootElement.GetProperty("loopPlan").GetProperty("finalAction").GetString());
                AssertEqual(1, joinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("attemptCount").GetInt32());
                AssertEqual(1, joinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("completedAttemptCount").GetInt32());
                AssertEqual(1, joinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finishMembershipAttemptCount").GetInt32());
                AssertEqual(
                    "finishCloudDeviceJoined",
                    joinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finalAction").GetString());
                AssertEqual(
                    "cloudProviderDiagnostics",
                    joinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("planType").GetString());
                AssertEqual(
                    "compatible",
                    joinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
                AssertEqual(
                    "success",
                    joinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("scheduleClassification").GetString());
                AssertEqual(
                    "cloudObservabilityEvent",
                    joinResult.RootElement.GetProperty("observabilityEvent").GetProperty("planType").GetString());
                AssertEqual(
                    "finishCloudDeviceJoined",
                    joinResult.RootElement.GetProperty("observabilityEvent").GetProperty("finalAction").GetString());
                AssertEqual("success", joinResult.RootElement.GetProperty("classification").GetString());
                AssertTrue(joinResult.RootElement.GetProperty("completed").GetBoolean());
                AssertFalse(joinResult.RootElement.GetProperty("shouldRetry").GetBoolean());
                AssertFalse(joinResult.RootElement.GetProperty("shouldReread").GetBoolean());
                AssertEqual("finishCloudDeviceJoined", joinResult.RootElement.GetProperty("clientAction").GetString());
                AssertEqual(
                    "membershipRound",
                    joinResult.RootElement.GetProperty("membershipRound").GetProperty("planType").GetString());
                AssertEqual(
                    "join",
                    joinResult.RootElement.GetProperty("membershipRound").GetProperty("membershipAction").GetString());
            }

            AssertTrue(File.Exists(Path.Combine(cloudRoot, "Actro", "BrowserSync", "manifest.json")));
            AssertTrue(File.Exists(Path.Combine(cloudRoot, "Actro", "BrowserSync", "devices", "device-win.json")));

            var revokeRequestJson = JsonSerializer.Serialize(new Dictionary<string, object>
            {
                ["scope"] = "profile",
                ["actorDeviceId"] = "device-win",
                ["memberDeviceId"] = "device-win",
                ["membershipAction"] = "revoke",
                ["generatedAtUnixSeconds"] = 1830,
            });

            using var revokeResult = JsonDocument.Parse(await client.RunLocalFolderMembershipRoundAsync(
                cloudRoot,
                revokeRequestJson,
                1830));

            AssertEqual("cloudLocalFolderMembership", revokeResult.RootElement.GetProperty("endpointType").GetString());
            AssertEqual("success", revokeResult.RootElement.GetProperty("classification").GetString());
            AssertEqual(
                "compatible",
                revokeResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("classification").GetString());
            AssertTrue(revokeResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("documentCount").GetInt32() > 0);
            AssertEqual(
                "compatible",
                revokeResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
            AssertEqual(
                "cloudObservabilityEvent",
                revokeResult.RootElement.GetProperty("observabilityEvent").GetProperty("planType").GetString());
            AssertEqual(JsonValueKind.Null, revokeResult.RootElement.GetProperty("schemaRolloutPlan").ValueKind);
            AssertEqual(JsonValueKind.Null, revokeResult.RootElement.GetProperty("schemaMigrationPlaybook").ValueKind);
            AssertEqual(JsonValueKind.Null, revokeResult.RootElement.GetProperty("recoverySnapshotRound").ValueKind);
            AssertEqual(
                "cloudMembershipStatusPolicy",
                revokeResult.RootElement.GetProperty("statusPolicy").GetProperty("planType").GetString());
            AssertEqual(
                "success",
                revokeResult.RootElement.GetProperty("statusPolicy").GetProperty("classification").GetString());
            AssertEqual(
                "finishCloudDeviceRevoked",
                revokeResult.RootElement.GetProperty("statusPolicy").GetProperty("clientAction").GetString());
            AssertEqual(
                "cloudMembershipScheduleState",
                revokeResult.RootElement.GetProperty("scheduleState").GetProperty("planType").GetString());
            AssertEqual(
                "success",
                revokeResult.RootElement.GetProperty("scheduleState").GetProperty("classification").GetString());
            AssertEqual(
                "idle",
                revokeResult.RootElement.GetProperty("scheduleState").GetProperty("schedulerAction").GetString());
            AssertEqual(
                "cloudMembershipAttemptPlan",
                revokeResult.RootElement.GetProperty("attemptPlan").GetProperty("planType").GetString());
            AssertEqual(
                "success",
                revokeResult.RootElement.GetProperty("attemptPlan").GetProperty("classification").GetString());
            AssertEqual(
                "finishCloudDeviceRevoked",
                revokeResult.RootElement.GetProperty("attemptPlan").GetProperty("attemptAction").GetString());
            AssertEqual(
                "cloudMembershipLoopPlan",
                revokeResult.RootElement.GetProperty("loopPlan").GetProperty("planType").GetString());
            AssertEqual(1, revokeResult.RootElement.GetProperty("loopPlan").GetProperty("roundCount").GetInt32());
            AssertEqual(
                "finishCloudDeviceRevoked",
                revokeResult.RootElement.GetProperty("loopPlan").GetProperty("finalAction").GetString());
            AssertEqual(1, revokeResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("attemptCount").GetInt32());
            AssertEqual(1, revokeResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finishMembershipAttemptCount").GetInt32());
            AssertEqual(
                "finishCloudDeviceRevoked",
                revokeResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finalAction").GetString());
            AssertTrue(revokeResult.RootElement.GetProperty("completed").GetBoolean());
            AssertEqual("finishCloudDeviceRevoked", revokeResult.RootElement.GetProperty("clientAction").GetString());
            AssertEqual(
                "revoke",
                revokeResult.RootElement.GetProperty("membershipRound").GetProperty("membershipAction").GetString());
            AssertEqual(
                "revoked",
                revokeResult.RootElement.GetProperty("membershipRound").GetProperty("memberStatus").GetString());

            var manifestPath = Path.Combine(cloudRoot, "Actro", "BrowserSync", "manifest.json");
            using var manifest = JsonDocument.Parse(File.ReadAllText(manifestPath));
            AssertTrue(manifest.RootElement.GetProperty("devices").EnumerateArray().Any(device =>
                device.GetProperty("deviceId").GetString() == "device-win"
                && device.GetProperty("status").GetString() == "revoked"));
            var mutatedManifest = JsonNode.Parse(File.ReadAllText(manifestPath))!.AsObject();
            mutatedManifest["storageSchemaVersion"] = BrowserStorageDatabase.SchemaVersion + 1;
            File.WriteAllText(manifestPath, mutatedManifest.ToJsonString());

            var blockedJoinRequestJson = JsonSerializer.Serialize(new Dictionary<string, object>
            {
                ["scope"] = "profile",
                ["actorDeviceId"] = "device-win",
                ["memberDeviceId"] = "device-win-2",
                ["displayName"] = "Actro Windows 2",
                ["platform"] = "Windows",
                ["membershipAction"] = "connect",
                ["generatedAtUnixSeconds"] = 1840,
            });

            using var blockedJoinResult = JsonDocument.Parse(await client.RunLocalFolderMembershipRoundAsync(
                cloudRoot,
                blockedJoinRequestJson,
                1840));

            AssertEqual("clientUpgradeRequired", blockedJoinResult.RootElement.GetProperty("classification").GetString());
            AssertFalse(blockedJoinResult.RootElement.GetProperty("completed").GetBoolean());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("compatibilityPreflight").GetProperty("shouldBlockSync").GetBoolean());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("membershipRound").GetProperty("planningErrorKind").GetString());
            AssertEqual(
                "cloudMembershipStatusPolicy",
                blockedJoinResult.RootElement.GetProperty("statusPolicy").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("statusPolicy").GetProperty("classification").GetString());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("statusPolicy").GetProperty("clientAction").GetString());
            AssertEqual(
                "cloudMembershipScheduleState",
                blockedJoinResult.RootElement.GetProperty("scheduleState").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("scheduleState").GetProperty("classification").GetString());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("scheduleState").GetProperty("requiresUserAction").GetBoolean());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("scheduleState").GetProperty("userAction").GetString());
            AssertEqual(
                "cloudMembershipAttemptPlan",
                blockedJoinResult.RootElement.GetProperty("attemptPlan").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("attemptPlan").GetProperty("classification").GetString());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("attemptPlan").GetProperty("attemptAction").GetString());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("attemptPlan").GetProperty("requiresUserAction").GetBoolean());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("attemptPlan").GetProperty("shouldStopUntilRecovery").GetBoolean());
            AssertEqual(
                "cloudMembershipLoopPlan",
                blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("planType").GetString());
            AssertEqual(1, blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("roundCount").GetInt32());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("finalAction").GetString());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("shouldStopUntilRecovery").GetBoolean());
            AssertEqual(1, blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("attemptCount").GetInt32());
            AssertEqual(1, blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("stopUntilRecoveryAttemptCount").GetInt32());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finalAction").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("loopPlan").GetProperty("loopSummary").GetProperty("finalClassification").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("classification").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("compatibilityClassification").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("providerDiagnostics").GetProperty("scheduleClassification").GetString());
            AssertEqual(
                "cloudObservabilityEvent",
                blockedJoinResult.RootElement.GetProperty("observabilityEvent").GetProperty("planType").GetString());
            AssertEqual(
                "cloudSchemaRolloutPlan",
                blockedJoinResult.RootElement.GetProperty("schemaRolloutPlan").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("schemaRolloutPlan").GetProperty("classification").GetString());
            AssertEqual(
                "cloudSchemaMigrationPlaybook",
                blockedJoinResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("planType").GetString());
            AssertEqual(
                "clientUpgradeRequired",
                blockedJoinResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("classification").GetString());
            AssertFalse(blockedJoinResult.RootElement.GetProperty("schemaMigrationPlaybook").GetProperty("requiresProviderBackup").GetBoolean());
            AssertEqual(
                "recoverySnapshotRound",
                blockedJoinResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("planType").GetString());
            AssertTrue(blockedJoinResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("completed").GetBoolean());
            AssertEqual(
                "upgradeClient",
                blockedJoinResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("clientAction").GetString());
            AssertFalse(blockedJoinResult.RootElement.GetProperty("recoverySnapshotRound").GetProperty("resultPlan").GetProperty("snapshotRequested").GetBoolean());
            AssertFalse(File.Exists(Path.Combine(cloudRoot, "Actro", "BrowserSync", "devices", "device-win-2.json")));
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
            if (Directory.Exists(cloudRoot))
            {
                Directory.Delete(cloudRoot, recursive: true);
            }
        }
    }),
    ("skips browser storage sync coordinator without credentials", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            var transport = new FakeBrowserStorageSyncTransport("{}");
            var coordinator = new BrowserStorageSyncCoordinator(
                new BrowserStorageSyncClient(database, transport),
                new FakeBrowserStorageSyncCredentialsProvider(null));

            using var result = JsonDocument.Parse(await coordinator.RunSyncCycleAsync(1400, 20, observedAtUnixSeconds: 1500));

            AssertFalse(result.RootElement.GetProperty("credentialsAvailable").GetBoolean());
            AssertEqual("credentialsUnavailable", result.RootElement.GetProperty("classification").GetString());
            AssertFalse(result.RootElement.GetProperty("shouldRetry").GetBoolean());
            AssertEqual("refreshCredentials", result.RootElement.GetProperty("clientAction").GetString());
            AssertEqual(
                "waitForCredentials",
                result.RootElement.GetProperty("statusPolicy").GetProperty("schedulerAction").GetString());
            AssertEqual(
                "refreshUserSession",
                result.RootElement.GetProperty("statusPolicy").GetProperty("credentialAction").GetString());
            AssertEqual(
                "waitForCredentials",
                result.RootElement.GetProperty("scheduleState").GetProperty("schedulerAction").GetString());
            AssertEqual(0, result.RootElement.GetProperty("scheduleState").GetProperty("nextRetryAttempt").GetInt32());
            AssertEqual(
                JsonValueKind.Null,
                result.RootElement.GetProperty("scheduleState").GetProperty("nextRunAtUnixSeconds").ValueKind);
            AssertTrue(result.RootElement.GetProperty("scheduleState").GetProperty("waitingForCredentials").GetBoolean());
            AssertEqual(null, transport.LastRequest);
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("runs browser storage sync coordinator with credentials", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            var cycleResponseJson = ActroCore.BuildBrowserStorageSyncCycleResponseJson(
                ActroCore.LoadEmptyBrowserStorageSyncBatchJson("profile", "device-win", 1410),
                """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-win",
                    "syncedAtUnixSeconds": 1411,
                    "records": [],
                    "cursors": []
                }
                """);
            var transport = new FakeBrowserStorageSyncTransport(cycleResponseJson);
            var coordinator = new BrowserStorageSyncCoordinator(
                new BrowserStorageSyncClient(database, transport),
                new FakeBrowserStorageSyncCredentialsProvider(
                    new BrowserStorageSyncCredentials(
                        "http://127.0.0.1:8787/",
                        "token-win-coordinator",
                        "profile",
                        "device-win")));

            using var result = JsonDocument.Parse(await coordinator.RunSyncCycleAsync(1412, 20, 3, 1512));

            AssertTrue(result.RootElement.GetProperty("credentialsAvailable").GetBoolean());
            AssertEqual("success", result.RootElement.GetProperty("classification").GetString());
            AssertEqual("Bearer token-win-coordinator", transport.LastRequest?.Headers["Authorization"]);
            AssertEqual(
                "idle",
                result.RootElement.GetProperty("statusPolicy").GetProperty("schedulerAction").GetString());
            AssertTrue(result.RootElement.GetProperty("statusPolicy").GetProperty("resetRetryAttempt").GetBoolean());
            AssertEqual(
                "idle",
                result.RootElement.GetProperty("scheduleState").GetProperty("schedulerAction").GetString());
            AssertEqual(0, result.RootElement.GetProperty("scheduleState").GetProperty("nextRetryAttempt").GetInt32());
            AssertEqual(
                JsonValueKind.Null,
                result.RootElement.GetProperty("scheduleState").GetProperty("nextRunAtUnixSeconds").ValueKind);
            AssertEqual(
                "success",
                result.RootElement.GetProperty("syncResult").GetProperty("classification").GetString());
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("schedules browser storage sync retry from status policy", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            var transport = new FakeBrowserStorageSyncTransport("", statusCode: 503);
            var coordinator = new BrowserStorageSyncCoordinator(
                new BrowserStorageSyncClient(database, transport),
                new FakeBrowserStorageSyncCredentialsProvider(
                    new BrowserStorageSyncCredentials(
                        "http://127.0.0.1:8787/",
                        "token-win-retry",
                        "profile",
                        "device-win")));

            using var result = JsonDocument.Parse(await coordinator.RunSyncCycleAsync(1420, 20, 2, 2000));

            AssertEqual("serverUnavailable", result.RootElement.GetProperty("classification").GetString());
            AssertEqual(
                "scheduleRetry",
                result.RootElement.GetProperty("statusPolicy").GetProperty("schedulerAction").GetString());
            AssertEqual(3, result.RootElement.GetProperty("scheduleState").GetProperty("nextRetryAttempt").GetInt32());
            AssertEqual(20, result.RootElement.GetProperty("scheduleState").GetProperty("nextRunDelaySeconds").GetInt32());
            AssertEqual(2020, result.RootElement.GetProperty("scheduleState").GetProperty("nextRunAtUnixSeconds").GetInt64());
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("browser storage sync scheduler consumes schedule state and resets after success", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        try
        {
            var database = new BrowserStorageDatabase(databasePath);
            var transport = new FakeBrowserStorageSyncTransport("", statusCode: 503);
            var coordinator = new BrowserStorageSyncCoordinator(
                new BrowserStorageSyncClient(database, transport),
                new FakeBrowserStorageSyncCredentialsProvider(
                    new BrowserStorageSyncCredentials(
                        "http://127.0.0.1:8787/",
                        "token-win-scheduler",
                        "profile",
                        "device-win")));
            using var scheduler = new BrowserStorageSyncScheduler(coordinator);

            await scheduler.RunNowAsync(1430, 20, 3000);
            var retrySnapshot = scheduler.Snapshot;
            AssertEqual("scheduleRetry", retrySnapshot.SchedulerAction);
            AssertEqual(1U, retrySnapshot.RetryAttempt);
            AssertEqual<uint?>(5U, retrySnapshot.NextRunDelaySeconds);
            AssertEqual<long?>(3005L, retrySnapshot.NextRunAtUnixSeconds);
            AssertTrue(retrySnapshot.PendingRunScheduled);

            using var retryStatus = JsonDocument.Parse(scheduler.StatusJson);
            AssertEqual("retryScheduled", retryStatus.RootElement.GetProperty("status").GetString());
            AssertEqual("serverUnavailable", retryStatus.RootElement.GetProperty("lastClassification").GetString());
            AssertTrue(retryStatus.RootElement.GetProperty("pendingRunScheduled").GetBoolean());

            scheduler.CancelPendingSync();

            transport.StatusCode = 200;
            transport.ResponseJson = ActroCore.BuildBrowserStorageSyncCycleResponseJson(
                ActroCore.LoadEmptyBrowserStorageSyncBatchJson("profile", "device-win", 1440),
                """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-win",
                    "syncedAtUnixSeconds": 1441,
                    "records": [],
                    "cursors": []
                }
                """);

            await scheduler.RunNowAsync(1442, 20, 3010);
            var successSnapshot = scheduler.Snapshot;
            AssertEqual("idle", successSnapshot.SchedulerAction);
            AssertEqual(0U, successSnapshot.RetryAttempt);
            AssertEqual<long?>(null, successSnapshot.NextRunAtUnixSeconds);
            AssertFalse(successSnapshot.PendingRunScheduled);

            using var successStatus = JsonDocument.Parse(scheduler.StatusJson);
            AssertEqual("idle", successStatus.RootElement.GetProperty("status").GetString());
            AssertEqual(0, successStatus.RootElement.GetProperty("retryAttempt").GetInt32());
            AssertFalse(successStatus.RootElement.GetProperty("pendingRunScheduled").GetBoolean());
        }
        finally
        {
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("formats browser sync status for product UI", () =>
    {
        AssertEqual(
            "Browser data sync retry scheduled in 5s (last result: server unavailable)",
            BrowserSyncStatusPolicy.FormatStatusJson(
                """
                {
                    "status": "retryScheduled",
                    "nextRunDelaySeconds": 5,
                    "lastClassification": "serverUnavailable"
                }
                """));
        AssertEqual(
            "Browser data sync needs sign-in",
            BrowserSyncStatusPolicy.FormatStatusJson(
                """
                {
                    "status": "waitingForCredentials",
                    "lastClassification": "credentialsUnavailable"
                }
                """));
        AssertEqual(
            "Browser data sync requires an app update",
            BrowserSyncStatusPolicy.FormatStatusJson(
                """
                {
                    "status": "requiresUserAction",
                    "lastClassification": "clientUpgradeRequired"
                }
                """));
        AssertEqual(
            BrowserSyncStatusPolicy.UnavailableStatus,
            BrowserSyncStatusPolicy.FormatStatusJson("{"));
        return Task.CompletedTask;
    }),
    ("persists browser storage sync credentials", async () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserStorageSyncCredentialStore(filePath);
        try
        {
            store.Save(store.CreateSnapshot(
                " http://127.0.0.1:8787/ ",
                " token-win-stored ",
                " profile ",
                " device-win-stored ",
                " beta ",
                12_345));

            var snapshot = store.Load();
            AssertEqual(2, snapshot?.Version);
            AssertEqual("beta", snapshot?.Environment);
            AssertEqual("http://127.0.0.1:8787/", snapshot?.BaseUrl);
            AssertEqual("token-win-stored", snapshot?.AccessToken);
            AssertEqual("profile", snapshot?.Scope);
            AssertEqual("device-win-stored", snapshot?.DeviceId);
            AssertEqual(12_345L, snapshot?.SavedAtUnixSeconds);

            // The file on disk must be the self-describing envelope, never a
            // plaintext token: DPAPI on Windows, an explicit plain envelope on
            // non-Windows test hosts only.
            var storedText = File.ReadAllText(filePath);
            using (var storedDocument = JsonDocument.Parse(storedText))
            {
                AssertEqual(
                    OperatingSystem.IsWindows() ? "dpapi" : "plain",
                    storedDocument.RootElement.GetProperty("format").GetString());
                AssertTrue(storedDocument.RootElement.TryGetProperty("payload", out _));
            }

            AssertFalse(storedText.Contains("token-win-stored", StringComparison.Ordinal));

            var credentials = await store.GetCredentialsAsync();
            AssertEqual("http://127.0.0.1:8787/", credentials?.BaseUrl);
            AssertEqual("token-win-stored", credentials?.AccessToken);
            AssertEqual("profile", credentials?.Scope);
            AssertEqual("device-win-stored", credentials?.DeviceId);

            store.SaveSession("prod", " token-win-session ");
            snapshot = store.Load();
            AssertEqual("prod", snapshot?.Environment);
            AssertEqual("https://account.actro.chat", snapshot?.BaseUrl);
            AssertEqual("token-win-session", snapshot?.AccessToken);
            AssertEqual("default", snapshot?.Scope);
            AssertTrue((snapshot?.SavedAtUnixSeconds ?? 0) > 0);

            store.ClearSession("beta");
            AssertTrue(store.Load() is not null);
            store.ClearSession("prod");
            AssertEqual(null, store.Load());

            File.WriteAllText(
                filePath,
                """
                {
                    "version": 1,
                    "baseUrl": " http://127.0.0.1:8788/ ",
                    "accessToken": " legacy-token ",
                    "scope": " legacy-scope ",
                    "deviceId": " legacy-device "
                }
                """);
            snapshot = store.Load();
            AssertEqual(2, snapshot?.Version);
            AssertEqual("prod", snapshot?.Environment);
            AssertEqual("http://127.0.0.1:8788/", snapshot?.BaseUrl);
            AssertEqual("legacy-token", snapshot?.AccessToken);
            AssertEqual("legacy-scope", snapshot?.Scope);
            AssertEqual("legacy-device", snapshot?.DeviceId);
            AssertTrue((snapshot?.SavedAtUnixSeconds ?? 0) > 0);

            // Loading a legacy plaintext file migrates it in place: the file
            // is rewritten as an envelope and the token is no longer readable.
            var migratedText = File.ReadAllText(filePath);
            AssertTrue(migratedText.Contains("\"format\"", StringComparison.Ordinal));
            AssertFalse(migratedText.Contains("legacy-token", StringComparison.Ordinal));

            snapshot = store.Load();
            AssertEqual("legacy-token", snapshot?.AccessToken);
            AssertEqual("legacy-device", snapshot?.DeviceId);

            store.Clear();
            AssertEqual(null, store.Load());
            AssertEqual(null, await store.GetCredentialsAsync());
        }
        finally
        {
            File.Delete(filePath);
        }
    }),
    ("fails loudly on corrupt browser sync credential files", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserStorageSyncCredentialStore(filePath);
        try
        {
            // Not JSON at all.
            File.WriteAllText(filePath, "{");
            AssertCredentialLoadFails(store);

            // Envelope with an unknown format.
            File.WriteAllText(filePath, """{"format":"unknown","payload":""}""");
            AssertCredentialLoadFails(store);

            // Envelope whose payload is not base64.
            File.WriteAllText(filePath, """{"format":"dpapi","payload":"%%%"}""");
            AssertCredentialLoadFails(store);

            // DPAPI envelope with an undecryptable payload: on Windows the
            // DPAPI unprotect fails; on non-Windows hosts the format itself
            // is unreadable. Both must throw, never report "signed out".
            File.WriteAllText(
                filePath,
                $$"""{"format":"dpapi","payload":"{{Convert.ToBase64String(new byte[] { 1, 2, 3, 4 })}}"}""");
            AssertCredentialLoadFails(store);
        }
        finally
        {
            File.Delete(filePath);
        }

        return Task.CompletedTask;
    }),
    ("runs browser storage sync coordinator with stored credentials", async () =>
    {
        var databasePath = CreateTempDatabasePath();
        var credentialPath = CreateTempPath();
        var credentialStore = new BrowserStorageSyncCredentialStore(credentialPath);
        try
        {
            credentialStore.Save(credentialStore.CreateSnapshot(
                "http://127.0.0.1:8787/",
                "token-win-stored",
                "profile",
                "device-win-stored"));

            var database = new BrowserStorageDatabase(databasePath);
            var cycleResponseJson = ActroCore.BuildBrowserStorageSyncCycleResponseJson(
                ActroCore.LoadEmptyBrowserStorageSyncBatchJson("profile", "device-win-stored", 1420),
                """
                {
                    "version": 1,
                    "storageSchemaVersion": 6,
                    "scope": "profile",
                    "deviceId": "device-win-stored",
                    "syncedAtUnixSeconds": 1421,
                    "records": [],
                    "cursors": []
                }
                """);
            var transport = new FakeBrowserStorageSyncTransport(cycleResponseJson);
            var coordinator = new BrowserStorageSyncCoordinator(
                new BrowserStorageSyncClient(database, transport),
                credentialStore);

            using var result = JsonDocument.Parse(await coordinator.RunSyncCycleAsync(1422, 20));

            AssertTrue(result.RootElement.GetProperty("credentialsAvailable").GetBoolean());
            AssertEqual("success", result.RootElement.GetProperty("classification").GetString());
            AssertEqual("Bearer token-win-stored", transport.LastRequest?.Headers["Authorization"]);
            AssertEqual(
                "success",
                result.RootElement.GetProperty("syncResult").GetProperty("classification").GetString());
        }
        finally
        {
            credentialStore.Clear();
            File.Delete(credentialPath);
            DeleteDatabaseFiles(databasePath);
        }
    }),
    ("loads auth endpoint contract from core", () =>
    {
        AssertEqual("prod", ActroCore.LoadAuthEnvironmentProd());
        AssertEqual("https://account.actro.chat", ActroCore.LoadAuthAccountBaseUrl("prod"));
        AssertEqual("https://account-actro.betaenv.com", ActroCore.LoadAuthAccountBaseUrl("beta"));
        AssertEqual("http://account-actro.develenv.com", ActroCore.LoadAuthAccountBaseUrl("dev"));
        AssertEqual("https://account.actro.chat", ActroCore.LoadAuthAccountBaseUrl("unknown"));
        return Task.CompletedTask;
    }),
    ("builds safe browser history document", () =>
    {
        var document = BrowserHistoryDocumentPolicy.BuildDocument(
            [
                new BrowserHistoryItemSnapshot(
                    "Example <Docs>",
                    "https://example.com/?q=<x>",
                    100,
                    2),
            ]);

        AssertTrue(document.Contains("Example &lt;Docs&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("https://example.com/?q=&lt;x&gt;", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-list-search", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-filter=\"Example &lt;Docs&gt; https://example.com/?q=&lt;x&gt;\"", StringComparison.Ordinal));
        AssertTrue(document.Contains("No matching history.", StringComparison.Ordinal));
        AssertTrue(document.Contains("data-remove-history=\"https://example.com/?q=&lt;x&gt;\"", StringComparison.Ordinal));
        AssertTrue(document.Contains("removeHostMethod = \"removeHistoryEntry\"", StringComparison.Ordinal));
        AssertTrue(document.Contains("2 visits", StringComparison.Ordinal));
        AssertFalse(document.Contains("Example <Docs>", StringComparison.Ordinal));
        AssertFalse(document.Contains("q=<x>", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("round trips browser window state snapshots", () =>
    {
        var filePath = CreateTempPath();
        var store = new BrowserWindowStateStore(filePath);
        var snapshot = store.CreateSnapshot(120, 80, 1440, 900, isMaximized: true);

        store.Save(snapshot);
        var restored = store.Load();

        AssertEqual(1, restored?.Version);
        AssertEqual(120, restored?.X);
        AssertEqual(80, restored?.Y);
        AssertEqual(1440, restored?.Width);
        AssertEqual(900, restored?.Height);
        AssertEqual(true, restored?.IsMaximized);
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("normalizes browser window state size", () =>
    {
        var store = new BrowserWindowStateStore(CreateTempPath());
        var snapshot = store.CreateSnapshot(12, 34, 10, 20, isMaximized: false);

        AssertEqual(12, snapshot.X);
        AssertEqual(34, snapshot.Y);
        AssertEqual(BrowserWindowStateStore.MinimumWidth, snapshot.Width);
        AssertEqual(BrowserWindowStateStore.MinimumHeight, snapshot.Height);
        AssertFalse(snapshot.IsMaximized);
        return Task.CompletedTask;
    }),
    ("ignores corrupt browser window state files", () =>
    {
        var filePath = CreateTempPath();
        File.WriteAllText(filePath, "{not-json");
        var store = new BrowserWindowStateStore(filePath);

        AssertEqual(null, store.Load());
        File.Delete(filePath);
        return Task.CompletedTask;
    }),
    ("reopens closed tabs in last-closed order", () =>
    {
        var stack = new BrowserClosedTabStack();

        stack.Push(new BrowserSessionTabSnapshot("One", "https://example.com/one"));
        stack.Push(new BrowserSessionTabSnapshot("Two", "example.com/two"));

        AssertTrue(stack.CanReopen);
        AssertEqual(2, stack.Count);

        var second = stack.Pop();
        AssertEqual("Two", second?.Title);
        AssertEqual("https://example.com/two", second?.Address);

        var first = stack.Pop();
        AssertEqual("One", first?.Title);
        AssertEqual("https://example.com/one", first?.Address);
        AssertFalse(stack.CanReopen);
        return Task.CompletedTask;
    }),
    ("ignores invalid closed tab snapshots", () =>
    {
        var stack = new BrowserClosedTabStack();

        stack.Push(new BrowserSessionTabSnapshot("Bad", "javascript:alert(1)"));

        AssertFalse(stack.CanReopen);
        AssertEqual(0, stack.Count);
        AssertEqual(null, stack.Pop());
        return Task.CompletedTask;
    }),
    ("returns core status over host bridge", async () =>
    {
        var dispatcher = CreateDispatcher(Guid.NewGuid());
        using var response = await DispatchAsync(dispatcher, "core", HostBridgeDispatcher.GetCoreStatusMethod);

        AssertOkResponse(response, "core");
        var result = response.RootElement.GetProperty("result");
        AssertEqual("Actro", result.GetProperty("productName").GetString());
        AssertEqual((uint)1, result.GetProperty("cApiVersion").GetUInt32());
        AssertEqual("ask", result.GetProperty("supportedContractName").GetString());
    }),
    ("dispatches open tab params with owner tab id", async () =>
    {
        var ownerTabId = Guid.NewGuid();
        Guid? receivedOwnerTabId = null;
        HostOpenTabRequest? receivedRequest = null;
        var dispatcher = CreateDispatcher(
            ownerTabId,
            openTabAsync: (sourceTabId, request) =>
            {
                receivedOwnerTabId = sourceTabId;
                receivedRequest = request;
                var tab = new HostTabPayload("new-tab", request.Title ?? "New Tab", request.Url ?? "", true, false, false, false);
                return Task.FromResult(new HostOpenTabPayload(tab.TabId, tab));
            });

        using var response = await DispatchAsync(
            dispatcher,
            "open",
            HostBridgeDispatcher.OpenTabMethod,
            new { url = "https://example.com", title = "Docs", activate = false });

        AssertOkResponse(response, "open");
        AssertEqual(ownerTabId, receivedOwnerTabId);
        AssertEqual("https://example.com", receivedRequest?.Url);
        AssertEqual("Docs", receivedRequest?.Title);
        AssertEqual(false, receivedRequest?.Activate);
        AssertEqual("new-tab", response.RootElement.GetProperty("result").GetProperty("tabId").GetString());
    }),
    ("dispatches remove bookmark params", async () =>
    {
        HostRemoveBookmarkRequest? receivedRequest = null;
        var dispatcher = CreateDispatcher(
            Guid.NewGuid(),
            removeBookmarkAsync: request =>
            {
                receivedRequest = request;
                return Task.FromResult(true);
            });

        using var response = await DispatchAsync(
            dispatcher,
            "remove-bookmark",
            HostBridgeDispatcher.RemoveBookmarkMethod,
            new { address = "https://example.com/docs" });

        AssertOkResponse(response, "remove-bookmark");
        AssertEqual("https://example.com/docs", receivedRequest?.Address);
        AssertTrue(response.RootElement.GetProperty("result").GetBoolean());
    }),
    ("dispatches remove history params", async () =>
    {
        HostRemoveHistoryEntryRequest? receivedRequest = null;
        var dispatcher = CreateDispatcher(
            Guid.NewGuid(),
            removeHistoryEntryAsync: request =>
            {
                receivedRequest = request;
                return Task.FromResult(true);
            });

        using var response = await DispatchAsync(
            dispatcher,
            "remove-history",
            HostBridgeDispatcher.RemoveHistoryEntryMethod,
            new { address = "https://example.com/docs" });

        AssertOkResponse(response, "remove-history");
        AssertEqual("https://example.com/docs", receivedRequest?.Address);
        AssertTrue(response.RootElement.GetProperty("result").GetBoolean());
    }),
    ("rejects host bridge calls from remote documents", async () =>
    {
        var dispatcher = CreateDispatcher(Guid.NewGuid());
        using var response = await DispatchAsync(
            dispatcher,
            "blocked",
            HostBridgeDispatcher.GetTabsMethod,
            source: "https://example.com/app");

        AssertErrorResponse(response, "blocked", "forbidden_source");
    }),
    ("keeps request id when params are invalid", async () =>
    {
        var dispatcher = CreateDispatcher(Guid.NewGuid());
        using var response = await DispatchRawAsync(
            dispatcher,
            """
            {
              "type": "actro.host.request",
              "id": "bad-params",
              "method": "actro.host.navigate",
              "params": "not-an-object"
            }
            """);

        AssertErrorResponse(response, "bad-params", "invalid_json");
    }),
    ("resolves browser theme from system app mode", () =>
    {
        AssertTrue(ReferenceEquals(BrowserTheme.Dark, BrowserThemeSelectionPolicy.Resolve(0)));
        AssertTrue(ReferenceEquals(BrowserTheme.Light, BrowserThemeSelectionPolicy.Resolve(1)));
        AssertTrue(ReferenceEquals(BrowserTheme.Light, BrowserThemeSelectionPolicy.Resolve(null)));
        AssertTrue(ReferenceEquals(BrowserTheme.Light, BrowserThemeSelectionPolicy.Resolve(2)));
        AssertTrue(ReferenceEquals(BrowserTheme.Light, BrowserThemeSelectionPolicy.Resolve(-1)));
        AssertFalse(BrowserTheme.Light.IsDark);
        AssertTrue(BrowserTheme.Dark.IsDark);
        return Task.CompletedTask;
    }),
    ("keeps browser theme palettes complete and distinct", () =>
    {
        var lightColors = BrowserTheme.Light.Colors;
        var darkColors = BrowserTheme.Dark.Colors;
        AssertEqual(16, lightColors.Count);
        AssertEqual(lightColors.Count, darkColors.Count);
        foreach (var (role, lightColor) in lightColors)
        {
            AssertTrue(darkColors.ContainsKey(role));
            AssertEqual(255, (int)lightColor.A);
            AssertEqual(255, (int)darkColors[role].A);
            if (role == nameof(BrowserTheme.Accent))
            {
                // The accent is deliberately shared between both palettes and
                // the internal document stylesheet.
                AssertEqual(lightColor, darkColors[role]);
            }
            else
            {
                AssertFalse(lightColor == darkColors[role]);
            }
        }

        AssertEqual("Segoe UI", BrowserTheme.UiFontFamily);
        AssertEqual(10f, BrowserTheme.UiFontPointSize);
        AssertTrue(BrowserTheme.MonoFontFamilies.Count > 0);
        AssertTrue(BrowserTheme.GlyphFontFamilies.Count > 0);
        return Task.CompletedTask;
    }),
    ("scales browser theme metrics per dpi", () =>
    {
        AssertEqual(44, BrowserTheme.Metrics.ToolbarHeight);
        AssertEqual(40, BrowserTheme.Metrics.FindBarHeight);
        AssertEqual(32, BrowserTheme.Metrics.TabRowHeight);
        AssertEqual(180, BrowserTheme.Metrics.SidebarMinWidth);
        AssertEqual(240, BrowserTheme.Metrics.SidebarDefaultWidth);
        AssertEqual(88, BrowserTheme.Metrics.CollapsedSegmentWidth);

        AssertEqual(44, BrowserTheme.Metrics.Scale(44, 96));
        AssertEqual(55, BrowserTheme.Metrics.Scale(44, 120));
        AssertEqual(66, BrowserTheme.Metrics.Scale(44, 144));
        AssertEqual(88, BrowserTheme.Metrics.Scale(44, 192));
        AssertEqual(50, BrowserTheme.Metrics.Scale(40, 120));
        // Half values round away from zero: 3 * 120 / 96 = 3.75, 1 * 144 / 96 = 1.5.
        AssertEqual(4, BrowserTheme.Metrics.Scale(3, 120));
        AssertEqual(2, BrowserTheme.Metrics.Scale(1, 144));
        AssertEqual(0, BrowserTheme.Metrics.Scale(0, 192));
        return Task.CompletedTask;
    }),
    ("clamps sidebar width between the scaled minimum and half the window", () =>
    {
        AssertEqual(180, BrowserSidebarLayoutPolicy.MinWidth(96));
        AssertEqual(225, BrowserSidebarLayoutPolicy.MinWidth(120));
        AssertEqual(270, BrowserSidebarLayoutPolicy.MinWidth(144));
        AssertEqual(360, BrowserSidebarLayoutPolicy.MinWidth(192));
        AssertEqual(240, BrowserSidebarLayoutPolicy.DefaultWidth(96));
        AssertEqual(300, BrowserSidebarLayoutPolicy.DefaultWidth(120));
        AssertEqual(480, BrowserSidebarLayoutPolicy.DefaultWidth(192));

        AssertEqual(640, BrowserSidebarLayoutPolicy.MaxWidth(1280, 96));
        // The 50% rule never squeezes the band below the scaled minimum:
        // on a too-narrow window the minimum wins.
        AssertEqual(180, BrowserSidebarLayoutPolicy.MaxWidth(300, 96));
        AssertEqual(360, BrowserSidebarLayoutPolicy.MaxWidth(500, 192));

        AssertEqual(180, BrowserSidebarLayoutPolicy.ClampWidth(100, 1280, 96));
        AssertEqual(240, BrowserSidebarLayoutPolicy.ClampWidth(240, 1280, 96));
        AssertEqual(640, BrowserSidebarLayoutPolicy.ClampWidth(5000, 1280, 96));
        AssertEqual(225, BrowserSidebarLayoutPolicy.ClampWidth(0, 1280, 120));
        AssertEqual(480, BrowserSidebarLayoutPolicy.ClampWidth(700, 960, 192));
        AssertEqual(180, BrowserSidebarLayoutPolicy.ClampWidth(240, 300, 96));
        return Task.CompletedTask;
    }),
    ("maps toolbar commands to Segoe MDL2 Assets glyphs", () =>
    {
        // Codepoint-to-name pairs verified against the Segoe MDL2 Assets
        // reference. The app targets Win10 19041, where Segoe Fluent Icons
        // is not installed, so a Fluent-only codepoint would render as tofu
        // under the MDL2 fallback. Extend this whitelist only with
        // codepoints confirmed to exist in Segoe MDL2 Assets.
        var mdl2Whitelist = new Dictionary<char, string>
        {
            ['\uE710'] = "Add",
            ['\uE711'] = "Cancel",
            ['\uE712'] = "More",
            ['\uE72A'] = "Forward",
            ['\uE72B'] = "Back",
            ['\uE72C'] = "Refresh",
            ['\uE734'] = "FavoriteStar",
            ['\uE735'] = "FavoriteStarFill",
            ['\uE89F'] = "ClosePane",
            ['\uE8A0'] = "OpenPane",
            ['\uE8A9'] = "ViewAll",
        };

        AssertTrue(BrowserGlyphs.All.Count > 0);
        foreach (var glyph in BrowserGlyphs.All)
        {
            // Icon-font glyphs live in the Private Use Area, never among
            // printable characters.
            AssertTrue(glyph >= '\uE700' && glyph <= '\uF8FF');
            AssertTrue(mdl2Whitelist.ContainsKey(glyph));
        }

        AssertEqual(BrowserGlyphs.All.Count, BrowserGlyphs.All.Distinct().Count());

        // Exactly the toolbar-invocable commands are mapped; the flip states
        // (stop, filled star) and the non-command glyph (the app menu
        // ellipsis) stay inside All so nothing bypasses the whitelist above.
        var expectedCommands = new[]
        {
            BrowserMenuCommand.NewTab,
            BrowserMenuCommand.ToggleTabOverview,
            BrowserMenuCommand.ToggleSidebar,
            BrowserMenuCommand.Back,
            BrowserMenuCommand.Forward,
            BrowserMenuCommand.Reload,
            BrowserMenuCommand.Stop,
            BrowserMenuCommand.BookmarkPage,
        };
        AssertEqual(expectedCommands.Length, BrowserGlyphs.CommandGlyphs.Count);
        foreach (var command in expectedCommands)
        {
            AssertTrue(BrowserGlyphs.CommandGlyphs.ContainsKey(command));
            AssertEqual(BrowserGlyphs.CommandGlyphs[command], BrowserGlyphs.Get(command));
            AssertTrue(BrowserGlyphs.All.Contains(BrowserGlyphs.Get(command)));
        }

        // The state flips must be visibly distinct pairs.
        AssertTrue(BrowserGlyphs.Get(BrowserMenuCommand.Reload) != BrowserGlyphs.Get(BrowserMenuCommand.Stop));
        AssertTrue(BrowserGlyphs.BookmarkStar != BrowserGlyphs.BookmarkStarFilled);

        // An unmapped command is a wiring bug and must throw loudly.
        var unmappedCommandThrew = false;
        try
        {
            _ = BrowserGlyphs.Get(BrowserMenuCommand.Exit);
        }
        catch (ArgumentOutOfRangeException)
        {
            unmappedCommandThrew = true;
        }

        AssertTrue(unmappedCommandThrew);
        return Task.CompletedTask;
    }),
    ("computes two-segment toolbar geometry with a collapsed stub", () =>
    {
        // Expanded: the sidebar segment tracks the sidebar width.
        var expanded = BrowserToolbarLayoutPolicy.Segments(240, false, 1280, 96);
        AssertEqual(240, expanded.SidebarSegmentWidth);
        AssertEqual(240, expanded.ContentSegmentLeft);
        AssertEqual(1040, expanded.ContentSegmentWidth);

        // Collapsed: the fixed 88 px stub keeps [+ new tab][overview slot]
        // visible instead of vanishing with the sidebar.
        var collapsed = BrowserToolbarLayoutPolicy.Segments(240, true, 1280, 96);
        AssertEqual(88, collapsed.SidebarSegmentWidth);
        AssertEqual(88, collapsed.ContentSegmentLeft);
        AssertEqual(1192, collapsed.ContentSegmentWidth);

        // The stub is also the expanded floor for degenerate widths.
        AssertEqual(88, BrowserToolbarLayoutPolicy.Segments(40, false, 1280, 96).SidebarSegmentWidth);

        // DPI scaling: 88 * 192 / 96 = 176; 88 * 120 / 96 = 110.
        AssertEqual(176, BrowserToolbarLayoutPolicy.Segments(0, true, 1280, 192).SidebarSegmentWidth);
        AssertEqual(110, BrowserToolbarLayoutPolicy.CollapsedStubWidth(120));
        AssertEqual(88, BrowserToolbarLayoutPolicy.CollapsedStubWidth(96));

        // A transient mid-layout toolbar narrower than the segment clamps
        // instead of producing a negative content width.
        var tiny = BrowserToolbarLayoutPolicy.Segments(240, false, 60, 96);
        AssertEqual(60, tiny.SidebarSegmentWidth);
        AssertEqual(0, tiny.ContentSegmentWidth);
        AssertEqual(0, BrowserToolbarLayoutPolicy.Segments(240, true, -5, 96).SidebarSegmentWidth);

        // The stub reserves room for [pad][+ new tab][gap][overview slot]
        // at every supported DPI, so the collapsed segment never clips.
        foreach (var dpi in new[] { 96, 120, 144, 192 })
        {
            AssertTrue(
                BrowserToolbarLayoutPolicy.EdgePadding(dpi)
                + BrowserToolbarLayoutPolicy.ButtonWidth(dpi)
                + BrowserToolbarLayoutPolicy.ButtonGap(dpi)
                + BrowserToolbarLayoutPolicy.OverviewSlotWidth(dpi)
                <= BrowserToolbarLayoutPolicy.CollapsedStubWidth(dpi));
        }

        return Task.CompletedTask;
    }),
    ("caps the toolbar address box at its scaled maximum width", () =>
    {
        AssertEqual(720, BrowserToolbarLayoutPolicy.AddressMaxWidth(96));
        AssertEqual(900, BrowserToolbarLayoutPolicy.AddressMaxWidth(120));
        AssertEqual(1440, BrowserToolbarLayoutPolicy.AddressMaxWidth(192));

        // Below the cap the box takes all leftover space; above it the box
        // stops growing (the toolbar centers it); degenerate space clamps
        // to zero.
        AssertEqual(300, BrowserToolbarLayoutPolicy.AddressWidth(300, 96));
        AssertEqual(720, BrowserToolbarLayoutPolicy.AddressWidth(720, 96));
        AssertEqual(720, BrowserToolbarLayoutPolicy.AddressWidth(5000, 96));
        AssertEqual(0, BrowserToolbarLayoutPolicy.AddressWidth(0, 96));
        AssertEqual(0, BrowserToolbarLayoutPolicy.AddressWidth(-10, 96));
        AssertEqual(900, BrowserToolbarLayoutPolicy.AddressWidth(5000, 120));
        return Task.CompletedTask;
    }),
    ("collapses the sidebar below the scaled width threshold", () =>
    {
        AssertEqual(90, BrowserSidebarLayoutPolicy.CollapseThreshold(96));
        AssertEqual(112, BrowserSidebarLayoutPolicy.CollapseThreshold(120));
        AssertEqual(180, BrowserSidebarLayoutPolicy.CollapseThreshold(192));

        AssertTrue(BrowserSidebarLayoutPolicy.ShouldCollapse(89, 96));
        AssertTrue(BrowserSidebarLayoutPolicy.ShouldCollapse(0, 96));
        AssertTrue(BrowserSidebarLayoutPolicy.ShouldCollapse(-10, 96));
        AssertFalse(BrowserSidebarLayoutPolicy.ShouldCollapse(90, 96));
        AssertFalse(BrowserSidebarLayoutPolicy.ShouldCollapse(240, 96));
        AssertTrue(BrowserSidebarLayoutPolicy.ShouldCollapse(179, 192));
        AssertFalse(BrowserSidebarLayoutPolicy.ShouldCollapse(180, 192));
        return Task.CompletedTask;
    }),
    ("builds shared document stylesheet with both palettes", () =>
    {
        var css = BrowserDocumentStylePolicy.RootCss;
        AssertTrue(css.Contains(":root {", StringComparison.Ordinal));
        AssertTrue(css.Contains("@media (prefers-color-scheme: dark)", StringComparison.Ordinal));
        AssertTrue(css.Contains("#165dff", StringComparison.Ordinal));
        AssertEqual("#f6f7f9", BrowserDocumentStylePolicy.FormatColor(BrowserTheme.Light.WindowBackground));
        AssertTrue(css.Contains(
            BrowserDocumentStylePolicy.FormatColor(BrowserTheme.Light.WindowBackground),
            StringComparison.Ordinal));
        AssertTrue(css.Contains(
            BrowserDocumentStylePolicy.FormatColor(BrowserTheme.Dark.WindowBackground),
            StringComparison.Ordinal));
        AssertTrue(css.Contains("color-scheme: light", StringComparison.Ordinal));
        AssertTrue(css.Contains("color-scheme: dark", StringComparison.Ordinal));
        return Task.CompletedTask;
    }),
    ("embeds shared stylesheet exactly once per generated document", () =>
    {
        var css = BrowserDocumentStylePolicy.RootCss;
        var homeDocument = BrowserHomeDocumentPolicy.BuildDocument(
            ActroCoreStatus.Available("Actro", 1, "ask", 1, 2, 3));
        var errorDocument = BrowserNavigationErrorPolicy.BuildErrorDocument(
            "https://example.com/",
            "Timeout");
        var listDocument = BrowserLocalListDocumentPolicy.BuildDocument(new BrowserLocalListDocumentOptions(
            "History",
            "No history yet.",
            "Search history",
            "Search history",
            "No matches.",
            "data-remove-history",
            "removeHistoryEntry",
            [new BrowserLocalListDocumentItem("Example", "https://example.com/", "today", "example")]));

        foreach (var document in new[] { homeDocument, errorDocument, listDocument })
        {
            AssertEqual(1, CountOccurrences(document, css));
            AssertTrue(document.Contains("var(--page-background)", StringComparison.Ordinal));
        }

        return Task.CompletedTask;
    }),
    ("centralizes user-visible chrome strings under well-formed keys", () =>
    {
        AssertTrue(BrowserStrings.All.Count > 0);
        foreach (var (key, value) in BrowserStrings.All)
        {
            AssertFalse(string.IsNullOrWhiteSpace(value));
            AssertTrue(key.Contains('.', StringComparison.Ordinal));
            AssertFalse(key.StartsWith('.'));
            AssertFalse(key.EndsWith('.'));
            AssertTrue(key.All(character => char.IsAsciiLetterOrDigit(character) || character == '.'));
        }

        AssertEqual("Actro", BrowserStrings.Get("window.title.product"));
        AssertEqual("Actro Private", BrowserStrings.Get("window.title.privateProduct"));
        AssertEqual("New Tab", BrowserStrings.Get("tab.defaultTitle"));
        AssertEqual("Browser Data Sync", BrowserStrings.Get("dialog.sync.title"));
        return Task.CompletedTask;
    }),
    ("falls back to the key itself for unknown chrome strings", () =>
    {
        AssertFalse(BrowserStrings.All.ContainsKey("status.doesNotExist"));
        AssertEqual("status.doesNotExist", BrowserStrings.Get("status.doesNotExist"));
        AssertEqual("status.doesNotExist", BrowserStrings.Format("status.doesNotExist"));
        return Task.CompletedTask;
    }),
    ("formats parameterized chrome strings with declared placeholder arity", () =>
    {
        foreach (var (key, value) in BrowserStrings.All)
        {
            var indices = CollectFormatPlaceholderIndices(value);
            var arity = indices.Count == 0 ? 0 : indices.Max() + 1;
            // Contiguous from {0}: a skipped slot would mean an argument list
            // that can never satisfy every placeholder.
            for (var slot = 0; slot < arity; slot++)
            {
                AssertTrue(indices.Contains(slot));
            }

            var args = Enumerable.Range(0, arity)
                .Select(slot => (object)$"[arg{slot}]")
                .ToArray();
            var formatted = BrowserStrings.Format(key, args);
            for (var slot = 0; slot < arity; slot++)
            {
                AssertTrue(formatted.Contains($"[arg{slot}]", StringComparison.Ordinal));
            }
        }

        AssertEqual(
            "Search engine: DuckDuckGo",
            BrowserStrings.Format("status.searchEngineChanged", "DuckDuckGo"));
        AssertEqual("Zoom 125%", BrowserStrings.Format("status.zoomLevel", "125%"));
        AssertEqual(
            "Page - Actro",
            BrowserStrings.Format("window.title.documentFormat", "Page", "Actro"));
        return Task.CompletedTask;
    }),
    ("exposes every menu command in the compact menu model exactly once", () =>
    {
        var commandItems = BrowserMenuModel.Flatten(BrowserMenuModel.BuildItems())
            .Where(item => item.Command is not null)
            .ToList();
        var engines = BrowserSearchEnginePolicy.Engines;

        foreach (var command in Enum.GetValues<BrowserMenuCommand>())
        {
            var matches = commandItems.Where(item => item.Command == command).ToList();
            if (command == BrowserMenuCommand.SelectSearchEngine)
            {
                // One checkable item per Rust-core engine, addressed through
                // the argument channel with pairwise-distinct engine ids.
                AssertEqual(engines.Count, matches.Count);
                var arguments = matches.Select(item => item.Argument).ToList();
                AssertEqual(engines.Count, arguments.Distinct(StringComparer.Ordinal).Count());
                foreach (var engine in engines)
                {
                    AssertTrue(arguments.Contains(engine.Id));
                    AssertTrue(matches.All(item => item.IsCheckable));
                }
            }
            else
            {
                AssertEqual(1, matches.Count);
                AssertTrue(matches[0].Argument is null);
            }
        }

        return Task.CompletedTask;
    }),
    ("keeps compact menu shortcuts unique and aliases valid", () =>
    {
        var items = BrowserMenuModel.Flatten(BrowserMenuModel.BuildItems()).ToList();
        var primaries = items
            .Where(item => item.Shortcut is not null)
            .Select(item => item.Shortcut!)
            .ToList();
        AssertEqual(primaries.Count, primaries.Distinct().Count());

        var aliasShortcuts = BrowserMenuModel.Aliases.Select(alias => alias.Shortcut).ToList();
        AssertEqual(aliasShortcuts.Count, aliasShortcuts.Distinct().Count());
        foreach (var alias in BrowserMenuModel.Aliases)
        {
            // Aliases add key variants, they never shadow a primary chord,
            // and each one targets a command that exists in the menu.
            AssertFalse(primaries.Contains(alias.Shortcut));
            AssertTrue(items.Any(item => item.Command == alias.Command));
        }

        var openers = BrowserMenuModel.MenuOpenShortcuts;
        AssertEqual(3, openers.Count);
        AssertEqual(openers.Count, openers.Distinct().Count());
        foreach (var opener in openers)
        {
            AssertFalse(primaries.Contains(opener));
            AssertFalse(aliasShortcuts.Contains(opener));
        }

        foreach (var shortcut in primaries.Concat(aliasShortcuts).Concat(openers))
        {
            AssertFalse(string.IsNullOrWhiteSpace(shortcut.Key));
        }

        return Task.CompletedTask;
    }),
    ("preserves the legacy keyboard shortcut table across primaries and aliases", () =>
    {
        var expected = new Dictionary<(bool Control, bool Shift, bool Alt, string Key), BrowserMenuCommand>
        {
            [(true, false, false, "T")] = BrowserMenuCommand.NewTab,
            [(true, false, false, "N")] = BrowserMenuCommand.NewWindow,
            [(true, true, false, "N")] = BrowserMenuCommand.NewPrivateWindow,
            [(true, false, false, "W")] = BrowserMenuCommand.CloseTab,
            [(true, true, false, "T")] = BrowserMenuCommand.ReopenClosedTab,
            [(true, false, false, "L")] = BrowserMenuCommand.FocusAddress,
            [(true, false, false, "F")] = BrowserMenuCommand.Find,
            [(true, false, false, "H")] = BrowserMenuCommand.ShowHistory,
            [(true, false, false, "D")] = BrowserMenuCommand.BookmarkPage,
            [(true, false, false, "J")] = BrowserMenuCommand.OpenDownloadsFolder,
            [(true, false, false, "R")] = BrowserMenuCommand.Reload,
            [(true, false, false, "P")] = BrowserMenuCommand.Print,
            [(true, false, false, "S")] = BrowserMenuCommand.SavePdf,
            [(true, false, false, "O")] = BrowserMenuCommand.OpenFile,
            [(true, true, false, "O")] = BrowserMenuCommand.ShowBookmarks,
            [(true, true, false, "I")] = BrowserMenuCommand.DevTools,
            [(true, true, false, "Delete")] = BrowserMenuCommand.ClearBrowsingData,
            [(true, false, false, "Oemplus")] = BrowserMenuCommand.ZoomIn,
            [(true, true, false, "Oemplus")] = BrowserMenuCommand.ZoomIn,
            [(true, false, false, "Add")] = BrowserMenuCommand.ZoomIn,
            [(true, false, false, "OemMinus")] = BrowserMenuCommand.ZoomOut,
            [(true, false, false, "Subtract")] = BrowserMenuCommand.ZoomOut,
            [(true, false, false, "D0")] = BrowserMenuCommand.ZoomReset,
            [(true, false, false, "NumPad0")] = BrowserMenuCommand.ZoomReset,
            [(false, false, false, "F11")] = BrowserMenuCommand.FullScreen,
            // Added with the vertical sidebar slice: Edge vertical-tabs chord.
            [(true, true, false, "OemComma")] = BrowserMenuCommand.ToggleSidebar,
            // Added with the tab-overview slice: overview toggle chord.
            [(true, true, false, "A")] = BrowserMenuCommand.ToggleTabOverview,
            [(false, false, false, "F5")] = BrowserMenuCommand.Reload,
            [(false, false, false, "F3")] = BrowserMenuCommand.FindNext,
            [(false, true, false, "F3")] = BrowserMenuCommand.FindPrevious,
            [(false, false, true, "Left")] = BrowserMenuCommand.Back,
            [(false, false, true, "Right")] = BrowserMenuCommand.Forward,
            [(false, false, true, "Home")] = BrowserMenuCommand.Home,
            [(false, false, true, "F4")] = BrowserMenuCommand.Exit,
        };

        var actual = new Dictionary<(bool, bool, bool, string), BrowserMenuCommand>();
        foreach (var item in BrowserMenuModel.Flatten(BrowserMenuModel.BuildItems()))
        {
            if (item.Shortcut is BrowserShortcut shortcut && item.Command is BrowserMenuCommand command)
            {
                actual[(shortcut.Control, shortcut.Shift, shortcut.Alt, shortcut.Key)] = command;
            }
        }

        foreach (var alias in BrowserMenuModel.Aliases)
        {
            var shortcut = alias.Shortcut;
            actual[(shortcut.Control, shortcut.Shift, shortcut.Alt, shortcut.Key)] = alias.Command;
        }

        AssertEqual(expected.Count, actual.Count);
        foreach (var (chord, command) in expected)
        {
            AssertTrue(actual.TryGetValue(chord, out var actualCommand));
            AssertEqual(command, actualCommand);
        }

        // Tab switching stays in MainForm.TryHandleTabShortcut: the model
        // must never claim the reserved Ctrl tab-cycling and tab-index keys.
        var reservedKeys = new[]
        {
            "Tab", "PageDown", "PageUp",
            "D1", "D2", "D3", "D4", "D5", "D6", "D7", "D8", "D9",
            "NumPad1", "NumPad2", "NumPad3", "NumPad4", "NumPad5",
            "NumPad6", "NumPad7", "NumPad8", "NumPad9",
        };
        foreach (var (chord, _) in actual)
        {
            AssertFalse(chord.Item1 && reservedKeys.Contains(chord.Item4));
        }

        return Task.CompletedTask;
    }),
    ("resolves every compact menu string key", () =>
    {
        foreach (var item in BrowserMenuModel.Flatten(BrowserMenuModel.BuildItems()))
        {
            if (item.IsSeparator)
            {
                AssertTrue(item.Command is null);
                AssertTrue(item.StringKey is null);
                continue;
            }

            if (item.StringKey is string stringKey)
            {
                AssertTrue(BrowserStrings.All.ContainsKey(stringKey));
                AssertTrue(stringKey.StartsWith("menu.", StringComparison.Ordinal));
            }
            else
            {
                // Only runtime-labelled items (search engines) may skip the
                // string table, and they must carry a display name.
                AssertEqual(BrowserMenuCommand.SelectSearchEngine, item.Command);
                AssertFalse(string.IsNullOrWhiteSpace(item.LiteralText));
            }

            // Submenu headers group commands, they do not execute one.
            if (item.Children.Count > 0)
            {
                AssertTrue(item.Command is null);
                AssertTrue(item.Shortcut is null);
            }
        }

        return Task.CompletedTask;
    }),
    ("resolves escape precedence in order", () =>
    {
        // Full truth table: overview > find bar > full screen (page or
        // browser) > stop. The overview overlay is the topmost surface, so
        // its dismissal outranks everything below it.
        AssertEqual(BrowserEscapeAction.None, BrowserEscapePolicy.Resolve(false, false, false, false, false));
        AssertEqual(BrowserEscapeAction.Stop, BrowserEscapePolicy.Resolve(false, false, false, false, true));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, false, true, false));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, false, true, true));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, true, false, false));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, true, false, true));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, true, true, false));
        AssertEqual(BrowserEscapeAction.ExitFullScreen, BrowserEscapePolicy.Resolve(false, false, true, true, true));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, false, false, false));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, false, false, true));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, false, true, false));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, false, true, true));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, true, false, false));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, true, false, true));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, true, true, false));
        AssertEqual(BrowserEscapeAction.CloseFindBar, BrowserEscapePolicy.Resolve(false, true, true, true, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, false, false, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, false, false, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, false, true, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, false, true, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, true, false, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, true, false, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, true, true, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, false, true, true, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, false, false, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, false, false, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, false, true, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, false, true, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, true, false, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, true, false, true));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, true, true, false));
        AssertEqual(BrowserEscapeAction.DismissOverview, BrowserEscapePolicy.Resolve(true, true, true, true, true));
        return Task.CompletedTask;
    }),
    ("computes overview card grid columns and rectangles", () =>
    {
        // 1280 px viewport, 4 cells at 96 DPI: four 296 px cards fill one
        // row from the padded origin (content 1232 = 4*296 + 3*16).
        AssertEqual(4, BrowserTabOverviewPolicy.Columns(1280, 4, 96));
        var fullRow = BrowserTabOverviewPolicy.CardRects(1280, 4, 96);
        AssertEqual(4, fullRow.Count);
        AssertEqual(new Rectangle(24, 24, 296, 221), fullRow[0]);
        AssertEqual(new Rectangle(336, 24, 296, 221), fullRow[1]);
        AssertEqual(new Rectangle(960, 24, 296, 221), fullRow[3]);

        // Two cells cap at the 320 px maximum and the grid centers in the
        // leftover width (content 1232, grid 656 -> origin 312).
        AssertEqual(2, BrowserTabOverviewPolicy.Columns(1280, 2, 96));
        var centered = BrowserTabOverviewPolicy.CardRects(1280, 2, 96);
        AssertEqual(new Rectangle(312, 24, 320, 236), centered[0]);
        AssertEqual(new Rectangle(648, 24, 320, 236), centered[1]);

        // A narrow viewport still yields one full-width column, and card
        // height follows the 16:10 thumbnail plus the title strip.
        AssertEqual(1, BrowserTabOverviewPolicy.Columns(300, 1, 96));
        AssertEqual(new Rectangle(24, 24, 252, 193), BrowserTabOverviewPolicy.CardRects(300, 1, 96)[0]);
        AssertEqual(185, BrowserTabOverviewPolicy.ThumbnailHeight(296));
        AssertEqual(221, BrowserTabOverviewPolicy.CardHeight(296, 96));

        // Degenerate widths clamp to 1 px cards, never negative; an empty
        // cell list resolves to an empty grid.
        var degenerate = BrowserTabOverviewPolicy.CardRects(0, 3, 96);
        AssertEqual(3, degenerate.Count);
        foreach (var cardRect in degenerate)
        {
            AssertTrue(cardRect.Width >= 1);
            AssertTrue(cardRect.Height >= 1);
        }

        AssertEqual(0, BrowserTabOverviewPolicy.Columns(1280, 0, 96));
        AssertEqual(0, BrowserTabOverviewPolicy.CardRects(1280, 0, 96).Count);
        AssertEqual(0, BrowserTabOverviewPolicy.ContentHeight(1280, 0, 96));
        return Task.CompletedTask;
    }),
    ("scales overview card grid metrics with DPI", () =>
    {
        AssertEqual(30, BrowserTabOverviewPolicy.OuterPadding(120));
        AssertEqual(48, BrowserTabOverviewPolicy.OuterPadding(192));
        AssertEqual(32, BrowserTabOverviewPolicy.CardGap(192));
        AssertEqual(400, BrowserTabOverviewPolicy.CardMinWidth(192));
        AssertEqual(640, BrowserTabOverviewPolicy.CardMaxWidth(192));
        AssertEqual(72, BrowserTabOverviewPolicy.CardTitleHeight(192));

        // 2560 px viewport, 8 cells at 192 DPI: five 467 px columns and a
        // second row (content 2464; 5 fit at the 400 px scaled minimum).
        AssertEqual(5, BrowserTabOverviewPolicy.Columns(2560, 8, 192));
        var scaledGrid = BrowserTabOverviewPolicy.CardRects(2560, 8, 192);
        AssertEqual(8, scaledGrid.Count);
        AssertEqual(new Rectangle(48, 48, 467, 363), scaledGrid[0]);
        AssertEqual(new Rectangle(48, 443, 467, 363), scaledGrid[5]);

        // Scrollable height: outer padding + rows + inter-row gaps.
        AssertEqual(854, BrowserTabOverviewPolicy.ContentHeight(2560, 8, 192));
        AssertEqual(269, BrowserTabOverviewPolicy.ContentHeight(1280, 4, 96));
        return Task.CompletedTask;
    }),
};

foreach (var test in tests)
{
    await test.Run();
    Console.WriteLine($"[ok] {test.Name}");
}

static HostBridgeDispatcher CreateDispatcher(
    Guid ownerTabId,
    Func<Guid, HostOpenTabRequest, Task<HostOpenTabPayload>>? openTabAsync = null,
    Func<HostRemoveBookmarkRequest, Task<bool>>? removeBookmarkAsync = null,
    Func<HostRemoveHistoryEntryRequest, Task<bool>>? removeHistoryEntryAsync = null)
{
    return new HostBridgeDispatcher(
        ownerTabId,
        new HostBridgeActions(
            () => ActroCoreStatus.Available("Actro", 1, "ask", 1, 2, 3),
            openTabAsync ?? ((_, request) =>
            {
                var tab = new HostTabPayload("new-tab", request.Title ?? "New Tab", request.Url ?? "", true, false, false, false);
                return Task.FromResult(new HostOpenTabPayload(tab.TabId, tab));
            }),
            (_, _) => Task.FromResult(true),
            _ => Task.FromResult(true),
            (_, request) =>
            {
                var tab = new HostTabPayload("active", "Active", request.Url ?? request.Address ?? "", true, false, false, false);
                return Task.FromResult(tab);
            },
            () => new HostTabsPayload("active", [new HostTabPayload("active", "Active", "actro://home", true, false, false, false)]),
            () => new HostTabPayload("active", "Active", "actro://home", true, false, false, false),
            removeBookmarkAsync ?? (_ => Task.FromResult(false)),
            removeHistoryEntryAsync ?? (_ => Task.FromResult(false))));
}

static Task<JsonDocument> DispatchAsync(
    HostBridgeDispatcher dispatcher,
    string id,
    string method,
    object? parameters = null,
    string source = "")
{
    var request = JsonSerializer.Serialize(new
    {
        type = HostBridgeDispatcher.RequestType,
        id,
        method,
        @params = parameters,
    });

    return DispatchRawAsync(dispatcher, request, source);
}

static async Task<JsonDocument> DispatchRawAsync(HostBridgeDispatcher dispatcher, string request, string source = "")
{
    var response = await dispatcher.HandleMessageJsonAsync(request, source);
    return JsonDocument.Parse(response);
}

static void AssertOkResponse(JsonDocument response, string id)
{
    var root = response.RootElement;
    AssertEqual(HostBridgeDispatcher.ResponseType, root.GetProperty("type").GetString());
    AssertEqual(id, root.GetProperty("id").GetString());
    AssertTrue(root.GetProperty("ok").GetBoolean());
}

static void AssertErrorResponse(JsonDocument response, string id, string code)
{
    var root = response.RootElement;
    AssertEqual(HostBridgeDispatcher.ResponseType, root.GetProperty("type").GetString());
    AssertEqual(id, root.GetProperty("id").GetString());
    AssertFalse(root.GetProperty("ok").GetBoolean());
    AssertEqual(code, root.GetProperty("error").GetProperty("code").GetString());
}

static int CountOccurrences(string text, string value)
{
    var count = 0;
    var index = 0;
    while ((index = text.IndexOf(value, index, StringComparison.Ordinal)) >= 0)
    {
        count += 1;
        index += value.Length;
    }

    return count;
}

// Collects the {n} composite-format placeholder indices of a string table
// value. The table only uses plain positional placeholders, so anything
// between braces that is not a decimal index (or an escaped brace) is a
// malformed entry and fails the test via the thrown parse exception.
static List<int> CollectFormatPlaceholderIndices(string format)
{
    var indices = new List<int>();
    for (var position = 0; position < format.Length; position++)
    {
        if (format[position] != '{')
        {
            continue;
        }

        if (position + 1 < format.Length && format[position + 1] == '{')
        {
            position += 1;
            continue;
        }

        var closing = format.IndexOf('}', position + 1);
        AssertTrue(closing > position);
        indices.Add(int.Parse(format[(position + 1)..closing], System.Globalization.CultureInfo.InvariantCulture));
        position = closing;
    }

    return indices;
}

static void AssertTrue(bool value)
{
    if (!value)
    {
        throw new InvalidOperationException("Expected true.");
    }
}

static void AssertFalse(bool value)
{
    if (value)
    {
        throw new InvalidOperationException("Expected false.");
    }
}

static void AssertEqual<T>(T expected, T actual)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"Expected '{expected}', got '{actual}'.");
    }
}

static void AssertCredentialLoadFails(BrowserStorageSyncCredentialStore store)
{
    try
    {
        store.Load();
    }
    catch (InvalidOperationException)
    {
        return;
    }

    throw new InvalidOperationException(
        "Expected loading a corrupt browser sync credential file to throw.");
}

static string CreateTempPath()
{
    return Path.Combine(Path.GetTempPath(), $"actro-windows-tests-{Guid.NewGuid():N}.json");
}

static string CreateTempDatabasePath()
{
    return Path.Combine(Path.GetTempPath(), $"actro-windows-tests-{Guid.NewGuid():N}.db");
}

static void DeleteBrowserStoreFiles(string legacyFilePath)
{
    File.Delete(legacyFilePath);
    var databasePath = BrowserStorageDatabase.GetDatabasePathForLegacyFile(legacyFilePath);
    DeleteDatabaseFiles(databasePath);
}

static void DeleteDatabaseFiles(string databasePath)
{
    // Microsoft.Data.Sqlite pools connections by default; on Windows the
    // pooled handles keep the database file locked, so deleting a temp
    // database without draining the pool throws IOException.
    SqliteConnection.ClearAllPools();
    File.Delete(databasePath);
    File.Delete($"{databasePath}-shm");
    File.Delete($"{databasePath}-wal");
}

static string CreateTempDirectoryPath()
{
    return Path.Combine(Path.GetTempPath(), $"actro-windows-tests-{Guid.NewGuid():N}");
}

static void CreateLegacyBrowserStorageDatabase(string databasePath, int schemaVersion)
{
    using var connection = new SqliteConnection(new SqliteConnectionStringBuilder
    {
        DataSource = databasePath,
        Mode = SqliteOpenMode.ReadWriteCreate,
    }.ToString());
    connection.Open();

    var syncColumns = schemaVersion >= 3
        ? """
          ,
              sync_state TEXT NOT NULL DEFAULT 'local',
              sync_revision INTEGER NOT NULL DEFAULT 0,
              origin_device_id TEXT NOT NULL DEFAULT '',
              last_synced_at INTEGER,
              deleted_at INTEGER
          """
        : string.Empty;
    var tabIdColumn = schemaVersion >= 2
        ? "tab_id TEXT NOT NULL DEFAULT '',"
        : string.Empty;

    ExecuteSql(connection, "PRAGMA foreign_keys = OFF");
    ExecuteSql(connection, """
        CREATE TABLE browser_schema_meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        )
        """);
    ExecuteSql(connection, $"INSERT INTO browser_schema_meta (key, value) VALUES ('browser_storage_schema_version', '{schemaVersion}')");

    ExecuteSql(connection, $"""
        CREATE TABLE browser_history_pages (
            address TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            visit_count INTEGER NOT NULL DEFAULT 1,
            last_visited_at INTEGER NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            favicon TEXT{syncColumns}
        )
        """);
    ExecuteSql(connection, $"""
        CREATE TABLE browser_history_visits (
            id TEXT PRIMARY KEY,
            address TEXT NOT NULL,
            title TEXT NOT NULL,
            visited_at INTEGER NOT NULL,
            transition TEXT{syncColumns}
        )
        """);
    ExecuteSql(connection, $"""
        CREATE TABLE browser_bookmark_folders (
            id TEXT PRIMARY KEY,
            parent_id TEXT,
            title TEXT NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL{syncColumns}
        )
        """);
    ExecuteSql(connection, $"""
        CREATE TABLE browser_bookmarks (
            id TEXT PRIMARY KEY,
            folder_id TEXT,
            title TEXT NOT NULL,
            address TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL,
            sort_order INTEGER NOT NULL DEFAULT 0,
            favicon TEXT,
            is_deleted INTEGER NOT NULL DEFAULT 0{syncColumns}
        )
        """);
    ExecuteSql(connection, $"""
        CREATE TABLE browser_sessions (
            id TEXT PRIMARY KEY,
            active_tab_index INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL{syncColumns}
        )
        """);
    ExecuteSql(connection, $"""
        CREATE TABLE browser_session_tabs (
            session_id TEXT NOT NULL,
            tab_index INTEGER NOT NULL,
            {tabIdColumn}
            title TEXT NOT NULL,
            address TEXT NOT NULL,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL{syncColumns},
            PRIMARY KEY (session_id, tab_index)
        )
        """);

    ExecuteSql(connection, $"""
        INSERT INTO browser_history_pages (address, title, visit_count, last_visited_at, created_at, updated_at, favicon)
        VALUES ('https://example.com/history-v{schemaVersion}', 'History v{schemaVersion}', 2, {100 + schemaVersion}, {90 + schemaVersion}, {100 + schemaVersion}, NULL)
        """);
    ExecuteSql(connection, $"""
        INSERT INTO browser_history_visits (id, address, title, visited_at, transition)
        VALUES ('visit-v{schemaVersion}', 'https://example.com/history-v{schemaVersion}', 'History v{schemaVersion}', {100 + schemaVersion}, 'typed')
        """);
    ExecuteSql(connection, $"""
        INSERT INTO browser_bookmarks (id, folder_id, title, address, created_at, updated_at, sort_order, favicon, is_deleted)
        VALUES ('bookmark-v{schemaVersion}', NULL, 'Bookmark v{schemaVersion}', 'https://example.com/bookmark-v{schemaVersion}', {200 + schemaVersion}, {200 + schemaVersion}, 0, NULL, 0)
        """);
    ExecuteSql(connection, $"""
        INSERT INTO browser_sessions (id, active_tab_index, created_at, updated_at)
        VALUES ('default', 0, {300 + schemaVersion}, {300 + schemaVersion})
        """);

    if (schemaVersion >= 2)
    {
        ExecuteSql(connection, $"""
            INSERT INTO browser_session_tabs (session_id, tab_index, tab_id, title, address, created_at, updated_at)
            VALUES ('default', 0, 'tab-v{schemaVersion}', 'Session v{schemaVersion}', 'https://example.com/session-v{schemaVersion}', {300 + schemaVersion}, {300 + schemaVersion})
            """);
    }
    else
    {
        ExecuteSql(connection, $"""
            INSERT INTO browser_session_tabs (session_id, tab_index, title, address, created_at, updated_at)
            VALUES ('default', 0, 'Session v{schemaVersion}', 'https://example.com/session-v{schemaVersion}', {300 + schemaVersion}, {300 + schemaVersion})
            """);
    }
}

static void AssertBrowserStorageV6Shape(string databasePath, int legacyVersion)
{
    AssertEqual("6", QueryScalarString(databasePath, "SELECT value FROM browser_schema_meta WHERE key = 'browser_storage_schema_version'"));
    AssertTrue(TableExists(databasePath, "browser_sync_cursors"));
    AssertTrue(IndexExists(databasePath, "idx_browser_sync_cursors_updated"));
    AssertTrue(IndexExists(databasePath, "idx_browser_session_tabs_tab_id"));

    foreach (var table in new[]
             {
                 "browser_history_pages",
                 "browser_history_visits",
                 "browser_bookmark_folders",
                 "browser_bookmarks",
                 "browser_sessions",
                 "browser_session_tabs",
             })
    {
        AssertTrue(ColumnExists(databasePath, table, "sync_state"));
        AssertTrue(ColumnExists(databasePath, table, "sync_revision"));
        AssertTrue(ColumnExists(databasePath, table, "origin_device_id"));
        AssertTrue(ColumnExists(databasePath, table, "last_synced_at"));
        AssertTrue(ColumnExists(databasePath, table, "deleted_at"));
    }

    AssertTrue(ColumnExists(databasePath, "browser_session_tabs", "tab_id"));
    AssertTrue(ColumnExists(databasePath, "browser_bookmarks", "owner_kind"));
    AssertTrue(ColumnExists(databasePath, "browser_bookmarks", "owner_id"));
    AssertTrue(ColumnExists(databasePath, "browser_sessions", "owner_kind"));
    AssertTrue(ColumnExists(databasePath, "browser_sessions", "owner_id"));
    AssertTrue(ColumnExists(databasePath, "browser_sessions", "surface_kind"));
    AssertTrue(ColumnExists(databasePath, "browser_sessions", "surface_id"));
    AssertEqual(
        "user",
        QueryScalarString(databasePath, $"SELECT owner_kind FROM browser_bookmarks WHERE id = 'bookmark-v{legacyVersion}'"));
    AssertEqual(
        string.Empty,
        QueryScalarString(databasePath, $"SELECT owner_id FROM browser_bookmarks WHERE id = 'bookmark-v{legacyVersion}'"));
    AssertEqual(
        "device",
        QueryScalarString(databasePath, "SELECT owner_kind FROM browser_sessions WHERE id = 'default'"));
    AssertEqual(
        string.Empty,
        QueryScalarString(databasePath, "SELECT owner_id FROM browser_sessions WHERE id = 'default'"));
    AssertEqual(
        "browser",
        QueryScalarString(databasePath, "SELECT surface_kind FROM browser_sessions WHERE id = 'default'"));
    AssertEqual(
        "default",
        QueryScalarString(databasePath, "SELECT surface_id FROM browser_sessions WHERE id = 'default'"));
}

static bool BrowserSyncBatchHasRecord(JsonDocument batch, string recordType, string? recordId = null)
{
    if (!batch.RootElement.TryGetProperty("records", out var records))
    {
        return false;
    }

    foreach (var record in records.EnumerateArray())
    {
        if (!string.Equals(record.GetProperty("recordType").GetString(), recordType, StringComparison.Ordinal))
        {
            continue;
        }

        if (recordId is null || string.Equals(record.GetProperty("recordId").GetString(), recordId, StringComparison.Ordinal))
        {
            return true;
        }
    }

    return false;
}

static bool BrowserSyncBatchHasBookmarkOwner(JsonDocument batch, string recordId, string ownerKind, string ownerId)
{
    if (!batch.RootElement.TryGetProperty("records", out var records))
    {
        return false;
    }

    foreach (var record in records.EnumerateArray())
    {
        if (!string.Equals(record.GetProperty("recordType").GetString(), "bookmark", StringComparison.Ordinal)
            || !string.Equals(record.GetProperty("recordId").GetString(), recordId, StringComparison.Ordinal)
            || !record.TryGetProperty("payload", out var payload))
        {
            continue;
        }

        return string.Equals(payload.GetProperty("ownerKind").GetString(), ownerKind, StringComparison.Ordinal)
            && string.Equals(payload.GetProperty("ownerId").GetString(), ownerId, StringComparison.Ordinal);
    }

    return false;
}

static bool BrowserSyncBatchHasDeletedBookmark(JsonDocument batch, string recordId)
{
    if (!batch.RootElement.TryGetProperty("records", out var records))
    {
        return false;
    }

    foreach (var record in records.EnumerateArray())
    {
        if (!string.Equals(record.GetProperty("recordType").GetString(), "bookmark", StringComparison.Ordinal)
            || !string.Equals(record.GetProperty("recordId").GetString(), recordId, StringComparison.Ordinal))
        {
            continue;
        }

        return record.GetProperty("deletedAtUnixSeconds").ValueKind == JsonValueKind.Number
            && record.GetProperty("payload").GetProperty("isDeleted").GetBoolean();
    }

    return false;
}

static bool TableExists(string databasePath, string table)
{
    return string.Equals(
        "1",
        QueryScalarString(
            databasePath,
            $"SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = '{table}' LIMIT 1"),
        StringComparison.Ordinal);
}

static bool IndexExists(string databasePath, string index)
{
    return string.Equals(
        "1",
        QueryScalarString(
            databasePath,
            $"SELECT 1 FROM sqlite_master WHERE type = 'index' AND name = '{index}' LIMIT 1"),
        StringComparison.Ordinal);
}

static bool ColumnExists(string databasePath, string table, string column)
{
    using var connection = OpenSqlite(databasePath);
    using var command = connection.CreateCommand();
    command.CommandText = $"PRAGMA table_info({table})";
    using var reader = command.ExecuteReader();
    while (reader.Read())
    {
        if (string.Equals(reader.GetString(1), column, StringComparison.OrdinalIgnoreCase))
        {
            return true;
        }
    }

    return false;
}

static string? QueryScalarString(string databasePath, string sql)
{
    using var connection = OpenSqlite(databasePath);
    using var command = connection.CreateCommand();
    command.CommandText = sql;
    return Convert.ToString(command.ExecuteScalar());
}

static SqliteConnection OpenSqlite(string databasePath)
{
    var connection = new SqliteConnection(new SqliteConnectionStringBuilder
    {
        DataSource = databasePath,
        Mode = SqliteOpenMode.ReadWrite,
    }.ToString());
    connection.Open();
    return connection;
}

static void ExecuteSql(SqliteConnection connection, string sql)
{
    using var command = connection.CreateCommand();
    command.CommandText = sql;
    command.ExecuteNonQuery();
}

sealed class FakeBrowserStorageSyncTransport : IBrowserStorageSyncTransport
{
    public FakeBrowserStorageSyncTransport(string responseJson, int statusCode = 200)
    {
        ResponseJson = responseJson;
        StatusCode = statusCode;
    }

    public string ResponseJson { get; set; }
    public int StatusCode { get; set; }
    public BrowserStorageSyncHttpRequest? LastRequest { get; private set; }

    public Task<BrowserStorageSyncHttpResponse> SendAsync(
        BrowserStorageSyncHttpRequest request,
        CancellationToken cancellationToken = default)
    {
        LastRequest = request;
        return Task.FromResult(new BrowserStorageSyncHttpResponse(StatusCode, ResponseJson));
    }
}

sealed class FakeBrowserStorageSyncCredentialsProvider(
    BrowserStorageSyncCredentials? credentials) : IBrowserStorageSyncCredentialsProvider
{
    public Task<BrowserStorageSyncCredentials?> GetCredentialsAsync(
        CancellationToken cancellationToken = default)
    {
        return Task.FromResult(credentials);
    }
}
