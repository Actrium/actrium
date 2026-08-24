//
//  WebViewTabView.swift
//  Actrium
//
//  WKWebView Encapsulate for display Web Contents
//  System Auto-Managing WebContent Process, no manual process pool
//

import SwiftUI
import WebKit
import Combine

// MARK: - WebView Tab View

/// SwiftUI wrapper for WKWebView
/// For displaying http/https Web content
struct WebViewTabView: NSViewRepresentable {
    let url: URL
    let containerSize: CGSize

    @Binding var title: String
    @Binding var isLoading: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()

        // Enable Developer Tool
        config.preferences.setValue(true, forKey: "developerExtrasEnabled")
#endif

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.uiDelegate = context.coordinator

        // Allow return gestures
        webView.allowsBackForwardNavigationGestures = true

        // Load URL
        context.coordinator.loadedURL = url
        let request = URLRequest(url: url)
        webView.load(request)

        NSLog("🌐 WebViewTabView: Loading page")

        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        // Only if requested. URL Reload when the real change
        // Use coordinator Track, avoid. webView.url In Loading nil Resulting duplicate load
        if context.coordinator.loadedURL != url {
            context.coordinator.loadedURL = url
            let request = URLRequest(url: url)
            webView.load(request)
            NSLog("🌐 WebViewTabView: URL changed, reloading")
        }
    }

    // MARK: - Coordinator

    class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        var parent: WebViewTabView
        var loadedURL: URL?

        init(_ parent: WebViewTabView) {
            self.parent = parent
        }

        // MARK: - WKNavigationDelegate

        func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
            parent.isLoading = true
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            parent.isLoading = false
            if let title = webView.title, !title.isEmpty {
                parent.title = title
            }
            NSLog("🌐 WebViewTabView: Finished loading")
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            let nsError = error as NSError
            NSLog("🌐 WebViewTabView: Navigation failed (%@/%d)", nsError.domain, nsError.code)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            let nsError = error as NSError
            NSLog("🌐 WebViewTabView: Provisional navigation failed (%@/%d)", nsError.domain, nsError.code)
        }

        // MARK: - WKUIDelegate

        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            // Process target="_blank" Link, Current webView Center Open
            if navigationAction.targetFrame == nil {
                webView.load(navigationAction.request)
            }
            return nil
        }
    }
}

// MARK: - WebView Multi-Process Tab View

/// Web Multi-process content Tab View
/// WKWebView Managed by the system automatically WebContent Process
///
/// Rendering policy:
/// - Overview Mode: In TabPanel Internal display cardSize It's... web Preview
/// - Focus Mode: By FocusedWebOverlay Yes. actrium Independent rendering
///   (Because... scaleEffect Yeah. NSViewRepresentable Not effective)
struct WebViewMultiProcessTabView: View {
    let tab: Tab
    let url: URL
    let containerSize: CGSize
    var isFocused: Bool = false

    @State private var webTitle: String = ""
    @State private var isLoading: Bool = true

