import Foundation
import React
import React_RCTAppDelegate
import ReactAppDependencyProvider

private let logger = AppLogger.reactNative

// 便捷日志函数 - 同时输出到 os.Logger（以及可选的远程服务器）
/// Convenience logging helpers.
///
/// - Always logs to `os.Logger`.
/// - Only forwards to the remote log server in DEBUG (to avoid production noise / PII risk).
private func nlog(_ message: String) {
    logger.debug(message)
    #if DEBUG
    NativeLogger.nativeLog(message)
    #endif
}

private func nnotice(_ message: String) {
    logger.notice(message)
    #if DEBUG
    NativeLogger.nativeLog(message)
    #endif
}

private func nwarn(_ message: String) {
    logger.warning(message)
    #if DEBUG
    NativeLogger.nativeWarn(message)
    #endif
}

private func nerror(_ message: String) {
    logger.error(message)
    #if DEBUG
    NativeLogger.nativeError(message)
    #endif
}

private func nfault(_ message: String) {
    logger.fault(message)
    #if DEBUG
    NativeLogger.nativeError(message)
    #endif
}

final class ActroReactNativeFactoryDelegate: RCTDefaultReactNativeFactoryDelegate {
    override init() {
        super.init()
        self.dependencyProvider = RCTAppDependencyProvider()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func bundleURL() -> URL? {
        #if DEBUG
        // 开发时强制走 Metro（支持热重载）；Debug 环境不回退到离线 actr.jsbundle（避免落到过期 bundle 导致"白屏无渲染"）
        // 🟢 特别注意：在 iPad 真机调试时，RCTBundleURLProvider 对 IP:端口 的解析不稳定
        // 直接构造 URL 是确保 iPad 能精准连接到 Mac Metro (8082) 的最可靠方式
        let packagerHost = "localhost" // 或者填写 Mac 的局域网 IP 地址
        let port = 8082
        let url = URL(string: "http://\(packagerHost):\(port)/index.bundle?platform=ios&dev=true&minify=false")
        
        print("🌐 [Native] 强制指向 Metro URL: \(url?.absoluteString ?? "nil")")
        return url
        #else
        // 发布版走离线 bundle
        // 注意：不要叫 main.jsbundle，避免与 RN 默认 bundling 产物重名导致 Xcode "Multiple commands produce …/main.jsbundle"
        return Bundle.main.url(forResource: "actr", withExtension: "jsbundle")
        #endif
    }

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        return bundleURL()
    }
}

final class ActroReactNativeHost {
    static let shared = ActroReactNativeHost()

    private let factoryDelegate: ActroReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory
    private var bridge: RCTBridge?
    private var viewCache: [UUID: RCTRootView] = [:]

    private init() {
        nnotice("🏗️ [Native] ActroReactNativeHost 初始化")

        #if DEBUG
        // macOS RN：LogBox/RedBox 在部分版本上可能触发 UIBlock 异常（如 NULL CGColor），导致 UI 不渲染
        // 先禁用 RedBox（LogBox.show 也会被 gate），避免影响主界面渲染。
        RCTRedBoxSetEnabled(false)
        nwarn("🧪 [Native] Debug：已禁用 RedBox/LogBox（避免 UIBlock 异常导致白屏）")
        #endif

        factoryDelegate = ActroReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
        nnotice("✅ [Native] Factory 创建完成")
    }

    func viewForTab(tabId: UUID, initialProperties: [String: Any]) -> UIView {
        if let cachedView = viewCache[tabId] {
            nlog("♻️ [Native] 使用缓存的 RootView: \(tabId)")
            return cachedView
        }

        nnotice("📱 [Native] 为 Tab \(tabId) 创建新 RootView")
        let rootView = createRootView(initialProperties: initialProperties)
        if let rctRootView = rootView as? RCTRootView {
            viewCache[tabId] = rctRootView
        }
        return rootView
    }

