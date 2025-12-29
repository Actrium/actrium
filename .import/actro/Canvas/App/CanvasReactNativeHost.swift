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

// 🕐 性能计时器 - 跟踪 RN 启动各阶段耗时
private class NativePerfTimer {
    static let shared = NativePerfTimer()

    private let appStartTime: CFAbsoluteTime
    private var marks: [(name: String, time: CFAbsoluteTime)] = []

    private init() {
        appStartTime = CFAbsoluteTimeGetCurrent()
    }

    func mark(_ name: String) {
        let now = CFAbsoluteTimeGetCurrent()
        let elapsed = (now - appStartTime) * 1000  // 转换为毫秒
        marks.append((name, now))

        // 计算与上一个标记的间隔
        var delta: Double = 0
        if marks.count > 1 {
            let prevTime = marks[marks.count - 2].time
            delta = (now - prevTime) * 1000
        }

        nlog("⏱️ [NativePerf] \(name): T+\(String(format: "%.0f", elapsed))ms (+\(String(format: "%.0f", delta))ms)")
    }

    func summary() {
        nlog("⏱️ [NativePerf] ===== 性能摘要 =====")
        for (index, mark) in marks.enumerated() {
            let total = (mark.time - appStartTime) * 1000
            var delta: Double = 0
            if index > 0 {
                delta = (mark.time - marks[index - 1].time) * 1000
            }
            nlog("⏱️ [NativePerf]   \(index + 1). \(mark.name): T+\(String(format: "%.0f", total))ms (+\(String(format: "%.0f", delta))ms)")
        }
        nlog("⏱️ [NativePerf] ====================")
    }
}

final class CanvasReactNativeFactoryDelegate: RCTDefaultReactNativeFactoryDelegate {
    override init() {
        NativePerfTimer.shared.mark("FactoryDelegate init 开始")
        super.init()
        self.dependencyProvider = RCTAppDependencyProvider()
        NativePerfTimer.shared.mark("FactoryDelegate init 完成")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func bundleURL() -> URL? {
        NativePerfTimer.shared.mark("bundleURL() 调用")
        #if DEBUG
        // 开发时强制走 Metro（支持热重载）；Debug 环境不回退到离线 actr.jsbundle（避免落到过期 bundle 导致"白屏无渲染"）
        // 直接使用 localhost:8082，避免 RCTBundleURLProvider 返回空 hostname 的问题
        // 端口 8082 与 Canvas/Subsystem 的 dev.ts 默认配置保持一致
        let url = RCTBundleURLProvider.jsBundleURL(
            forBundleRoot: "index",
            packagerHost: "localhost:8082",
            enableDev: true,
            enableMinification: false,
            inlineSourceMap: false
        )
        nlog("🔗 [Native] Bundle URL: \(url?.absoluteString ?? "nil")")
        NativePerfTimer.shared.mark("bundleURL() 返回")
        return url
        #else
        // 发布版走离线 bundle
        // 注意：不要叫 main.jsbundle，避免与 RN 默认 bundling 产物重名导致 Xcode "Multiple commands produce …/main.jsbundle"
        NativePerfTimer.shared.mark("bundleURL() 返回 (Release)")
        return Bundle.main.url(forResource: "actr", withExtension: "jsbundle")
        #endif
    }

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        NativePerfTimer.shared.mark("sourceURL(for bridge:) 调用")
        return bundleURL()
    }
}

final class CanvasReactNativeHost {
    static let shared = CanvasReactNativeHost()

    private let factoryDelegate: CanvasReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory
    private var bridge: RCTBridge?

    private init() {
        NativePerfTimer.shared.mark("Host init 开始")
        nnotice("🏗️ [Native] CanvasReactNativeHost 初始化")

        #if DEBUG
        // macOS RN：LogBox/RedBox 在部分版本上可能触发 UIBlock 异常（如 NULL CGColor），导致 UI 不渲染
        // 先禁用 RedBox（LogBox.show 也会被 gate），避免影响主界面渲染。
        RCTRedBoxSetEnabled(false)
        nwarn("🧪 [Native] Debug：已禁用 RedBox/LogBox（避免 UIBlock 异常导致白屏）")
        #endif

        NativePerfTimer.shared.mark("创建 FactoryDelegate")
        factoryDelegate = CanvasReactNativeFactoryDelegate()
        NativePerfTimer.shared.mark("创建 Factory")
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
        NativePerfTimer.shared.mark("Host init 完成")
        nnotice("✅ [Native] Factory 创建完成")
    }

    func createRootView(initialProperties: [String: Any]) -> NSView {
        NativePerfTimer.shared.mark("createRootView 开始")
        nnotice("📱 [Native] 创建 RootView，模块名: ActrApp")
        nlog("📱 [Native] initialProperties keys: \(initialProperties.keys.joined(separator: ", "))")

        NativePerfTimer.shared.mark("调用 factory.rootViewFactory.view")
        let rootView = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: initialProperties,
            launchOptions: nil
        )
        NativePerfTimer.shared.mark("RootView 实例创建完成")