    var body: some View {
        ZStack {
            // Overview Mode: Show screenshots or placeholders
            Color(nsColor: .windowBackgroundColor)

            if !isFocused {
                // Try to show cached snapshot first
                if let snapshot = WebViewHolderManager.shared.snapshot(for: tab.id) {
                    Image(nsImage: snapshot)
                        .resizable()
                        .aspectRatio(contentMode: .fill)
                        .frame(width: containerSize.width, height: containerSize.height)
                        .clipped()
                } else {
                    // Fallback to placeholder if no snapshot available
                    VStack {
                        Image(systemName: "globe")
                            .font(.system(size: 40))
                            .foregroundColor(.secondary)
                        Text(url.host ?? url.absoluteString)
                            .font(.caption)
                            .foregroundColor(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
        .frame(width: containerSize.width, height: containerSize.height)
    }
}

// MARK: - Autofill Popover Helpers (none needed — uses NSPopover directly)

// MARK: - Shared Process Pool

/// Shared WKProcessPool♪ ♪ ♪ ♪ To ensure that all ♪ WebView Use same WebContent Process
private let sharedProcessPool = WKProcessPool()

/// Browser identity appended to WebKit's native User-Agent.
///
/// Actro intentionally keeps WebKit's platform and engine identity instead of
/// impersonating a particular Safari release. This keeps HTTP and JavaScript
/// identity aligned as WebKit evolves with the operating system.
struct BrowserUserAgentIdentity {
    private static let productName = "Actro"

    static func applicationName(version: String?) -> String {
        let rawVersion = version?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let safeVersion = rawVersion.unicodeScalars
            .filter { $0.value < 128 && (CharacterSet.alphanumerics.contains($0) || "._-".unicodeScalars.contains($0)) }
            .map(String.init)
            .joined()
        return "\(productName)/\(safeVersion.isEmpty ? "0" : safeVersion)"
    }

    static var currentApplicationName: String {
        let info = Bundle.main.infoDictionary
        let version = info?["CFBundleShortVersionString"] as? String
            ?? info?["CFBundleVersion"] as? String
        return applicationName(version: version)
    }
}

// MARK: - Persistent WebView Holder

/// Enduring WKWebView Examples, avoid SwiftUI Reloading of View Rebuild
/// Note: Not used @Published I'm gonna follow you. isLoadingAvoid SwiftUI Update Cycle
final class WebViewHolder: NSObject, WKNavigationDelegate {
    private static let browserSupportContentWorld = WKContentWorld.world(
        name: "io.actrium.actro.browser-support"
    )

    let webView: WKWebView
    private var navigationLoadState = WebNavigationLoadState()
    /// Tracks the last URL seen by KVO (avoids using change.oldValue which can dangle)
    private var lastKnownURL: URL?

    /// Callback when navigation finishes with title and URL
    var onNavigationFinished: ((String?, URL?) -> Void)?

    /// Callback when URL changes (including History.pushState/replaceState)
    var onURLChanged: ((URL?) -> Void)?

    /// Callback when loading state changes
    var onLoadingStateChanged: ((Bool) -> Void)?

    /// Callback when a new window should be opened (target="_blank")
    var onOpenNewWindow: ((URL) -> Void)?

    /// Callback when theme color changes
    var onThemeColorChanged: ((String) -> Void)?

    /// Callback when estimated progress changes
    var onProgressChanged: ((Double) -> Void)?

    /// Current loading state
    private(set) var isLoading: Bool = false

    /// Terminal page failures update only on navigation failure/recovery, so
    /// observing this value does not create the loading-state update loop that
    /// the persistent WebView intentionally avoids.
    @Published private(set) var pageFailure: WebPageFailure? = nil

    /// Cached snapshot for Overview mode
    private(set) var snapshot: NSImage?

    /// KVO observation for URL changes
    private var urlObservation: NSKeyValueObservation?

    /// KVO observation for loading progress
    private var progressObservation: NSKeyValueObservation?

    /// Autofill popover for credential suggestions
    private var autofillPopover: NSPopover?

    /// Actrium User-Agent string
    /// Format follows standard browser convention: Mozilla/5.0 (Platform) Engine/Version (Compatibility) Browser/Version Engine/Version
    private static let userAgent: String = {
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        let osVersion = ProcessInfo.processInfo.operatingSystemVersion
        let osVersionString = "\(osVersion.majorVersion)_\(osVersion.minorVersion)_\(osVersion.patchVersion)"
        // Match Safari's UA to avoid "unsupported browser" warnings from sites like Google
        return "Mozilla/5.0 (Macintosh; Intel Mac OS X \(osVersionString)) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.3 Safari/605.1.15"
    }()

    override init() {
        let config = WKWebViewConfiguration()
        config.processPool = sharedProcessPool
        config.applicationNameForUserAgent = BrowserUserAgentIdentity.currentApplicationName
#if DEBUG
        config.preferences.setValue(true, forKey: "developerExtrasEnabled")
#endif

        // Add script message handler for theme color
        let userContentController = WKUserContentController()
        config.userContentController = userContentController

        // Apply ad-blocking content rules if enabled and pre-compiled
        ContentBlockerManager.shared.apply(to: config)

        // Use custom DownloadableWebView for context menu download support
        self.webView = DownloadableWebView(frame: .zero, configuration: config)
        super.init()

        // Register self as message handler
        userContentController.add(self, name: "actriumThemeColor")
        userContentController.add(self, name: "actriumLog")
        userContentController.add(self, name: "actriumPasswordCapture")
        userContentController.add(self, name: "actriumAutofillSuggestion")
        userContentController.add(self, name: "actriumAutofillDismiss")

        // Inject script to observe theme color
        let script = """
        (function() {
            function log(msg) {
                window.webkit.messageHandlers.actriumLog.postMessage(msg);
            }

            function toHex(color) {
                if (!color) return null;
                if (color.startsWith('#')) return color;
                if (color.startsWith('rgb')) {
                    var parts = color.match(/\\d+/g);
                    if (!parts || parts.length < 3) return null;
                    var r = parseInt(parts[0]).toString(16).padStart(2, '0');
                    var g = parseInt(parts[1]).toString(16).padStart(2, '0');
                    var b = parseInt(parts[2]).toString(16).padStart(2, '0');
                    return '#' + r + g + b;
                }
                return null; // Handle named colors if needed, but computed style usually returns rgb
            }

            function reportColor() {
                var color = null;
                var source = 'none';

                // Priority 1: meta theme-color
                var meta = document.querySelector('meta[name="theme-color"]');
                if (meta) {
                    color = meta.content;
                    source = 'meta';
                }

                // Priority 2: computed body background color
                if (!color) {
                    var style = window.getComputedStyle(document.body);
                    if (style && style.backgroundColor &&
                        style.backgroundColor !== 'rgba(0, 0, 0, 0)' &&
                        style.backgroundColor !== 'transparent') {
                        color = style.backgroundColor;
                        source = 'body';
                    }
                }

                // Priority 3: computed html background color
                if (!color) {
                    var style = window.getComputedStyle(document.documentElement);
                    if (style && style.backgroundColor &&
                        style.backgroundColor !== 'rgba(0, 0, 0, 0)' &&
                        style.backgroundColor !== 'transparent') {
                        color = style.backgroundColor;
                        source = 'html';
                    }
                }

                // Fallback: If no color found, use browser default based on color scheme
                if (!color) {
                    // Check prefers-color-scheme
                    if (window.matchMedia && window.matchMedia('(prefers-color-scheme: dark)').matches) {
                        color = '#1e1e1e'; // Default dark background
                        source = 'fallback-dark';
                    } else {
                        color = '#ffffff'; // Default light background
                        source = 'fallback-light';
                    }
                }

                var hex = toHex(color);
                if (hex) {
                    log('Found color: ' + hex + ' from ' + source + ' (raw: ' + color + ')');
                    window.webkit.messageHandlers.actriumThemeColor.postMessage(hex);
                } else {
                    log('No color found (checked meta, body, html)');
                }
            }

            // Initial check
            if (document.readyState === 'complete') {
                reportColor();
            } else {
                window.addEventListener('load', reportColor);
            }

            // Observe meta tag changes
            var observer = new MutationObserver(function(mutations) {
                reportColor();
            });
            observer.observe(document.head, { subtree: true, childList: true, attributes: true });
        })();
        """
        let userScript = WKUserScript(
            source: script,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true,
            in: Self.browserSupportContentWorld
        )
        userContentController.addUserScript(userScript)

        // Password autofill & capture script
        let autofillScript = """
        (function() {
            function log(msg) {
                window.webkit.messageHandlers.actriumLog.postMessage('[Autofill] ' + msg);
            }
            log('script loaded');

            // --- Autofill injection point ---
            window.__actriumAutofill = function(username, password) {
                var nativeSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
                var pwFields = document.querySelectorAll('input[type="password"]');
                if (pwFields.length === 0) return;
                var pwField = pwFields[0];
                if (username) {
                    var userField = findUsernameField(pwField.form || document);
                    if (userField) {
                        nativeSetter.call(userField, username);
                        userField.dispatchEvent(new Event('input', {bubbles: true}));
                        userField.dispatchEvent(new Event('change', {bubbles: true}));
                    }
                }
                nativeSetter.call(pwField, password);
                pwField.dispatchEvent(new Event('input', {bubbles: true}));
                pwField.dispatchEvent(new Event('change', {bubbles: true}));
                log('credentials filled');
            };

            function findUsernameField(container) {
                var email = container.querySelector('input[type="email"]');
                if (email) return email;
                var inputs = container.querySelectorAll('input[type="text"], input[type="email"], input:not([type])');
                for (var i = 0; i < inputs.length; i++) {
                    var inp = inputs[i];
                    var id = (inp.id || '').toLowerCase();
                    var name = (inp.name || '').toLowerCase();
                    var auto = (inp.autocomplete || '').toLowerCase();
                    if (auto === 'username' || auto === 'email') return inp;
                    if (/user|email|login|account|nick/.test(id + ' ' + name)) return inp;
                }
                var pw = container.querySelector('input[type="password"]');
                if (pw) {
                    var prev = pw.previousElementSibling;
                    while (prev) {
                        if (prev.tagName === 'INPUT' && prev.type !== 'hidden') return prev;
                        prev = prev.previousElementSibling;
                    }
                }
                return null;
            }

            function captureCredentials() {
                var pwFields = document.querySelectorAll('input[type="password"]');
                for (var i = 0; i < pwFields.length; i++) {
                    var pw = pwFields[i];
                    if (!pw.value) continue;
                    var container = pw.form || document;
                    var userField = findUsernameField(container);
                    var username = userField ? userField.value : '';
                    log('capturing credentials for ' + location.hostname + ' user=' + username);
                    window.webkit.messageHandlers.actriumPasswordCapture.postMessage({
                        url: location.href,
                        username: username,
                        password: pw.value
                    });
                    return;
                }
                log('captureCredentials: no filled password field found');
            }

            // Listen for form submit
            document.addEventListener('submit', function(e) {
                log('form submit event');
                if (e.target.querySelector && e.target.querySelector('input[type="password"]')) {
                    captureCredentials();
                }
            }, true);

            // Listen for click on submit buttons (SPA compat)
            document.addEventListener('click', function(e) {
                var btn = e.target.closest(
                    'button[type="submit"], input[type="submit"], button:not([type]), ' +
                    '[role="button"], a.btn, a.button, div[role="button"]'
                );
                if (!btn) return;
                // Check if there's a password field in the same form or nearby
                var form = btn.closest('form');
                var hasPw = form
                    ? form.querySelector('input[type="password"]')
                    : document.querySelector('input[type="password"]');
                if (hasPw) {
                    log('submit-like click detected');
                    setTimeout(captureCredentials, 0);
                }
            }, true);

            // Listen for Enter key on password fields
            document.addEventListener('keydown', function(e) {
                if (e.key !== 'Enter') return;
                var field = e.target;
                if (!field || field.tagName !== 'INPUT') return;
                if (field.type === 'password' || isLoginField(field)) {
                    log('Enter key on login field');
                    setTimeout(captureCredentials, 0);
                }
            }, true);

            // --- Autofill suggestion (Chrome-style) ---
            function isLoginField(field) {
                if (field.type === 'password') return true;
                // Check if there's a password field in the same form or document
                var form = field.form || document;
                if (!form.querySelector('input[type="password"]')) return false;
                // Reuse findUsernameField: if this field IS the username field, it's a login field
                return findUsernameField(form) === field;
            }

            var __dismissTimer = null;

            document.addEventListener('focusin', function(e) {
                var field = e.target;
                if (field.tagName !== 'INPUT') return;
                var login = isLoginField(field);
                log('focusin: type=' + field.type + ' id=' + field.id + ' isLogin=' + login);
                if (!login) return;
                if (__dismissTimer) { clearTimeout(__dismissTimer); __dismissTimer = null; }
                var rect = field.getBoundingClientRect();
                window.webkit.messageHandlers.actriumAutofillSuggestion.postMessage({
                    domain: location.hostname,
                    fieldRect: {
                        x: rect.left,
                        y: rect.top,
                        width: rect.width,
                        height: rect.height
                    }
                });
            }, true);

            document.addEventListener('focusout', function(e) {
                if (__dismissTimer) { clearTimeout(__dismissTimer); __dismissTimer = null; }
                __dismissTimer = setTimeout(function() {
                    window.webkit.messageHandlers.actriumAutofillDismiss.postMessage(null);
                }, 150);
            }, true);

            window.addEventListener('scroll', function() {
                window.webkit.messageHandlers.actriumAutofillDismiss.postMessage(null);
            }, true);

            window.addEventListener('resize', function() {
                window.webkit.messageHandlers.actriumAutofillDismiss.postMessage(null);
            });
        })();
        """
        let autofillUserScript = WKUserScript(
            source: autofillScript,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true,
            in: Self.browserSupportContentWorld
        )
        userContentController.addUserScript(autofillUserScript)

        webView.allowsBackForwardNavigationGestures = true
        webView.navigationDelegate = self
#if DEBUG
        webView.isInspectable = true  // Enable Web Inspector
#endif

        // Observe URL changes (including History.pushState/replaceState)
        // WORKAROUND: On macOS 26 (Tahoe), webView.url can return an NSURL whose internal
        // string buffer points to freed WebKit memory (GPU Carveout reserved region), causing
        // EXC_BAD_ACCESS in _platform_strlen when accessing .absoluteString.
        // We avoid webView.url entirely and use evaluateJavaScript("location.href") instead,
        // which returns a fresh JS string with no NSURL bridging issues.
        urlObservation = webView.observe(\.url) { [weak self] _, _ in
            // Defer everything — don't touch WebKit state during KVO notification
            DispatchQueue.main.async { [weak self] in
                guard let self = self else { return }
                self.webView.evaluateJavaScript("location.href") { [weak self] result, _ in
                    guard let self = self,
                          let urlString = result as? String,
                          let newURL = URL(string: urlString) else { return }
                    guard self.navigationLoadState.observeWebViewURL(newURL) else { return }
                    if self.lastKnownURL != newURL {
                        NSLog("🔗 WebViewHolder: URL changed")
                        self.lastKnownURL = newURL
                        self.onURLChanged?(newURL)
                        self.dismissAutofillPopover()
                    }
                }
            }
        }

        // Observe estimated progress for loading bar
        progressObservation = webView.observe(\.estimatedProgress) { [weak self] webView, _ in
            DispatchQueue.main.async { [weak self] in
                guard let self, !self.navigationLoadState.isInvalidated else { return }
                self.onProgressChanged?(webView.estimatedProgress)
            }
        }
    }

    deinit {
        urlObservation?.invalidate()
        progressObservation?.invalidate()
        dismissAutofillPopover()
    }

    func loadIfNeeded(_ url: URL) {
        // Just... URL Reload when the real change
        if loadedURL != url {
            loadedURL = url
            let request = URLRequest(url: url)
            webView.load(request)
        }
        contentController.removeAllUserScripts()

        dismissAutofillPopover()
    }

    /// Attach the persistent WebView for the first time. Reattaching an
    /// already-loaded Tab intentionally does nothing.
    func loadInitialURLIfNeeded(_ url: URL) {
        guard let requestURL = navigationLoadState.initialRequest(for: url) else { return }
        pageFailure = nil
        webView.load(URLRequest(url: requestURL))
    }

    /// Navigate only for a model-originated URL change while this Tab is
    /// mounted. WebKit-originated changes are recorded by the URL observer and
    /// therefore do not produce a second request here.
    func navigateIfNeeded(to url: URL) {
        guard let requestURL = navigationLoadState.navigationRequest(for: url) else { return }
        pageFailure = nil
        webView.load(URLRequest(url: requestURL))
    }

    // MARK: - Navigation

    var canGoBack: Bool { webView.canGoBack }
    var canGoForward: Bool { webView.canGoForward }
    var currentURL: URL? { lastKnownURL }
    var currentTitle: String? { webView.title }

    func goBack() {
        guard !navigationLoadState.isInvalidated else { return }
        webView.goBack()
    }

    func goForward() {
        guard !navigationLoadState.isInvalidated else { return }
        webView.goForward()
    }

    func reload() {
        guard !navigationLoadState.isInvalidated else { return }
        pageFailure = nil
        webView.reload()
    }

    func retryAfterFailure() {
        guard !navigationLoadState.isInvalidated else { return }
        let retryURL = pageFailure?.retryURL ?? lastKnownURL ?? navigationLoadState.currentURL
        pageFailure = nil
        if let retryURL {
            webView.load(URLRequest(url: retryURL))
        } else {
            webView.reload()
        }
    }

    func stopLoading() {
        guard !navigationLoadState.isInvalidated else { return }
        webView.stopLoading()
    }

    // MARK: - Web Inspector

    /// Show WebKit Web Inspector for this webView
    func showInspector() {
#if DEBUG
        // Use private API to open Web Inspector
        // WKWebView has a private _inspector property that returns WKInspector
        guard let inspector = webView.value(forKey: "_inspector") as? NSObject else {
            NSLog("⚠️ WebViewHolder: Could not get _inspector")
            return
        }
        inspector.perform(Selector(("show")))
        NSLog("🔧 WebViewHolder: Opened Web Inspector")
#else
        NSLog("🔧 WebViewHolder: Web Inspector is available only in debug builds")
#endif
    }

    // MARK: - Print

    /// Print the current web page
    func print() {
        // WKWebView requires the print operation to be run on the main thread
        // and the view must have a proper window context
        guard let window = webView.window else {
            NSLog("🖨️ WebViewHolder: Cannot print - webView has no window")
            return
        }

        let printInfo = NSPrintInfo.shared.copy() as! NSPrintInfo
        printInfo.horizontalPagination = .fit
        printInfo.verticalPagination = .automatic
        printInfo.isHorizontallyCentered = true
        printInfo.isVerticallyCentered = true

        let printOperation = webView.printOperation(with: printInfo)
        printOperation.showsPrintPanel = true
        printOperation.showsProgressPanel = true

        // Run the print operation attached to the window to avoid the frame initialization issue
        printOperation.runModal(for: window, delegate: nil, didRun: nil, contextInfo: nil)
    }

    // MARK: - Zoom

    /// Current page zoom level (1.0 = 100%)
    var zoomLevel: Double {
        get { webView.pageZoom }
        set { webView.pageZoom = newValue }
    }

    /// Zoom in (increase by 10%)
    func zoomIn() {
        let newZoom = min(webView.pageZoom + 0.1, 3.0)  // Max 300%
        webView.pageZoom = newZoom
    }

    /// Zoom out (decrease by 10%)
    func zoomOut() {
        let newZoom = max(webView.pageZoom - 0.1, 0.5)  // Min 50%
        webView.pageZoom = newZoom
    }

    /// Reset zoom to 100%
    func resetZoom() {
        webView.pageZoom = 1.0
    }

    // MARK: - Snapshot

    /// Take a snapshot of the current web content for Overview mode
    @MainActor
    func takeSnapshot() async {
        // Only take snapshot if webView has content
        guard !navigationLoadState.isInvalidated, lastKnownURL != nil else { return }

        do {
            let config = WKSnapshotConfiguration()
            // Use the webView's current size for the snapshot
            let image = try await webView.takeSnapshot(configuration: config)
            guard !navigationLoadState.isInvalidated else { return }
            self.snapshot = image
            NSLog("📸 WebViewHolder: Snapshot captured")
        } catch {
            let nsError = error as NSError
            NSLog("📸 WebViewHolder: Snapshot failed (%@/%d)", nsError.domain, nsError.code)
        }
    }

    // MARK: - Autofill Popover

    /// Convert JS field rect to NSView rect and show an NSPopover below it
    private func showPopover(fieldRect: CGRect, contentVC: NSViewController) {
        dismissAutofillPopover()
        guard webView.window != nil else { return }

        let zoom = webView.pageZoom

        // JS getBoundingClientRect() uses top-left origin (y downward).
        // WKWebView.isFlipped == true → same coordinate system, direct mapping.
        // WKWebView.isFlipped == false → need to flip y.
        let nsRect: NSRect
        let edge: NSRectEdge
        if webView.isFlipped {
            nsRect = NSRect(
                x: fieldRect.origin.x * zoom,
                y: fieldRect.origin.y * zoom,
                width: fieldRect.width * zoom,
                height: fieldRect.height * zoom
            )
            edge = .maxY  // maxY = bottom edge in flipped coords → popover below
        } else {
            let viewBounds = webView.bounds
            nsRect = NSRect(
                x: fieldRect.origin.x * zoom,
                y: viewBounds.height - (fieldRect.origin.y + fieldRect.height) * zoom,
                width: fieldRect.width * zoom,
                height: fieldRect.height * zoom
            )
            edge = .minY  // minY = bottom edge in non-flipped coords → popover below
        }

        let popover = NSPopover()
        popover.behavior = .semitransient
        popover.contentViewController = contentVC
        popover.show(relativeTo: nsRect, of: webView, preferredEdge: edge)
        self.autofillPopover = popover
    }

    func showAutofillPopover(credentials: [(VaultItem, String, String)], fieldRect: CGRect) {
        let items = credentials.map { (item, username, password) in
            AutofillSuggestionItem(
                username: username,
                domain: item.title,
                password: password
            )
        }
        guard !items.isEmpty else { return }

        let vc = NSHostingController(
            rootView: AutofillSuggestionView(items: items) { [weak self] selected in
                self?.fillCredential(username: selected.username, password: selected.password)
                self?.dismissAutofillPopover()
            } onManagePasswords: { [weak self] in
                self?.dismissAutofillPopover()
                Task { @MainActor in
                    guard let appDelegate = NSApp.delegate as? AppDelegate else { return }
                    _ = appDelegate.focusedViewModel?.openSystemTab(.settings)
                }
            }
        )
        showPopover(fieldRect: fieldRect, contentVC: vc)
    }

    func showVaultLockedPopover(fieldRect: CGRect, pageURL: URL) {
        let vc = NSHostingController(
            rootView: VaultLockedPromptView { [weak self] in
                guard let self = self else { return }
                let credentials = AutofillManager.shared.findCredentials(for: pageURL)
                if credentials.count == 1 {
                    NSLog("🔑 onUnlocked: single match, filling directly")
                    self.fillCredential(username: credentials[0].1, password: credentials[0].2)
                    self.dismissAutofillPopover()
                } else if !credentials.isEmpty {
                    NSLog("🔑 onUnlocked: multiple matches, showing picker")
                    self.showAutofillPopover(credentials: credentials, fieldRect: fieldRect)
                } else {
                    NSLog("🔑 onUnlocked: no credentials, dismissing")
                    self.dismissAutofillPopover()
                }
            }
        )
        showPopover(fieldRect: fieldRect, contentVC: vc)
    }

    func dismissAutofillPopover() {
        autofillPopover?.performClose(nil)
        autofillPopover = nil
    }

    private func fillCredential(username: String, password: String) {
        let safeUser = username
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "'", with: "\\'")
        let safePass = password
            .replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "'", with: "\\'")
        let js = "window.__actriumAutofill('\(safeUser)', '\(safePass)')"
        NSLog("🔑 fillCredential: evaluating JS for user=%@", safeUser)
        webView.evaluateJavaScript(js) { result, error in
            if let error = error {
                NSLog("🔑 fillCredential: JS error - %@", error.localizedDescription)
            } else {
                NSLog("🔑 fillCredential: JS succeeded, result=%@", "\(result ?? "nil")")
            }
        }
    }

    // MARK: - WKNavigationDelegate

    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
        guard !navigationLoadState.isInvalidated else { return }
        pageFailure = nil
        isLoading = true
        onLoadingStateChanged?(true)
        dismissAutofillPopover()
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        guard !navigationLoadState.isInvalidated else { return }
        pageFailure = nil
        isLoading = false
        onLoadingStateChanged?(false)
        // Notify with title and URL when navigation completes
        // Avoid accessing webView.url directly — it can return a corrupted NSURL on macOS 26.
        // Use evaluateJavaScript to safely retrieve the URL string.
        let title = webView.title
        webView.evaluateJavaScript("location.href") { [weak self] result, _ in
            guard let self, !self.navigationLoadState.isInvalidated else { return }
            let url: URL?
            if let urlString = result as? String, let parsed = URL(string: urlString) {
                self.lastKnownURL = parsed
                url = parsed
            } else {
                url = self.lastKnownURL
            }
            self.onNavigationFinished?(title, url)
        }

        // Safari-style: Take snapshot immediately after page load for Overview mode
        // Use a short delay to allow page rendering to complete
        Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 300_000_000) // 0.3s delay
            await self?.takeSnapshot()
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        guard !navigationLoadState.isInvalidated else { return }
        isLoading = false
        onLoadingStateChanged?(false)
        pageFailure = WebPageFailure.navigation(
            error: error,
            retryURL: lastKnownURL ?? navigationLoadState.currentURL
        )
        let nsError = error as NSError
        NSLog("🌐 WebViewHolder: Navigation failed (%@/%d)", nsError.domain, nsError.code)
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        guard !navigationLoadState.isInvalidated else { return }
        isLoading = false
        onLoadingStateChanged?(false)
        let nsError = error as NSError
        if let failure = WebPageFailure.navigation(
            error: error,
            retryURL: navigationLoadState.currentURL ?? lastKnownURL
        ) {
            pageFailure = failure
            NSLog("🌐 WebViewHolder: Provisional navigation failed (%@/%d)", nsError.domain, nsError.code)
        }
    }

    func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
        guard !navigationLoadState.isInvalidated else { return }
        isLoading = false
        onLoadingStateChanged?(false)
        pageFailure = .processTerminated(
            retryURL: lastKnownURL ?? navigationLoadState.currentURL
        )
        NSLog("🌐 WebViewHolder: Web content process terminated")
    }

