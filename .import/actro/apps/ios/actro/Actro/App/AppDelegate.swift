import UIKit

class AppDelegate: NSObject, UIApplicationDelegate {
    /// 关键属性：React Native 的原生代码（如 RCTKeyboardObserver）会尝试通过 application.delegate 访问 window。
    /// 在 SwiftUI 架构下，我们需要显式声明它以防止 unrecognized selector 导致的崩溃。
    var window: UIWindow?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey : Any]? = nil) -> Bool {
        return true
    }

    func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
        appLogger.warning("⚠️ [AppDelegate] 收到系统内存警告，触发自动回收")
        Task { @MainActor in
            BrowserViewModel.active?.handleMemoryWarning()
        }
    }
}
