//
//  WebViewTabView.swift
//  Canvas
//
//  WKWebView 封装，用于显示 Web 内容
//  系统自动管理 WebContent 进程，无需手动进程池
//

import SwiftUI
import WebKit
import Combine

// MARK: - WebView Tab View

/// SwiftUI wrapper for WKWebView
/// 用于显示 http/https 网页内容
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

        // 启用开发者工具
        config.preferences.setValue(true, forKey: "developerExtrasEnabled")

        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.uiDelegate = context.coordinator

        // 允许返回手势
        webView.allowsBackForwardNavigationGestures = true

        // 加载 URL
        context.coordinator.loadedURL = url
        let request = URLRequest(url: url)
        webView.load(request)

        NSLog("🌐 WebViewTabView: Loading \(url.absoluteString)")

        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        // 只有当请求的 URL 真正变化时才重新加载
        // 使用 coordinator 跟踪，避免 webView.url 在加载中为 nil 导致的重复加载
        if context.coordinator.loadedURL != url {
            context.coordinator.loadedURL = url
            let request = URLRequest(url: url)
            webView.load(request)
            NSLog("🌐 WebViewTabView: URL changed, reloading \(url.absoluteString)")
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
            NSLog("🌐 WebViewTabView: Finished loading, title: \(webView.title ?? "nil")")
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            NSLog("🌐 WebViewTabView: Navigation failed: \(error.localizedDescription)")
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            NSLog("🌐 WebViewTabView: Provisional navigation failed: \(error.localizedDescription)")
        }

        // MARK: - WKUIDelegate

        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            // 处理 target="_blank" 链接，在当前 webView 中打开
            if navigationAction.targetFrame == nil {
                webView.load(navigationAction.request)
            }
            return nil
        }
    }
}

// MARK: - WebView Multi-Process Tab View

/// Web 内容的多进程 Tab 视图
/// WKWebView 由系统自动管理 WebContent 进程
///
/// 渲染策略：
/// - Overview 模式：在 TabPanel 内显示 cardSize 的 web 预览
/// - Focus 模式：由 FocusedWebOverlay 在 canvas 外独立渲染
///   (因为 scaleEffect 对 NSViewRepresentable 不生效)
struct WebViewMultiProcessTabView: View {
    let tab: Tab
    let url: URL
    let containerSize: CGSize
    var isFocused: Bool = false

    @State private var webTitle: String = ""
    @State private var isLoading: Bool = true