    // MARK: - Download Detection (Navigation Response)

    /// Detect downloads by checking response headers
    func webView(_ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse, decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void) {
        guard !navigationLoadState.isInvalidated else {
            decisionHandler(.cancel)
            return
        }

        guard let response = navigationResponse.response as? HTTPURLResponse else {
            decisionHandler(.allow)
            return
        }

        let contentDisposition = response.value(forHTTPHeaderField: "Content-Disposition") ?? ""
        let isAttachment = contentDisposition
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
            .hasPrefix("attachment")

        // Any response WebKit cannot render is a download. Restricting this to
        // a handwritten MIME allowlist silently breaks legitimate file types.
        if isAttachment || !navigationResponse.canShowMIMEType {
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Response became a download")
        } else {
            decisionHandler(.allow)
        }
    }

    /// Handle navigation action with download attribute or Option+click
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        guard !navigationLoadState.isInvalidated else {
            decisionHandler(.cancel)
            return
        }

        let url = navigationAction.request.url
        NSLog("📥 WebViewHolder: decidePolicyFor action - shouldDownload: %d, navigationType: %ld",
              navigationAction.shouldPerformDownload ? 1 : 0,
              navigationAction.navigationType.rawValue)

        // Check for download attribute on links
        if navigationAction.shouldPerformDownload {
            guard let url, WebNavigationSecurityPolicy.canDownload(url) else {
                decisionHandler(.cancel)
                NSLog("📥 WebViewHolder: Blocked download from unsupported URL scheme")
                return
            }
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Link has download attribute - returning .download")
            return
        }