    func removeViewForTab(tabId: UUID) {
        if viewCache.removeValue(forKey: tabId) != nil {
            nlog("🗑️ [Native] 移除 Tab 缓存视图: \(tabId)")
            // 可以在这里进行额外的清理
        }
    }

    func createRootView(initialProperties: [String: Any]) -> UIView {
        nnotice("📱 [Native] 创建 RootView，模块名: ActrApp")

        let rootView = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: initialProperties,
            launchOptions: nil
        )

        if let rctRootView = rootView as? RCTRootView {
            // Store bridge reference
            self.bridge = rctRootView.bridge
            
            // 监听 React 内容首次挂载（当 contentView 有了第一个子视图）
            NotificationCenter.default.addObserver(
                forName: NSNotification.Name(rawValue: "RCTContentDidAppearNotification"),
                object: rctRootView,
                queue: .main
            ) { _ in
                
                // 输出性能摘要
            }

            // 监听 Bridge 加载完成
            NotificationCenter.default.addObserver(
                forName: .RCTJavaScriptDidLoad,
                object: nil,
                queue: .main
            ) { _ in
            }

            // 监听 Bridge 加载失败
            NotificationCenter.default.addObserver(
                forName: .RCTJavaScriptDidFailToLoad,
                object: nil,
                queue: .main
            ) { notification in
                if let error = notification.userInfo?["error"] as? Error {
                    nerror("❌ [Native] JS 加载失败: \(error.localizedDescription)")
                }
            }

            nnotice("✅ [Native] RCTRootView 创建成功")

            // 检查 bridge 状态
            let bridge = rctRootView.bridge

            #if DEBUG
            // 禁用 RN Dev Menu 默认快捷键（Cmd+D、Cmd+I）
            // 我们用自己的 DevTools 代替
            if let devMenu = bridge.module(forName: "DevMenu") as? RCTDevMenu {
                devMenu.hotkeysEnabled = false
            }
            
            // 使用 RCTDevSettings 配置禁用 Shake 手势
            if let devSettings = bridge.devSettings {
                devSettings.isShakeToShowDevMenuEnabled = false
            }
            #endif

            // 设置背景透明
            rctRootView.backgroundColor = UIColor.clear
            

            // 延迟检查 JavaScript 是否加载成功
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {

                // 手动修复 RCTRootContentView 的 frame
                let contentView = rctRootView.contentView
                let parentFrame = rctRootView.bounds
                contentView.frame = parentFrame
                contentView.setNeedsLayout()
                contentView.layoutIfNeeded()
            }

            DispatchQueue.main.asyncAfter(deadline: .now() + 5.0) {

                // 递归打印视图层级
                func logSubviews(_ view: UIView, depth: Int = 0) {
                    let indent = String(repeating: "  ", count: depth)
                    for (index, subview) in view.subviews.enumerated() {
                        nlog("\(indent)[\(index)] \(type(of: subview)) - frame: \(subview.frame.size)")
                        logSubviews(subview, depth: depth + 1)
                    }
                }
                logSubviews(rctRootView)

                // 检查 RCTRootContentView 子视图
                let contentView = rctRootView.contentView
                if contentView.subviews.isEmpty {
                    nerror("❌ [Native] ContentView 无子视图！React 可能未渲染")
                    nerror("👉 [Native] Debug：请确认 Metro 已启动（`cd Actro/ActroRuntime && bun run start`）；Release：确保已生成 `actr.jsbundle`")
                } else {
                }
            }
        } else {
            nfault("❌ [Native] RootView 不是 RCTRootView 类型")
        }

        return rootView
    }
    
    func evaluateScript(_ code: String, tabId: UUID) {
        guard let bridge = self.bridge else { return }
        
        // Send event to ActrApp to evaluate in the specific engine
        bridge.enqueueJSCall(
            "RCTDeviceEventEmitter",
            method: "emit",
            args: [
                "DEVTOOLS_EVAL",
                [
                    "tabId": tabId.uuidString,
                    "code": code
                ]
            ],
            completion: nil
        )
    }
}
