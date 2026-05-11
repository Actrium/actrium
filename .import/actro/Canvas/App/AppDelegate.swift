import UIKit

class AppDelegate: NSObject, UIApplicationDelegate {
    /// 关键属性：React Native 的原生代码（如 RCTKeyboardObserver）会尝试通过 application.delegate 访问 window。
    /// 在 SwiftUI 架构下，我们需要显式声明它以防止 unrecognized selector 导致的崩溃。
    var window: UIWindow?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey : Any]? = nil) -> Bool {
        return true
    }

    func application(_ app: UIApplication, open url: URL, options: [UIApplication.OpenURLOptionsKey : Any] = [:]) -> Bool {
        appLogger.debug("接收到 URL：\(url)")
        // 微信回调路径包含 weChatAppId，需优先交给微信处理；
        // AskAway 通用链接（路径不含 weChatAppId）返回 false，由 SwiftUI onOpenURL 处理。
        if url.path.contains(AppConstants.weChatAppId) {
            return WeChatManager.shared.handleOpenURL(url)
        }
        return false
    }

    func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
        appLogger.warning("⚠️ [AppDelegate] 收到系统内存警告，触发自动回收")
        Task { @MainActor in
            BrowserViewModel.active?.handleMemoryWarning()
        }
    }
}