        // Safari-style: Option+click to force download any link
        if navigationAction.modifierFlags.contains(.option),
           let url = url,
           url.scheme == "http" || url.scheme == "https" {
            // Keep the request inside WebKit so cookies, authentication and
            // proxy/session state match the page that initiated the download.
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Option+click download")
            return
        }

        if let url, !WebNavigationSecurityPolicy.canNavigateInsideWebView(url) {
            decisionHandler(.cancel)
            NSLog("🌐 WebViewHolder: Blocked unsupported top-level URL scheme")
            return
        }

        decisionHandler(.allow)
    }

    /// Handle navigation response becoming a download
    func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) {
        guard !navigationLoadState.isInvalidated else {
            download.cancel { _ in }
            return
        }
        NSLog("📥 WebViewHolder: navigationResponse became download")
        setupDownload(download, sourceURL: navigationResponse.response.url)
    }

    /// Handle navigation action becoming a download (for links with download attribute)
    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) {
        guard !navigationLoadState.isInvalidated else {
            download.cancel { _ in }
            return
        }
        NSLog("📥 WebViewHolder: navigationAction became download")
        setupDownload(download, sourceURL: navigationAction.request.url)
    }

    /// Setup download with DownloadManager
    private func setupDownload(_ download: WKDownload, sourceURL: URL?) {
        guard !navigationLoadState.isInvalidated else {
            download.cancel { _ in }
            return
        }
        let filename = sourceURL?.lastPathComponent ?? "download"

        Task { @MainActor [weak self] in
            guard let self, !self.navigationLoadState.isInvalidated else {
                _ = await download.cancel()
                return
            }
            let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: sourceURL)
            DownloadManager.shared.adoptWebKitDownload(
                download,
                item: item,
                sourceWebView: self.webView
            )

            NSLog("📥 WebViewHolder: Download started")
        }
    }

}