        if let rctRootView = rootView as? RCTRootView {
            // Store bridge reference
            self.bridge = rctRootView.bridge
            
            // 监听 React 内容首次挂载（当 contentView 有了第一个子视图）
            NotificationCenter.default.addObserver(
                forName: NSNotification.Name(rawValue: "RCTContentDidAppearNotification"),
                object: rctRootView,
                queue: .main
            ) { _ in
                NativePerfTimer.shared.mark("🎉 RCTContentDidAppear (JS渲染完成)")
                nlog("✅ [Native] RCTContentDidAppearNotification：React 内容已挂载")
                // 输出性能摘要
                NativePerfTimer.shared.summary()
            }

            // 监听 Bridge 加载完成
            NotificationCenter.default.addObserver(
                forName: .RCTJavaScriptDidLoad,
                object: nil,
                queue: .main
            ) { _ in
                NativePerfTimer.shared.mark("JS Bundle 加载完成 (RCTJavaScriptDidLoad)")
            }

            // 监听 Bridge 加载失败
            NotificationCenter.default.addObserver(
                forName: .RCTJavaScriptDidFailToLoad,
                object: nil,
                queue: .main
            ) { notification in
                NativePerfTimer.shared.mark("❌ JS Bundle 加载失败")
                if let error = notification.userInfo?["error"] as? Error {
                    nerror("❌ [Native] JS 加载失败: \(error.localizedDescription)")
                }
            }

            nnotice("✅ [Native] RCTRootView 创建成功")
            nlog("📍 [Native] moduleName: \(rctRootView.moduleName)")

            // 检查 bridge 状态
            let bridge = rctRootView.bridge
            NativePerfTimer.shared.mark("Bridge 状态检查")
            nlog("🌉 [Native] Bridge valid: \(bridge.isValid), loading: \(bridge.isLoading)")

            #if DEBUG
            // 禁用 RN Dev Menu 默认快捷键（Cmd+D、Cmd+I）
            // 我们用自己的 DevTools 代替
            if let devMenu = bridge.module(forName: "DevMenu") as? RCTDevMenu {
                devMenu.hotkeysEnabled = false
                nlog("🔧 [Native] 已禁用 RN DevMenu 快捷键（使用我们的 DevTools）")
            }
            #endif

            // 🔴 关键：在 macOS 上必须设置 sizeFlexibility 为 .none
            rctRootView.sizeFlexibility = .none
            rctRootView.backgroundColor = NSColor.clear

            nlog("🔧 [Native] 设置 sizeFlexibility = .none")
            NativePerfTimer.shared.mark("createRootView 返回")

            // 延迟检查 JavaScript 是否加载成功
            DispatchQueue.main.asyncAfter(deadline: .now() + 2.0) {
                nlog("⏱️ [Native] 2秒检查 - Bridge valid: \(bridge.isValid), loading: \(bridge.isLoading)")
                nlog("⏱️ [Native] 2秒检查 - RootView bounds: \(rctRootView.bounds.size)")
                nlog("⏱️ [Native] 2秒检查 - RootView subviews: \(rctRootView.subviews.count)")

                // 手动修复 RCTRootContentView 的 frame
                let contentView = rctRootView.contentView
                let parentFrame = rctRootView.bounds
                nlog("🔧 [Native] 设置 ContentView frame: \(parentFrame.size)")
                contentView.frame = parentFrame
                contentView.needsLayout = true
                contentView.layoutSubtreeIfNeeded()
            }

            DispatchQueue.main.asyncAfter(deadline: .now() + 5.0) {
                nlog("⏱️ [Native] 5秒检查 - Bridge valid: \(bridge.isValid), loading: \(bridge.isLoading)")
                nlog("⏱️ [Native] 5秒检查 - RootView bounds: \(rctRootView.bounds.size)")
                nlog("⏱️ [Native] 5秒检查 - RootView subviews: \(rctRootView.subviews.count)")

                // 递归打印视图层级
                func logSubviews(_ view: NSView, depth: Int = 0) {
                    let indent = String(repeating: "  ", count: depth)
                    for (index, subview) in view.subviews.enumerated() {
                        nlog("\(indent)[\(index)] \(type(of: subview)) - frame: \(subview.frame.size)")
                        logSubviews(subview, depth: depth + 1)
                    }
                }
                nlog("📊 [Native] 视图层级:")
                logSubviews(rctRootView)

                // 检查 RCTRootContentView 子视图
                let contentView = rctRootView.contentView
                nlog("📊 [Native] ContentView 子视图数: \(contentView.subviews.count)")
                if contentView.subviews.isEmpty {
                    nerror("❌ [Native] ContentView 无子视图！React 可能未渲染")
                    nerror("👉 [Native] Debug：请确认 Metro 已启动（`cd Canvas/Subsystem && bun run start`）；Release：确保已生成 `actr.jsbundle`")
                } else {
                    nlog("✅ [Native] React 已渲染 \(contentView.subviews.count) 个子视图")
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
        bridge.eventDispatcher().sendAppEvent(withName: "DEVTOOLS_EVAL", body: [
            "tabId": tabId.uuidString,
            "code": code
        ])
    }
}
