import Foundation
import React

private let logger = AppLogger.reactNative

/// Tab Event Emitter - 向 RN 发送 tab 相关事件
@objc(TabEventEmitter)
class TabEventEmitter: RCTEventEmitter {
    
    static var shared: TabEventEmitter?
    
    override init() {
        super.init()
        TabEventEmitter.shared = self
        setupNotificationObservers()
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    // MARK: - RCTEventEmitter
    
    override func supportedEvents() -> [String]! {
        return [
            "onTabOpened",
            "onTabClosed",
            "onTabSwitched",
            "onTabUpdated",
            "onTabsReordered",
            "onTabEnteredFullscreen",
            "onTabExitedFullscreen"
        ]
    }
    
    override static func requiresMainQueueSetup() -> Bool {
        return true
    }
    
    // MARK: - Notification Observers
    
    private func setupNotificationObservers() {
        // Tab opened
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabOpened(_:)),
            name: .tabOpenedFromNative,
            object: nil
        )
        
        // Tab closed
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabClosed(_:)),
            name: .tabClosedFromNative,
            object: nil
        )
        
        // Tab switched
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabSwitched(_:)),
            name: .tabSwitchedFromNative,
            object: nil
        )
        
        // Tab updated
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabUpdated(_:)),
            name: .tabUpdatedFromNative,
            object: nil
        )
        
        // Tabs reordered
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabsReordered(_:)),
            name: .tabsReorderedFromNative,
            object: nil
        )
        
        // Tab entered fullscreen
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabEnteredFullscreen(_:)),
            name: .tabEnteredFullscreenFromNative,
            object: nil
        )
        
        // Tab exited fullscreen
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(handleTabExitedFullscreen(_:)),
            name: .tabExitedFullscreenFromNative,
            object: nil
        )
    }
    
    // MARK: - Event Handlers
    
    @objc private func handleTabOpened(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String,
              let title = userInfo["title"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabOpened notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "title": title,
            "systemTabType": userInfo["systemTabType"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabOpened: \(tabId)")
        sendEvent(withName: "onTabOpened", body: event)
    }
    
    @objc private func handleTabClosed(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabClosed notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabClosed: \(tabId)")
        sendEvent(withName: "onTabClosed", body: event)
    }
    
    @objc private func handleTabSwitched(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabSwitched notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "previousTabId": userInfo["previousTabId"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabSwitched: \(tabId)")
        sendEvent(withName: "onTabSwitched", body: event)
    }
    
    @objc private func handleTabUpdated(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabUpdated notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "title": userInfo["title"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabUpdated: \(tabId)")
        sendEvent(withName: "onTabUpdated", body: event)
    }
    
    @objc private func handleTabsReordered(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabIds = userInfo["tabIds"] as? [String] else {
            logger.warning("[TabEventEmitter] Invalid tabsReordered notification")
            return
        }
        
        let event: [String: Any] = [
            "tabIds": tabIds,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabsReordered")
        sendEvent(withName: "onTabsReordered", body: event)
    }
    
    @objc private func handleTabEnteredFullscreen(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabEnteredFullscreen notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "title": userInfo["title"] as? String ?? NSNull(),
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabEnteredFullscreen: \(tabId)")
        sendEvent(withName: "onTabEnteredFullscreen", body: event)
    }
    
    @objc private func handleTabExitedFullscreen(_ notification: Notification) {
        guard let userInfo = notification.userInfo,
              let tabId = userInfo["tabId"] as? String else {
            logger.warning("[TabEventEmitter] Invalid tabExitedFullscreen notification")
            return
        }
        
        let event: [String: Any] = [
            "tabId": tabId,
            "timestamp": Date().timeIntervalSince1970 * 1000
        ]
        
        logger.debug("[TabEventEmitter] Sending onTabExitedFullscreen: \(tabId)")
        sendEvent(withName: "onTabExitedFullscreen", body: event)
    }
    
    // MARK: - Public Methods
    
    /// Exit fullscreen mode and return to tab list (iPhone only)
    @objc
    public func exitFullscreen(
        _ resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        #if os(iOS)
        if UIDevice.current.userInterfaceIdiom != .phone {
            reject("UNSUPPORTED_DEVICE", "This method is only supported on iPhone", nil)
            return
        }
        #else
        reject("UNSUPPORTED_PLATFORM", "This method is only supported on iOS", nil)
        return
        #endif

        DispatchQueue.main.async {
            // 发送通知，让 PhoneMainView 退出全屏
            NotificationCenter.default.post(
                name: .exitFullscreenFromRN,
                object: nil
            )
            resolve(true)
        }
    }
    
    /// Open a specific tab by ID (Phone: enter fullscreen, Pad: switch tab)
    /// If tabId is empty or not found, creates a new tab
    @objc
    public func openTab(
        _ dict: [String: Any],
        resolve: @escaping RCTPromiseResolveBlock,
        reject: @escaping RCTPromiseRejectBlock
    ) {
        DispatchQueue.main.async {
            guard let viewModel = BrowserViewModel.active else {
                reject("NO_VIEW_MODEL", "BrowserViewModel not available", nil)
                return
            }
            
            let tabId = dict["tabId"] as? String
            var targetTab: Tab?
            
            // 如果提供了 tabId，尝试查找对应的 tab
            if let tabId = tabId, !tabId.isEmpty,
               let uuid = UUID(uuidString: tabId) {
                targetTab = viewModel.tabs.first(where: { $0.id == uuid })
            }
            
            // 如果没有找到 tab（或 tabId 为空），创建新 tab
            if targetTab == nil {
                targetTab = viewModel.addNewTab()
                logger.info("[TabEventEmitter] Created new tab: \(targetTab!.id.uuidString)")
            }
            
            guard let tab = targetTab else {
                reject("TAB_ERROR", "Failed to create or find tab", nil)
                return
            }
            
            // 切换到目标 tab
            viewModel.currentTabId = tab.id
            
            #if os(iOS)
            if UIDevice.current.userInterfaceIdiom == .phone {
                // Phone 端：发送通知进入全屏
                NotificationCenter.default.post(
                    name: .openTabFromRN,
                    object: nil,
                    userInfo: ["tabId": tab.id.uuidString]
                )
            }
            #endif
            
            resolve(["tabId": tab.id.uuidString])
        }
    }
}

// MARK: - Notification Names

extension Notification.Name {
    static let tabOpenedFromNative = Notification.Name("tabOpenedFromNative")
    static let tabClosedFromNative = Notification.Name("tabClosedFromNative")
    static let tabSwitchedFromNative = Notification.Name("tabSwitchedFromNative")
    static let tabUpdatedFromNative = Notification.Name("tabUpdatedFromNative")
    static let tabsReorderedFromNative = Notification.Name("tabsReorderedFromNative")
    static let tabEnteredFullscreenFromNative = Notification.Name("tabEnteredFullscreenFromNative")
    static let tabExitedFullscreenFromNative = Notification.Name("tabExitedFullscreenFromNative")
    static let exitFullscreenFromRN = Notification.Name("exitFullscreenFromRN")
    static let openTabFromRN = Notification.Name("openTabFromRN")
}
