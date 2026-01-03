//
//  WebViewTabView.swift
//  Canvas
//
//  WKWebView 封装，用于显示 Web 内容
//  系统自动管理 WebContent 进程，无需手动进程池
//

import SwiftUI
import WebKit

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

        fputs("🌐 WebViewTabView: Loading \(url.absoluteString)\n", stderr)

        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        // 只有当请求的 URL 真正变化时才重新加载
        // 使用 coordinator 跟踪，避免 webView.url 在加载中为 nil 导致的重复加载
        if context.coordinator.loadedURL != url {
            context.coordinator.loadedURL = url
            let request = URLRequest(url: url)
            webView.load(request)
            fputs("🌐 WebViewTabView: URL changed, reloading \(url.absoluteString)\n", stderr)
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
            fputs("🌐 WebViewTabView: Finished loading, title: \(webView.title ?? "nil")\n", stderr)
        }

        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            fputs("🌐 WebViewTabView: Navigation failed: \(error.localizedDescription)\n", stderr)
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
            fputs("🌐 WebViewTabView: Provisional navigation failed: \(error.localizedDescription)\n", stderr)
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
            // Focus 模式：显示占位符（实际内容由 FocusedWebOverlay 渲染）
            // Overview 模式：显示实际 web 内容
            if isFocused {
                // 占位符 - 实际内容在 FocusedWebOverlay 层
                Color(nsColor: .windowBackgroundColor)
            } else {
                WebViewTabView(
                    url: url,
                    containerSize: containerSize,
                    title: $webTitle,
                    isLoading: $isLoading
                )
            }

            if isLoading && !isFocused {
                ProgressView()
                    .scaleEffect(0.8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color.black.opacity(0.3))
            }
        }
        .frame(width: containerSize.width, height: containerSize.height)
    }
}

// MARK: - Focused Web Overlay

/// Focus 模式下的全屏 Web 内容层
/// 在 canvas 变换之外独立渲染，避免 NSViewRepresentable 缩放问题
struct FocusedWebOverlay: View {
    let tab: Tab
    let url: URL
    let viewportSize: CGSize

    @State private var webTitle: String = ""
    @State private var isLoading: Bool = true

    var body: some View {
        ZStack {
            WebViewTabView(
                url: url,
                containerSize: viewportSize,
                title: $webTitle,
                isLoading: $isLoading
            )

            if isLoading {
                ProgressView()
                    .scaleEffect(0.8)
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .background(Color.black.opacity(0.3))
            }
        }
        .frame(width: viewportSize.width, height: viewportSize.height)
    }
}
