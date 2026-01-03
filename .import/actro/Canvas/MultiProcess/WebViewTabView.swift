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
final class WebViewHolder: NSObject, WKNavigationDelegate, WKDownloadDelegate {
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

    /// Active downloads
    private var activeDownloads: [WKDownload: DownloadInfo] = [:]

    /// Download info for tracking
    private struct DownloadInfo {
        let suggestedFilename: String
        var destinationURL: URL?
        var bytesWritten: Int64 = 0
        var totalBytes: Int64 = 0
    }

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

        self.webView = WKWebView(frame: .zero, configuration: config)
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
            "application/pdf",  // PDFs can be downloaded
            "application/msword",
            "application/vnd.openxmlformats-officedocument",
            "application/x-apple-diskimage",
            "application/x-msdos-program"
        ]
        let shouldDownload = isAttachment || downloadMIMETypes.contains(where: { mimeType.hasPrefix($0) })

        if shouldDownload && navigationResponse.canShowMIMEType == false || isAttachment {
            // Trigger download
            decisionHandler(.download)
            NSLog("📥 WebViewHolder: Detected download - MIME: \(mimeType), Disposition: \(contentDisposition)")
        } else {
            decisionHandler(.allow)
        }
    }

    /// Handle navigation response becoming a download
    func webView(_ webView: WKWebView, navigationResponse: WKNavigationResponse, didBecome download: WKDownload) {
        download.delegate = self
        NSLog("📥 WebViewHolder: Navigation became download")
    }

    /// Handle navigation action becoming a download (for links with download attribute)
    func webView(_ webView: WKWebView, navigationAction: WKNavigationAction, didBecome download: WKDownload) {
        download.delegate = self
        NSLog("📥 WebViewHolder: Navigation action became download")
    }

    // MARK: - WKDownloadDelegate

    func download(_ download: WKDownload, decideDestinationUsing response: URLResponse, suggestedFilename: String, completionHandler: @escaping (URL?) -> Void) {
        // Get the Downloads folder
        let downloadsURL = FileManager.default.urls(for: .downloadsDirectory, in: .userDomainMask).first!
        var destinationURL = downloadsURL.appendingPathComponent(suggestedFilename)

        // Handle duplicate filenames
        var counter = 1
        let fileManager = FileManager.default
        let baseName = (suggestedFilename as NSString).deletingPathExtension
        let ext = (suggestedFilename as NSString).pathExtension

        while fileManager.fileExists(atPath: destinationURL.path) {
            let newName = ext.isEmpty ? "\(baseName) (\(counter))" : "\(baseName) (\(counter)).\(ext)"
            destinationURL = downloadsURL.appendingPathComponent(newName)
            counter += 1
        }

        // Store download info
        activeDownloads[download] = DownloadInfo(
            suggestedFilename: suggestedFilename,
            destinationURL: destinationURL
        )

        NSLog("📥 WebViewHolder: Downloading to \(destinationURL.path)")
        completionHandler(destinationURL)
    }

    func download(_ download: WKDownload, didReceive challenge: URLAuthenticationChallenge, completionHandler: @escaping (URLSession.AuthChallengeDisposition, URLCredential?) -> Void) {
        completionHandler(.performDefaultHandling, nil)
    }

    func downloadDidFinish(_ download: WKDownload) {
        if let info = activeDownloads[download], let url = info.destinationURL {
            NSLog("📥 WebViewHolder: Download completed - \(info.suggestedFilename)")
            // Reveal in Finder
            NSWorkspace.shared.activateFileViewerSelecting([url])
        }
        activeDownloads.removeValue(forKey: download)
    }

    func download(_ download: WKDownload, didFailWithError error: Error, resumeData: Data?) {
        if let info = activeDownloads[download] {
            NSLog("📥 WebViewHolder: Download failed - \(info.suggestedFilename): \(error.localizedDescription)")
        }
        activeDownloads.removeValue(forKey: download)
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

    // Coordinator 只处理 UIDelegate（target=_blank 链接）
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
