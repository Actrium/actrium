import UIKit

class AppDelegate: NSObject, UIApplicationDelegate {
    /// Key Properties:React Native or the original code of the RCTKeyboardObserver♪ I'll try through ♪ application.delegate Visits window.
    /// Yes. SwiftUI Under the structure, we need to make a visible statement about it to prevent it from happening. unrecognized selector It led to a collapse.
    var window: UIWindow?

    func application(_ application: UIApplication, didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey : Any]? = nil) -> Bool {
        return true
    }

    func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
        appLogger.warning("⚠️ [AppDelegate] You're gonna need to get a system alert and start automatic recovery.")
        Task { @MainActor in
            BrowserViewModel.active?.handleMemoryWarning()
        }
    }
}