    var body: some View {
        ZStack {
            // Overview 模式：显示截图或占位符
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

// MARK: - Shared Process Pool

/// 共享的 WKProcessPool，确保所有 WebView 使用相同的 WebContent 进程
private let sharedProcessPool = WKProcessPool()

// MARK: - Persistent WebView Holder

/// 持久化 WKWebView 实例，避免 SwiftUI 视图重建导致的重复加载
/// 注意：不使用 @Published 来跟踪 isLoading，避免 SwiftUI 更新循环
final class WebViewHolder: NSObject, WKNavigationDelegate {
    let webView: WKWebView
    private var loadedURL: URL?

    /// Callback when navigation finishes with title and URL
    var onNavigationFinished: ((String?, URL?) -> Void)?

    /// Callback when loading state changes
    var onLoadingStateChanged: ((Bool) -> Void)?

    /// Callback when a new window should be opened (target="_blank")
    var onOpenNewWindow: ((URL) -> Void)?

    /// Current loading state
    private(set) var isLoading: Bool = false

    /// Cached snapshot for Overview mode
    private(set) var snapshot: NSImage?

    /// Download delegate handlers (keep strong reference)
    private var downloadHandlers: [WKDownload: DownloadDelegateHandler] = [:]

    /// Canvas User-Agent string
    /// Format follows standard browser convention: Mozilla/5.0 (Platform) Engine/Version (Compatibility) Browser/Version Engine/Version
    private static let userAgent: String = {
        let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "1.0"
        let osVersion = ProcessInfo.processInfo.operatingSystemVersion
        let osVersionString = "\(osVersion.majorVersion)_\(osVersion.minorVersion)_\(osVersion.patchVersion)"
        // WebKit version from macOS (605.1.15 is standard for Safari/WebKit on macOS)
        return "Mozilla/5.0 (Macintosh; Intel Mac OS X \(osVersionString)) AppleWebKit/605.1.15 (KHTML, like Gecko) Canvas/\(appVersion) Safari/605.1.15"
    }()

    override init() {
        let config = WKWebViewConfiguration()
        config.processPool = sharedProcessPool
        config.preferences.setValue(true, forKey: "developerExtrasEnabled")

        // Use custom DownloadableWebView for context menu download support
        self.webView = DownloadableWebView(frame: .zero, configuration: config)
        super.init()

        webView.allowsBackForwardNavigationGestures = true
        webView.navigationDelegate = self
        webView.customUserAgent = Self.userAgent
    }

    func loadIfNeeded(_ url: URL) {
        // 只有 URL 真正变化时才重新加载
        if loadedURL != url {
            loadedURL = url
            let request = URLRequest(url: url)
            webView.load(request)
        }
    }

    // MARK: - Navigation

    var canGoBack: Bool { webView.canGoBack }
    var canGoForward: Bool { webView.canGoForward }
    var currentURL: URL? { webView.url }
    var currentTitle: String? { webView.title }

    func goBack() { webView.goBack() }
    func goForward() { webView.goForward() }
    func reload() { webView.reload() }

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
        guard webView.url != nil else { return }

        do {
            let config = WKSnapshotConfiguration()
            // Use the webView's current size for the snapshot
            let image = try await webView.takeSnapshot(configuration: config)
            self.snapshot = image
            NSLog("📸 WebViewHolder: Snapshot captured")
        } catch {
            NSLog("📸 WebViewHolder: Failed to capture snapshot - \(error.localizedDescription)")
        }
    }

    // MARK: - WKNavigationDelegate

    func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
        isLoading = true
        onLoadingStateChanged?(true)
    }

    func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        isLoading = false
        onLoadingStateChanged?(false)
        // Notify with title and URL when navigation completes
        onNavigationFinished?(webView.title, webView.url)

        // Safari-style: Take snapshot immediately after page load for Overview mode
        // Use a short delay to allow page rendering to complete
        Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 300_000_000) // 0.3s delay
            await self?.takeSnapshot()
        }
    }

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        isLoading = false
        onLoadingStateChanged?(false)
        NSLog("🌐 WebViewHolder: Navigation failed - \(error.localizedDescription)")
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        isLoading = false
        onLoadingStateChanged?(false)
        let nsError = error as NSError
        // Ignore cancelled requests (e.g., due to new navigation)
        if nsError.code != NSURLErrorCancelled {
            NSLog("🌐 WebViewHolder: Provisional navigation failed - \(error.localizedDescription)")
        }
    }

    // MARK: - Download Detection (Navigation Response)

    /// Detect downloads by checking response headers
    func webView(_ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse, decisionHandler: @escaping (WKNavigationResponsePolicy) -> Void) {
        guard let response = navigationResponse.response as? HTTPURLResponse else {
            decisionHandler(.allow)
            return
        }

        // Check for Content-Disposition: attachment header
        let contentDisposition = response.allHeaderFields["Content-Disposition"] as? String ?? ""
        let isAttachment = contentDisposition.lowercased().contains("attachment")

        // Check for non-renderable MIME types
        let mimeType = response.mimeType ?? ""
        let downloadMIMETypes = [
            "application/octet-stream",
            "application/zip",
            "application/x-zip-compressed",
            "application/x-rar-compressed",
            "application/x-7z-compressed",
            "application/x-tar",
            "application/gzip",
            "application/msword",
            "application/vnd.openxmlformats-officedocument",
            "application/x-apple-diskimage",
            "application/x-msdos-program",
            "application/x-bzip",
            "application/x-bzip2"
        ]
        let shouldDownloadByMIME = downloadMIMETypes.contains(where: { mimeType.hasPrefix($0) })

        // Download if: attachment header OR (can't show AND is download MIME type)
        if isAttachment || (!navigationResponse.canShowMIMEType && shouldDownloadByMIME) {
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Detected download - MIME: \(mimeType), Disposition: \(contentDisposition)")
        } else {
            decisionHandler(.allow)
        }
    }

    /// Handle navigation action with download attribute or Option+click
    func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction, decisionHandler: @escaping (WKNavigationActionPolicy) -> Void) {
        // Check for download attribute on links
        if navigationAction.shouldPerformDownload {
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Link has download attribute")
            return
        }

        // Safari-style: Option+click to force download any link
        if navigationAction.modifierFlags.contains(.option),
           let url = navigationAction.request.url,
           url.scheme == "http" || url.scheme == "https" {
            // Use URLSession to download directly (bypass WKDownload restrictions)
            let filename = url.lastPathComponent.isEmpty ? "download" : url.lastPathComponent
            Task { @MainActor in
                let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: url)
                URLSessionDownloader.shared.startDownload(url: url, item: item)
            }
            decisionHandler(.cancel)
            NSLog("📥 WebViewHolder: Option+click download - \(url.lastPathComponent)")
            return
        }

        decisionHandler(.allow)
    }

    /// Handle navigation response becoming a download
    func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) {
        setupDownload(download, sourceURL: navigationResponse.response.url)
    }

    /// Handle navigation action becoming a download (for links with download attribute)
    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) {
        setupDownload(download, sourceURL: navigationAction.request.url)
    }

    /// Setup download with DownloadManager
    private func setupDownload(_ download: WKDownload, sourceURL: URL?) {
        let filename = sourceURL?.lastPathComponent ?? "download"

        Task { @MainActor in
            let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: sourceURL)
            item.download = download

            let handler = DownloadDelegateHandler(item: item)
            self.downloadHandlers[download] = handler
            download.delegate = handler

            NSLog("📥 WebViewHolder: Download started - \(filename)")
        }
    }

    /// Clean up completed download handlers
    func cleanupDownloadHandler(for download: WKDownload) {
        downloadHandlers.removeValue(forKey: download)
    }
}