// MARK: - Global WebView Manager

/// Global Management WebViewHolder, avoid loss of view reconstruction
@MainActor
final class WebViewHolderManager {
    static let shared = WebViewHolderManager()
    private var holders: [UUID: WebViewHolder] = [:]

    private init() {}

    func holder(for tabId: UUID) -> WebViewHolder {
        if let existing = holders[tabId] {
            return existing
        }
        let new = WebViewHolder()
        holders[tabId] = new
        NSLog("🌐 WebViewHolderManager: Created holder for tab \(tabId.uuidString.prefix(8))")
        return new
    }

    /// Get existing holder without creating a new one
    func existingHolder(for tabId: UUID) -> WebViewHolder? {
        holders[tabId]
    }

    func remove(tabId: UUID) {
        guard let holder = holders.removeValue(forKey: tabId) else { return }
        holder.invalidate()
        NSLog("🌐 WebViewHolderManager: Removed holder for tab \(tabId.uuidString.prefix(8))")
    }

    func removeAll() {
        let retainedHolders = Array(holders.values)
        holders.removeAll()
        retainedHolders.forEach { $0.invalidate() }
        NSLog("🌐 WebViewHolderManager: Removed all holders")
    }

    /// Get cached snapshot for a tab
    func snapshot(for tabId: UUID) -> NSImage? {
        holders[tabId]?.snapshot
    }

    /// Take a snapshot for a specific tab
    func takeSnapshot(for tabId: UUID) async {
        await holders[tabId]?.takeSnapshot()
    }
}

// MARK: - Persistent WebView Representable

struct PersistentWebView: NSViewRepresentable {
    let holder: WebViewHolder
    let size: CGSize  // Visible Input Dimensions

    func makeCoordinator() -> Coordinator {
        Coordinator(holder: holder)
    }

    func makeNSView(context: Context) -> NSView {
        // Use container view to ensure the correct layout
        let container = NSView(frame: NSRect(origin: .zero, size: size))
        let webView = holder.webView

        // navigationDelegate Already WebViewHolder Medium Settings
        webView.uiDelegate = context.coordinator
        webView.frame = container.bounds
        webView.autoresizingMask = [.width, .height]

        container.addSubview(webView)
        return container
    }

    func updateNSView(_ container: NSView, context: Context) {
        // Use imported size Not container.bounds
        // Because... SwiftUI It could be temporarily delivered to the container. 0 Size
        guard size.width > 0 && size.height > 0 else { return }

        // Ensure that containers and webView They all have the right things. frame
        let targetFrame = NSRect(origin: .zero, size: size)
        if container.frame != targetFrame {
            container.frame = targetFrame
        }

        if let webView = container.subviews.first as? WKWebView,
           webView.frame != targetFrame {
            webView.frame = targetFrame
        }
    }

    // Coordinator Process UIDelegate(target=_blank Link, Download)
    class Coordinator: NSObject, WKUIDelegate, WKDownloadDelegate {
        let holder: WebViewHolder
        private var isPresentingPageModal = false
        private var activeAlert: NSAlert?

        init(holder: WebViewHolder) {
            self.holder = holder
        }

        // Process target="_blank" Link - Open in New Window
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if navigationAction.targetFrame == nil,
               let url = navigationAction.request.url {
                // Open in a new window if there is a back
                if let onOpenNewWindow = holder.onOpenNewWindow {
                    onOpenNewWindow(url)
                } else {
                    holder.webView.load(navigationAction.request)
                }
            }
            return nil
        }

        // MARK: - Media Capture Permission

        func webView(
            _ webView: WKWebView,
            requestMediaCapturePermissionFor origin: WKSecurityOrigin,
            initiatedByFrame frame: WKFrameInfo,
            type: WKMediaCaptureType,
            decisionHandler: @escaping (WKPermissionDecision) -> Void
        ) {
            guard WebMediaPermissionPolicy.canRequestCapture(
                scheme: origin.protocol,
                host: origin.host
            ), let pageURLAtRequest = webView.url,
               !isPresentingPageModal else {
                decisionHandler(.deny)
                return
            }

            let domain = origin.host
            let permissionLabel: String
            switch type {
            case .camera:
                permissionLabel = "camera"
            case .microphone:
                permissionLabel = "microphone"
            case .cameraAndMicrophone:
                permissionLabel = "camera and microphone"
            @unknown default:
                permissionLabel = "media device"
            }

            isPresentingPageModal = true
            DispatchQueue.main.async { [weak self, weak webView] in
                guard let self,
                      let webView,
                      let window = webView.window else {
                    self?.isPresentingPageModal = false
                    decisionHandler(.deny)
                    return
                }

                let alert = NSAlert()
                alert.messageText = "\"\(domain)\" Would Like to Access Your \(permissionLabel.capitalized)"
                alert.informativeText = "This website is requesting permission to use your \(permissionLabel)."
                alert.alertStyle = .informational
                alert.addButton(withTitle: "Allow")
                alert.addButton(withTitle: "Deny")
                self.activeAlert = alert

                alert.beginSheetModal(for: window) { [weak self, weak webView] response in
                    self?.activeAlert = nil
                    self?.isPresentingPageModal = false
                    guard let self,
                          let webView,
                          webView.window != nil,
                          webView.url == pageURLAtRequest,
                          self.holder.webView === webView else {
                        decisionHandler(.deny)
                        return
                    }
                    decisionHandler(response == .alertFirstButtonReturn ? .grant : .deny)
                }
            }
        }

        // MARK: - JavaScript Alert / Confirm / Prompt

        func webView(
            _ webView: WKWebView,
            runJavaScriptAlertPanelWithMessage message: String,
            initiatedByFrame frame: WKFrameInfo,
            completionHandler: @escaping () -> Void
        ) {
            guard !isPresentingPageModal else {
                completionHandler()
                return
            }
            isPresentingPageModal = true
            let rawDomain = frame.securityOrigin.host
            let domain = rawDomain.isEmpty ? "This webpage" : rawDomain
            let boundedMessage = WebDialogTextPolicy.bounded(message)
            DispatchQueue.main.async { [weak self, weak webView] in
                guard let self,
                      let webView,
                      let window = webView.window else {
                    self?.isPresentingPageModal = false
                    completionHandler()
                    return
                }
                let alert = NSAlert()
                alert.messageText = domain
                alert.informativeText = boundedMessage
                alert.addButton(withTitle: "OK")
                self.activeAlert = alert
                alert.beginSheetModal(for: window) { [weak self] _ in
                    self?.activeAlert = nil
                    self?.isPresentingPageModal = false
                    completionHandler()
                }
            }
        }

