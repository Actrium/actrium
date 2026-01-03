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
            // Overview 模式：显示占位符（实际 web 内容由 FocusedWebOverlay 在 Focus 时渲染）
            // 这样只有一个 WebView 实例（在 FocusedWebOverlay 中）
            Color(nsColor: .windowBackgroundColor)

            // 显示 URL 作为预览
            if !isFocused {
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

    // MARK: - WKNavigationDelegate

    func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        NSLog("🌐 WebViewHolder: Navigation failed - \(error.localizedDescription)")
    }

    func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        let nsError = error as NSError
        // Ignore cancelled requests (e.g., due to new navigation)
        if nsError.code != NSURLErrorCancelled {
            NSLog("🌐 WebViewHolder: Provisional navigation failed - \(error.localizedDescription)")
        }
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

    func remove(tabId: UUID) {
        holders.removeValue(forKey: tabId)
        NSLog("🌐 WebViewHolderManager: Removed holder for tab \(tabId.uuidString.prefix(8))")
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

        // 处理 target="_blank" 链接
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for navigationAction: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if navigationAction.targetFrame == nil {
                holder.webView.load(navigationAction.request)
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

    /// 从全局 Manager 获取持久化的 WebViewHolder
    private let holder: WebViewHolder

    init(tab: Tab, url: URL, viewportSize: CGSize) {
        self.tab = tab
        self.url = url
        self.viewportSize = viewportSize
        self.holder = WebViewHolderManager.shared.holder(for: tab.id)
    }

    var body: some View {
        // 不使用任何 @Published/@ObservedObject，避免 SwiftUI 更新循环
        // WKWebView 自己会显示加载状态
        PersistentWebView(holder: holder, size: viewportSize)
            .id(tab.id)
            .frame(width: viewportSize.width, height: viewportSize.height)
            .onAppear {
                holder.loadIfNeeded(url)
            }
            .onChange(of: url) { _, newURL in
                holder.loadIfNeeded(newURL)
            }
    }
}