// MARK: - Global WebView Manager

/// 全局管理 WebViewHolder，避免视图重建时丢失
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
        holders.removeValue(forKey: tabId)
        NSLog("🌐 WebViewHolderManager: Removed holder for tab \(tabId.uuidString.prefix(8))")
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
    let size: CGSize  // 显式传入尺寸

    func makeCoordinator() -> Coordinator {
        Coordinator(holder: holder)
    }

    func makeNSView(context: Context) -> NSView {
        // 使用容器视图来确保正确的布局
        let container = NSView(frame: NSRect(origin: .zero, size: size))
        let webView = holder.webView

        // navigationDelegate 已在 WebViewHolder 中设置
        webView.uiDelegate = context.coordinator
        webView.frame = container.bounds
        webView.autoresizingMask = [.width, .height]

        container.addSubview(webView)
        return container
    }

    func updateNSView(_ container: NSView, context: Context) {
        // 使用传入的 size 而不是 container.bounds
        // 因为 SwiftUI 可能会暂时给容器 0 大小
        guard size.width > 0 && size.height > 0 else { return }

        // 确保容器和 webView 都有正确的 frame
        let targetFrame = NSRect(origin: .zero, size: size)
        if container.frame != targetFrame {
            container.frame = targetFrame
        }

        if let webView = container.subviews.first as? WKWebView,
           webView.frame != targetFrame {
            webView.frame = targetFrame
        }
    }

    // Coordinator 处理 UIDelegate（target=_blank 链接、上下文菜单）
    class Coordinator: NSObject, WKUIDelegate {
        let holder: WebViewHolder

        init(holder: WebViewHolder) {
            self.holder = holder
        }

        // 处理 target="_blank" 链接 - 在新窗口打开
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if navigationAction.targetFrame == nil,
               let url = navigationAction.request.url {
                // 如果有回调，在新窗口打开；否则在当前窗口打开
                if let onOpenNewWindow = holder.onOpenNewWindow {
                    onOpenNewWindow(url)
                } else {
                    holder.webView.load(navigationAction.request)
                }
            }
            return nil
        }

        // macOS: Customize context menu with download options
        func webView(_ webView: WKWebView, willPerformClientRedirectTo URL: URL, delay: TimeInterval) {
            // Not used, but required for protocol conformance
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
        let js = """
        (function() {
            var x = \(point.x);
            var y = \(point.y);
            var element = document.elementFromPoint(x, y);
            var result = { imageURL: null, linkURL: null };

            if (element) {
                // Check for image
                if (element.tagName === 'IMG' && element.src) {
                    result.imageURL = element.src;
                }

                // Check parent if current is not IMG
                if (!result.imageURL) {
                    var img = element.querySelector && element.querySelector('img');
                    if (img && img.src) {
                        result.imageURL = img.src;
                    }
                }

                // Check for background image
                if (!result.imageURL) {
                    var bgImage = window.getComputedStyle(element).backgroundImage;
                    if (bgImage && bgImage !== 'none' && bgImage.startsWith('url(')) {
                        result.imageURL = bgImage.slice(5, -2).replace(/['"]/g, '');
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
                }
                if let linkURLString = json["linkURL"] as? String {
                    detectedLinkURL = URL(string: linkURLString)
                }
            }
            completed = true
        }

        // Wait for completion with a short timeout (100ms max)
        let deadline = Date().addingTimeInterval(0.1)
        while !completed && Date() < deadline {
            RunLoop.current.run(mode: .default, before: Date(timeIntervalSinceNow: 0.01))
        }

        contextMenuImageURL = detectedImageURL
        contextMenuLinkURL = detectedLinkURL
    }

    override func willOpenMenu(_ menu: NSMenu, with event: NSEvent) {
        super.willOpenMenu(menu, with: event)

        // Add menu items synchronously - URLs should already be detected
        addDownloadMenuItems(to: menu)
    }

    private func addDownloadMenuItems(to menu: NSMenu) {
        var addedSeparator = false

        if let imageURL = contextMenuImageURL {
            NSLog("📥 DownloadableWebView: Adding Download Image for %@", imageURL.absoluteString)

            if !addedSeparator {
                menu.addItem(NSMenuItem.separator())
                addedSeparator = true
            }

            // Use closure-based menu item to ensure action fires
            let downloadImageItem = ClosureMenuItem(title: "Download Image") { [weak self] in
                NSLog("📥 DownloadableWebView: Download Image clicked!")
                self?.startDownload(url: imageURL)
            }
            menu.addItem(downloadImageItem)
        }

        if let linkURL = contextMenuLinkURL, linkURL != contextMenuImageURL {
            NSLog("📥 DownloadableWebView: Adding Download Linked File for %@", linkURL.absoluteString)

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
        NSLog("📥 DownloadableWebView: startDownload - %@", url.absoluteString)

        // Check if URL is valid for download
        guard url.scheme == "http" || url.scheme == "https" else {
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

        NSLog("📥 DownloadableWebView: Starting download: %@", filename)

        Task { @MainActor in
            let item = DownloadManager.shared.addDownload(filename: filename, sourceURL: url)
            NSLog("📥 DownloadableWebView: Created item, starting URLSession download")
            URLSessionDownloader.shared.startDownload(url: url, item: item)
        }
    }
}

private extension String {
    var nilIfEmpty: String? {
        isEmpty ? nil : self
    }
}

// MARK: - Focused Web Overlay

/// Focus 模式下的全屏 Web 内容层
/// 使用 WebViewHolderManager 全局管理 WebView，避免视图重建导致的重复加载
struct FocusedWebOverlay: View {
    let tab: Tab
    let url: URL
    let viewportSize: CGSize
    /// Callback when navigation finishes (title, url)
    var onNavigationFinished: ((String?, URL?) -> Void)?
    /// Callback when loading state changes
    var onLoadingStateChanged: ((Bool) -> Void)?
    /// Callback when a new window should be opened (target="_blank")
    var onOpenNewWindow: ((URL) -> Void)?

    /// 从全局 Manager 获取持久化的 WebViewHolder
    private let holder: WebViewHolder

    init(tab: Tab, url: URL, viewportSize: CGSize,
         onNavigationFinished: ((String?, URL?) -> Void)? = nil,
         onLoadingStateChanged: ((Bool) -> Void)? = nil,
         onOpenNewWindow: ((URL) -> Void)? = nil) {
        self.tab = tab
        self.url = url
        self.viewportSize = viewportSize
        self.onNavigationFinished = onNavigationFinished
        self.onLoadingStateChanged = onLoadingStateChanged
        self.onOpenNewWindow = onOpenNewWindow
        self.holder = WebViewHolderManager.shared.holder(for: tab.id)
    }

    var body: some View {
        // 不使用任何 @Published/@ObservedObject，避免 SwiftUI 更新循环
        // WKWebView 自己会显示加载状态
        PersistentWebView(holder: holder, size: viewportSize)
            .id(tab.id)
            .frame(width: viewportSize.width, height: viewportSize.height)
            .onAppear {
                holder.onNavigationFinished = onNavigationFinished
                holder.onLoadingStateChanged = onLoadingStateChanged
                holder.onOpenNewWindow = onOpenNewWindow
                holder.loadIfNeeded(url)
            }
            .onChange(of: url) { _, newURL in
                holder.loadIfNeeded(newURL)
            }
    }
}