        func webView(
            _ webView: WKWebView,
            runJavaScriptConfirmPanelWithMessage message: String,
            initiatedByFrame frame: WKFrameInfo,
            completionHandler: @escaping (Bool) -> Void
        ) {
            guard !isPresentingPageModal else {
                completionHandler(false)
                return
            }
            isPresentingPageModal = true
            let rawDomain = frame.securityOrigin.host
            let domain = rawDomain.isEmpty ? "This webpage" : rawDomain
            let boundedMessage = WebDialogTextPolicy.bounded(message)
            DispatchQueue.main.async { [weak self, weak webView] in
                guard let self,
                      let webView,
                      let window = webView.window else {
                    self?.isPresentingPageModal = false
                    completionHandler(false)
                    return
                }
                let alert = NSAlert()
                alert.messageText = domain
                alert.informativeText = boundedMessage
                alert.addButton(withTitle: "OK")
                alert.addButton(withTitle: "Cancel")
                self.activeAlert = alert
                alert.beginSheetModal(for: window) { [weak self] response in
                    self?.activeAlert = nil
                    self?.isPresentingPageModal = false
                    completionHandler(response == .alertFirstButtonReturn)
                }
            }
        }

        func webView(
            _ webView: WKWebView,
            runJavaScriptTextInputPanelWithPrompt prompt: String,
            defaultText: String?,
            initiatedByFrame frame: WKFrameInfo,
            completionHandler: @escaping (String?) -> Void
        ) {
            guard !isPresentingPageModal else {
                completionHandler(nil)
                return
            }
            isPresentingPageModal = true
            let rawDomain = frame.securityOrigin.host
            let domain = rawDomain.isEmpty ? "This webpage" : rawDomain
            let boundedPrompt = WebDialogTextPolicy.bounded(prompt)
            let boundedDefault = defaultText.map { WebDialogTextPolicy.bounded($0) }
            DispatchQueue.main.async { [weak self, weak webView] in
                guard let self,
                      let webView,
                      let window = webView.window else {
                    self?.isPresentingPageModal = false
                    completionHandler(nil)
                    return
                }
                let alert = NSAlert()
                alert.messageText = domain
                alert.informativeText = boundedPrompt
                alert.addButton(withTitle: "OK")
                alert.addButton(withTitle: "Cancel")

                let textField = NSTextField(frame: NSRect(x: 0, y: 0, width: 260, height: 24))
                textField.stringValue = boundedDefault ?? ""
                alert.accessoryView = textField
                self.activeAlert = alert
                alert.beginSheetModal(for: window) { [weak self] response in
                    self?.activeAlert = nil
                    self?.isPresentingPageModal = false
                    let value = WebDialogTextPolicy.bounded(textField.stringValue)
                    completionHandler(response == .alertFirstButtonReturn ? value : nil)
                }
            }
        }

    }
}

// MARK: - Custom Menu Item with Closure

/// Menu item that executes a closure when selected
private class ClosureMenuItem: NSMenuItem {
    private var actionClosure: (() -> Void)?

    init(title: String, closure: @escaping () -> Void) {
        self.actionClosure = closure
        super.init(title: title, action: #selector(performAction), keyEquivalent: "")
        self.target = self
    }

    required init(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc private func performAction() {
        actionClosure?()
    }
}

// MARK: - Custom WebView with Context Menu

/// Custom WKWebView subclass that adds download options to context menu
class DownloadableWebView: WKWebView {
    /// Store the detected element info for context menu
    private var contextMenuImageURL: URL?
    private var contextMenuLinkURL: URL?

    /// Pre-detect element on right mouse down using synchronous approach
    override func rightMouseDown(with event: NSEvent) {
        let locationInView = convert(event.locationInWindow, from: nil)

        // Reset previous values
        contextMenuImageURL = nil
        contextMenuLinkURL = nil

        // Detect element synchronously
        detectElementSynchronously(at: locationInView)

        super.rightMouseDown(with: event)
    }

    private func detectElementSynchronously(at point: CGPoint) {
        // Note: point is in NSView coordinates (origin at bottom-left)
        // WebKit's elementFromPoint uses viewport coordinates (origin at top-left)
        // We need to flip Y coordinate
        let flippedY = bounds.height - point.y

        NSLog("📥 detectElement: NSView point=(%f, %f), flipped Y=%f, bounds=%f x %f",
              point.x, point.y, flippedY, bounds.width, bounds.height)

        let js = """
        (function() {
            var x = \(point.x);
            var y = \(flippedY);
            var result = { imageURL: null, linkURL: null, debug: '' };

            // Try elementFromPoint
            var element = document.elementFromPoint(x, y);
            result.debug = 'point=' + x + ',' + y + ' element=' + (element ? element.tagName : 'null');

            if (element) {
                // Check if element itself is an image
                if (element.tagName === 'IMG' && element.src) {
                    result.imageURL = element.src;
                    result.debug += ' [direct IMG]';
                }

                // If not directly on an image, find all images and check which one contains the click point
                if (!result.imageURL) {
                    var allImages = document.querySelectorAll('img');
                    var nearbyImages = [];
                    for (var i = 0; i < allImages.length; i++) {
                        var img = allImages[i];
                        if (!img.src) continue;
                        var rect = img.getBoundingClientRect();
                        // Log images within 200px of click
                        var dx = Math.min(Math.abs(x - rect.left), Math.abs(x - rect.right));
                        var dy = Math.min(Math.abs(y - rect.top), Math.abs(y - rect.bottom));
                        if (rect.width > 0 && rect.height > 0 && (dx < 200 || dy < 200)) {
                            nearbyImages.push(img.src.substring(img.src.lastIndexOf('/') + 1) + '@' + Math.round(rect.left) + ',' + Math.round(rect.top));
                        }
                        // Check if click point is inside this image's bounds
                        if (x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom) {
                            result.imageURL = img.src;
                            result.debug += ' [bounds match: ' + Math.round(rect.left) + ',' + Math.round(rect.top) + '-' + Math.round(rect.right) + ',' + Math.round(rect.bottom) + ']';
                            break;
                        }
                    }
                    if (!result.imageURL && nearbyImages.length > 0) {
                        result.debug += ' [nearby: ' + nearbyImages.slice(0,5).join(', ') + ']';
                    }
                }

                // Check for background image on element or parents
                if (!result.imageURL) {
                    var current = element;
                    while (current && !result.imageURL) {
                        var bgImage = window.getComputedStyle(current).backgroundImage;
                        if (bgImage && bgImage !== 'none' && bgImage.startsWith('url(')) {
                            result.imageURL = bgImage.slice(5, -2).replace(/['"]/g, '');
                            result.debug += ' [background]';
                        }
                        current = current.parentElement;
                    }
                }

                // Check for link
                var linkElement = element.closest ? element.closest('a') : null;
                if (linkElement && linkElement.href) {
                    result.linkURL = linkElement.href;
                }
            }

            return JSON.stringify(result);
        })();
        """

        // Use RunLoop to wait for async JS evaluation
        var completed = false
        var detectedImageURL: URL?
        var detectedLinkURL: URL?

        evaluateJavaScript(js) { result, error in
            if let jsonString = result as? String,
               let data = jsonString.data(using: .utf8),
               let json = try? JSONSerialization.jsonObject(with: data) as? [String: Any] {

                if let imageURLString = json["imageURL"] as? String {
                    detectedImageURL = URL(string: imageURLString)
                    NSLog("📥 detectElement: Found image URL")
                }
                if let linkURLString = json["linkURL"] as? String {
                    detectedLinkURL = URL(string: linkURLString)
                    NSLog("📥 detectElement: Found link URL")
                }
            } else if let error = error {
                let nsError = error as NSError
                NSLog("📥 detectElement JS error (%@/%d)", nsError.domain, nsError.code)
            }
            completed = true
        }

        // Wait for completion with timeout (300ms max for slower pages)
        let deadline = Date().addingTimeInterval(0.3)
        while !completed && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }

        if !completed {
            NSLog("📥 detectElement: Timeout waiting for JS evaluation")
        }

        contextMenuImageURL = detectedImageURL
        contextMenuLinkURL = detectedLinkURL
    }

    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)

