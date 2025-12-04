import Foundation
import React
import React_RCTAppDelegate
import ReactAppDependencyProvider

final class CanvasReactNativeFactoryDelegate: RCTDefaultReactNativeFactoryDelegate {
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
        // 开发时走 Metro，支持热重载
        let url = RCTBundleURLProvider.sharedSettings().jsBundleURL(forBundleRoot: "index")
        print("🔗 React Native Bundle URL: \(url?.absoluteString ?? "nil")")
        return url
        #else
        // 发布版走离线 bundle
        return Bundle.main.url(forResource: "main", withExtension: "jsbundle")
        #endif
    }

    override func sourceURL(for bridge: RCTBridge) -> URL? {
        return bundleURL()
    }
}

final class CanvasReactNativeHost {
    static let shared = CanvasReactNativeHost()

    private let factoryDelegate: CanvasReactNativeFactoryDelegate
    private let factory: RCTReactNativeFactory

    private init() {
        factoryDelegate = CanvasReactNativeFactoryDelegate()
        factory = RCTReactNativeFactory(delegate: factoryDelegate)
    }

    func createRootView(initialProperties: [String: Any]) -> NSView {
        let rootView = factory.rootViewFactory.view(
            withModuleName: "ActrApp",
            initialProperties: initialProperties,
            launchOptions: nil
        )
        return rootView
    }
}
