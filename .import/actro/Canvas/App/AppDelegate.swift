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
        // AskAway 通用链接由 SwiftUI onOpenURL 处理，此处只处理微信回调
        let askawayHosts: Set<String> = ["go.actrium.example", "go.actrium-dev.example"]
        if let host = url.host, askawayHosts.contains(host) {
            return false
        }
        return WeChatManager.shared.handleOpenURL(url)
    }
}