        // Replace WebKit's default download menu items with our working versions
        replaceDownloadMenuItems(in: menu)
    }

    private func replaceDownloadMenuItems(in menu: NSMenu) {
        // WebKit's default download menu items don't work because they bypass our delegate
        // We need to replace them with our own implementations that use URLSession

        // Capture URLs before iterating (they might change)
        let imageURL = contextMenuImageURL
        let linkURL = contextMenuLinkURL

        NSLog(
            "📥 DownloadableWebView: preparing context menu (image=%d, link=%d)",
            imageURL == nil ? 0 : 1,
            linkURL == nil ? 0 : 1
        )

        // Reset stored copy item
        pendingCopyImageItem = nil

        // Collect items to replace first, then replace (to avoid index issues)
        var imageDownloadItems: [NSMenuItem] = []
        var linkDownloadItems: [(item: NSMenuItem, url: URL)] = []

        for item in menu.items {
            let title = item.title.lowercased()

            // Find and store reference to "Copy Image" menu item
            if title.contains("copy image") || title.contains("Copy Image") ||
               title.contains("Copy Image") || title.contains("Copy Picture") {
                pendingCopyImageItem = item
                NSLog("📥 DownloadableWebView: Found Copy Image menu item: %@", item.title)
            }

            // Check for image download items - we'll replace these even without URL
            // because we can use Copy Image + save from clipboard approach
            if title.contains("download image") || title.contains("save image") ||
               title.contains("Download Image") || title.contains("Store Image") ||
               title.contains("Download Pictures") {
                imageDownloadItems.append(item)
            }

            // Check for link download items - these need URL
            if title.contains("download linked") || title.contains("Download Link") ||
               title.contains("Download Linked Files") {
                if let url = linkURL {
                    linkDownloadItems.append((item, url))
                }
            }
        }

        // Replace image download items - use Copy Image approach (doesn't need URL detection)
        for item in imageDownloadItems {
            if let index = menu.items.firstIndex(of: item) {
                NSLog("📥 DownloadableWebView: Replacing '%@' at index %d (using Copy Image approach)",
                      item.title, index)

                let replacement = ClosureMenuItem(title: item.title) { [weak self] in
                    NSLog("📥 DownloadableWebView: Download Image clicked! Using Copy-then-Save approach")
                    // Use Copy Image + clipboard save - more reliable than URL detection
                    self?.downloadImageViaCopy(url: self?.contextMenuImageURL)
                }
                menu.removeItem(at: index)
                menu.insertItem(replacement, at: index)
            }
        }

        // Replace link download items
        for (item, url) in linkDownloadItems {
            if let index = menu.items.firstIndex(of: item) {
                NSLog("📥 DownloadableWebView: Replacing download menu item")

                let replacement = ClosureMenuItem(title: item.title) { [weak self] in
                    NSLog("📥 DownloadableWebView: Download Linked File clicked!")
                    self?.startDownload(url: url)
                }
                menu.removeItem(at: index)
                menu.insertItem(replacement, at: index)
            }
        }

        // If no download items were found/replaced but we detected downloadable content, add our own
        if imageDownloadItems.isEmpty && linkDownloadItems.isEmpty && (imageURL != nil || linkURL != nil) {
            addCustomDownloadMenuItems(to: menu)
        }
    }

    private func addCustomDownloadMenuItems(to menu: NSMenu) {
        var addedSeparator = false

        if let imageURL = contextMenuImageURL {
            NSLog("📥 DownloadableWebView: Adding custom Download Image")

            if !addedSeparator {
                menu.addItem(NSMenuItem.separator())
                addedSeparator = true
            }

            let downloadImageItem = ClosureMenuItem(title: "Download Image") { [weak self] in
                NSLog("📥 DownloadableWebView: Download Image clicked!")
                // Use copy-then-save approach for images (more reliable)
                self?.downloadImageViaCopy(url: imageURL)
            }
            menu.addItem(downloadImageItem)
        }

        if let linkURL = contextMenuLinkURL, linkURL != contextMenuImageURL {
            NSLog("📥 DownloadableWebView: Adding custom Download Linked File")

            if !addedSeparator {
                menu.addItem(NSMenuItem.separator())
            }

            let downloadLinkItem = ClosureMenuItem(title: "Download Linked File") { [weak self] in
                NSLog("📥 DownloadableWebView: Download Linked File clicked!")
                self?.startDownload(url: linkURL)
            }
            menu.addItem(downloadLinkItem)
        }
    }

    private func startDownload(url: URL) {
        NSLog("📥 DownloadableWebView: startDownload")

        // Check if URL is valid for download
        guard url.scheme == "http" || url.scheme == "https" || url.scheme == "data" else {
            NSLog("📥 DownloadableWebView: Invalid URL scheme: %@", url.scheme ?? "nil")
            return
        }

        var filename = url.lastPathComponent

        // If filename is empty or has no extension, try to get a better name
        if filename.isEmpty || !filename.contains(".") {
            filename = "download"
            // Try to guess extension from URL path
            if let ext = url.pathExtension.nilIfEmpty {
                filename = "download.\(ext)"
            }
        }

        NSLog("📥 DownloadableWebView: Starting download")

        startDownload(using: URLRequest(url: url)) { [weak self] download in
            Task { @MainActor in
                guard let self else {
                    _ = await download.cancel()
                    return
                }
                let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: url)
                DownloadManager.shared.adoptWebKitDownload(
                    download,
                    item: item,
                    sourceWebView: self
                )
            }
        }
    }

    /// Stored reference to the "Copy Image" menu item for triggering
    private var pendingCopyImageItem: NSMenuItem?
    private var pendingImageURL: URL?

    /// Download image by triggering system Copy Image, then saving from clipboard
    /// This is more reliable than URL-based download because WebKit's Copy Image always works correctly
    private func downloadImageViaCopy(url: URL?) {
        NSLog("📥 DownloadableWebView: downloadImageViaCopy")

        // Store URL for later use (for filename generation)
        pendingImageURL = url

        // Trigger the system's Copy Image action if we have the menu item
        if let copyItem = pendingCopyImageItem {
            NSLog("📥 DownloadableWebView: Triggering system Copy Image")
            // Perform the copy action
            if let action = copyItem.action, let target = copyItem.target {
                _ = target.perform(action, with: copyItem)
            } else if let action = copyItem.action {
                NSApp.sendAction(action, to: copyItem.target, from: copyItem)
            }

            // Wait a moment for copy to complete, then save
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) { [weak self] in
                self?.saveImageFromClipboard(originalURL: url)
            }
        } else {
            // Fallback: try to save from clipboard directly
            NSLog("📥 DownloadableWebView: No Copy Image item found, trying clipboard directly")
            saveImageFromClipboard(originalURL: url)
        }
    }

    /// Save image from clipboard to Downloads folder
    private func saveImageFromClipboard(originalURL: URL?) {
        let pasteboard = NSPasteboard.general

        // Try to get image from clipboard
        guard let image = NSImage(pasteboard: pasteboard) else {
            NSLog("📥 DownloadableWebView: No image in clipboard")
            if let url = originalURL {
                NSLog("📥 DownloadableWebView: Falling back to WebKit download")
                startDownload(url: url)
            }
            return
        }

        // Try to get image URL from clipboard (WebKit might include it)
        var imageURL = originalURL
        if imageURL == nil {
            // Debug: log all pasteboard types
            if let types = pasteboard.types {
                NSLog("📥 DownloadableWebView: Pasteboard types: %@", types.map { $0.rawValue }.joined(separator: ", "))
            }

            // Check for URL types in pasteboard
            if let urlString = pasteboard.string(forType: .URL) ?? pasteboard.string(forType: NSPasteboard.PasteboardType("public.url")),
               let url = URL(string: urlString) {
                imageURL = url
                NSLog("📥 DownloadableWebView: Got URL from clipboard")
            } else if let urlStrings = pasteboard.propertyList(forType: NSPasteboard.PasteboardType("WebURLsWithTitlesPboardType")) as? [[String]],
                      let firstList = urlStrings.first,
                      let urlString = firstList.first,
                      let url = URL(string: urlString) {
                imageURL = url
                NSLog("📥 DownloadableWebView: Got URL from legacy clipboard data")
            }
        }

        // Generate filename
        var filename = imageURL?.lastPathComponent ?? ""
        if filename.isEmpty || !filename.contains(".") {
            filename = "image_\(Int(Date().timeIntervalSince1970)).png"
        }
        // Ensure .png extension since we're saving as PNG
        if !filename.lowercased().hasSuffix(".png") {
            filename = (filename as NSString).deletingPathExtension + ".png"
        }

        // Get downloads directory
        filename = DownloadDestinationPolicy.safeFilename(filename, fallback: "image.png")
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        let destinationURL = DownloadDestinationPolicy.availableDestinationURL(
            for: filename,
            in: downloadsURL
        )

        // Convert to PNG and save
        guard let tiffData = image.tiffRepresentation,
              let bitmap = NSBitmapImageRep(data: tiffData),
              let pngData = bitmap.representation(using: .png, properties: [:]) else {
            NSLog("📥 DownloadableWebView: Failed to convert image to PNG")
            if let url = originalURL {
                startDownload(url: url)
            }
            return
        }

        do {
            try pngData.write(to: destinationURL, options: .atomic)
            NSLog("📥 DownloadableWebView: Image saved")

            // Add to download manager for UI feedback
            Task { @MainActor in
                let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: originalURL)
                item.destinationURL = destinationURL
                item.bytesReceived = Int64(pngData.count)
                item.totalBytes = Int64(pngData.count)
                DownloadManager.shared.updateItem(item, state: .completed)
            }
        } catch {
            let nsError = error as NSError
            NSLog("📥 DownloadableWebView: Failed to save image (%@/%d)", nsError.domain, nsError.code)
            if let url = originalURL {
                startDownload(url: url)
            }
        }
    }
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}

