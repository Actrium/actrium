import SwiftUI
import WebKit

struct PrivateWebView: View {
    let url: URL
    let title: String?
    let hiddenBar: Bool
    @Environment(\.presentationMode) var presentationMode
    @State private var isLoading = true
    @State private var canGoBack = false
    @State private var canGoForward = false
    @State private var webTitle: String = ""
    @State private var showContent = false
    
    // 初始化方法
    init(url: URL, title: String? = nil, hiddenBar: Bool = false) {
        self.url = url
        self.title = title
        self.hiddenBar = hiddenBar
    }
    
    var body: some View {
        VStack(spacing: 0) {
            VStack {
                HStack {
                    Button(action: {
                        presentationMode.wrappedValue.dismiss()
                    }) {
                        Image("nav.back")
                            .resizable()
                            .renderingMode(.template)
                            .frame(width: 24, height: 24)
                            .foregroundColor(.black)
                    }
                    Spacer()
                    Text(title ?? webTitle)
                        .font(.system(size: 16, weight: .medium))
                        .foregroundColor(.black)
                    Spacer()
                    Color.clear
                        .frame(width: 18, height: 18)
                }
                .padding(.horizontal, 22)
                .background(Color.white)
            }
            .padding(.top, hiddenBar ? 24 : UIApplication.shared.statusBarHeight + 16)
            .padding(.bottom, 10)
            
            // WebView with loading indicator
            ZStack {
                if showContent {
                    WebViewRepresentable(
                        url: url,
                        isLoading: $isLoading,
                        canGoBack: $canGoBack,
                        canGoForward: $canGoForward,
                        webTitle: $webTitle
                    )
                    .transition(.opacity)
                }
//                
//                if isLoading {
//                    ProgressView()
//                        .progressViewStyle(CircularProgressViewStyle())
//                        .scaleEffect(1.2)
//                }
            }
            
            Spacer()
        }
        .background(Color.white)
        .navigationBarHidden(true)
        .ignoresSafeArea(.all)
        .onAppear {
            // 延迟显示内容，让导航动画先完成
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                withAnimation(.easeIn(duration: 0.2)) {
                    showContent = true
                }
            }
        }
    }
}

struct WebViewRepresentable: UIViewRepresentable {
    let url: URL
    @Binding var isLoading: Bool
    @Binding var canGoBack: Bool
    @Binding var canGoForward: Bool
    @Binding var webTitle: String
    
    func makeUIView(context: Context) -> WKWebView {
        // 优化的 WebView 配置
        let configuration = WKWebViewConfiguration()
        configuration.suppressesIncrementalRendering = false // 允许增量渲染，更快显示
        
        // 优化数据存储
        let dataStore = WKWebsiteDataStore.default()
        configuration.websiteDataStore = dataStore
        
        // 创建 WebView
        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.allowsBackForwardNavigationGestures = true
        
        // 设置 WebView 背景色为白色
        webView.backgroundColor = UIColor.white
        webView.scrollView.backgroundColor = UIColor.white
        webView.isOpaque = true
        
        // 优化滚动性能
        webView.scrollView.decelerationRate = .normal
        webView.scrollView.contentInsetAdjustmentBehavior = .never
        
        return webView
    }
    
    func updateUIView(_ webView: WKWebView, context: Context) {
        if webView.url != url {
            let request = URLRequest(url: url)
            webView.load(request)
        }
    }
    
    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }
    
    @MainActor
    class Coordinator: NSObject, WKNavigationDelegate {
        let parent: WebViewRepresentable
        
        init(_ parent: WebViewRepresentable) {
            self.parent = parent
        }
        
        func webView(_ webView: WKWebView, didStartProvisionalNavigation navigation: WKNavigation!) {
            parent.isLoading = true
        }
        
        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            parent.isLoading = false
            parent.canGoBack = webView.canGoBack
            parent.canGoForward = webView.canGoForward
            
            // 异步获取网页标题，不阻塞主线程
            Task { @MainActor in
                if let title = try? await webView.evaluateJavaScript("document.title") as? String,
                   !title.isEmpty {
                    self.parent.webTitle = title
                }
            }
        }
        
        func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
            parent.isLoading = false
        }
        
        func webView(_ webView: WKWebView, didCommit navigation: WKNavigation!) {
            // 页面开始渲染时就隐藏加载指示器，提供更快的视觉反馈
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
                self.parent.isLoading = false
            }
        }
    }
}
