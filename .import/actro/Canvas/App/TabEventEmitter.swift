import Foundation
import React

private let logger = AppLogger.reactNative

/// Tab Event Emitter - 向 RN 发送 tab 相关事件
@objc(TabEventEmitter)
public class TabEventEmitter: RCTEventEmitter {
    
    static var shared: TabEventEmitter?
    
    override init() {
        super.init()
        TabEventEmitter.shared = self
        setupNotificationObservers()
    }
    
    deinit {
        NotificationCenter.default.removeObserver(self)
    }
    
    // MARK: - RCTEventEmitter Required Methods
    
    @objc public override func supportedEvents() -> [String] {
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
    
    @objc public override static func requiresMainQueueSetup() -> Bool {
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
}