// MARK: - Focused Web Overlay

/// Focus Full Screen in Mode Web Content Layer
/// Use WebViewHolderManager Global Management WebViewavoids duplicate loads due to re-engineering of view
struct FocusedWebOverlay: View {
    let tab: Tab
    let url: URL
    let viewportSize: CGSize
    /// Callback when navigation finishes (title, url)
    var onNavigationFinished: ((String?, URL?) -> Void)?
    /// Callback when URL changes (including History.pushState/replaceState)
    var onURLChanged: ((URL?) -> Void)?
    /// Callback when loading state changes
    var onLoadingStateChanged: ((Bool) -> Void)?
    /// Callback when a new window should be opened (target="_blank")
    var onOpenNewWindow: ((URL) -> Void)?
    /// Callback when theme color changes
    var onThemeColorChanged: ((String) -> Void)?
    /// Callback when estimated progress changes
    var onProgressChanged: ((Double) -> Void)?

    /// From Global Manager Get Lasting WebViewHolder
    private let holder: WebViewHolder

    init(tab: Tab, url: URL, viewportSize: CGSize,
         onNavigationFinished: ((String?, URL?) -> Void)? = nil,
         onURLChanged: ((URL?) -> Void)? = nil,
         onLoadingStateChanged: ((Bool) -> Void)? = nil,
         onOpenNewWindow: ((URL) -> Void)? = nil,
         onThemeColorChanged: ((String) -> Void)? = nil,
         onProgressChanged: ((Double) -> Void)? = nil) {
        self.tab = tab
        self.url = url
        self.viewportSize = viewportSize
        self.onNavigationFinished = onNavigationFinished
        self.onURLChanged = onURLChanged
        self.onLoadingStateChanged = onLoadingStateChanged
        self.onOpenNewWindow = onOpenNewWindow
        self.onThemeColorChanged = onThemeColorChanged
        self.onProgressChanged = onProgressChanged
        let holder = WebViewHolderManager.shared.holder(for: tab.id)
        self._holder = ObservedObject(wrappedValue: holder)

        // Set callbacks immediately in init (SwiftUI View init can be called multiple times)
        holder.onNavigationFinished = onNavigationFinished
        holder.onURLChanged = onURLChanged
        holder.onLoadingStateChanged = onLoadingStateChanged
        holder.onOpenNewWindow = onOpenNewWindow
        holder.onThemeColorChanged = onThemeColorChanged
        holder.onProgressChanged = onProgressChanged
    }

    var body: some View {
        // Do Not Use any @Published/@ObservedObjectAvoid SwiftUI Update Cycle
        // WKWebView Shows the load status on your own
        PersistentWebView(holder: holder, size: viewportSize)
            .id(tab.id)
            .frame(width: viewportSize.width, height: viewportSize.height)
            .onAppear {
                holder.loadInitialURLIfNeeded(url)
            }
            .onChange(of: url) { _, newURL in
                holder.navigateIfNeeded(to: newURL)
            }
        }
        .frame(width: viewportSize.width, height: viewportSize.height)
        .onAppear {
            holder.loadInitialURLIfNeeded(url)
        }
        .onChange(of: url) { _, newURL in
            holder.navigateIfNeeded(to: newURL)
        }
    }
}

private struct WebPageFailureView: View {
    let failure: WebPageFailure
    let retry: () -> Void

    var body: some View {
        VStack(spacing: 14) {
            Image(systemName: failure.kind == .secureConnection ? "lock.trianglebadge.exclamationmark" : "exclamationmark.triangle")
                .font(.system(size: 34, weight: .regular))
                .foregroundStyle(.secondary)
            Text(failure.title)
                .font(.title3.weight(.semibold))
            Text(failure.message)
                .font(.body)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 420)
            Button("Reload", action: retry)
                .buttonStyle(.borderedProminent)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(nsColor: .windowBackgroundColor))
        .accessibilityElement(children: .contain)
    }
}

// MARK: - Script Message Handler

extension WebViewHolder: WKScriptMessageHandler {
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        if message.name == "actriumThemeColor", let colorHex = message.body as? String {
            if colorHex.hasPrefix("#") {
                Task { @MainActor [weak self] in
                    self?.onThemeColorChanged?(colorHex)
                }
            }
        } else if message.name == "actriumLog", let logMsg = message.body as? String {
            NSLog("🎨 [JS Color Log] %@", logMsg)
        } else if message.name == "actriumPasswordCapture", let dict = message.body as? [String: String] {
            let url = dict["url"] ?? ""
            let username = dict["username"] ?? ""
            let password = dict["password"] ?? ""
            guard !password.isEmpty else { return }
            NSLog("🔑 actriumPasswordCapture: url=%@, user=%@, vaultState=%@",
                  url, username, "\(VaultManager.shared.state)")
            Task { @MainActor in
                AutofillManager.shared.handleCapturedCredentials(url: url, username: username, password: password)
            }
        } else if message.name == "actriumAutofillSuggestion", let dict = message.body as? [String: Any] {
            let domain = dict["domain"] as? String ?? ""
            let rectDict = dict["fieldRect"] as? [String: Any] ?? [:]
            let x = (rectDict["x"] as? Double) ?? 0
            let y = (rectDict["y"] as? Double) ?? 0
            let w = (rectDict["width"] as? Double) ?? 0
            let h = (rectDict["height"] as? Double) ?? 0
            let fieldRect = CGRect(x: x, y: y, width: w, height: h)
            NSLog("🔑 actriumAutofillSuggestion: domain=%@, rect=(%.0f,%.0f,%.0f,%.0f), vaultState=%@",
                  domain, x, y, w, h, "\(VaultManager.shared.state)")
            Task { @MainActor [weak self] in
                guard let self = self else { return }
                let vaultState = VaultManager.shared.state
                if vaultState == .locked {
                    NSLog("🔑 autofill: vault locked, showing unlock prompt")
                    self.showVaultLockedPopover(fieldRect: fieldRect, pageURL: pageURL)
                    return
                }
                guard vaultState == .unlocked else { return }
                let credentials = AutofillManager.shared.findCredentials(for: pageURL)
                NSLog("🔑 autofill: found %d matching credentials", credentials.count)
                guard !credentials.isEmpty else { return }
                self.showAutofillPopover(credentials: credentials, fieldRect: fieldRect)
            }
        } else if message.name == "actriumAutofillDismiss" {
            Task { @MainActor [weak self] in
                self?.dismissAutofillPopover()
            }
        }
    }
}
